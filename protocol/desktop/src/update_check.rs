//! The update check and its notice, ported from Moonlight V+ PC's AutoUpdateChecker
//! (qiin2333/moonlight-qt, `app/backend/autoupdatechecker.cpp`) and pointed at
//! RamazanKara/nightjar: once per launch, when the setting is on and the PC is online, it asks
//! GitHub for the latest release and, when that is a stable version newer than this one, the
//! launcher says so. A copy the setup installed and an AppImage also get Update now, which starts
//! the download only when clicked (self_update.rs does the work); a portable Windows copy keeps the
//! link to the release page. A deb, rpm, Flatpak or archive gets updates from elsewhere, so it
//! doesn't check.
//!
//! The launcher holds the update back for a stream: a stream starting cancels a running download
//! and nothing is handed over until the launcher is back.
//!
//! Failures of the background check stay out of the UI (anonymous GitHub requests are rate limited,
//! as Android's UpdateChecker notes) and go to the log. Failures after Update now show in the notice.
//!
//! Settings → Check for updates now asks once more whenever it is chosen, setting on or off, as
//! Android's `UpdateFlow.checkNow`: its answer is a toast, or the notice when there is an update.
//! Nothing is asked without the setting or that choice.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use crate::self_update::{self, Asset, Attempt, CopyKind, Failure, Progress, Steps};
use crate::strings::{tr, trf};
use crate::update_net::{Native, Transport};

const LATEST: &str = "https://api.github.com/repos/RamazanKara/nightjar/releases/latest";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// What Update now downloads, or why it refuses to.
    pub asset: Result<Asset, Failure>,
}

/// Whether `tag` is a stable release newer than `current`. Delegated to `nightjar_policy::update`.
pub fn newer_stable(tag: &str, current: &str) -> bool {
    nightjar_policy::update::newer_stable_tag(tag, current)
}

/// The release GitHub answered, if it is an update for `current`. Drafts and pre-releases never
/// are (GitHub's latest release excludes them; the flags are checked as V+ PC does).
pub fn offer(json: &str, current: &str, copy: &CopyKind) -> Result<Option<Release>, String> {
    let parsed = nightjar_policy::update::Release::parse(json)?;
    if parsed.draft || parsed.prerelease || !newer_stable(&parsed.tag, current) {
        return Ok(None);
    }
    if !parsed.html_url.starts_with("https://github.com/") {
        return Err("the release has no page".into());
    }
    let version = parsed.tag.trim_start_matches('v').to_owned();
    let names = copy.asset_names(&version);
    let asset = parsed
        .select_desktop_asset(&names)
        .map_err(self_update::map_asset_error);
    Ok(Some(Release {
        version,
        url: parsed.html_url,
        asset,
    }))
}

/// A download on its way.
struct Job {
    progress: Arc<Progress>,
    size: u64,
    done: Receiver<Result<std::path::PathBuf, Failure>>,
}

/// What Check for updates now found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// A newer release: the notice shows it.
    Found,
    UpToDate,
    Failed,
}

impl Answer {
    /// Its toast, Android's `update_up_to_date` and `update_check_failed`; none for an update.
    pub fn toast(self) -> Option<&'static str> {
        match self {
            Self::Found => None,
            Self::UpToDate => Some(tr("You have the latest version")),
            Self::Failed => Some(tr("Couldn’t check for updates")),
        }
    }
}

/// What a check needs, kept for Check for updates now.
struct Asker {
    net: Arc<dyn Transport>,
    copy: CopyKind,
    current: &'static str,
    online: fn() -> bool,
}

/// The launch's one check, what it found and where the update stands.
#[derive(Default)]
pub struct Updates {
    pending: Option<Receiver<Release>>,
    /// None for a copy that doesn't check (a package manager updates it).
    asker: Option<Asker>,
    /// Check for updates now, waiting for GitHub.
    asked: Option<Receiver<Result<Option<Release>, String>>>,
    answer: Option<Answer>,
    found: Option<Release>,
    /// None for a copy that keeps the link.
    steps: Option<Steps>,
    wake: Option<crate::hosts::Waker>,
    job: Option<Job>,
    failure: Option<Failure>,
    held: bool,
    /// Held while `held`: tells another Nightjar's update, and the setup, about this stream.
    stream_mark: Option<self_update::Mark>,
    /// The version the user skipped: the launch's check does not offer it again, Check for
    /// updates now still does. Kept in `skip_file`.
    skipped: Option<String>,
    skip_file: Option<PathBuf>,
}

