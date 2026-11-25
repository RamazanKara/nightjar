//! Disk persistence stores for the console: per-host native library cache (`console-library/`),
//! recent launches (`console_recents.json`), favourites with damaged-file `.bak` preservation
//! (`console_favourites.json`), and hidden apps (`hidden_apps.json`).

use crate::hosts::io::{GuardedJsonFile, write_atomic};
use nightjar_policy::console::{
    parse_favourites, set_hidden_strict, toggle_favourite, Favourite, Recent, ANDROID_RECENT_LIMIT,
};
use nightjar_policy::http::NativeLibrary;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Sanitizes a host UUID into a safe single filename stem, matching Android `FileLibraryCache`.
pub fn sanitize_host_id(uuid: &str) -> String {
    let out: String = uuid
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out == "." || out == ".." {
        "_".to_owned()
    } else {
        out
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    write_atomic(path, bytes, false).map_err(|e| e.to_string())
}

fn console_file(base_dir: &Path, name: &str) -> GuardedJsonFile {
    GuardedJsonFile::new(
        base_dir.join("console").join(name),
        Arc::new(Mutex::new(())),
        false,
    )
}

/// Per-host cached native library files (`<base>/console-library/<sanitized_uuid>.json`).
pub struct LibraryCacheStore {
    dir: PathBuf,
    lock: Mutex<()>,
}

impl LibraryCacheStore {
    pub fn new(base_dir: &Path) -> Self {
        let dir = if base_dir.file_name() == Some(std::ffi::OsStr::new("console-library")) {
            base_dir.to_path_buf()
        } else {
            base_dir.join("console-library")
        };
        Self {
            dir,
            lock: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_for(&self, host_id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", sanitize_host_id(host_id)))
    }

    pub fn load(&self, host_id: &str) -> Option<NativeLibrary> {
        self.load_entry(host_id).map(|(lib, _, _)| lib)
    }

    pub fn load_entry(&self, host_id: &str) -> Option<(NativeLibrary, u64, Vec<String>)> {
        let _guard = self.lock.lock().ok()?;
        let text = fs::read_to_string(self.path_for(host_id)).ok()?;
        parse_cached_entry(&text)
    }

    pub fn load_for_hosts(&self, host_ids: &[String]) -> BTreeMap<String, NativeLibrary> {
        let mut out = BTreeMap::new();
        for id in host_ids {
            if let Some(lib) = self.load(id) {
                out.insert(id.clone(), lib);
            }
        }
        out
    }

    pub fn save(&self, host_id: &str, lib: &NativeLibrary) -> Result<(), String> {
        self.save_entry(host_id, lib, 0, &[])
    }

    pub fn save_entry(
        &self,
        host_id: &str,
        lib: &NativeLibrary,
        fetched_ms: u64,
        features: &[String],
    ) -> Result<(), String> {
        if host_id.is_empty() {
            return Ok(());
        }
        let _guard = self.lock.lock().map_err(|_| "lock poisoned".to_owned())?;
        let envelope = serde_json::json!({
            "fetched_ms": fetched_ms,
            "library": lib,
            "features": features,
        });
        let json = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        atomic_write(&self.path_for(host_id), &json)
    }

    pub fn forget(&self, host_id: &str) {
        if host_id.is_empty() {
            return;
        }
        let _guard = self.lock.lock();
        let _ = fs::remove_file(self.path_for(host_id));
    }

    pub fn has_host(&self, host_id: &str) -> bool {
        if host_id.is_empty() {
            return false;
        }
        let _guard = self.lock.lock();
        self.path_for(host_id).is_file()
    }

    pub fn has_any(&self) -> bool {
        let _guard = self.lock.lock();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let path = entry.path();
            path.extension().and_then(|e| e.to_str()) == Some("json")
                && fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| parse_cached_entry(&text))
                    .is_some_and(|(lib, _, _)| !lib.library.games.is_empty())
        })
    }
}

fn ensure_library_revision(val: &mut serde_json::Value) {
    if let Some(obj) = val.as_object_mut()
        && !obj.contains_key("revision")
    {
        obj.insert("revision".to_owned(), serde_json::Value::String(String::new()));
    }
}

