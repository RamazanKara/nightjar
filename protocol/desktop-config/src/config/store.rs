//! Persistent JSON config directory store, atomic private writes, and recovery backups.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nightjar_policy::http::NativeLibrary;
use serde::{Deserialize, Serialize};

use super::{HostRecord, Identity, Profiles, ScanUntrusted, Settings};
use crate::notify_internet_enabled;
pub use crate::{downloads_dir, register_internet_hook};
pub use nightjar_policy::console::{
    CLASSIC_APPS_VERSION, ClassicApp, ClassicApps, Favourite, OpenGame, OpenPlayState, RECENT_LIMIT,
    Recent, parse_favourites, remember, set_hidden, toggle_favourite,
};

pub struct Store {
    dir: PathBuf,
    /// The second screen's process (`second_screen.rs`) reads the files and leaves them as the
    /// first screen's process writes them.
    read_only: bool,
    /// Files a read failed on for a reason other than "there is none": they are left as they are
    /// this run, so a default that stood in for their contents never replaces them.
    unreadable: std::sync::Mutex<std::collections::BTreeSet<PathBuf>>,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(value) => Some(value),
        Err(e) => {
            eprintln!("nightjar: ignoring {}: {e}", path.display());
            None
        }
    }
}

/// Retains only valid array elements when reading partially damaged list files (such as `recent.json`).
fn keep_readable_array<T: for<'de> Deserialize<'de>>(value: &mut serde_json::Value) {
    let Some(items) = value.as_array_mut() else {
        *value = serde_json::Value::Array(Vec::new());
        return;
    };
    items.retain(|item| serde_json::from_value::<T>(item.clone()).is_ok());
}

/// Removes from a settings object the values this version can't read (one a newer version wrote,
/// or a hand edit), so one of them doesn't cost the others. Anything but an object becomes empty.
fn keep_readable_settings(values: &mut serde_json::Value) {
    let Some(map) = values.as_object_mut() else {
        *values = serde_json::Value::Object(serde_json::Map::new());
        return;
    };
    map.retain(|key, value| {
        let one = serde_json::Map::from_iter([(key.clone(), value.clone())]);
        let readable = serde_json::from_value::<Settings>(serde_json::Value::Object(one)).is_ok();
        if !readable {
            eprintln!("nightjar: leaving out the setting {key}: this version can't read {value}");
        }
        readable
    });
}

/// profiles.json with each profile's unreadable settings left out.
fn keep_readable_profiles(profiles: &mut serde_json::Value) {
    let entries = profiles.get_mut("entries").and_then(serde_json::Value::as_array_mut);
    for settings in entries.into_iter().flatten().filter_map(|profile| profile.get_mut("settings")) {
        keep_readable_settings(settings);
    }
}

/// Removes unreadable setting values from a `serde_json::Value` object in place.
pub fn keep_readable_settings_value(values: &mut serde_json::Value) {
    keep_readable_settings(values);
}

/// hosts.json with each PC's and game's unreadable settings left out, then the PCs that still
/// can't be read: the others keep their pairing.
fn keep_readable_hosts(hosts: &mut serde_json::Value) {
    nightjar_policy::hosts::keep_readable_hosts(
        hosts,
        keep_readable_settings,
        |host| serde_json::from_value::<HostRecord>(host.clone()).is_ok(),
    );
}

pub(super) const HOTKEYS_FILE: &str = "stream_hotkeys.json";
/// stream_hotkeys.json's format: `{"version": 1, "bindings": {"quit": "CTRL+F9", "overlay": "off"}}`.
const HOTKEYS_VERSION: u64 = 1;

/// stream_hotkeys.json as read: the bindings this version reads, and the entries it doesn't (not a
/// string), kept for the next save.
struct HotkeysFile {
    version: u64,
    bindings: nightjar_policy::shortcuts::HotkeyBindings,
    kept: serde_json::Map<String, serde_json::Value>,
}

