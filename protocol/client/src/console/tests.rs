//! Unit tests for `nightjar_core::console` covering stores, `.bak` damaged favourites,
//! legacy migration, `ConsoleRepository` parallel refresh, 5-min capability cooldown,
//! `add_to_library`, starred renumbering, and `forget_host`.

use super::*;
use nightjar_policy::http::native_api::capabilities::Capabilities;
use nightjar_policy::http::native_api::library::{Artwork, Game, Library, Store};
use nightjar_policy::http::native_api::session::{Owner, Session, SessionApp, State};
use nightjar_policy::http::{AddedGame, FilePermissions, GameExtra, NativeCapabilities, NativeLibrary};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nightjar-core-console-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_game(install_id: &str, app_id: u32, title: &str, game_id: Option<&str>) -> Game {
    Game {
        install_id: install_id.into(),
        app_id,
        title: title.into(),
        game_id: game_id.map(str::to_owned),
        store: Some(Store {
            source: "steam".into(),
            id: "1".into(),
            name: "Steam".into(),
        }),
        installed: Some(true),
        running: false,
        artwork: Artwork {
            cover: Some(format!("/c/{install_id}")),
            version: "v1".into(),
        },
        last_played: None,
        playtime_minutes: None,
        collections: Vec::new(),
    }
}

fn host_desc(uuid: &str, name: &str) -> HostDescriptor {
    HostDescriptor {
        uuid: uuid.into(),
        name: name.into(),
        online: true,
        paired: true,
        is_native: true,
        address: "127.0.0.1".into(),
        http_port: 47989,
        https_port: 47984,
        server_cert_pem: None,
        classic_apps: None,
        running_app_id: None,
        running_app_uuid: None,
    }
}

struct MockTransport {
    caps_calls: AtomicUsize,
    fail_caps: bool,
    libs: Mutex<BTreeMap<String, NativeLibrary>>,
}

impl NativeTransport for MockTransport {
    fn capabilities(&self, _host: &HostDescriptor) -> Result<NativeCapabilities, String> {
        self.caps_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_caps {
            return Err("caps unreachable".into());
        }
        let mut caps: Capabilities = serde_json::from_str(
            r#"{"api_version":1,"host":{"name":"Desk","version":"3.0.0","unique_id":"pc1"},"features":["library"],"permissions":{"list":true,"view":true,"launch":true,"host_control":false}}"#,
        )
        .unwrap();
        caps.features.push("library.unadded".into());
        Ok(NativeCapabilities {
            capabilities: caps,
            files: FilePermissions::default(),
        })
    }

    fn library(
        &self,
        host: &HostDescriptor,
        include_unadded: bool,
    ) -> Result<NativeLibrary, String> {
        let mut lib = self
            .libs
            .lock()
            .unwrap()
            .get(&host.uuid)
            .cloned()
            .ok_or_else(|| "no mock lib".to_owned())?;
        if !include_unadded {
            let keep: Vec<bool> = (0..lib.library.games.len())
                .map(|i| lib.added_at(i))
                .collect();
            let mut idx = 0;
            lib.library.games.retain(|_| {
                let k = keep[idx];
                idx += 1;
                k
            });
            let mut eidx = 0;
            lib.extra.retain(|_| {
                let k = keep[eidx];
                eidx += 1;
                k
            });
        }
        Ok(lib)
    }

    fn session(&self, _host: &HostDescriptor) -> Result<Session, String> {
        Ok(Session {
            revision: 1,
            state: State::Streaming,
            owner: Owner::You,
            app: Some(SessionApp {
                install_id: "i1".into(),
                app_id: 1,
                title: "Portal".into(),
            }),
            game_running: true,
            since_unix_ms: None,
            suspended_until_unix_ms: None,
            launcher: None,
            radeon_profile: None,
            microphone_busy: false,
            playing_on: None,
            moved_to: None,
            owns_game: true,
            press_failed: false,
            actions: Vec::new(),
        })
    }

    fn add_to_library(
        &self,
        _host: &HostDescriptor,
        game_id: &str,
    ) -> Result<Option<AddedGame>, String> {
        Ok(Some(AddedGame {
            game: sample_game("added-9", 99, "Celeste", Some(game_id)),
            extra: GameExtra::default(),
        }))
    }
}

