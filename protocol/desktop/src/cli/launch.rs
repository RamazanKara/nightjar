//! Host/app lookup and the frame-driven state machine (`CliLaunch`) for command-line launches.

use std::time::{Duration, Instant};

use nightjar_core::http::{App, AppControl};

use super::Launch;

/// How long the PC gets to answer and list its apps.
pub(super) const START_LIMIT: Duration = Duration::from_secs(60);
/// How long a launched stream gets to start: a backstop, the connection's own steps time out
/// sooner.
pub(super) const LAUNCH_LIMIT: Duration = Duration::from_secs(180);

/// A saved PC as `find_host` sees it.
pub struct HostEntry<'a> {
    pub id: u64,
    pub unique_id: &'a str,
    pub name: &'a str,
    pub alias: Option<&'a str>,
    pub paired: bool,
}

/// The saved PC with this unique id, or else the one with this name (its own or the one given
/// on this device), ignoring case. Unknown, ambiguous and unpaired PCs are errors.
pub fn find_host<'a>(
    hosts: impl IntoIterator<Item = HostEntry<'a>>,
    wanted: &str,
) -> Result<u64, String> {
    let hosts: Vec<HostEntry> = hosts.into_iter().collect();
    let found = match hosts
        .iter()
        .find(|h| !h.unique_id.is_empty() && h.unique_id.eq_ignore_ascii_case(wanted))
    {
        Some(host) => host,
        None => {
            let named: Vec<&HostEntry> = hosts
                .iter()
                .filter(|h| {
                    h.name.eq_ignore_ascii_case(wanted)
                        || h.alias.is_some_and(|a| a.eq_ignore_ascii_case(wanted))
                })
                .collect();
            match named.as_slice() {
                [] => return Err(format!("no saved PC is named {wanted} or has that UUID")),
                [host] => *host,
                _ => {
                    return Err(format!(
                        "several saved PCs are named {wanted}; give its UUID instead"
                    ))
                }
            }
        }
    };
    if !found.paired {
        return Err(format!(
            "the PC {wanted} is not paired; pair it in Nightjar first"
        ));
    }
    Ok(found.id)
}

/// The app with this UUID, or else the one with this name, ignoring case. Host actions
/// (disconnect, terminate) are no apps to stream.
pub fn find_app<'a>(apps: &'a [App], wanted: &str) -> Result<&'a App, String> {
    let found = match apps
        .iter()
        .find(|a| !a.uuid.is_empty() && a.uuid.eq_ignore_ascii_case(wanted))
    {
        Some(app) => app,
        None => {
            let named: Vec<&App> = apps
                .iter()
                .filter(|a| {
                    // A list read before the client stripped `legacy_ordering`'s prefix.
                    let bare = |s: &str| s.trim_start_matches(['\u{200b}', '\u{200c}']).to_owned();
                    bare(&a.name).eq_ignore_ascii_case(&bare(wanted))
                })
                .collect();
            match named.as_slice() {
                [] => {
                    return Err(format!(
                        "the PC has no app named {wanted} or with that UUID"
                    ))
                }
                [app] => *app,
                _ => {
                    return Err(format!(
                        "the PC has several apps named {wanted}; give its UUID instead"
                    ))
                }
            }
        }
    };
    if found.is_control_action() {
        return Err(format!(
            "{} is an action on the PC, not an app to stream",
            found.name
        ));
    }
    Ok(found)
}

/// Whether starting `app` resumes it, by the rules of a click on its poster (`ui.rs`). Another
/// app running is an error: the PC refuses a plain launch next to it unless it asks the user
/// itself.
pub(super) fn resume(host: &HostSeen, apps: &[App], app: &App) -> Result<bool, String> {
    if app.control != AppControl::None {
        return Ok(false);
    }
    let Some((id, uuid)) = &host.running else {
        return Ok(false);
    };
    if app.matches_running_app(*id, uuid.as_deref()) {
        return Ok(true);
    }
    if host.confirms_replacement {
        return Ok(false);
    }
    let running = apps
        .iter()
        .find(|a| a.matches_running_app(*id, uuid.as_deref()))
        .map_or("another app", |a| a.name.as_str());
    Err(format!(
        "the PC is running {running}; quit it first or start that app instead"
    ))
}