fn parse_cached_entry(text: &str) -> Option<(NativeLibrary, u64, Vec<String>)> {
    let mut val: serde_json::Value = serde_json::from_str(text).ok()?;
    if let Some(inner) = val.get_mut("library") {
        ensure_library_revision(inner);
        let lib = serde_json::from_value::<NativeLibrary>(inner.clone()).ok()?;
        let fetched_ms = val.get("fetched_ms").and_then(|v| v.as_u64()).unwrap_or(0);
        let features = val
            .get("features")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        Some((lib, fetched_ms, features))
    } else {
        ensure_library_revision(&mut val);
        let lib = serde_json::from_value::<NativeLibrary>(val).ok()?;
        Some((lib, 0, Vec::new()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoredRecent {
    #[serde(flatten)]
    pub recent: Recent,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host_name: String,
}

/// Recent game launches (`<base>/console/console_recents.json`), capped at [`ANDROID_RECENT_LIMIT`].
pub struct RecentsStore {
    file: GuardedJsonFile,
}

impl RecentsStore {
    pub fn new(base_dir: &Path) -> Self {
        Self { file: console_file(base_dir, "console_recents.json") }
    }

    fn read_stored_locked(&self) -> Vec<StoredRecent> {
        let Some(text) = self.file.read_text() else {
            return Vec::new();
        };
        let entries = match serde_json::from_str::<Vec<serde_json::Value>>(&text) {
            Ok(entries) => entries,
            Err(e) => {
                self.file.mark_unreadable(&e);
                return Vec::new();
            }
        };
        self.file.mark_readable();
        entries
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .take(ANDROID_RECENT_LIMIT)
            .collect()
    }

    fn write_stored_locked(&self, list: &[StoredRecent]) -> Result<(), String> {
        let json = serde_json::to_vec(list).map_err(|e| e.to_string())?;
        self.file.write_bytes(&json).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<Recent> {
        let _guard = self.file.lock();
        self.read_stored_locked()
            .into_iter()
            .map(|s| s.recent)
            .collect()
    }

    pub fn list_stored(&self) -> Vec<StoredRecent> {
        let _guard = self.file.lock();
        self.read_stored_locked()
    }

    pub fn record(&self, entry: Recent) -> Result<(), String> {
        self.record_with_host_name(entry, "")
    }

    pub fn record_with_host_name(&self, entry: Recent, host_name: &str) -> Result<(), String> {
        let _guard = self.file.try_lock()?;
        let mut stored = self.read_stored_locked();
        stored.retain(|r| !r.recent.same_game(&entry));
        stored.insert(
            0,
            StoredRecent {
                recent: entry,
                host_name: host_name.to_owned(),
            },
        );
        stored.truncate(ANDROID_RECENT_LIMIT);
        self.write_stored_locked(&stored)
    }

    pub fn forget_host(&self, host_id: &str) -> Result<(), String> {
        if host_id.is_empty() {
            return Ok(());
        }
        let _guard = self.file.try_lock()?;
        let mut list = self.read_stored_locked();
        let before = list.len();
        list.retain(|r| r.recent.host != host_id);
        if list.len() != before {
            self.write_stored_locked(&list)?;
        }
        Ok(())
    }

    /// One-shot migration helper for legacy JSON (`SharedPreferences "console_recents"`).
    /// Only writes if `console_recents.json` does not already exist on disk.
    pub fn import_legacy_json(&self, json: &str) -> Result<bool, String> {
        let _guard = self.file.try_lock()?;
        if self.file.path().exists() {
            return Ok(false);
        }
        let Ok(raw) = serde_json::from_str::<Vec<serde_json::Value>>(json) else {
            return Ok(false);
        };
        let mut list: Vec<StoredRecent> = raw
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        list.truncate(ANDROID_RECENT_LIMIT);
        if list.is_empty() {
            return Ok(false);
        }
        self.write_stored_locked(&list)?;
        Ok(true)
    }
}

/// Starred games (`<base>/console/console_favourites.json`), preserving `.bak` on damaged reads.
pub struct FavouritesStore {
    file: GuardedJsonFile,
}

impl FavouritesStore {
    pub fn new(base_dir: &Path) -> Self {
        Self { file: console_file(base_dir, "console_favourites.json") }
    }

    fn backup_path(&self) -> PathBuf {
        self.file.path().with_extension("json.bak")
    }

    fn read_locked(&self) -> Vec<Favourite> {
        read_favourites_guarded(&self.file, Some(&self.backup_path()))
    }

    fn write_locked(&self, list: &[Favourite]) -> Result<(), String> {
        let json = serde_json::to_vec(list).map_err(|e| e.to_string())?;
        self.file.write_bytes(&json).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<Favourite> {
        let _guard = self.file.lock();
        self.read_locked()
    }

    pub fn toggle(&self, entry: Favourite) -> Result<bool, String> {
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let starred = toggle_favourite(&mut list, entry);
        self.write_locked(&list)?;
        Ok(starred)
    }

    pub fn set_explicit(&self, entry: Favourite, starred: bool) -> Result<(), String> {
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let mut changed = false;
        if starred {
            if let Some(pos) = list.iter().position(|f| f.same_game(&entry)) {
                if list[pos] != entry {
                    list[pos] = entry.clone();
                    changed = true;
                }
                let mut idx = pos + 1;
                while idx < list.len() {
                    if list[idx].same_game(&entry) {
                        list.remove(idx);
                        changed = true;
                    } else {
                        idx += 1;
                    }
                }
            } else {
                list.insert(0, entry);
                changed = true;
            }
        } else {
            let before = list.len();
            list.retain(|f| !f.same_game(&entry));
            changed = list.len() != before;
        }
        if changed {
            self.write_locked(&list)?;
        }
        Ok(())
    }

    pub fn is_favourite(&self, host: &str, install_id: &str, app_id: u32) -> bool {
        let probe = Favourite {
            host: host.to_owned(),
            install_id: install_id.to_owned(),
            app_id,
            title: String::new(),
        };
        let _guard = self.file.lock();
        self.read_locked().iter().any(|f| f.same_game(&probe))
    }

    pub fn replace(&self, list: Vec<Favourite>) -> Result<(), String> {
        let _guard = self.file.try_lock()?;
        self.write_locked(&list)
    }

    pub fn forget_host(&self, host_id: &str) -> Result<(), String> {
        if host_id.is_empty() {
            return Ok(());
        }
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let before = list.len();
        list.retain(|f| f.host != host_id);
        if list.len() != before {
            self.write_locked(&list)?;
        }
        Ok(())
    }

    pub fn import_legacy_json(&self, json: &str) -> Result<bool, String> {
        let _guard = self.file.try_lock()?;
        if self.file.path().exists() {
            return Ok(false);
        }
        let (list, damaged) = parse_favourites(json);
        if damaged && !json.trim().is_empty() {
            let _ = fs::create_dir_all(self.file.path().parent().unwrap_or_else(|| Path::new(".")));
            let _ = fs::write(self.backup_path(), json.as_bytes());
        }
        if list.is_empty() {
            return Ok(false);
        }
        self.write_locked(&list)?;
        Ok(true)
    }
}

fn read_favourites_guarded(file: &GuardedJsonFile, bak_path: Option<&Path>) -> Vec<Favourite> {
    let Some(text) = file.read_text() else {
        return Vec::new();
    };
    let (list, damaged) = parse_favourites(&text);
    let mut bak_ok = false;
    if let Some(bak) = bak_path
        && damaged
        && !text.trim().is_empty()
    {
        if bak.exists() {
            bak_ok = true;
        } else {
            bak_ok = fs::write(bak, text.as_bytes()).is_ok();
        }
    }
    if let Err(e) = serde_json::from_str::<serde_json::Value>(&text)
        && !bak_ok
    {
        file.mark_unreadable(&e);
        return Vec::new();
    }
    file.mark_readable();
    list
}

/// Hidden games store (`<base>/console/hidden_apps.json`), mutex-serialized for concurrent updates.
pub struct HiddenStore {
    file: GuardedJsonFile,
}

impl HiddenStore {
    pub fn new(base_dir: &Path) -> Self {
        Self { file: console_file(base_dir, "hidden_apps.json") }
    }

    fn read_locked(&self) -> Vec<Favourite> {
        read_favourites_guarded(&self.file, None)
    }

    fn write_locked(&self, list: &[Favourite]) -> Result<(), String> {
        let json = serde_json::to_vec(list).map_err(|e| e.to_string())?;
        self.file.write_bytes(&json).map_err(|e| e.to_string())
    }

    pub fn list(&self) -> Vec<Favourite> {
        let _guard = self.file.lock();
        self.read_locked()
    }

    pub fn is_hidden(&self, host_id: &str, install_id: &str, app_id: u32) -> bool {
        let _guard = self.file.lock();
        let list = self.read_locked();
        nightjar_policy::console::is_hidden_entry(&list, host_id, install_id, app_id)
    }

    pub fn set_hidden(&self, games: Vec<Favourite>, hidden: bool) -> Result<bool, String> {
        if games.is_empty() {
            return Ok(false);
        }
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let changed = set_hidden_strict(&mut list, games, hidden);
        if changed {
            self.write_locked(&list)?;
        }
        Ok(changed)
    }

    pub fn forget_host(&self, host_id: &str) -> Result<(), String> {
        if host_id.is_empty() {
            return Ok(());
        }
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let before = list.len();
        list.retain(|h| h.host != host_id);
        if list.len() != before {
            self.write_locked(&list)?;
        }
        Ok(())
    }

    pub fn import_legacy_entries(&self, entries: Vec<Favourite>) -> Result<bool, String> {
        if entries.is_empty() {
            return Ok(false);
        }
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let before = list.len();
        for entry in entries {
            if entry.host.is_empty() || (entry.app_id == 0 && entry.install_id.is_empty()) {
                continue;
            }
            let exists = list.iter().any(|existing| existing.same_game_or_bare(&entry));
            if !exists {
                list.push(entry);
            }
        }
        if list.len() == before {
            return Ok(false);
        }
        self.write_locked(&list)?;
        Ok(true)
    }

    pub fn of_app_ids(&self, host_id: &str) -> BTreeSet<u32> {
        let _guard = self.file.lock();
        self.read_locked()
            .into_iter()
            .filter(|f| f.host == host_id)
            .map(|f| f.app_id)
            .collect()
    }

    pub fn stable_ids(&self, host_id: &str) -> BTreeMap<String, u32> {
        let _guard = self.file.lock();
        let mut map = BTreeMap::new();
        for f in self.read_locked() {
            if f.host == host_id && !f.install_id.is_empty() {
                map.insert(f.install_id, f.app_id);
            }
        }
        map
    }

    pub fn set_app(
        &self,
        host_id: &str,
        app_id: u32,
        stable_id: &str,
        hidden: bool,
    ) -> Result<(), String> {
        if host_id.is_empty() || (app_id == 0 && stable_id.is_empty()) {
            return Ok(());
        }
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let snapshot = list.clone();
        if hidden {
            if !stable_id.is_empty() {
                list.retain(|f| {
                    !(f.host == host_id
                        && f.install_id.is_empty()
                        && app_id != 0
                        && f.app_id == app_id)
                });
                let mut found: Option<usize> = None;
                let mut idx = 0;
                while idx < list.len() {
                    if list[idx].host == host_id && list[idx].install_id.eq_ignore_ascii_case(stable_id)
                    {
                        if let Some(first_idx) = found {
                            list.remove(idx);
                            if app_id != 0 && list[first_idx].app_id == 0 {
                                list[first_idx].app_id = app_id;
                            }
                            continue;
                        }
                        found = Some(idx);
                        if app_id != 0 && list[idx].app_id != app_id {
                            list[idx].app_id = app_id;
                        }
                        if list[idx].install_id != stable_id {
                            list[idx].install_id = stable_id.to_owned();
                        }
                    }
                    idx += 1;
                }
                if found.is_none() {
                    list.push(Favourite {
                        host: host_id.to_owned(),
                        install_id: stable_id.to_owned(),
                        app_id,
                        title: String::new(),
                    });
                }
            } else {
                let already = list.iter().any(|f| {
                    f.host == host_id && f.install_id.is_empty() && f.app_id == app_id
                });
                if !already {
                    list.push(Favourite {
                        host: host_id.to_owned(),
                        install_id: String::new(),
                        app_id,
                        title: String::new(),
                    });
                }
            }
        } else if !stable_id.is_empty() {
            list.retain(|f| {
                !(f.host == host_id
                    && (f.install_id.eq_ignore_ascii_case(stable_id)
                        || (f.install_id.is_empty() && app_id != 0 && f.app_id == app_id)))
            });
        } else {
            list.retain(|f| {
                !(f.host == host_id && f.install_id.is_empty() && f.app_id == app_id)
            });
        }
        if list != snapshot {
            self.write_locked(&list)?;
        }
        Ok(())
    }

    pub fn following(
        &self,
        host_id: &str,
        games: &BTreeMap<String, u32>,
    ) -> Result<BTreeSet<u32>, String> {
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let mut changed = false;

        let lookup = |stable_id: &str| -> Option<u32> {
            games.get(stable_id).copied().or_else(|| {
                games
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case(stable_id))
                    .map(|(_, &v)| v)
            })
        };

        for entry in &mut list {
            if entry.host != host_id || entry.install_id.is_empty() {
                continue;
            }
            if let Some(now) = lookup(&entry.install_id)
                && now != 0
                && entry.app_id != now
            {
                entry.app_id = now;
                changed = true;
            }
        }

        for (stable_id, &app_id) in games {
            if stable_id.is_empty() || app_id == 0 {
                continue;
            }
            let already_known = list.iter().any(|f| {
                f.host == host_id && f.install_id.eq_ignore_ascii_case(stable_id)
            });
            if !already_known
                && let Some(bare) = list
                    .iter_mut()
                    .find(|f| f.host == host_id && f.install_id.is_empty() && f.app_id == app_id)
            {
                bare.install_id = stable_id.clone();
                changed = true;
            }
        }

        if changed {
            self.write_locked(&list)?;
        }

        let hidden_ids = list
            .iter()
            .filter(|f| f.host == host_id && f.app_id > 0)
            .map(|f| f.app_id)
            .collect();
        Ok(hidden_ids)
    }

    pub fn changed(
        &self,
        host_id: &str,
        shown: &[u32],
        hidden: &[u32],
    ) -> Result<BTreeSet<u32>, String> {
        let _guard = self.file.try_lock()?;
        let mut list = self.read_locked();
        let before = list.len();
        list.retain(|f| !(f.host == host_id && shown.contains(&f.app_id)));
        let mut changed = list.len() != before;
        for &app_id in hidden {
            if app_id == 0 {
                continue;
            }
            if !list.iter().any(|f| f.host == host_id && f.app_id == app_id) {
                list.push(Favourite {
                    host: host_id.to_owned(),
                    install_id: String::new(),
                    app_id,
                    title: String::new(),
                });
                changed = true;
            }
        }
        if changed {
            self.write_locked(&list)?;
        }
        Ok(list
            .into_iter()
            .filter(|f| f.host == host_id && f.app_id > 0)
            .map(|f| f.app_id)
            .collect())
    }
}

/// Removes all persisted console records (cached library, recents, favourites, hidden) for `host_id`.
pub fn forget_host(base_dir: &Path, host_id: &str) {
    if host_id.is_empty() {
        return;
    }
    LibraryCacheStore::new(base_dir).forget(host_id);
    let _ = RecentsStore::new(base_dir).forget_host(host_id);
    let _ = FavouritesStore::new(base_dir).forget_host(host_id);
    let _ = HiddenStore::new(base_dir).forget_host(host_id);
}

/// Whether `<base_dir>/console-library` has at least one cached non-empty native library.
pub fn has_any_library(base_dir: &Path) -> bool {
    LibraryCacheStore::new(base_dir).has_any()
}

/// Whether `<base_dir>/console-library/<sanitized>.json` exists for `host_id`.
pub fn has_library(base_dir: &Path, host_id: &str) -> bool {
    LibraryCacheStore::new(base_dir).has_host(host_id)
}

/// Whether `app_list_dir` has a cached classic app list XML/JSON file for `host_id` (not empty).
pub fn has_classic_app_list(app_list_dir: &Path, host_id: &str) -> bool {
    if host_id.is_empty() {
        return false;
    }
    let sanitized = sanitize_host_id(host_id);
    for ext in ["json", "xml"] {
        let p = app_list_dir.join(format!("{sanitized}.{ext}"));
        if let Ok(meta) = fs::metadata(&p)
            && meta.len() > 2
        {
            return true;
        }
    }
    let bare = app_list_dir.join(format!("applist_{sanitized}"));
    fs::metadata(bare).is_ok_and(|m| m.len() > 2)
}

/// Cover-art lookup result from a host's cached native library (`nightjar#130`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CachedLibraryCover {
    pub explicitly_no_cover: bool,
    pub app_id: u32,
    pub title: String,
    pub cover_version: String,
}

/// Looks up an added game in `<base_dir>/console-library/<host_id>.json` by `install_id`
/// (case-insensitive) or fallback `app_id` (`!= 0`), returning `None` if no cache or match exists.
pub fn find_cached_library_cover(
    base_dir: &Path,
    host_id: &str,
    app_id: u32,
    install_id: &str,
) -> Option<CachedLibraryCover> {
    if host_id.is_empty() {
        return None;
    }
    let lib = LibraryCacheStore::new(base_dir).load(host_id)?;
    let mut matched = None;
    for (idx, game) in lib.library.games.iter().enumerate() {
        if !lib.added_at(idx) {
            continue;
        }
        if !install_id.is_empty() && game.install_id.eq_ignore_ascii_case(install_id) {
            matched = Some(game);
            break;
        }
        if matched.is_none() && app_id != 0 && game.app_id == app_id {
            matched = Some(game);
        }
    }
    let game = matched?;
    let explicitly_no_cover = game.artwork.cover.is_none() && game.artwork.version.is_empty();
    Some(CachedLibraryCover {
        explicitly_no_cover,
        app_id: game.app_id,
        title: game.title.clone(),
        cover_version: game.artwork.version.clone(),
    })
}
