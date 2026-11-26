//! A relay's side of the away wake relay, without sockets: which paired PCs it asks for an
//! assignment and when, and what it keeps of their answers (`docs/internet-play.md`, "Away wake
//! relay"). The desktop's `nightjar wake-relay` drives it: it asks each PC [`Driver::due`] names,
//! hands [`Driver::on_result`] the answer, gives [`Driver::enrollments`] to
//! [`WakeRelayPeer::set_enrollments`] and saves [`Driver::saved`] ([`super::saved`]) after each
//! change.
//!
//! - A PC that cannot be reached keeps its enrollment: asleep is when the relay is needed.
//! - A PC that does not take the relay for now (403 `forbidden`: Away wake relay or Play over
//!   the internet off on it, or the relay not on its network) is no longer served at once, but
//!   its assignment is kept, saved with the PC's words, and refreshed on its interval; the first
//!   refresh that succeeds serves it again, without `--enroll`. A relay started again in between
//!   does not serve it either.
//! - A PC that refuses the relay for good (401 `not_paired`, 404 `not_found`: it removed the
//!   relay, `host_control_required`, `permission_required`) drops the enrollment at once, and
//!   the refusal is saved, so a relay started again does not enroll itself with a PC that
//!   revoked it; only an explicit enroll does ([`Driver::new`] with `enroll_again`,
//!   `nightjar wake-relay --enroll`).
//! - Saved, a refusal without an assignment is for good, and one with an assignment for now.
//! - A PC is asked again every `refresh_seconds` it gave (one minute to an hour), and one that
//!   could not be reached or failed as often, never sooner.
//! - Neither an assignment's secret nor a device id is in any `Debug` or note.
use super::saved::{Saved, SavedHost};
use crate::http::Error;
use crate::http::native_api::capabilities::Capabilities;
use crate::http::native_api::error::ErrorCode;
use crate::http::native_api::feature;
use crate::http::native_api::wake_relay::{RelayAssignment, RelayEnroll, RelayEnrolled};
use std::fmt;

/// What a relay's IO loop runs with, so its app needs no crate of its own for them.
pub use rubylight_traverse::record::Nat;
pub use rubylight_traverse::wake_relay::{Enrollment, Interface, Status, is_lan};
pub use rubylight_traverse::wake_relay_node::{RelayConfig, RelayStatus, UdpLan, WakeRelayPeer};

/// How often a PC is asked when it gave no other interval, or never answered.
pub const DEFAULT_REFRESH_SECONDS: u64 = 300;
const MIN_REFRESH_SECONDS: u64 = 60;
const MAX_REFRESH_SECONDS: u64 = 3600;
/// A PC's reason is shown as it came, cut to this.
const MAX_REASON_CHARS: usize = 200;

/// The relay's socket on every IPv4 interface, any port, with the default STUN servers and
/// DHT routers: it has to reach them and the devices away, as the PC's internet play does.
pub fn relay_config() -> RelayConfig {
    RelayConfig {
        bind: std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, 0)),
        stun_servers: rubylight_traverse::STUN_SERVERS
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        bootstrap: rubylight_traverse::DHT_BOOTSTRAP
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
    }
}

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// What the relay asks a PC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    /// Enroll, after the PC's capabilities say it offers a relay to this device.
    Enroll,
    /// Fetch the assignment again; never enrolls.
    Refresh,
}

impl Ask {
    /// The request body; `name` is how the PC lists this relay, the device's name without it.
    pub fn request(self, name: Option<&str>) -> RelayEnroll {
        match self {
            Ask::Enroll => RelayEnroll::Enroll {
                name: name.map(str::to_owned),
            },
            Ask::Refresh => RelayEnroll::Refresh,
        }
    }
}

