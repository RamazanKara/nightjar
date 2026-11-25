//! Unit tests for `nightjar_policy::console` covering merging, hidden items, recents,
//! favourites, Home filtering/sorting/grouping, and classic app matching.

use super::filters::project_home;
use super::*;
use crate::http::native_api::library::{Artwork, Game, Library, Store};
use crate::http::native_api::session::{Action, ActionId, Owner, Session, SessionApp, State};
use crate::http::GameExtra;
use std::collections::BTreeMap;

fn game(install_id: &str, app_id: u32, title: &str, game_id: Option<&str>, store: Option<(&str, &str)>, installed: Option<bool>, running: bool) -> Game {
    Game {
        install_id: install_id.into(),
        app_id,
        title: title.into(),
        game_id: game_id.map(str::to_owned),
        store: store.map(|(src, name)| Store {
            source: src.into(),
            id: "1".into(),
            name: name.into(),
        }),
        installed,
        running,
        artwork: Artwork {
            cover: Some(format!("/c/{install_id}")),
            version: "v1".into(),
        },
        last_played: None,
        playtime_minutes: None,
        collections: Vec::new(),
    }
}

fn source<'a>(id: u64, uid: &'a str, name: &'a str, online: bool, lib: &'a Library) -> Source<'a> {
    Source {
        id,
        unique_id: uid,
        name,
        online,
        paired: true,
        library: Some(lib),
        classic: false,
        last_played: None,
        playtime_minutes: None,
        collections: None,
        session: None,
        host_control: false,
    }
}

#[test]
fn merges_by_game_id_and_keeps_manual_apps_separate() {
    let lib_a = Library {
        revision: "r1".into(),
        games: vec![
            game("i1", 1, "Portal", Some("steam:400"), Some(("steam", "Steam")), Some(true), false),
            game("m1", 2, "Notepad", None, None, None, false),
        ],
    };
    let lib_b = Library {
        revision: "r2".into(),
        games: vec![
            game("i9", 9, "Portal", Some("steam:400"), Some(("steam", "Steam")), Some(false), false),
            game("m1", 2, "Notepad", None, None, None, false),
        ],
    };
    let s1 = source(1, "pc-a", "A", true, &lib_a);
    let s2 = source(2, "pc-b", "B", true, &lib_b);
    let merged = merge_library(&[s1, s2]);
    assert_eq!(merged.len(), 3);
    let portal = merged.iter().find(|e| e.title == "Portal").unwrap();
    assert_eq!(portal.copies.len(), 2);
    assert!(portal.installed());
    assert_eq!(portal.best().host_uid, "pc-a");
}

#[test]
fn merge_rich_preserves_unadded_and_extra_art() {
    let lib = Library {
        revision: "r1".into(),
        games: vec![game("", 0, "Celeste", Some("gog:c"), Some(("gog", "GOG")), Some(true), false)],
    };
    let extra = vec![GameExtra {
        added: false,
        launchable: Some(false),
        hero: Some("/hero".into()),
        logo: Some("/logo".into()),
        landscape: None,
    }];
    let rich = RichSource {
        source: source(1, "pc-a", "A", true, &lib),
        extra: Some(&extra),
    };
    let merged = merge_rich_library(&[rich]);
    assert_eq!(merged.len(), 1);
    assert!(!merged[0].added());
    assert_eq!(merged[0].best().launchable, Some(false));
    assert_eq!(merged[0].best().hero_path.as_deref(), Some("/hero"));
}

#[test]
fn hidden_split_and_tracker_and_reconcile_work_end_to_end() {
    let lib = Library {
        revision: "r1".into(),
        games: vec![
            game("uuid-a", 101, "Portal", Some("steam:400"), Some(("steam", "Steam")), Some(true), false),
            game("uuid-b", 202, "Hades", Some("epic:h"), Some(("epic", "Epic")), Some(true), false),
        ],
    };
    let mut src = source(1, "pc-1", "Living Room", true, &lib);
    let src_copy = src;
    let keys = hide_keys(&[src_copy], 1, 101);
    assert_eq!(keys.len(), 1);
    let mut tracker = HiddenTracker::following(Vec::new());
    assert!(tracker.changed(&keys));
    assert!(!tracker.changed(&keys));

    let merged = merge_library(&[src]);
    let (shown, hidden) = split_hidden(merged, &[src], &keys);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].title, "Hades");
    assert_eq!(hidden.len(), 1);
    assert_eq!(hidden[0].title, "Portal");

    src.paired = true;
    let stale_fav = Favourite {
        host: "pc-1".into(),
        install_id: "uuid-a".into(),
        app_id: 99,
        title: "Old Title".into(),
    };
    let repaired = reconcile_starred(&[stale_fav], &[src]).unwrap();
    assert_eq!(repaired[0].app_id, 101);
    assert_eq!(repaired[0].title, "Portal");
}

#[test]
fn ambiguous_install_id_hides_only_targeted_app_id() {
    let lib = Library {
        revision: "r1".into(),
        games: vec![
            game("dup-id", 101, "Alpha", Some("steam:1"), None, Some(true), false),
            game("dup-id", 202, "Beta", Some("steam:2"), None, Some(true), false),
            game("dup-manual", 303, "Manual A", None, None, Some(true), false),
            game("dup-manual", 404, "Manual B", None, None, Some(true), false),
        ],
    };
    let src = source(1, "pc-1", "Desk", true, &lib);
    let keys = hide_keys(&[src], 1, 101);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].install_id, "");
    assert_eq!(keys[0].app_id, 101);
    assert!(is_hidden(&src, "dup-id", 101, &keys));
    assert!(!is_hidden(&src, "dup-id", 202, &keys));

    let manual_keys = hide_keys(&[src], 1, 303);
    assert_eq!(manual_keys.len(), 1);
    assert_eq!(manual_keys[0].install_id, "");
    assert_eq!(manual_keys[0].app_id, 303);
    assert!(is_hidden(&src, "dup-manual", 303, &manual_keys));
    assert!(!is_hidden(&src, "dup-manual", 404, &manual_keys));

    let keys_by_id = hide_keys_by_id(&[src], "pc-1", 202);
    assert_eq!(keys_by_id.len(), 1);
    assert_eq!(keys_by_id[0].install_id, "");
    assert!(is_hidden(&src, "dup-id", 202, &keys_by_id));
    assert!(!is_hidden(&src, "dup-id", 101, &keys_by_id));
}