/// How often a launch before the network was up asks the system again whether it is.
const OFFLINE_RETRY: Duration = if cfg!(test) { Duration::from_millis(20) } else { Duration::from_secs(30) };

impl Updates {
    /// Starts the check on its own thread; `wake` redraws the launcher when it has an answer. With
    /// the setting off the thread only clears what an earlier update left.
    /// `skip_file` keeps the version the user skipped.
    pub fn start(enabled: bool, wake: crate::hosts::Waker, skip_file: PathBuf) -> Self {
        let copy = self_update::current();
        if !copy.checks() { return Self::default(); }
        let os = if cfg!(windows) {
            "Windows"
        } else if cfg!(target_os = "macos") {
            "macOS"
        } else {
            "Linux"
        };
        let agent = format!("Nightjar/{} ({os})", crate::VERSION);
        let net: Arc<dyn Transport> = Arc::new(Native::new(agent));
        let steps = Steps::for_copy(copy, net.clone());
        Self::begin(enabled, copy.clone(), net, steps, crate::VERSION, online, wake).with_skip_file(skip_file)
    }

    /// Reads the version skipped earlier from `file`, where Skip this version keeps it.
    fn with_skip_file(mut self, file: PathBuf) -> Self {
        self.skipped = std::fs::read_to_string(&file).ok().map(|text| text.trim().to_owned()).filter(|v| !v.is_empty());
        self.skip_file = Some(file);
        self
    }

    /// Skip this version: the notice goes, and the launch's check does not offer it again.
    fn skip(&mut self) {
        let Some(release) = self.found.take() else { return };
        self.failure = None;
        if let Some(file) = &self.skip_file
            && let Err(e) = std::fs::write(file, &release.version) { eprintln!("nightjar: update: {}: {e}", file.display()); }
        eprintln!("nightjar: update: Nightjar {} skipped", release.version);
        self.skipped = Some(release.version);
    }

    fn begin(enabled: bool, copy: CopyKind, net: Arc<dyn Transport>, steps: Option<Steps>, current: &'static str, online: fn() -> bool, wake: crate::hosts::Waker) -> Self {
        let (mut found, mut failure) = (None, None);
        if let Some(attempt) = steps.as_ref().filter(|_| enabled).and_then(|s| s.marker.as_deref()).and_then(|marker| failed_attempt(marker, current)) {
            eprintln!("nightjar: update: Nightjar {} did not install", attempt.version);
            found = Some(Release { version: attempt.version, url: attempt.url, asset: Err(Failure::NotInstalled) });
            failure = Some(Failure::NotInstalled);
        }
        let leftovers = steps.as_ref().map(|s| (s.dir.clone(), s.own_dir));
        let asker = Asker { net: net.clone(), copy: copy.clone(), current, online };
        let (send, receive) = channel();
        let waker = wake.clone();
        let spawned = std::thread::Builder::new().name("update-check".into()).spawn(move || {
            if let Some((dir, own_dir)) = leftovers { self_update::remove_stale(&dir, own_dir); }
            if !enabled { return; }
            // A launch before the network was up (a laptop opened before its Wi-Fi) checks once it is.
            if !online() { eprintln!("nightjar: update check: offline, asking again when online"); }
            while !online() { std::thread::sleep(OFFLINE_RETRY); }
            match check(&*net, &copy, current, online) {
                Ok(Some(release)) => {
                    eprintln!("nightjar: update check: Nightjar {} is available", release.version);
                    if let Err(refused) = &release.asset { eprintln!("nightjar: update check: no Update now: {refused:?}"); }
                    let _ = send.send(release);
                    waker();
                }
                Ok(None) => eprintln!("nightjar: update check: up to date"),
                Err(e) => eprintln!("nightjar: update check: {e}"),
            }
        });
        if let Err(e) = spawned { eprintln!("nightjar: update check: no thread: {e}"); }
        // Off, the copy still answers Check for updates now, and an update it finds installs.
        if !enabled { return Self { asker: Some(asker), steps, wake: Some(wake), ..Self::default() }; }
        Self { pending: Some(receive), asker: Some(asker), found, steps, wake: Some(wake), failure, ..Self::default() }
    }