/// What asking one PC came to.
#[derive(Clone, PartialEq, Eq)]
pub enum Outcome {
    Assigned(Box<RelayAssignment>),
    /// No answer: asleep, off, or not on this network. The enrollment stays.
    Unreachable(String),
    /// The PC does not take this relay for now: Away wake relay or Play over the internet is off
    /// on it, or the relay is not on its network (403 `forbidden`), or, before enrolling, its
    /// capabilities offer no relay to this device. Asked again later. Before enrolling nothing
    /// is saved; an enrolled relay stops serving the PC but keeps and refreshes its assignment.
    NotOffered(String),
    /// The PC refused this relay for good: it removed the relay (404 `not_found`), the pairing
    /// ended (401 `not_paired`), or Host control was taken away (`host_control_required`,
    /// `permission_required`). The enrollment goes, and the refusal is saved.
    Refused(String),
    /// Anything else (the PC busy, a reply that cannot be read). The enrollment stays.
    Failed(String),
}

impl fmt::Debug for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Assigned(assignment) => f
                .debug_struct("Assigned")
                .field("target_id", &assignment.target_id)
                .field("epoch", &assignment.epoch)
                .field("devices", &assignment.devices.len())
                .finish_non_exhaustive(),
            Outcome::Unreachable(why) => f.debug_tuple("Unreachable").field(why).finish(),
            Outcome::NotOffered(why) => f.debug_tuple("NotOffered").field(why).finish(),
            Outcome::Refused(why) => f.debug_tuple("Refused").field(why).finish(),
            Outcome::Failed(why) => f.debug_tuple("Failed").field(why).finish(),
        }
    }
}

/// `text` on one line, without control characters, at most [`MAX_REASON_CHARS`] long: a PC's
/// words go to a terminal or a journal.
fn plain(text: &str) -> String {
    let mut out: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_REASON_CHARS)
        .collect();
    if text.chars().count() > MAX_REASON_CHARS {
        out.push('…');
    }
    out.trim().to_owned()
}

/// How a native API answer refuses this relay.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// Removed, not paired, or without Host control: only `--enroll` asks again.
    ForGood,
    /// `forbidden`: the PC's setting is off or the relay is not on its network, while the PC
    /// still lists the relay.
    ForNow,
}

/// Whether a native API answer refuses this relay, and for how long: by its code, and by its
/// status when the code says nothing more.
fn refusal(status: u16, code: ErrorCode) -> Option<Refusal> {
    match code {
        ErrorCode::NotFound
        | ErrorCode::NotPaired
        | ErrorCode::HostControlRequired
        | ErrorCode::PermissionRequired => Some(Refusal::ForGood),
        ErrorCode::Forbidden => Some(Refusal::ForNow),
        _ => match status {
            401 | 404 => Some(Refusal::ForGood),
            403 => Some(Refusal::ForNow),
            _ => None,
        },
    }
}

/// What a failed request means for the relay.
pub fn of_error(error: &Error) -> Outcome {
    match error {
        Error::Io(_) | Error::Tls(_) | Error::CertificateMismatch | Error::Cancelled => {
            Outcome::Unreachable(plain(&error.to_string()))
        }
        Error::Api { status, error: api } => match refusal(*status, api.code) {
            Some(Refusal::ForGood) => Outcome::Refused(plain(&api.message)),
            Some(Refusal::ForNow) => Outcome::NotOffered(plain(&api.message)),
            None => Outcome::Failed(plain(&error.to_string())),
        },
        Error::Status { code: 401, .. } | Error::NotPaired => {
            Outcome::Refused("the PC no longer knows this device: it is not paired".into())
        }
        Error::Status { code: 403, .. } => {
            Outcome::NotOffered("the PC does not take this wake relay now".into())
        }
        Error::Status { code: 404, .. } | Error::NotFound(_) => {
            Outcome::Refused("the PC has no wake relay for this device".into())
        }
        other => Outcome::Failed(plain(&other.to_string())),
    }
}