#[test]
fn stores_handle_sanitize_recents_cap_damaged_favourites_and_forget() {
    let dir = temp_dir("stores");
    assert_eq!(sanitize_host_id("a/b:c"), "a_b_c");
    assert_eq!(sanitize_host_id(".."), "_");

    let recents = RecentsStore::new(&dir);
    for i in 1..=25u32 {
        recents
            .record(Recent {
                host: "h1".into(),
                install_id: format!("id-{i}"),
                app_id: i,
                title: format!("Game {i}"),
                at: i as u64,
            })
            .unwrap();
    }
    assert_eq!(recents.list().len(), ANDROID_RECENT_LIMIT);
    assert_eq!(recents.list()[0].app_id, 25);

    // Legacy damaged favourites creates .bak while reading valid entries.
    let favs = FavouritesStore::new(&dir);
    assert!(favs
        .import_legacy_json(r#"[{"host":"h1","install_id":"i1","app_id":1,"title":"Good"},{"bad":true}]"#)
        .unwrap());
    assert_eq!(favs.list().len(), 1);
    assert!(dir.join("console").join("console_favourites.json.bak").exists());

    forget_host(&dir, "h1");
    assert!(recents.list().is_empty());
    assert!(favs.list().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repository_refreshes_respects_cooldown_and_reconciles_favourites() {
    let dir = temp_dir("repo");
    let repo = ConsoleRepository::new(&dir);
    repo.set_favourite(Favourite {
        host: "pc-1".into(),
        install_id: "i1".into(),
        app_id: 500,
        title: "Old".into(),
    })
    .unwrap();

    let mut lib = NativeLibrary::from(Library {
        revision: "r1".into(),
        games: vec![
            sample_game("i1", 1, "Portal", Some("steam:400")),
            sample_game("", 0, "Celeste", Some("gog:c")),
        ],
    });
    lib.extra[1].added = false;
    let mut map = BTreeMap::new();
    map.insert("pc-1".into(), lib);

    let failing_caps = MockTransport {
        caps_calls: AtomicUsize::new(0),
        fail_caps: true,
        libs: Mutex::new(map.clone()),
    };
    let hosts = [host_desc("pc-1", "Desk")];
    let snap1 = repo.refresh(&hosts, &failing_caps);
    assert_eq!(failing_caps.caps_calls.load(Ordering::SeqCst), 1);
    // Second refresh hits 5-minute cooldown instead of retrying capabilities.
    let _ = repo.refresh(&hosts, &failing_caps);
    assert_eq!(failing_caps.caps_calls.load(Ordering::SeqCst), 1);

    // Classic renumbered favourite was repaired on cached/refresh snapshot.
    assert_eq!(snap1.starred[0].app_id, 1);
    assert_eq!(snap1.starred[0].title, "Portal");
    assert_eq!(snap1.continue_playing.len(), 1);

    // add_to_library returns the added entry without mutating the cached library until refresh.
    let working = MockTransport {
        caps_calls: AtomicUsize::new(0),
        fail_caps: false,
        libs: Mutex::new(map),
    };
    let added = repo.add_to_library(&hosts[0], "gog:c", &working).unwrap().unwrap();
    assert_eq!(added.game.install_id, "added-9");
    let snap2 = repo.cached(&hosts);
    assert_eq!(snap2.titles.len(), 1);
    assert_eq!(snap2.titles[0].title, "Portal");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn find_cached_library_cover_handles_precedence_unadded_explicit_none_and_formats() {
    let dir = temp_dir("cached-cover");
    let store = LibraryCacheStore::new(&dir);

    assert_eq!(find_cached_library_cover(&dir, "", 1, "i1"), None);
    assert_eq!(find_cached_library_cover(&dir, "missing", 1, "i1"), None);

    // Envelope format: unadded entry skipped, install_id case-insensitive takes precedence over app_id,
    // explicitly_no_cover set when cover is None and version is empty.
    let mut g_unadded = sample_game("UNADDED-ID", 10, "Unadded", None);
    g_unadded.artwork.cover = Some("/c/unadded".into());
    g_unadded.artwork.version = "v9".into();

    let mut g_by_appid = sample_game("other-id", 42, "By AppId", None);
    g_by_appid.artwork.cover = Some("/c/42".into());
    g_by_appid.artwork.version = "v42".into();

    let mut g_by_install = sample_game("Target-Install", 77, "By InstallId", None);
    g_by_install.artwork.cover = Some("/c/77".into());
    g_by_install.artwork.version = "v77".into();

    let mut g_no_cover = sample_game("no-cover-id", 88, "No Cover", None);
    g_no_cover.artwork.cover = None;
    g_no_cover.artwork.version = String::new();

    let mut lib = NativeLibrary::from(Library {
        revision: "r1".into(),
        games: vec![g_unadded, g_by_appid, g_by_install, g_no_cover],
    });
    lib.extra[0].added = false;
    store.save_entry("pc-env", &lib, 1000, &[]).unwrap();

    assert_eq!(find_cached_library_cover(&dir, "pc-env", 10, "UNADDED-ID"), None);
    assert_eq!(
        find_cached_library_cover(&dir, "pc-env", 42, "target-install"),
        Some(CachedLibraryCover {
            explicitly_no_cover: false,
            app_id: 77,
            title: "By InstallId".into(),
            cover_version: "v77".into(),
        })
    );
    assert_eq!(
        find_cached_library_cover(&dir, "pc-env", 42, ""),
        Some(CachedLibraryCover {
            explicitly_no_cover: false,
            app_id: 42,
            title: "By AppId".into(),
            cover_version: "v42".into(),
        })
    );
    assert_eq!(
        find_cached_library_cover(&dir, "pc-env", 88, ""),
        Some(CachedLibraryCover {
            explicitly_no_cover: true,
            app_id: 88,
            title: "No Cover".into(),
            cover_version: String::new(),
        })
    );

    // Bare format (`{"revision":..,"games":..}`) also parses cleanly.
    let bare = serde_json::json!({
        "revision": "r-bare",
        "games": [{
            "install_id": "bare-1",
            "app_id": 123,
            "title": "Bare Game",
            "running": false,
            "added": true,
            "artwork": { "cover": "/c/bare", "version": "vbare" }
        }]
    });
    std::fs::write(
        dir.join("console-library").join("pc-bare.json"),
        serde_json::to_vec(&bare).unwrap(),
    )
    .unwrap();
    assert_eq!(
        find_cached_library_cover(&dir, "pc-bare", 123, ""),
        Some(CachedLibraryCover {
            explicitly_no_cover: false,
            app_id: 123,
            title: "Bare Game".into(),
            cover_version: "vbare".into(),
        })
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn library_cache_keeps_features_defaults_older_files_and_repairs_broken_json() {
    let dir = temp_dir("lib-cache-parity");
    let store = LibraryCacheStore::new(&dir);
    let lib = NativeLibrary::from(Library {
        revision: "r1".into(),
        games: vec![sample_game("i1", 1, "Portal", Some("steam:400"))],
    });
    store
        .save_entry(
            "pc1",
            &lib,
            42,
            &["library".into(), "library.unadded".into()],
        )
        .unwrap();
    let (loaded, fetched_ms, features) = store.load_entry("pc1").unwrap();
    assert_eq!(loaded.library.games.len(), 1);
    assert_eq!(fetched_ms, 42);
    assert_eq!(features, vec!["library", "library.unadded"]);

    // Older cache file lacking `"features"` and `"revision"` still loads cleanly.
    std::fs::write(
        store.path_for("legacy"),
        r#"{"fetched_ms":3,"library":{"games":[]}}"#,
    )
    .unwrap();
    let (legacy_lib, legacy_ms, legacy_features) = store.load_entry("legacy").unwrap();
    assert_eq!(legacy_ms, 3);
    assert!(legacy_features.is_empty());
    assert!(legacy_lib.library.games.is_empty());
    assert_eq!(legacy_lib.library.revision, "");

    // Empty games[] cache does not count as having any cached library.
    store.forget("pc1");
    assert!(!has_any_library(&dir));

    // Broken JSON reads as None and is repaired on the next save.
    std::fs::write(store.path_for("broken"), "{not json").unwrap();
    assert!(store.load("broken").is_none());
    assert!(!has_any_library(&dir));

    store
        .save_entry("broken", &lib, 99, &["library".into()])
        .unwrap();
    assert_eq!(store.load("broken").unwrap().library.games.len(), 1);
    assert!(has_any_library(&dir));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn favourites_preserve_order_recover_from_corrupt_json_and_match_stable_id_before_app_id() {
    let dir = temp_dir("fav-parity");
    let store = FavouritesStore::new(&dir);
    assert!(store.list().is_empty());

    store
        .set_explicit(
            Favourite {
                host: "a".into(),
                install_id: "inst-1".into(),
                app_id: 5,
                title: "One".into(),
            },
            true,
        )
        .unwrap();
    store
        .set_explicit(
            Favourite {
                host: "b".into(),
                install_id: "inst-2".into(),
                app_id: 2,
                title: "Two".into(),
            },
            true,
        )
        .unwrap();
    assert_eq!(
        store.list().iter().map(|f| f.title.as_str()).collect::<Vec<_>>(),
        ["Two", "One"]
    );

    // Re-starring updates stored fields in place while preserving position.
    store
        .set_explicit(
            Favourite {
                host: "a".into(),
                install_id: "inst-1".into(),
                app_id: 5,
                title: "One HD".into(),
            },
            true,
        )
        .unwrap();
    assert_eq!(
        store.list().iter().map(|f| f.title.as_str()).collect::<Vec<_>>(),
        ["Two", "One HD"]
    );

    // Unstarring a missing entry is a no-op.
    store
        .set_explicit(
            Favourite {
                host: "a".into(),
                install_id: "missing".into(),
                app_id: 99,
                title: "Nope".into(),
            },
            false,
        )
        .unwrap();
    assert_eq!(store.list().len(), 2);

    // Stable ID matches before app_id; differing install_id on the same app_id must not match.
    assert!(!store.is_favourite("a", "inst-9", 5));
    assert!(store.is_favourite("a", "inst-1", 40));
    assert!(store.is_favourite("a", "", 5));

    // Renumbered removal matches by install_id.
    store
        .set_explicit(
            Favourite {
                host: "a".into(),
                install_id: "inst-1".into(),
                app_id: 40,
                title: "One HD".into(),
            },
            false,
        )
        .unwrap();
    assert!(!store.is_favourite("a", "inst-1", 5));

    // forget_host isolates other hosts.
    store.forget_host("b").unwrap();
    assert!(store.list().is_empty());

    // Direct corrupt file on disk reads as empty, writes .bak, and recovers on next write.
    let fav_path = dir.join("console").join("console_favourites.json");
    std::fs::create_dir_all(fav_path.parent().unwrap()).unwrap();
    std::fs::write(&fav_path, "{not json").unwrap();
    assert!(store.list().is_empty());
    assert_eq!(
        std::fs::read_to_string(dir.join("console").join("console_favourites.json.bak")).unwrap(),
        "{not json"
    );
    store
        .set_explicit(
            Favourite {
                host: "a".into(),
                install_id: "inst-3".into(),
                app_id: 3,
                title: "Three".into(),
            },
            true,
        )
        .unwrap();
    assert_eq!(store.list().len(), 1);
    assert!(store.is_favourite("a", "inst-3", 3));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hidden_store_and_repo_prevent_collisions_bare_leaks_and_split_brain() {
    let dir = temp_dir("hidden-bugs");
    let hidden = HiddenStore::new(&dir);

    // Challenger Bug #1: distinct install_ids sharing an app_id do not collide or cancel out.
    let alpha = Favourite {
        host: "h".into(),
        install_id: "alpha".into(),
        app_id: 1,
        title: "Alpha".into(),
    };
    let beta = Favourite {
        host: "h".into(),
        install_id: "beta".into(),
        app_id: 1,
        title: "Beta".into(),
    };
    assert!(hidden.set_hidden(vec![alpha.clone()], true).unwrap());
    assert!(hidden.set_hidden(vec![beta.clone()], true).unwrap());
    assert_eq!(hidden.list().len(), 2);
    assert!(hidden.is_hidden("h", "alpha", 1));
    assert!(hidden.is_hidden("h", "beta", 1));
    assert!(hidden.set_hidden(vec![beta.clone()], false).unwrap());
    assert!(hidden.is_hidden("h", "alpha", 1));
    assert!(!hidden.is_hidden("h", "beta", 1));

    // import_legacy_entries keeps distinct install_ids sharing an app_id.
    assert!(
        hidden
            .import_legacy_entries(vec![Favourite {
                host: "h2".into(),
                install_id: "alpha".into(),
                app_id: 1,
                title: "A".into(),
            }, Favourite {
                host: "h2".into(),
                install_id: "beta".into(),
                app_id: 1,
                title: "B".into(),
            }])
            .unwrap()
    );
    assert!(hidden.is_hidden("h2", "alpha", 1));
    assert!(hidden.is_hidden("h2", "beta", 1));

    // Challenger Bug #2: set_app cleans matching bare entry and following is case-insensitive
    // without minting new hidden entries for another game at the old app_id.
    hidden.changed("host", &[], &[1]).unwrap();
    hidden.set_app("host", 1, "Alpha", true).unwrap();
    let mut games = BTreeMap::new();
    games.insert("alpha".into(), 7);
    games.insert("beta".into(), 1);
    let ids = hidden.following("host", &games).unwrap();
    assert_eq!(ids.into_iter().collect::<Vec<_>>(), vec![7]);
    assert!(hidden.is_hidden("host", "alpha", 7));
    assert!(!hidden.is_hidden("host", "beta", 1));

    // ConsoleRepository::refresh reconciles hidden_apps.json so Home and AppView agree,
    // emits capabilities_fetched on the initial read only, and ignores unpaired classic_apps.
    let repo_dir = temp_dir("repo-reconcile");
    let repo = ConsoleRepository::new(&repo_dir);
    repo.set_hidden(
        vec![Favourite {
            host: "pc1".into(),
            install_id: "alpha".into(),
            app_id: 1,
            title: "Alpha".into(),
        }],
        true,
    )
    .unwrap();
    let mut map = BTreeMap::new();
    map.insert(
        "pc1".into(),
        NativeLibrary::from(Library {
            revision: "r2".into(),
            games: vec![
                sample_game("alpha", 7, "Alpha", None),
                sample_game("beta", 1, "Beta", None),
            ],
        }),
    );
    let transport = MockTransport {
        caps_calls: AtomicUsize::new(0),
        fail_caps: false,
        libs: Mutex::new(map),
    };
    let mut unpaired_classic = host_desc("stranger", "Stranger");
    unpaired_classic.paired = false;
    unpaired_classic.is_native = false;
    unpaired_classic.classic_apps = Some(nightjar_policy::console::ClassicApps {
        version: 1,
        apps: vec![nightjar_policy::console::ClassicApp {
            id: 1,
            name: "Desktop".into(),
            uuid: String::new(),
            art_version: String::new(),
        }],
    });
    let hosts = [host_desc("pc1", "Desk"), unpaired_classic];
    let snap1 = repo.refresh(&hosts, &transport);
    assert_eq!(snap1.titles.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), vec!["Beta"]);
    assert_eq!(
        snap1.hidden_titles.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec!["Alpha"]
    );
    assert_eq!(
        repo.hidden_store().of_app_ids("pc1").into_iter().collect::<Vec<_>>(),
        vec![7]
    );
    assert_eq!(snap1.host_states.len(), 1);
    assert!(snap1.host_states[0].capabilities_fetched);

    let snap2 = repo.refresh(&hosts, &transport);
    assert!(!snap2.host_states[0].capabilities_fetched);

    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&repo_dir);
}

#[test]
fn console_stores_refuse_corrupt_overwrite_and_clear_guard_on_good_read() {
    let dir = temp_dir("console-guard");
    let recents = RecentsStore::new(&dir);
    let favs = FavouritesStore::new(&dir);
    let hidden = HiddenStore::new(&dir);

    std::fs::create_dir_all(dir.join("console")).unwrap();
    std::fs::write(dir.join("console").join("console_recents.json"), b"{bad").unwrap();
    assert!(recents.list().is_empty());
    assert!(recents
        .record(Recent {
            host: "h1".into(),
            install_id: "g1".into(),
            app_id: 1,
            title: "One".into(),
            at: 1,
        })
        .is_err());
    std::fs::write(dir.join("console").join("console_recents.json"), b"[]").unwrap();
    assert!(recents.list().is_empty());
    assert!(recents
        .record(Recent {
            host: "h1".into(),
            install_id: "g1".into(),
            app_id: 1,
            title: "One".into(),
            at: 1,
        })
        .is_ok());

    // FavouritesStore recovers corrupt JSON into .bak, and guards against non-NotFound I/O errors.
    let fav_path = dir.join("console").join("console_favourites.json");
    std::fs::create_dir_all(&fav_path).unwrap();
    assert!(favs.list().is_empty());
    assert!(favs
        .toggle(Favourite {
            host: "h1".into(),
            install_id: "g1".into(),
            app_id: 1,
            title: "One".into(),
        })
        .is_err());
    std::fs::remove_dir(&fav_path).unwrap();
    assert!(favs.list().is_empty());
    assert!(favs
        .toggle(Favourite {
            host: "h1".into(),
            install_id: "g1".into(),
            app_id: 1,
            title: "One".into(),
        })
        .is_ok());

    std::fs::write(dir.join("console").join("hidden_apps.json"), b"{bad").unwrap();
    assert!(hidden.of_app_ids("h1").is_empty());
    assert!(hidden
        .set_hidden(
            vec![Favourite {
                host: "h1".into(),
                install_id: "g1".into(),
                app_id: 1,
                title: "One".into(),
            }],
            true
        )
        .is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn repository_caps_refresh_after_ttl_and_forget_host_beats_racing_refresh() {
    use std::sync::{Arc, Barrier};

    let dir = temp_dir("repo-race-ttl");
    let repo = Arc::new(ConsoleRepository::new(&dir));
    let mut map = BTreeMap::new();
    map.insert(
        "pc-race".into(),
        NativeLibrary::from(Library {
            revision: "r1".into(),
            games: vec![sample_game("g1", 1, "Game", None)],
        }),
    );

    struct RacingTransport {
        entered: Arc<Barrier>,
        release: Arc<Barrier>,
        caps_calls: AtomicUsize,
        libs: BTreeMap<String, NativeLibrary>,
    }
    impl NativeTransport for RacingTransport {
        fn capabilities(&self, _: &HostDescriptor) -> Result<NativeCapabilities, String> {
            self.caps_calls.fetch_add(1, Ordering::SeqCst);
            Ok(NativeCapabilities::parse_lenient(&serde_json::json!({})).unwrap())
        }
        fn library(&self, h: &HostDescriptor, _: bool) -> Result<NativeLibrary, String> {
            self.entered.wait();
            self.release.wait();
            self.libs.get(&h.uuid).cloned().ok_or_else(|| "missing".into())
        }
        fn session(&self, _: &HostDescriptor) -> Result<Session, String> {
            Err("none".into())
        }
        fn add_to_library(&self, _: &HostDescriptor, _: &str) -> Result<Option<AddedGame>, String> {
            Ok(None)
        }
    }

    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let transport = Arc::new(RacingTransport {
        entered: Arc::clone(&entered),
        release: Arc::clone(&release),
        caps_calls: AtomicUsize::new(0),
        libs: map,
    });

    // Normal refresh caches capabilities, TTL expiry re-probes capabilities cleanly.
    let no_wait_transport = MockTransport {
        caps_calls: AtomicUsize::new(0),
        fail_caps: false,
        libs: Mutex::new({
            let mut m = BTreeMap::new();
            m.insert(
                "pc-ttl".into(),
                NativeLibrary::from(Library {
                    revision: "r1".into(),
                    games: vec![sample_game("g2", 2, "Two", None)],
                }),
            );
            m
        }),
    };
    let ttl_hosts = [host_desc("pc-ttl", "TTL")];
    assert!(repo.refresh(&ttl_hosts, &no_wait_transport).host_states[0].capabilities_fetched);
    assert!(!repo.refresh(&ttl_hosts, &no_wait_transport).host_states[0].capabilities_fetched);
    repo.expire_capabilities_for_test("pc-ttl");
    assert!(repo.refresh(&ttl_hosts, &no_wait_transport).host_states[0].capabilities_fetched);
    assert_eq!(no_wait_transport.caps_calls.load(Ordering::SeqCst), 2);

    // Racing forget_host while refresh is in-flight prevents stale disk or memory re-save.
    let race_hosts = vec![host_desc("pc-race", "Race")];
    let repo_bg = Arc::clone(&repo);
    let tr_bg = Arc::clone(&transport);
    let t = std::thread::spawn(move || repo_bg.refresh(&race_hosts, &*tr_bg));
    entered.wait();
    repo.forget_host("pc-race");
    release.wait();
    let _ = t.join().unwrap();

    assert!(!repo.library_store().has_host("pc-race"));
    let _ = std::fs::remove_dir_all(&dir);
}