    /// Check for updates now: one check, asked for by the user. False when this copy doesn't check
    /// or a check or download is already running; its answer comes from [`Self::answer`].
    pub fn check_now(&mut self) -> bool {
        if self.asked.is_some() || self.job.is_some() { return false; }
        let Some(asker) = &self.asker else { return false };
        let (net, copy, current, online) = (asker.net.clone(), asker.copy.clone(), asker.current, asker.online);
        let wake = self.wake.clone();
        let (send, receive) = channel();
        let spawned = std::thread::Builder::new().name("update-check-now".into()).spawn(move || {
            let _ = send.send(check(&*net, &copy, current, online));
            if let Some(wake) = wake { wake(); }
        });
        match spawned {
            Ok(_) => self.asked = Some(receive),
            Err(e) => {
                eprintln!("nightjar: update check: no thread: {e}");
                self.answer = Some(Answer::Failed);
            }
        }
        true
    }

    /// Whether Check for updates now is waiting for GitHub.
    #[cfg(test)]
    pub fn checking(&self) -> bool {
        self.asked.is_some()
    }

    /// Check for updates now's answer, once.
    pub fn answer(&mut self) -> Option<Answer> {
        self.answer.take()
    }

    /// Whether a stream is running or starting. Called on every pass of the main loop, so a stream
    /// never shares the network with a download or loses Nightjar to an installer.
    pub fn hold(&mut self, busy: bool) {
        if busy != self.held {
            self.stream_mark = busy.then(|| self_update::Mark::new(self_update::STREAM_MUTEX)).and_then(|mark| mark.map_err(|e| eprintln!("nightjar: update: {e}")).ok());
        }
        self.held = busy;
        if busy && self.job.is_some() {
            eprintln!("nightjar: update: a stream is starting, the download stops");
            self.cancel();
        }
    }

    /// Update now was clicked.
    fn update_now(&mut self) {
        if self.held || self.job.is_some() { return; }
        let (Some(steps), Some(release)) = (&self.steps, &self.found) else { return };
        if (steps.stream_elsewhere)() {
            eprintln!("nightjar: update: refused: another Nightjar is streaming");
            self.failure = Some(Failure::StreamElsewhere);
            return;
        }
        let asset = match &release.asset {
            Ok(asset) => asset.clone(),
            Err(refused) => {
                eprintln!("nightjar: update: refused: {refused:?}");
                self.failure = Some(refused.clone());
                return;
            }
        };
        let wake = self.wake.clone().unwrap_or_else(|| Arc::new(|| {}));
        let progress = Arc::new(Progress { cancel: AtomicBool::new(false), received: AtomicU64::new(0), wake: wake.clone() });
        let (send, done) = channel();
        let (net, allow, part, shared, size) = (steps.net.clone(), steps.allow, steps.part(), progress.clone(), asset.size);
        let spawned = std::thread::Builder::new().name("update-download".into()).spawn(move || {
            let result = self_update::download(&*net, allow, &asset, &part, &shared).map(|()| part);
            // Nobody waits any more: Cancel arrived after the last check. cancel() sets the flag
            // before it looks in the channel, so one of the two always sees the file.
            let part = result.as_ref().ok().cloned();
            let unwanted = send.send(result).is_err() || shared.cancel.load(Ordering::SeqCst);
            if let (true, Some(part)) = (unwanted, part) { let _ = std::fs::remove_file(part); }
            wake();
        });
        match spawned {
            Ok(_) => {
                self.failure = None;
                self.job = Some(Job { progress, size, done });
            }
            Err(e) => self.failure = Some(Failure::Network(e.to_string())),
        }
    }