/// What an `enroll` or `refresh` answer means for the relay.
pub fn outcome(result: Result<RelayEnrolled, Error>) -> Outcome {
    match result {
        Ok(RelayEnrolled {
            assignment: Some(assignment),
            ..
        }) => Outcome::Assigned(Box::new(assignment)),
        Ok(_) => Outcome::Failed("the PC answered without an assignment".into()),
        Err(error) => of_error(&error),
    }
}

/// Whether the PC's capabilities let this device enroll: `Ok` to go on, or what asking came to.
pub fn offered(result: Result<Capabilities, Error>) -> Result<(), Outcome> {
    let capabilities = match result {
        Ok(capabilities) => capabilities,
        Err(error) => {
            return Err(match of_error(&error) {
                Outcome::Refused(why) => Outcome::NotOffered(why),
                other => other,
            });
        }
    };
    if !capabilities
        .features
        .iter()
        .any(|f| f == feature::WAKE_RELAY)
    {
        return Err(Outcome::NotOffered(
            "Away wake relay is off on the PC: turn it on, with Play over the internet, in its console".into(),
        ));
    }
    if !capabilities.permissions.host_control {
        return Err(Outcome::NotOffered(
            "turn on Host control for this device in the PC's console".into(),
        ));
    }
    Ok(())
}

/// A change worth a line, for one PC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// Enrolled, or served again after a refusal for now; this many devices may wake the PC.
    Enrolled {
        devices: usize,
    },
    /// The PC changed its devices or its keys.
    Changed {
        devices: usize,
        epoch: u64,
    },
    /// It answers again after it could not be reached.
    Back,
    Unreachable(String),
    /// The PC does not take the relay now; it is not served until the PC does.
    NotOffered(String),
    Refused(String),
    Failed(String),
    /// No longer saved or paired here: its enrollment is dropped, as it cannot be refreshed.
    Forgotten,
}

/// One PC as the relay knows it, for a summary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostView {
    pub id: String,
    pub name: String,
    /// The devices that may wake the PC, while it is enrolled and served.
    pub devices: Option<usize>,
    /// Why the PC refused this relay for good.
    pub refused: Option<String>,
    /// Why the PC does not take this relay for now: it is not served until a refresh succeeds.
    pub off: Option<String>,
}

struct Host {
    id: String,
    name: String,
    /// Whether the PC is saved and paired here now.
    listed: bool,
    assignment: Option<RelayAssignment>,
    enrollment: Option<Enrollment>,
    /// Refused for good; never with an assignment.
    refused: Option<String>,
    /// Refused for now (`forbidden`), in the PC's words; only with an assignment, which is kept
    /// and refreshed but not served.
    off: Option<String>,
    /// The next ask enrolls even with an assignment (`--enroll`).
    enroll: bool,
    next: u64,
    /// `None` before its first answer.
    reachable: Option<bool>,
    /// The last NotOffered or Failed reason shown, so it is not shown at every ask.
    shown: Option<String>,
}

impl Host {
    fn interval(&self) -> u64 {
        self.assignment
            .as_ref()
            .map_or(DEFAULT_REFRESH_SECONDS, refresh_seconds)
    }

    /// A problem's note, once until another comes.
    fn problem(&mut self, note: Note, why: String) -> Option<Note> {
        if self.shown.as_ref() == Some(&why) {
            return None;
        }
        self.shown = Some(why);
        Some(note)
    }
}

fn refresh_seconds(assignment: &RelayAssignment) -> u64 {
    u64::from(assignment.refresh_seconds).clamp(MIN_REFRESH_SECONDS, MAX_REFRESH_SECONDS)
}

/// The enrollment an assignment from the PC `id` gives, if it is for that PC and well formed.
fn usable(id: &str, assignment: &RelayAssignment) -> Option<Enrollment> {
    if !assignment.target_id.eq_ignore_ascii_case(id) {
        return None;
    }
    super::enrollment(assignment)
}

/// Every PC's state, and what the relay asks next.
pub struct Driver {
    hosts: Vec<Host>,
    changed: bool,
}