#[test]
fn home_projection_filters_searches_and_sorts() {
    let mut g1 = game("i1", 1, "Alpha", Some("steam:1"), Some(("steam", "Steam")), Some(true), false);
    g1.last_played = Some(100);
    let mut g2 = game("i2", 2, "Zeta", Some("epic:2"), Some(("epic", "Epic Games")), Some(false), false);
    g2.last_played = Some(500);
    let lib = Library {
        revision: "r1".into(),
        games: vec![g1, g2],
    };
    let mut times = BTreeMap::new();
    times.insert("i1".into(), 100);
    times.insert("i2".into(), 500);
    let mut src = source(1, "pc-1", "Desk", true, &lib);
    src.last_played = Some(&times);
    let merged = merge_library(&[src]);
    let proj = project_home(
        &merged,
        &[],
        &[src],
        &[],
        &LibraryFilter::All,
        "ze",
        Sort {
            by: HomeSort::LastPlayed,
            order: HomeOrder::Descending,
        },
    );
    assert_eq!(proj.cards.len(), 1);
    assert_eq!(proj.cards[0].title, "Zeta");
    assert!(proj.chips.iter().any(|c| c.key == "filter:installed"));
}

#[test]
fn detail_identifies_other_device_and_classic_app_matching() {
    let lib = Library {
        revision: "r1".into(),
        games: vec![game("u1", 10, "Portal", None, None, Some(true), false)],
    };
    let session = Session {
        revision: 1,
        state: State::Streaming,
        owner: Owner::OtherDevice,
        app: Some(SessionApp {
            install_id: "u1".into(),
            app_id: 10,
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
        owns_game: false,
        press_failed: false,
        actions: vec![Action {
            id: ActionId::CloseGame,
            allowed: true,
            reason: None,
        }],
    };
    let mut src = source(1, "pc-1", "Desk", true, &lib);
    src.session = Some(&session);
    src.host_control = true;
    let d = detail(&src, 10, true);
    assert_eq!(d.state, GameState::Running);
    assert!(d.held_by_other);
    assert_eq!(d.end, EndAction::CloseGame);
    assert!(!d.can_start);

    assert_eq!(classic_app(&[(77, "U1")], "u1", 999), Some(77));
    assert_eq!(classic_app(&[(77, "")], "", 77), Some(77));

    let mut cols: BTreeMap<String, Vec<String>> = BTreeMap::new();
    cols.insert("u1".into(), vec!["Co-op".into()]);
    assert_eq!(cols["u1"][0], "Co-op");
}

#[test]
fn library_memo_invalidates_when_source_metadata_or_session_changes() {
    let lib = Library {
        revision: "r1".into(),
        games: vec![game(
            "u1",
            10,
            "Portal",
            Some("steam:400"),
            Some(("steam", "Steam")),
            Some(true),
            false,
        )],
    };
    let memo = LibraryMemo::default();
    let src = source(1, "pc-1", "Desk", true, &lib);
    let first = memo.get(&[src], &[]);
    assert_eq!(memo.merges(), 1);
    assert_eq!(first.shown()[0].best().host_name, "Desk");
    assert!(!first.shown()[0].running());

    // Identical inputs hit the cache without re-merging.
    let _ = memo.get(&[src], &[]);
    assert_eq!(memo.merges(), 1);

    // Renaming the PC must invalidate the memo.
    let mut renamed = src;
    renamed.name = "Living Room";
    let merged_renamed = memo.get(&[renamed], &[]);
    assert_eq!(memo.merges(), 2);
    assert_eq!(merged_renamed.shown()[0].best().host_name, "Living Room");

    // Updating playtime or collections must invalidate the memo.
    let mut playtime = BTreeMap::new();
    playtime.insert("u1".into(), 120);
    let mut with_playtime = renamed;
    with_playtime.playtime_minutes = Some(&playtime);
    let merged_pt = memo.get(&[with_playtime], &[]);
    assert_eq!(memo.merges(), 3);
    assert_eq!(merged_pt.shown()[0].playtime_minutes(), Some(120));

    let mut cols = BTreeMap::new();
    cols.insert("u1".into(), vec!["Favorites".into()]);
    let mut with_cols = with_playtime;
    with_cols.collections = Some(&cols);
    let merged_cols = memo.get(&[with_cols], &[]);
    assert_eq!(memo.merges(), 4);
    assert_eq!(merged_cols.shown()[0].collections(), vec!["Favorites"]);

    // Starting a session on the PC must invalidate the memo even when Library is unchanged.
    let session = Session {
        revision: 1,
        state: State::Streaming,
        owner: Owner::OtherDevice,
        app: Some(SessionApp {
            install_id: "u1".into(),
            app_id: 10,
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
    };
    let mut with_session = with_cols;
    with_session.session = Some(&session);
    let merged_running = memo.get(&[with_session], &[]);
    assert_eq!(memo.merges(), 5);
    assert!(merged_running.shown()[0].running());
}
