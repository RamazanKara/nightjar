//! File-backed PIN lock persistence (`pin_lock.json`) with crash-safe try counting,
//! atomic private writes, background file coalescing and one-time legacy prefs migration.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nightjar_policy::pin::{
    Clock, FILE, HAND_OVER_MS, HandOver, MAX_DIGITS, MIN_DIGITS, Mode, Outcome, PinCrypto,
    PinStoreBackend, Problem, RETRY_MS, Saved, View, grace_of, throttle,
};
use serde::Deserialize;

/// One reader or writer of the lock's file at a time, across every store in the process.
pub(crate) static TRY_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn file_lock() -> std::sync::MutexGuard<'static, ()> {
    TRY_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The lock's state in memory, which every caller reads and changes; the file follows it.
pub(crate) struct Cache {
    pub(crate) saved: Saved,
    pub(crate) problem: Option<Problem>,
    /// When a file with a problem is read next, and whether that read is queued.
    pub(crate) retry_at: Instant,
    pub(crate) reading: bool,
}

/// Work for the store's file thread.
enum Job {
    /// Write what memory holds now (or remove the file when no PIN is set).
    Save,
    /// Read a file that had a problem again.
    Read,
    /// Answers once the jobs before it are done.
    Flush(Sender<()>),
}

/// The file-backed PIN store. Reads and changes work on memory and never wait for the file; a
/// background thread writes the file behind them. Slow tries and PIN updates run on a worker.
#[derive(Clone)]
pub struct PinStore {
    path: PathBuf,
    clock: Arc<dyn Clock>,
    crypto: Arc<dyn PinCrypto>,
    pub(crate) cache: Arc<Mutex<Cache>>,
    jobs: Sender<Job>,
}

impl PinStore {
    /// Reads `pin_lock.json` in `dir` once and starts the background save thread.
    pub fn new(dir: &Path, clock: Arc<dyn Clock>, crypto: Arc<dyn PinCrypto>) -> Self {
        let path = dir.join(FILE);
        let cache = Arc::new(Mutex::new(Cache {
            saved: Saved::default(),
            problem: None,
            retry_at: Instant::now(),
            reading: false,
        }));
        {
            let _file = file_lock();
            read_into(&path, &cache, crypto.as_ref());
        }
        let (jobs, queue) = channel();
        let (thread_path, thread_cache, thread_crypto) =
            (path.clone(), cache.clone(), crypto.clone());
        std::thread::Builder::new()
            .name("pin-lock-file".into())
            .spawn(move || file_thread(&thread_path, &thread_cache, &queue, thread_crypto.as_ref()))
            .expect("start the PIN lock's file thread");
        Self {
            path,
            clock,
            crypto,
            cache,
            jobs,
        }
    }