impl Driver {
    /// Starts from the saved file. `enroll_again` (`--enroll`) forgets every refusal for good and
    /// enrolls with every PC again. A saved assignment that is not usable is dropped, with a
    /// refusal for now that came with it. One refused for now stays unserved until a refresh or
    /// an enroll succeeds.
    pub fn new(saved: Saved, enroll_again: bool) -> Self {
        let mut changed = false;
        let mut hosts: Vec<Host> = Vec::new();
        for entry in saved.hosts {
            if entry.host.is_empty() || hosts.iter().any(|host| host.id == entry.host) {
                changed = true;
                continue;
            }
            let enrollment = entry
                .assignment
                .as_ref()
                .and_then(|assignment| usable(&entry.host, assignment));
            changed |= entry.assignment.is_some() && enrollment.is_none();
            // Saved with an assignment, a refusal is for now; without one, for good.
            let (refused, off) = if entry.assignment.is_some() {
                (None, entry.refused.filter(|_| enrollment.is_some()))
            } else {
                changed |= enroll_again && entry.refused.is_some();
                (entry.refused.filter(|_| !enroll_again), None)
            };
            if enrollment.is_none() && refused.is_none() {
                continue;
            }
            hosts.push(Host {
                listed: enrollment.is_some(),
                assignment: enrollment.as_ref().and(entry.assignment),
                enrollment,
                id: entry.host,
                name: entry.name,
                refused,
                off,
                enroll: enroll_again,
                next: 0,
                reachable: None,
                shown: None,
            });
        }
        Self { hosts, changed }
    }

    /// The PCs saved and paired here now, by unique id and name. A new one is asked at once; one
    /// gone loses its enrollment, and its refusal stays.
    pub fn set_hosts(&mut self, paired: &[(String, String)]) -> Vec<(String, Note)> {
        let mut notes = Vec::new();
        for host in &mut self.hosts {
            let now_listed = paired.iter().find(|(id, _)| *id == host.id);
            if let Some((_, name)) = now_listed
                && *name != host.name
            {
                host.name = name.clone();
                self.changed = true;
            }
            if now_listed.is_none() && host.assignment.is_some() {
                host.assignment = None;
                host.enrollment = None;
                host.off = None;
                self.changed = true;
                notes.push((host.name.clone(), Note::Forgotten));
            }
            if now_listed.is_some() && !host.listed {
                host.next = 0;
            }
            host.listed = now_listed.is_some();
        }
        self.hosts
            .retain(|host| host.listed || host.assignment.is_some() || host.refused.is_some());
        for (id, name) in paired {
            if id.is_empty() || self.hosts.iter().any(|host| host.id == *id) {
                continue;
            }
            self.hosts.push(Host {
                id: id.clone(),
                name: name.clone(),
                listed: true,
                assignment: None,
                enrollment: None,
                refused: None,
                off: None,
                enroll: false,
                next: 0,
                reachable: None,
                shown: None,
            });
        }
        notes
    }

    /// The PCs to ask now, and what. A PC that refused this relay for good is never asked; one
    /// that refused it for now is, on its interval.
    pub fn due(&self, now: u64) -> Vec<(String, Ask)> {
        self.hosts
            .iter()
            .filter(|host| host.listed && host.refused.is_none() && host.next <= now)
            .map(|host| {
                let ask = if host.assignment.is_some() && !host.enroll {
                    Ask::Refresh
                } else {
                    Ask::Enroll
                };
                (host.id.clone(), ask)
            })
            .collect()
    }