pub enum AppsSeen {
    NotLoaded,
    Loading,
    Loaded(Vec<App>),
    Failed(String),
}

pub struct HostSeen {
    pub answered: bool,
    pub paired: bool,
    /// Why the PC could not be reached, when its last check failed.
    pub offline: Option<String>,
    /// The app the PC runs, as it reports it: its id and UUID.
    pub running: Option<(i32, Option<String>)>,
    pub confirms_replacement: bool,
    pub apps: AppsSeen,
}

pub struct Seen {
    pub identity: bool,
    /// None once the PC is no longer saved.
    pub host: Option<HostSeen>,
    /// The launcher's error message, if one is up.
    pub message: Option<String>,
    /// A launch is under way: the network check, the connection or the stream.
    pub busy: bool,
    /// The launcher asks the user to confirm closing the app the PC runs.
    pub confirming: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Wait,
    OpenApps(u64),
    LoadApps(u64),
    /// Host, app, and whether to resume it.
    Launch(u64, i32, bool),
    Fail(String),
    /// The user cancelled the start: close without an error.
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Starting,
    /// The launch was asked for; waiting for the stream.
    Launched,
    Streaming,
    /// Nothing more to drive (the app list is open, or the stream ended without quitting).
    Idle,
}

/// Drives one command-line start through the launcher, one step per frame.
pub struct CliLaunch {
    pub launch: Launch,
    pub host: u64,
    phase: Phase,
    /// The app list was asked for and has not started loading since.
    apps_asked: bool,
    /// When the current phase began.
    pub(super) started: Instant,
    /// The user cancelled the launch.
    cancelled: bool,
    /// Why the start failed; the process then exits with an error.
    pub failed: Option<String>,
}

impl CliLaunch {
    pub fn new(launch: Launch, hosts: &crate::hosts::Hosts) -> Result<Self, String> {
        let entries = hosts.list.iter().map(|h| HostEntry {
            id: h.id,
            unique_id: &h.record.unique_id,
            name: &h.record.name,
            alias: h.record.alias.as_deref().filter(|a| !a.is_empty()),
            paired: h.record.server_certificate.is_some(),
        });
        let host = find_host(entries, &launch.host)?;
        Ok(Self::resolved(launch, host))
    }

    pub(super) fn resolved(launch: Launch, host: u64) -> Self {
        Self {
            launch,
            host,
            phase: Phase::Starting,
            apps_asked: false,
            started: Instant::now(),
            cancelled: false,
            failed: None,
        }
    }

    pub fn host_seen(&self, hosts: &crate::hosts::Hosts) -> Option<HostSeen> {
        use crate::hosts::{Apps, Reach};
        let host = hosts.get(self.host)?;
        // Once launched (or idle), the state machine no longer inspects the app list or game UUID.
        let need_apps = matches!(self.phase, Phase::Starting) && self.launch.app.is_some();
        Some(HostSeen {
            answered: host.info.is_some(),
            paired: host.paired(),
            offline: (host.reach == Reach::Offline).then(|| host.error.clone().unwrap_or_default()),
            running: if need_apps {
                host.running_app().map(|id| {
                    let uuid = host.info.as_ref().and_then(|i| i.current_game_uuid.clone());
                    (id, uuid)
                })
            } else {
                None
            },
            confirms_replacement: host.confirms_replacement(),
            apps: if need_apps && self.launch.join.is_none() {
                match &host.apps {
                    Apps::NotLoaded => AppsSeen::NotLoaded,
                    Apps::Loading => AppsSeen::Loading,
                    Apps::Loaded(apps) => AppsSeen::Loaded(apps.as_ref().clone()),
                    Apps::Failed(e) => AppsSeen::Failed(e.clone()),
                }
            } else {
                AppsSeen::NotLoaded
            },
        })
    }