    /// Cancel was clicked, or a stream is starting: the thread drops its file at its next chunk.
    fn cancel(&mut self) {
        let Some(job) = self.job.take() else { return };
        job.progress.cancel.store(true, Ordering::SeqCst);
        // The download may have finished just before: its file has no other owner now.
        if let Ok(Ok(part)) = job.done.try_recv()
            && let Err(e) = std::fs::remove_file(&part) { eprintln!("nightjar: update: leftover {}: {e}", part.display()); }
    }

    /// Takes in what the threads finished, once per pass of the main loop. A verified download that
    /// nothing stands in the way of comes back: the caller saves what the next Nightjar reads, then
    /// gives it to [`Self::hand_over`].
    pub fn step(&mut self) -> Option<PathBuf> {
        if let Some(release) = self.pending.as_ref().and_then(|r| r.try_recv().ok()) {
            self.pending = None;
            if self.skipped.as_deref() == Some(release.version.as_str()) {
                eprintln!("nightjar: update check: Nightjar {} was skipped", release.version);
            } else {
                self.found = Some(release);
            }
        }
        if let Some(answer) = self.asked.as_ref().and_then(|r| match r.try_recv() {
            Ok(answer) => Some(answer),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("the check stopped".into())),
        }) {
            self.asked = None;
            self.answer = Some(match answer {
                Ok(Some(release)) => {
                    eprintln!("nightjar: update check now: Nightjar {} is available", release.version);
                    // A notice dismissed earlier shows again; a download in progress keeps its own.
                    if self.job.is_none() {
                        self.failure = None;
                        self.found = Some(release);
                    }
                    Answer::Found
                }
                Ok(None) => {
                    eprintln!("nightjar: update check now: up to date");
                    Answer::UpToDate
                }
                Err(e) => {
                    eprintln!("nightjar: update check now: {e}");
                    Answer::Failed
                }
            });
        }
        let job = self.job.as_ref()?;
        let result = match job.done.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(Failure::Network("the download stopped".into())),
        };
        self.job = None;
        let ready = result.and_then(|part| self.refusal(&part).map(|()| part));
        self.settle(ready)
    }

    /// True when Nightjar should exit now for the update: the installer is running or the new
    /// AppImage has started.
    pub fn hand_over(&mut self, part: &Path) -> bool {
        let result = self.apply(part);
        self.settle(result).is_some()
    }

    fn settle<T>(&mut self, result: Result<T, Failure>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(Failure::Cancelled) => None,
            Err(failure) => {
                eprintln!("nightjar: update: {failure:?}");
                self.failure = Some(failure);
                None
            }
        }
    }

    /// What stops the update now that the download is here. The file goes with it.
    fn refusal(&self, part: &Path) -> Result<(), Failure> {
        let Some(steps) = &self.steps else { return Err(Failure::Cancelled) };
        let refused = if (steps.stream_elsewhere)() {
            Failure::StreamElsewhere
        } else if (steps.setup_running)() {
            // A second setup would stop at once and start Nightjar, on files the first is replacing.
            Failure::Apply("another Nightjar is installing an update".into())
        } else {
            return Ok(());
        };
        if let Err(e) = std::fs::remove_file(part) { eprintln!("nightjar: update: leftover {}: {e}", part.display()); }
        Err(refused)
    }

    fn apply(&self, part: &Path) -> Result<(), Failure> {
        let (Some(steps), Some(release)) = (&self.steps, &self.found) else { return Err(Failure::Cancelled) };
        let Ok(asset) = &release.asset else { return Err(Failure::NoInstaller) };
        let marker = steps.marker.as_deref();
        if let Some(marker) = marker {
            let attempt = Attempt { version: release.version.clone(), url: release.url.clone() };
            if let Err(e) = self_update::write_attempt(marker, &attempt) { eprintln!("nightjar: update: {}: {e}", marker.display()); }
        }
        let result = (steps.apply)(part, asset);
        if result.is_err()
            && let Some(marker) = marker { let _ = std::fs::remove_file(marker); }
        result
    }

    /// The notice over the launcher until it is opened or dismissed.
    pub fn show(&mut self, ctx: &egui::Context) {
        let Some(release) = &self.found else { return };
        let refused = self.failure.as_ref().is_some_and(Failure::is_final);
        let state = crate::update_banner::UpdateBannerState {
            version: release.version.clone(),
            url: release.url.clone(),
            progress: self.job.as_ref().map(|job| (job.progress.received.load(Ordering::Relaxed), job.size)),
            failure_text: self.failure.as_ref().map(|f| failure_text(f, &release.version)),
            can_install: self.steps.is_some() && !refused,
            held: self.held,
        };
        match crate::update_banner::show(ctx, &state) {
            Some(crate::update_banner::UpdateBannerAction::UpdateNow) => self.update_now(),
            Some(crate::update_banner::UpdateBannerAction::Cancel) => self.cancel(),
            Some(crate::update_banner::UpdateBannerAction::OpenDownloadPage(url)) => {
                if let Err(e) = crate::sdl::open_url(&url) {
                    eprintln!("nightjar: update page: {e}");
                }
                self.found = None;
                self.failure = None;
            }
            Some(crate::update_banner::UpdateBannerAction::SkipVersion) => self.skip(),
            Some(crate::update_banner::UpdateBannerAction::Close) => {
                self.found = None;
                self.failure = None;
            }
            None => {}
        }
    }
}