    /// Takes what asking the PC `id` came to at `now`; a note when it is worth a line.
    pub fn on_result(&mut self, id: &str, outcome: Outcome, now: u64) -> Option<Note> {
        let host = self.hosts.iter_mut().find(|host| host.id == id)?;
        let back = host.reachable == Some(false);
        match outcome {
            Outcome::Assigned(assignment) => {
                let Some(enrollment) = usable(&host.id, &assignment) else {
                    host.next = now + host.interval();
                    host.reachable = Some(true);
                    let why = "the PC's assignment cannot be used".to_owned();
                    return host.problem(Note::Failed(why.clone()), why);
                };
                let assignment = *assignment;
                host.next = now + refresh_seconds(&assignment);
                host.enroll = false;
                host.reachable = Some(true);
                host.shown = None;
                // Taken again after a refusal for now: served again.
                let resumed = host.off.take().is_some();
                let devices = assignment.devices.len();
                let note = match &host.assignment {
                    None => Some(Note::Enrolled { devices }),
                    _ if resumed => Some(Note::Enrolled { devices }),
                    Some(old) if old.devices.len() != devices || old.epoch != assignment.epoch => {
                        Some(Note::Changed {
                            devices,
                            epoch: assignment.epoch,
                        })
                    }
                    _ if back => Some(Note::Back),
                    _ => None,
                };
                if resumed || host.assignment.as_ref() != Some(&assignment) {
                    self.changed = true;
                }
                host.assignment = Some(assignment);
                host.enrollment = Some(enrollment);
                note
            }
            Outcome::Unreachable(why) => {
                host.next = now + host.interval();
                host.reachable = Some(false);
                host.shown = None;
                (!back).then_some(Note::Unreachable(why))
            }
            Outcome::Refused(why) => Self::refuse(host, why, &mut self.changed),
            // An enrolled relay the PC does not take for now: not served, but kept and refreshed
            // on the PC's interval, so it is served again once the PC takes it.
            Outcome::NotOffered(why) if host.assignment.is_some() => {
                host.next = now + host.interval();
                host.reachable = Some(true);
                if host.off.as_ref() == Some(&why) {
                    return None;
                }
                host.off = Some(why.clone());
                self.changed = true;
                Some(Note::NotOffered(why))
            }
            Outcome::NotOffered(why) => {
                host.next = now + DEFAULT_REFRESH_SECONDS;
                host.reachable = Some(true);
                host.problem(Note::NotOffered(why.clone()), why)
            }
            Outcome::Failed(why) => {
                host.next = now + host.interval();
                host.reachable = Some(true);
                host.problem(Note::Failed(why.clone()), why)
            }
        }
    }

    fn refuse(host: &mut Host, why: String, changed: &mut bool) -> Option<Note> {
        host.assignment = None;
        host.enrollment = None;
        host.off = None;
        host.enroll = false;
        host.reachable = Some(true);
        host.refused = Some(why.clone());
        *changed = true;
        Some(Note::Refused(why))
    }

    /// What the relay serves now: one enrollment per PC listed and enrolled that takes it.
    pub fn enrollments(&self) -> Vec<Enrollment> {
        self.hosts
            .iter()
            .filter(|host| host.listed && host.off.is_none())
            .filter_map(|host| host.enrollment.clone())
            .collect()
    }

    /// What to save: every PC enrolled or refused. A refusal for now is saved with the
    /// assignment it keeps, one for good without.
    pub fn saved(&self) -> Saved {
        Saved {
            hosts: self
                .hosts
                .iter()
                .filter(|host| host.assignment.is_some() || host.refused.is_some())
                .map(|host| SavedHost {
                    host: host.id.clone(),
                    name: host.name.clone(),
                    assignment: host.assignment.clone(),
                    refused: host.refused.clone().or_else(|| host.off.clone()),
                })
                .collect(),
        }
    }

    /// Whether the enrollments or what is saved changed since last asked.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Every PC the relay knows, enrolled or refused, for a summary.
    pub fn hosts(&self) -> Vec<HostView> {
        self.hosts
            .iter()
            .map(|host| HostView {
                id: host.id.clone(),
                name: host.name.clone(),
                devices: host
                    .assignment
                    .as_ref()
                    .filter(|_| host.off.is_none())
                    .map(|a| a.devices.len()),
                refused: host.refused.clone(),
                off: host.off.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