    /// What to do next, given what the launcher can see now. Each action is asked for once.
    pub fn next(&mut self, seen: &Seen) -> Step {
        match self.phase {
            Phase::Streaming | Phase::Idle => return Step::Wait,
            Phase::Launched => return self.launched(seen),
            Phase::Starting => {}
        }
        // An error up now stops the start. None is up when the launch goes out, so every
        // message in `Launched` is about the launch.
        if let Some(message) = &seen.message {
            return Step::Fail(message.clone());
        }
        let Some(host) = &seen.host else {
            return Step::Fail(format!("the PC {} is no longer saved", self.launch.host));
        };
        if !seen.identity {
            return self.in_time(Step::Wait);
        }
        let Some(wanted) = self.launch.app.clone() else {
            self.phase = Phase::Idle;
            return Step::OpenApps(self.host);
        };
        if let Some(why) = &host.offline {
            return Step::Fail(format!(
                "the PC {} cannot be reached: {why}",
                self.launch.host
            ));
        }
        if !host.answered {
            return self.in_time(Step::Wait);
        }
        if !host.paired {
            return Step::Fail(format!(
                "the PC {} is not paired; pair it in Nightjar first",
                self.launch.host
            ));
        }
        match &host.apps {
            AppsSeen::NotLoaded if !self.apps_asked => {
                self.apps_asked = true;
                Step::LoadApps(self.host)
            }
            AppsSeen::NotLoaded => self.in_time(Step::Wait),
            AppsSeen::Loading => {
                // A list that drops back to unloaded later is asked for again.
                self.apps_asked = false;
                self.in_time(Step::Wait)
            }
            AppsSeen::Failed(e) => Step::Fail(format!("could not load the apps: {e}")),
            AppsSeen::Loaded(apps) => {
                let found = match self.launch.join {
                    Some(id) => Ok((id, true)),
                    None => find_app(apps, &wanted).and_then(|app| Ok((app.id, resume(host, apps, app)?))),
                };
                match found {
                    Ok((id, resume)) => {
                        self.phase = Phase::Launched;
                        self.started = Instant::now();
                        Step::Launch(self.host, id, resume)
                    }
                    Err(why) => Step::Fail(why),
                }
            }
        }
    }

    /// After the launch was asked for: the stream starts, or the launch ends without one.
    fn launched(&mut self, seen: &Seen) -> Step {
        if let Some(message) = &seen.message {
            return Step::Fail(message.clone());
        }
        if seen.confirming {
            // The user takes their time; the launch that follows the answer gets the full limit.
            self.started = Instant::now();
            return Step::Wait;
        }
        if !seen.busy {
            // The launcher dropped the launch without saying why, or the user cancelled it.
            return if self.cancelled {
                Step::Close
            } else {
                Step::Fail("the stream did not start".into())
            };
        }
        if self.started.elapsed() > LAUNCH_LIMIT {
            return Step::Fail("the stream did not start in time".into());
        }
        Step::Wait
    }

    fn in_time(&self, step: Step) -> Step {
        if self.started.elapsed() > START_LIMIT {
            Step::Fail(format!(
                "the PC {} did not answer in time",
                self.launch.host
            ))
        } else {
            step
        }
    }

    pub fn streaming_began(&mut self) {
        self.phase = Phase::Streaming;
    }

    /// The user cancelled the launch: Nightjar closes once it has stopped.
    pub fn cancel(&mut self) {
        if self.phase == Phase::Launched {
            self.cancelled = true;
        }
    }

    /// The stream is over (no reconnect follows). True when Nightjar should close now;
    /// `message` is the error the launcher shows for it, if any.
    pub fn stream_ended(&mut self, message: Option<String>) -> bool {
        if self.phase != Phase::Streaming {
            return false;
        }
        self.phase = Phase::Idle;
        if !self.launch.quit_on_exit {
            return false;
        }
        if let Some(message) = message {
            self.fail(&message);
        }
        true
    }

    pub fn fail(&mut self, why: &str) {
        eprintln!("nightjar: --host start failed: {why}");
        self.failed = Some(why.to_owned());
    }
}