fn failure_text(failure: &Failure, version: &str) -> String {
    match failure {
        Failure::NoInstaller => tr("This release has no installer for this copy of Nightjar.").to_owned(),
        Failure::NoDigest => tr("This release has no checksum, so it was not installed.").to_owned(),
        Failure::Digest => tr("The download failed its checksum, so it was not installed.").to_owned(),
        Failure::Size => tr("The download has the wrong size, so it was not installed.").to_owned(),
        Failure::Address(_) => tr("The download came from an unexpected address, so it was not installed.").to_owned(),
        Failure::Network(why) => trf("The download failed: {}", &[why]),
        Failure::Apply(why) => trf("The update could not start: {}", &[why]),
        Failure::Restart(why) => trf("The new version is installed, but could not start: {}", &[why]),
        Failure::NotInstalled => trf("Nightjar {} did not install.", &[&version]),
        Failure::StreamElsewhere => tr("Another Nightjar window is streaming, so nothing was installed.").to_owned(),
        Failure::Cancelled => String::new(),
    }
}

/// What the last run's update left, when this run isn't that version or newer.
fn failed_attempt(marker: &Path, current: &str) -> Option<Attempt> {
    self_update::take_attempt(marker).filter(|attempt| newer_stable(&attempt.version, current))
}

fn check(net: &dyn Transport, copy: &CopyKind, current: &str, online: fn() -> bool) -> Result<Option<Release>, String> {
    if !online() { return Err("skipped, offline".into()); }
    let response = self_update::open(net, LATEST, self_update::api_host, true).map_err(|e| format!("{e:?}"))?;
    let status = response.status;
    let mut body = Vec::new();
    response.body.take(4 << 20).read_to_end(&mut body).map_err(|e| format!("read: {e}"))?;
    match status {
        200 => offer(std::str::from_utf8(&body).map_err(|_| "the answer is not UTF-8")?, current, copy),
        403 | 429 => Err(format!("GitHub rate limit (HTTP {status})")),
        _ => Err(format!("GitHub answered HTTP {status}")),
    }
}

/// Windows says the internet is reachable (not only a local network).
#[cfg(windows)]
fn online() -> bool {
    use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};
    NetworkInformation::GetInternetConnectionProfile()
        .and_then(|profile| profile.GetNetworkConnectivityLevel())
        .is_ok_and(|level| level == NetworkConnectivityLevel::InternetAccess)
}

#[cfg(not(windows))]
fn online() -> bool {
    true
}

#[cfg(test)]
mod tests;
