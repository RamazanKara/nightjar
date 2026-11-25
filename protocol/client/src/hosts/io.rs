//! Atomic private file I/O, `.bad` damaged-file backup, unreadable-file overwrite refusal,
//! and per-canonical-directory mutex serialization for `hosts.json`, `host_meta.json`,
//! `wake_targets.json`, and `identity.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static DIR_LOCKS: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
static NEXT_TMP: AtomicU64 = AtomicU64::new(0);

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(Component::ParentDir);
                }
            }
            Component::Normal(s) => {
                if s != OsStr::new("") {
                    out.push(s);
                }
            }
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

/// Canonical directory key used to synchronize store reads and writes within the process.
pub fn canonical_dir(dir: &Path) -> PathBuf {
    fs::canonicalize(dir).unwrap_or_else(|_| normalize_lexical(dir))
}

/// Returns the process-wide mutex guarding store files inside `dir`.
pub fn dir_lock(dir: &Path) -> Arc<Mutex<()>> {
    let key = canonical_dir(dir);
    let map = DIR_LOCKS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut guard = map.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

/// Writes `bytes` to `path` atomically (unique temp file, `sync_all`, rename). When `private` is
/// true, Unix permissions are restricted to `0o600`.
#[cfg_attr(not(unix), allow(unused_variables))]
pub fn write_atomic(path: &Path, bytes: &[u8], private: bool) -> io::Result<()> {
    use std::io::Write as _;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("no file name"))?
        .to_string_lossy();
    let temporary = path.with_file_name(format!(
        "{name}.{}.{}.tmp",
        std::process::id(),
        NEXT_TMP.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        // OS intent: restrict sensitive host/identity files to owner (0600) on Unix/Android/macOS.
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        } else {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o644);
        }
        let mut file = options.open(&temporary)?;
        #[cfg(unix)]
        // OS intent: bypass umask so private files stay strictly 0600 on Unix/Android/macOS.
        if private {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(PermissionsExt::from_mode(0o600))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// Copies `path` to `<name>.<suffix>` (`<name>.<suffix>.2`, ...) unless an identical copy exists.
pub fn back_up(path: &Path, suffix: &str) -> io::Result<()> {
    let bytes = fs::read(path)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    for n in 1usize.. {
        let copy = path.with_file_name(if n == 1 {
            format!("{name}.{suffix}")
        } else {
            format!("{name}.{suffix}.{n}")
        });
        match fs::read(&copy) {
            Ok(earlier) if earlier == bytes => return Ok(()),
            Ok(_) => continue,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                write_atomic(&copy, &bytes, true)?;
                eprintln!(
                    "nightjar: copied {} to {} before replacing it",
                    path.display(),
                    copy.display()
                );
                return Ok(());
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("a free backup name")
}

/// Tracks files that could not be read this run so defaults never overwrite them.
#[derive(Debug, Default)]
pub(crate) struct UnreadableGuard {
    paths: Mutex<BTreeSet<PathBuf>>,
}

impl UnreadableGuard {
    pub(crate) fn leave_as_it_is(&self, path: &Path) {
        self.paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(path.to_owned());
    }

    pub(crate) fn clear(&self, path: &Path) {
        self.paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(path);
    }

    pub(crate) fn check_writable(&self, path: &Path) -> io::Result<()> {
        let flagged = self
            .paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(path);
        let non_file = match fs::symlink_metadata(path) {
            Ok(meta) => !meta.is_file(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(_) => true,
        };
        if flagged || non_file {
            return Err(io::Error::other(format!(
                "{} could not be read, so it is left as it is to keep what it holds",
                path.display()
            )));
        }
        Ok(())
    }
}

/// Synchronized JSON store file that treats only `NotFound` as empty, refuses overwrites after
/// an unreadable or corrupt read, and clears the unreadable flag once a subsequent read succeeds.
#[derive(Clone, Debug)]
pub(crate) struct GuardedJsonFile {
    path: PathBuf,
    lock: Arc<Mutex<()>>,
    unreadable: Arc<UnreadableGuard>,
    private: bool,
}

impl GuardedJsonFile {
    pub(crate) fn new(path: PathBuf, lock: Arc<Mutex<()>>, private: bool) -> Self {
        Self {
            path,
            lock,
            unreadable: Arc::new(UnreadableGuard::default()),
            private,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn try_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, String> {
        self.lock.lock().map_err(|_| "lock poisoned".to_owned())
    }

    pub(crate) fn mark_readable(&self) {
        self.unreadable.clear(&self.path);
    }

    pub(crate) fn mark_unreadable(&self, reason: &dyn std::fmt::Display) {
        eprintln!(
            "nightjar: cannot read {} ({reason}); it is left as it is",
            self.path.display()
        );
        self.unreadable.leave_as_it_is(&self.path);
    }

    /// Reads the raw file text: returns `Ok(None)` strictly on `NotFound` (clearing any stale
    /// guard) and records non-`NotFound` I/O failures so writes refuse to clobber the file.
    pub(crate) fn read_text(&self) -> Option<String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                self.unreadable.clear(&self.path);
                None
            }
            Err(e) => {
                self.mark_unreadable(&e);
                None
            }
        }
    }

    /// Reads and parses JSON `T` (falling back to `T::default()` on `NotFound` or error),
    /// clearing the guard on a good read and refusing subsequent overwrites on any non-`NotFound`
    /// I/O or parse failure.
    pub(crate) fn read_json_or_default<T>(&self) -> T
    where
        T: serde::de::DeserializeOwned + Default,
    {
        let Some(text) = self.read_text() else {
            return T::default();
        };
        match serde_json::from_str::<T>(&text) {
            Ok(value) => {
                self.mark_readable();
                value
            }
            Err(e) => {
                self.mark_unreadable(&e);
                T::default()
            }
        }
    }

    pub(crate) fn write_bytes(&self, bytes: &[u8]) -> io::Result<()> {
        self.unreadable.check_writable(&self.path)?;
        write_atomic(&self.path, bytes, self.private)
    }
}