    pub(crate) fn memory(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn crypto(&self) -> &Arc<dyn PinCrypto> {
        &self.crypto
    }

    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    fn queue(&self, job: Job) {
        if self.jobs.send(job).is_err() {
            eprintln!(
                "nightjar: the PIN lock's file thread is gone; the change stays in memory only"
            );
        }
    }

    /// Re-reads the file from disk immediately under the file lock (used after legacy import).
    pub fn reload_now(&self) {
        let _file = file_lock();
        read_into(&self.path, &self.cache, self.crypto.as_ref());
    }

    /// The state as memory holds it. A file with a problem is read again, in the background,
    /// once its retry time has come.
    fn saved(&self) -> Saved {
        let mut memory = self.memory();
        if memory.problem.is_some() && !memory.reading && Instant::now() >= memory.retry_at {
            memory.reading = true;
            drop(memory);
            self.queue(Job::Read);
            return self.memory().saved.clone();
        }
        memory.saved.clone()
    }

    /// Changes the state and has it written. Refused while the file has a problem so the closed
    /// state in memory is never written over the user's file.
    fn edit(&self, change: impl FnOnce(&mut Saved)) {
        {
            let mut memory = self.memory();
            if let Some(problem) = &memory.problem {
                eprintln!(
                    "nightjar: the PIN lock is not changed while its file cannot be used: {problem:?}"
                );
                return;
            }
            change(&mut memory.saved);
        }
        self.queue(Job::Save);
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn problem(&self) -> Option<Problem> {
        self.saved();
        self.memory().problem.clone()
    }

    pub fn has_pin(&self) -> bool {
        self.saved().hash.is_some()
    }

    pub fn view(&self) -> View {
        let saved = self.saved();
        let has_pin = saved.hash.is_some();
        View {
            has_pin,
            mode: if has_pin { saved.mode } else { Mode::Off },
            pcs: saved.pcs,
            grace_minutes: grace_of(saved.grace_min),
        }
    }

    pub fn pin_length(&self) -> usize {
        let saved = self.saved();
        if saved.hash.is_some() {
            saved.length.clamp(MIN_DIGITS, MAX_DIGITS)
        } else {
            0
        }
    }

    pub fn mode(&self) -> Mode {
        let saved = self.saved();
        if saved.hash.is_some() {
            saved.mode
        } else {
            Mode::Off
        }
    }

    pub fn set_mode(&self, mode: Mode) {
        self.edit(|s| s.mode = mode);
    }

    pub fn set_pcs(&self, pcs: Vec<String>) {
        self.edit(|s| s.pcs = pcs);
    }

    pub fn grace_minutes(&self) -> u32 {
        grace_of(self.saved().grace_min)
    }

    pub fn set_grace_minutes(&self, minutes: u32) {
        self.edit(|s| s.grace_min = minutes);
    }

    pub fn set_pin(&self, pin: &[u8]) -> Result<(), String> {
        self.set_pin_with(pin, self.crypto.calibrate())
    }

    pub fn set_pin_with(&self, pin: &[u8], iterations: u32) -> Result<(), String> {
        if !(MIN_DIGITS..=MAX_DIGITS).contains(&pin.len()) || !pin.iter().all(u8::is_ascii_digit) {
            return Err(format!("a PIN has {MIN_DIGITS} to {MAX_DIGITS} digits"));
        }
        let hash = self.crypto.create(pin, iterations)?;
        let _file = file_lock();
        let mut candidate = {
            let memory = self.memory();
            if memory.problem.is_some() {
                return Err("the PIN lock's file cannot be used".into());
            }
            memory.saved.clone()
        };
        candidate.hash = Some(hash);
        candidate.length = pin.len();
        (candidate.fails, candidate.lock_until, candidate.lock_boot) = (0, 0, 0);
        write_file(&self.path, &candidate).map_err(|e| format!("saving the PIN failed: {e}"))?;
        self.memory().saved = candidate;
        Ok(())
    }

    pub fn remove_pin(&self) {
        self.edit(|s| *s = Saved::default());
    }

    fn throttle_state(saved: &Saved) -> throttle::State {
        throttle::State {
            fails: saved.fails,
            lock_until: saved.lock_until,
            lock_boot: saved.lock_boot,
        }
    }

    fn put_throttle(saved: &mut Saved, state: throttle::State) {
        (saved.fails, saved.lock_until, saved.lock_boot) =
            (state.fails, state.lock_until, state.lock_boot);
    }

    fn current(&self, memory: &mut Cache, now: i64) -> throttle::State {
        let stored = Self::throttle_state(&memory.saved);
        let state = throttle::on_boot(
            stored,
            now,
            self.clock.boot(),
            self.clock.is_this_boot(stored.lock_boot),
        );
        if state != stored && memory.problem.is_none() {
            Self::put_throttle(&mut memory.saved, state);
        }
        state
    }

    pub fn locked_for_ms(&self) -> i64 {
        let now = self.clock.elapsed();
        let (state, renewed) = {
            let mut memory = self.memory();
            let before = Self::throttle_state(&memory.saved);
            let state = self.current(&mut memory, now);
            (state, Self::throttle_state(&memory.saved) != before)
        };
        if renewed {
            self.queue(Job::Save);
        }
        throttle::remaining_ms(state, now)
    }

    pub fn try_pin(&self, pin: &[u8]) -> Outcome {
        let crypto = self.crypto.clone();
        self.try_pin_with(pin, |stored, pin| crypto.verify(stored, pin))
    }

    pub fn try_pin_with(&self, pin: &[u8], verify: impl FnOnce(&str, &[u8]) -> bool) -> Outcome {
        match self.try_pin_fallible(pin, |stored, pin| {
            Ok::<bool, std::convert::Infallible>(verify(stored, pin))
        }) {
            Ok(outcome) => outcome,
        }
    }

    /// Runs `try_pin` with a fallible verifier (e.g. a JNI callback that may throw). If `verify`
    /// returns `Err(e)`, the failure count already committed on disk stays intact and `Err(e)`
    /// propagates without panicking.
    pub fn try_pin_fallible<E>(
        &self,
        pin: &[u8],
        verify: impl FnOnce(&str, &[u8]) -> Result<bool, E>,
    ) -> Result<Outcome, E> {
        let _file = file_lock();
        if self.memory().problem.is_some() {
            read_into(&self.path, &self.cache, self.crypto.as_ref());
        }
        let now = self.clock.elapsed();
        let boot = self.clock.boot();
        let (counted, hash) = {
            let mut memory = self.memory();
            if memory.problem.is_some() {
                return Ok(Outcome::Unavailable);
            }
            let state = self.current(&mut memory, now);
            let locked = throttle::remaining_ms(state, now);
            if locked > 0 {
                return Ok(Outcome::Locked {
                    locked_for_ms: locked,
                });
            }
            let counted = throttle::failed(state, now, boot);
            Self::put_throttle(&mut memory.saved, counted);
            (counted, memory.saved.hash.clone().unwrap_or_default())
        };
        self.write_now();
        if !verify(&hash, pin)? {
            return Ok(Outcome::Wrong {
                locked_for_ms: throttle::remaining_ms(counted, self.clock.elapsed()),
            });
        }
        Self::put_throttle(&mut self.memory().saved, throttle::State::default());
        self.write_now();
        Ok(Outcome::Ok)
    }

    fn write_now(&self) {
        let saved = self.memory().saved.clone();
        if let Err(e) = write_file(&self.path, &saved) {
            eprintln!("nightjar: saving the PIN lock failed: {e}");
        }
    }

    pub fn set_active(&self, elapsed: i64, boot: i64) {
        self.edit(|s| (s.active_at, s.active_boot) = (elapsed, boot));
    }

    pub fn active_at(&self) -> i64 {
        self.saved().active_at
    }

    pub fn active_boot(&self) -> i64 {
        self.saved().active_boot
    }

    pub fn is_this_boot(&self, stored: i64) -> bool {
        self.clock.is_this_boot(stored)
    }

    pub fn hand_over(&self) -> Result<Option<String>, String> {
        if !self.has_pin() {
            return Ok(None);
        }
        let token = self.crypto.random_token()?;
        let entry = HandOver {
            hash: self.crypto.token_hash(&token),
            at: self.clock.elapsed(),
            boot: self.clock.boot(),
        };
        let _file = file_lock();
        let mut candidate = {
            let memory = self.memory();
            if memory.problem.is_some() {
                return Err("the PIN lock's file cannot be used".into());
            }
            memory.saved.clone()
        };
        candidate.hand_over = Some(entry);
        write_file(&self.path, &candidate)
            .map_err(|e| format!("saving the PIN lock failed: {e}"))?;
        self.memory().saved = candidate;
        Ok(Some(token))
    }

    pub fn end_hand_over(&self) {
        if self.saved().hand_over.is_some() {
            self.edit(|s| s.hand_over = None);
        }
    }

    pub fn handed_over(&self, token: &str) -> bool {
        let saved = self.saved();
        let Some(entry) = saved
            .hand_over
            .filter(|_| saved.hash.is_some() && !token.is_empty())
        else {
            return false;
        };
        entry.hash == self.crypto.token_hash(token)
            && self.clock.is_this_boot(entry.boot)
            && (0..HAND_OVER_MS).contains(&(self.clock.elapsed() - entry.at))
    }

    pub fn elapsed(&self) -> i64 {
        self.clock.elapsed()
    }

    pub fn boot(&self) -> i64 {
        self.clock.boot()
    }

    pub fn flush(&self) {
        let (done, wait) = channel();
        self.queue(Job::Flush(done));
        if wait.recv().is_err() {
            eprintln!("nightjar: the PIN lock's file thread ended before it wrote everything");
        }
    }
}

impl PinStoreBackend for PinStore {
    fn path(&self) -> &Path {
        self.path()
    }
    fn problem(&self) -> Option<Problem> {
        self.problem()
    }
    fn has_pin(&self) -> bool {
        self.has_pin()
    }
    fn view(&self) -> View {
        self.view()
    }
    fn pin_length(&self) -> usize {
        self.pin_length()
    }
    fn mode(&self) -> Mode {
        self.mode()
    }
    fn set_mode(&self, mode: Mode) {
        self.set_mode(mode)
    }
    fn set_pcs(&self, pcs: Vec<String>) {
        self.set_pcs(pcs)
    }
    fn grace_minutes(&self) -> u32 {
        self.grace_minutes()
    }
    fn set_grace_minutes(&self, minutes: u32) {
        self.set_grace_minutes(minutes)
    }
    fn set_pin(&self, pin: &[u8]) -> Result<(), String> {
        self.set_pin(pin)
    }
    fn remove_pin(&self) {
        self.remove_pin()
    }
    fn locked_for_ms(&self) -> i64 {
        self.locked_for_ms()
    }
    fn try_pin(&self, pin: &[u8]) -> Outcome {
        self.try_pin(pin)
    }
    fn set_active(&self, elapsed: i64, boot: i64) {
        self.set_active(elapsed, boot)
    }
    fn active_at(&self) -> i64 {
        self.active_at()
    }
    fn active_boot(&self) -> i64 {
        self.active_boot()
    }
    fn is_this_boot(&self, stored: i64) -> bool {
        self.is_this_boot(stored)
    }
    fn hand_over(&self) -> Result<Option<String>, String> {
        self.hand_over()
    }
    fn end_hand_over(&self) {
        self.end_hand_over()
    }
    fn handed_over(&self, token: &str) -> bool {
        self.handed_over(token)
    }
    fn elapsed(&self) -> i64 {
        self.elapsed()
    }
    fn boot(&self) -> i64 {
        self.boot()
    }
    fn flush(&self) {
        self.flush()
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct LegacyAndroidPrefs {
    pin_hash: Option<String>,
    pin_length: usize,
    pin_mode: String,
    pin_pcs: Vec<String>,
    pin_grace_min: u32,
    pin_fails: u32,
    pin_lock_until: i64,
    pin_lock_boot: i64,
    pin_active_at: i64,
    pin_active_boot: i64,
}

impl Default for LegacyAndroidPrefs {
    fn default() -> Self {
        Self {
            pin_hash: None,
            pin_length: MAX_DIGITS,
            pin_mode: "off".into(),
            pin_pcs: Vec::new(),
            pin_grace_min: nightjar_policy::pin::DEFAULT_GRACE_MINUTES,
            pin_fails: 0,
            pin_lock_until: 0,
            pin_lock_boot: 0,
            pin_active_at: 0,
            pin_active_boot: -1,
        }
    }
}

/// One-time migration of Android `SharedPreferences("pin_lock")` into `pin_lock.json`.
///
/// Returns `Ok(false)` if `pin_lock.json` already exists (never overwriting it), `Ok(true)` if
/// migrated (or if legacy prefs held no PIN), or `Err(...)` if the JSON or stored hash is invalid.
pub fn import_legacy_json(dir: &Path, json: &str, crypto: &dyn PinCrypto) -> Result<bool, String> {
    let legacy: LegacyAndroidPrefs =
        serde_json::from_str(json).map_err(|e| format!("invalid legacy pin JSON: {e}"))?;
    let _file = file_lock();
    let path = dir.join(FILE);
    match std::fs::symlink_metadata(&path) {
        Ok(_) => return Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("checking existing pin_lock.json failed: {e}")),
    }
    let Some(hash) = legacy.pin_hash else {
        return Ok(true);
    };
    if !crypto.well_formed(&hash) {
        return Err("legacy PIN hash is not well-formed".into());
    }
    std::fs::create_dir_all(dir)
        .map_err(|e| format!("creating PIN store directory failed: {e}"))?;
    let saved = Saved {
        hash: Some(hash),
        length: legacy.pin_length.clamp(MIN_DIGITS, MAX_DIGITS),
        mode: Mode::from_str_lossy(&legacy.pin_mode),
        pcs: legacy.pin_pcs,
        grace_min: grace_of(legacy.pin_grace_min),
        fails: legacy.pin_fails,
        lock_until: legacy.pin_lock_until,
        lock_boot: legacy.pin_lock_boot,
        active_at: legacy.pin_active_at,
        active_boot: legacy.pin_active_boot,
        hand_over: None,
    };
    write_file(&path, &saved).map_err(|e| format!("writing migrated pin_lock.json failed: {e}"))?;
    Ok(true)
}

fn file_thread(path: &Path, cache: &Mutex<Cache>, queue: &Receiver<Job>, crypto: &dyn PinCrypto) {
    while let Ok(first) = queue.recv() {
        let (mut save, mut read) = (false, false);
        let mut flushes = Vec::new();
        for job in std::iter::once(first).chain(queue.try_iter()) {
            match job {
                Job::Save => save = true,
                Job::Read => read = true,
                Job::Flush(done) => flushes.push(done),
            }
        }
        {
            let _file = file_lock();
            if read {
                read_into(path, cache, crypto);
            }
            if save {
                let saved = {
                    let memory = cache
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    memory.problem.is_none().then(|| memory.saved.clone())
                };
                if let Some(saved) = saved
                    && let Err(e) = write_file(path, &saved)
                {
                    eprintln!("nightjar: saving the PIN lock failed: {e}");
                }
            }
        }
        for done in flushes {
            let _ = done.send(());
        }
    }
}

fn read_into(path: &Path, cache: &Mutex<Cache>, crypto: &dyn PinCrypto) {
    let read = match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Saved>(&bytes)
            .map_err(|e| Problem::Damaged(e.to_string()))
            .and_then(|saved| match &saved.hash {
                Some(hash) if !crypto.well_formed(hash) => Err(Problem::Damaged(
                    "the stored PIN is not a hash this version wrote".into(),
                )),
                _ => Ok(saved),
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Saved::default()),
        Err(e) => Err(Problem::Unreadable(e.to_string())),
    };
    let mut memory = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    memory.reading = false;
    memory.retry_at = Instant::now() + Duration::from_millis(RETRY_MS);
    match read {
        Ok(saved) => {
            if memory.problem.take().is_some() {
                eprintln!(
                    "nightjar: the PIN lock file {} is usable again",
                    path.display()
                );
            }
            memory.saved = saved;
        }
        Err(problem) => {
            if memory.problem.as_ref() != Some(&problem) {
                match &problem {
                    Problem::Unreadable(why) => eprintln!(
                        "nightjar: the PIN lock file {} cannot be read now ({why}); the app stays locked and reads it again",
                        path.display()
                    ),
                    Problem::Damaged(why) => eprintln!(
                        "nightjar: the PIN lock file {} is damaged ({why}); the app stays locked until it is deleted",
                        path.display()
                    ),
                }
            }
            memory.problem = Some(problem);
            memory.saved = Saved::closed();
        }
    }
}

fn write_file(path: &Path, saved: &Saved) -> std::io::Result<()> {
    if saved.hash.is_none() {
        return match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_vec_pretty(saved).map_err(std::io::Error::other)?;
    write_private(path, &text)
}

/// Writes `bytes` to `path` atomically (temp file + `sync_all` + rename) with `0600` on Unix.
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
        // Unix file mode 0600 (Linux/macOS/Android): PIN lock holds the hash, only user may read.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let mut file = options.open(&temporary)?;
        // Unix file mode 0600 (Linux/macOS/Android): set explicitly past umask.
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