/// None for a missing file; an error for one that can't be read or isn't the format's shape.
fn read_hotkeys(path: &Path) -> Result<Option<HotkeysFile>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let version = value.get("version").and_then(serde_json::Value::as_u64).ok_or("no version")?;
    let entries = value.get("bindings").and_then(serde_json::Value::as_object).ok_or("no bindings")?;
    let mut file = HotkeysFile { version, bindings: Default::default(), kept: serde_json::Map::new() };
    for (id, entry) in entries {
        match entry.as_str() {
            Some(text) => {
                file.bindings.insert(id.clone(), text.to_string());
            }
            None => {
                file.kept.insert(id.clone(), entry.clone());
            }
        }
    }
    Ok(Some(file))
}

/// Copies `path` to `<path>.<suffix>` (`.<suffix>.2` and on beside earlier, different copies),
/// unless a copy already holds the same bytes.
fn back_up(path: &Path, suffix: &str) -> std::io::Result<()> {
    let bytes = std::fs::read(path)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    for n in 1.. {
        let copy = path.with_file_name(if n == 1 {
            format!("{name}.{suffix}")
        } else {
            format!("{name}.{suffix}.{n}")
        });
        match std::fs::read(&copy) {
            Ok(earlier) if earlier == bytes => return Ok(()),
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                write_private(&copy, &bytes)?;
                eprintln!("nightjar: copied {} to {} before replacing it", path.display(), copy.display());
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("a free name")
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(std::io::Error::other)?;
    write_private(path, text.as_bytes())
}

/// Writes `bytes` to `path` whole or not at all: a temporary file of its own name, synced to disk,
/// then renamed over `path`, so neither a crash nor a second writer leaves half a file. On Unix only
/// the user can read it (0600): the identity holds the client's private key, the PIN lock its hash.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("no file name"))?
        .to_string_lossy();
    let temporary = path.with_file_name(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        // POSIX (both Linux and macOS): create private config files with 0600 permissions.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary)?;
        // POSIX (both Linux and macOS): mode above is masked by umask, so enforce 0600 explicitly.
        #[cfg(unix)]
        file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written
}

/// The platform config directory, or `NIGHTJAR_CONFIG_DIR`. Not created here.
pub fn config_dir() -> std::io::Result<PathBuf> {
    match std::env::var_os("NIGHTJAR_CONFIG_DIR") {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => directories::ProjectDirs::from("", "", "Nightjar")
            .map(|d| d.config_dir().to_path_buf())
            .ok_or_else(|| std::io::Error::other("no config directory")),
    }
}

impl Store {
    /// The platform config directory, or `NIGHTJAR_CONFIG_DIR` (used by tests and the
    /// end-to-end harness).
    pub fn open() -> std::io::Result<Self> {
        Self::at(config_dir()?)
    }

    pub fn at(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            read_only: false,
            unreadable: std::sync::Mutex::new(std::collections::BTreeSet::new()),
        })
    }

    /// The same files, read and never written or removed: every save does nothing.
    pub fn read_only(self) -> Self {
        Self { read_only: true, ..self }
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn write<T: Serialize>(&self, path: &Path, value: &T) -> std::io::Result<()> {
        if self.read_only {
            return Ok(());
        }
        if self.left_as_it_is(path) {
            return Err(std::io::Error::other(format!(
                "{} could not be read, so it is left as it is to keep what it holds",
                path.display()
            )));
        }
        write_json(path, value)
    }

    /// Records that `path` could not be read and must not be replaced.
    fn leave_as_it_is(&self, path: &Path) {
        self.unreadable
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(path.to_owned());
    }

    fn left_as_it_is(&self, path: &Path) -> bool {
        self.unreadable
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(path)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads `name`, or the default when there is none. A file this version can't read whole is
    /// first copied to `<name>.bad`, because the next save replaces it; then it is read again
    /// with what `keep_readable` left of it, so one unreadable value doesn't cost the rest.
    fn read_or_keep_readable<T: Default + for<'de> Deserialize<'de>>(
        &self,
        name: &str,
        keep_readable: fn(&mut serde_json::Value),
    ) -> T {
        let path = self.dir.join(name);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return T::default(),
            // Not an empty file: something is there that this run cannot read. Keep it as it is,
            // so a default written in its place does not cost what it holds.
            Err(e) => {
                eprintln!("nightjar: cannot read {} ({e}); it is left as it is", path.display());
                self.leave_as_it_is(&path);
                return T::default();
            }
        };
        let error = match serde_json::from_str(&text) {
            Ok(value) => return value,
            Err(error) => error,
        };
        eprintln!("nightjar: {} can't be read whole: {error}", path.display());
        if !self.read_only
            && let Err(e) = back_up(&path, "bad")
        {
            eprintln!("nightjar: could not keep a copy of {}: {e}", path.display());
        }
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return T::default();
        };
        keep_readable(&mut value);
        serde_json::from_value(value).unwrap_or_else(|e| {
            eprintln!("nightjar: ignoring {}: {e}", path.display());
            T::default()
        })
    }

    pub fn settings(&self) -> Settings {
        let mut settings: Settings = self.read_or_keep_readable("settings.json", keep_readable_settings);
        settings.migrate();
        notify_internet_enabled(settings.internet_play);
        settings
    }

    pub fn save_settings(&self, settings: &Settings) -> std::io::Result<()> {
        // Every change of the settings passes here: the tunnels follow Play over the internet.
        notify_internet_enabled(settings.internet_play);
        self.write(&self.dir.join("settings.json"), settings)
    }

    pub fn profiles(&self) -> Profiles {
        self.read_or_keep_readable("profiles.json", keep_readable_profiles)
    }

    pub fn save_profiles(&self, profiles: &Profiles) -> std::io::Result<()> {
        self.write(&self.dir.join("profiles.json"), profiles)
    }

    pub fn controller_mappings(&self) -> BTreeMap<String, crate::gamepad_types::Mapping> {
        let mut mappings: BTreeMap<String, crate::gamepad_types::Mapping> =
            self.read_or_keep_readable("controllers.json", |_| {});
        mappings.retain(|_, mapping| mapping.valid());
        mappings
    }

    pub fn shortcuts(&self) -> crate::shortcuts::Shortcuts {
        use std::io::Read as _;
        let path = self.dir.join("shortcuts.json");
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return crate::shortcuts::Shortcuts::default();
            }
            Err(e) => {
                eprintln!(
                    "nightjar: cannot read {} ({e}); it is left as it is",
                    path.display()
                );
                self.leave_as_it_is(&path);
                return crate::shortcuts::Shortcuts::default();
            }
        };
        let mut text = String::new();
        if let Err(e) = file
            .take((crate::shortcuts::MAX_BYTES + 1) as u64)
            .read_to_string(&mut text)
        {
            eprintln!(
                "nightjar: cannot read {} ({e}); it is left as it is",
                path.display()
            );
            self.leave_as_it_is(&path);
            return crate::shortcuts::Shortcuts::default();
        }
        match crate::shortcuts::parse(&text) {
            Ok(shortcuts) => shortcuts,
            Err(e) => {
                eprintln!("nightjar: {} can't be read whole: {e}", path.display());
                if !self.read_only
                    && let Err(copy_err) = back_up(&path, "bad")
                {
                    eprintln!(
                        "nightjar: could not keep a copy of {}: {copy_err}",
                        path.display()
                    );
                }
                crate::shortcuts::Shortcuts::default()
            }
        }
    }

    pub fn window_placement(&self) -> Option<crate::window_place_types::Placement> {
        self.read_or_keep_readable("window.json", |_| {})
    }

    pub fn save_window_placement(
        &self,
        placement: &crate::window_place_types::Placement,
    ) -> std::io::Result<()> {
        self.write(&self.dir.join("window.json"), placement)
    }

    pub fn save_shortcuts(&self, shortcuts: &crate::shortcuts::Shortcuts) -> std::io::Result<()> {
        self.write(&self.dir.join("shortcuts.json"), shortcuts)
    }

    /// The user's changes to the stream hotkeys, by action ID (nightjar-policy's `HotkeyBindings`),
    /// read entry by entry: an entry this version can't read is logged and kept in the file. A
    /// missing file, or one that doesn't read at all (logged), leaves every hotkey at its default.
    pub fn hotkeys(&self) -> nightjar_policy::shortcuts::HotkeyBindings {
        let path = self.dir.join(HOTKEYS_FILE);
        match read_hotkeys(&path) {
            Ok(Some(file)) => {
                if file.version > HOTKEYS_VERSION {
                    eprintln!(
                        "nightjar: {} is from a newer Nightjar (version {}); reading the entries this one knows",
                        path.display(),
                        file.version
                    );
                }
                for id in file.kept.keys() {
                    eprintln!(
                        "nightjar: keeping {id} in {} unread: not a chord this version reads",
                        path.display()
                    );
                }
                file.bindings
            }
            Ok(None) => Default::default(),
            Err(e) => {
                eprintln!(
                    "nightjar: cannot read {}: {e}; the stream hotkeys keep their defaults, and the file is backed up before a change replaces it",
                    path.display()
                );
                Default::default()
            }
        }
    }

    /// Saves the stream hotkeys over stream_hotkeys.json, keeping the entries it holds that this
    /// version doesn't read and the newer version that wrote it. A file that doesn't read, or one a
    /// newer Nightjar wrote, is copied beside it first (`.unreadable`, `.v<version>`), never lost.
    pub fn save_hotkeys(
        &self,
        bindings: &nightjar_policy::shortcuts::HotkeyBindings,
    ) -> std::io::Result<()> {
        let path = self.dir.join(HOTKEYS_FILE);
        let (version, mut entries) = match read_hotkeys(&path) {
            Ok(Some(file)) => {
                if file.version > HOTKEYS_VERSION && !self.read_only {
                    back_up(&path, &format!("v{}", file.version))?;
                }
                (file.version.max(HOTKEYS_VERSION), file.kept)
            }
            Ok(None) => (HOTKEYS_VERSION, serde_json::Map::new()),
            Err(_) if self.read_only => (HOTKEYS_VERSION, serde_json::Map::new()),
            Err(_) => {
                back_up(&path, "unreadable")?;
                (HOTKEYS_VERSION, serde_json::Map::new())
            }
        };
        for (id, text) in bindings {
            entries.insert(id.clone(), serde_json::Value::String(text.clone()));
        }
        self.write(&path, &serde_json::json!({"version": version, "bindings": entries}))
    }

    pub fn save_controller_mappings(
        &self,
        mappings: &BTreeMap<String, crate::gamepad_types::Mapping>,
    ) -> std::io::Result<()> {
        self.write(&self.dir.join("controllers.json"), mappings)
    }

    /// The saved identity: `Ok(None)` only when there is no `identity.json`. A file that cannot be
    /// read is backed up when it could be (`.bad`) and left as it is, never replaced by a new
    /// identity, whose save would overwrite the pairing key the user may still recover.
    pub fn identity(&self) -> Result<Option<Identity>, String> {
        let path = self.dir.join("identity.json");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                self.leave_as_it_is(&path);
                return Err(format!("{} cannot be read: {e}", path.display()));
            }
        };
        match serde_json::from_str(&text) {
            Ok(identity) => Ok(Some(identity)),
            Err(e) => {
                if !self.read_only
                    && let Err(copy) = back_up(&path, "bad")
                {
                    eprintln!("nightjar: could not keep a copy of {}: {copy}", path.display());
                }
                self.leave_as_it_is(&path);
                Err(format!("{} cannot be read: {e}", path.display()))
            }
        }
    }

    pub fn save_identity(&self, identity: &Identity) -> std::io::Result<()> {
        self.write(&self.dir.join("identity.json"), identity)
    }

    pub fn hosts(&self) -> Vec<HostRecord> {
        self.read_or_keep_readable("hosts.json", keep_readable_hosts)
    }

    pub fn save_hosts(&self, hosts: &[HostRecord]) -> std::io::Result<()> {
        self.write(&self.dir.join("hosts.json"), &hosts)
    }

    pub fn recents(&self) -> Vec<Recent> {
        self.read_or_keep_readable("recent.json", keep_readable_array::<Recent>)
    }

    pub fn save_recents(&self, recents: &[Recent]) -> std::io::Result<()> {
        self.write(&self.dir.join("recent.json"), &recents)
    }

    pub fn open_play(&self) -> OpenPlayState {
        self.read_or_keep_readable("open_play.json", |_| {})
    }

    pub fn save_open_play(&self, state: &OpenPlayState) -> std::io::Result<()> {
        self.write(&self.dir.join("open_play.json"), state)
    }

    /// The displays an earlier run found not refreshing on present (`stream::scan_sync`).
    pub fn scan_untrusted(&self) -> ScanUntrusted {
        self.read_or_keep_readable("scan_untrusted.json", |_| {})
    }

    pub fn save_scan_untrusted(&self, untrusted: &ScanUntrusted) -> std::io::Result<()> {
        self.write(&self.dir.join("scan_untrusted.json"), untrusted)
    }

    /// Forgets one display of `scan_untrusted.json`, or every display with `None`: the user chose
    /// Nightjar's wait, which does not take the display check's switch to Windows V-Sync
    /// (`stream::scan_sync`).
    pub fn clear_scan_untrusted(&self, display: Option<&str>) -> std::io::Result<()> {
        let kept = self.scan_untrusted();
        if kept.displays.is_empty() {
            return Ok(());
        }
        let displays = kept
            .displays
            .into_iter()
            .filter(|each| display.is_some_and(|one| one != each))
            .collect();
        self.save_scan_untrusted(&ScanUntrusted { displays })
    }

    fn read_favourites_file(&self, name: &str) -> Vec<Favourite> {
        let path = self.dir.join(name);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(e) => {
                eprintln!("nightjar: cannot read {} ({e}); it is left as it is", path.display());
                self.leave_as_it_is(&path);
                return Vec::new();
            }
        };
        let (list, damaged) = parse_favourites(&text);
        if damaged {
            // Keep the list as it was, so a bad entry (or a file the next save would replace) is not lost.
            eprintln!("nightjar: {name} has entries that could not be read; kept as {name}.bak");
            if !self.read_only
                && let Err(e) = back_up(&path, "bak")
            {
                eprintln!("nightjar: could not keep a copy of {}: {e}", path.display());
            }
        }
        list
    }

    pub fn favourites(&self) -> Vec<Favourite> {
        self.read_favourites_file("favourites.json")
    }

    pub fn save_favourites(&self, favourites: &[Favourite]) -> std::io::Result<()> {
        self.write(&self.dir.join("favourites.json"), &favourites)
    }

    /// The games hidden on this device, like the favourites' file; no other device reads it.
    pub fn hidden_games(&self) -> Vec<Favourite> {
        self.read_favourites_file("hidden-games.json")
    }

    pub fn save_hidden_games(&self, games: &[Favourite]) -> std::io::Result<()> {
        self.write(&self.dir.join("hidden-games.json"), &games)
    }

    /// Box art cache: `art/<host>/<app>.img`, or `<app>-<version>.img` for a cover the host
    /// versions (the native API), so a changed cover is fetched again.
    pub fn art_path(&self, host: &str, app_id: i32, version: &str) -> PathBuf {
        let version: String = version.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let name = if version.is_empty() {
            format!("{app_id}.img")
        } else {
            format!("{app_id}-{version}.img")
        };
        self.dir.join("art").join(host_path_segment(host)).join(name)
    }

    /// The last native library a PC sent, for Home while the PC is away or not polled yet.
    pub fn library(&self, host: &str) -> Option<NativeLibrary> {
        let _ = host_key(host)?;
        read_json(&self.library_path(host))
    }

    pub fn save_library(&self, host: &str, library: &NativeLibrary) -> std::io::Result<()> {
        if self.read_only || host_key(host).is_none() {
            return Ok(());
        }
        let path = self.library_path(host);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.write(&path, library)
    }

    /// The last app list a PC without the native library sent, for Home while the PC is away or
    /// not polled yet (Android keeps the same per PC). None when nothing usable is saved.
    pub fn classic_apps(&self, host: &str) -> Option<ClassicApps> {
        let _ = host_key(host)?;
        read_json::<ClassicApps>(&self.classic_apps_path(host))
    }

    pub fn save_classic_apps(&self, host: &str, apps: &ClassicApps) -> std::io::Result<()> {
        if self.read_only || host_key(host).is_none() {
            return Ok(());
        }
        let path = self.classic_apps_path(host);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        self.write(&path, apps)
    }

    pub fn forget_classic_apps(&self, host: &str) {
        if !self.read_only && host_key(host).is_some() {
            let _ = std::fs::remove_file(self.classic_apps_path(host));
        }
    }

    /// The saved native library and app list of a PC.
    pub fn forget_library(&self, host: &str) {
        if !self.read_only && host_key(host).is_some() {
            let _ = std::fs::remove_file(self.library_path(host));
        }
        self.forget_classic_apps(host);
    }

    /// Everything kept for a PC that is removed: its library, its covers, and its games in the
    /// recent and favourite lists. Returns both lists as they are now.
    pub fn forget_host(
        &self,
        host: &str,
        recents: &[Recent],
        favourites: &[Favourite],
    ) -> (Vec<Recent>, Vec<Favourite>) {
        self.forget_library(host);
        if !self.read_only
            && let Some(key) = host_key(host)
            && !key.is_empty()
        {
            let _ = std::fs::remove_dir_all(self.dir.join("art").join(key));
        }
        let mut open_play = self.open_play();
        if open_play.game.as_ref().is_some_and(|g| g.host == host) {
            open_play.game = None;
            if let Err(e) = self.save_open_play(&open_play) {
                eprintln!("nightjar: saving the game to open failed: {e}");
            }
        }
        let kept: Vec<Recent> = recents.iter().filter(|r| r.host != host).cloned().collect();
        if kept.len() != recents.len()
            && let Err(e) = self.save_recents(&kept)
        {
            eprintln!("nightjar: saving recent games failed: {e}");
        }
        let stars: Vec<Favourite> = favourites.iter().filter(|f| f.host != host).cloned().collect();
        if stars.len() != favourites.len()
            && let Err(e) = self.save_favourites(&stars)
        {
            eprintln!("nightjar: saving favourites failed: {e}");
        }
        (kept, stars)
    }

    pub(super) fn library_path(&self, host: &str) -> PathBuf {
        self.dir.join("library").join(format!("{}.json", host_path_segment(host)))
    }

    pub(super) fn classic_apps_path(&self, host: &str) -> PathBuf {
        self.dir.join("apps").join(format!("{}.json", host_path_segment(host)))
    }
}

fn sanitize(name: &str) -> String {
    name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect()
}

/// Non-empty, filesystem-safe cache key for `host`, or `None` when `host` is empty.
/// Keeps the existing ASCII alphanumeric/hyphen name for clean UUIDs, and suffixes a stable hash
/// whenever characters are stripped so distinct IDs differing only in stripped characters cannot collide.
fn host_key(host: &str) -> Option<String> {
    if host.is_empty() {
        return None;
    }
    let clean = sanitize(host);
    if clean.len() == host.len() {
        return Some(clean);
    }
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in host.as_bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    Some(if clean.is_empty() {
        format!("h-{hash:016x}")
    } else {
        format!("{clean}-h-{hash:016x}")
    })
}

fn host_path_segment(host: &str) -> String {
    host_key(host).unwrap_or_else(|| "h-empty".to_owned())
}
