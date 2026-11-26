//! The device's side of an away wake: find the relay's record, publish its own, punch, send
//! one signed request (repeated as is, never with a new nonce, while no answer came) and read
//! the relay's statuses until a final one. It blocks; callers run it on a thread of their own.

use super::*;
use crate::{crypto::Key, wake_relay::WakeRequest};
use std::{sync::mpsc, time::Instant};

/// Where the relay is, when the caller already knows (a test, or a relay on the same network).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelayInfo {
    pub session: [u8; 16],
    pub endpoints: Vec<SocketAddr>,
    pub nat: Nat,
}

#[derive(Clone, Debug)]
pub struct RequestConfig {
    pub target_id: String,
    pub device: DeviceId,
    pub epoch: u64,
    /// This device's wake secret for this relay, as the PC handed it.
    pub secret: Key,
    pub known: Option<RelayInfo>,
    /// Where this device's socket binds (port 0 for any).
    pub bind: SocketAddr,
    pub stun_servers: Vec<String>,
    pub bootstrap: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Looking for the relay's record.
    LookingUp,
    /// Opening a path to the relay.
    Punching,
    /// The request is on its way.
    Requested,
    /// The relay answered.
    Status(WakeStatus),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The relay published no record lately: it is off or offline.
    RelayOffline,
    /// No path to the relay opened, or it never answered.
    RelayUnreachable,
    Cancelled,
    /// This device could not open its socket.
    Network(String),
}

/// A relay record older than this counts as a relay that is gone: one that runs republishes
/// every [`PUBLISH_EVERY`].
const RECORD_STALE: u64 = 2 * PUBLISH_EVERY / 1000;
const RESEND_EVERY: Duration = Duration::from_secs(1);
const SENDS: u32 = 3;
/// After the last send, how long the first answer may take.
const ANSWER_WAIT: Duration = Duration::from_secs(3);
/// After a burst, how long the final status may take.
const FINAL_WAIT: Duration =
    Duration::from_secs(crate::wake_relay::BOOT_WAIT_SECONDS + 10);

/// Asks the relay to wake the PC, reporting `progress`, until its final status (`Ok`): the PC
/// came up ([`Status::HostAwake`]) or did not ([`Status::TimedOut`]), or the relay refused. A
/// [`Status::PacketSent`] with no final status after it comes back as is.
pub fn request_wake(
    config: RequestConfig,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<WakeStatus, Failure> {
    let node = Node::bind(config.bind, config.bootstrap.clone())
        .map_err(|error| Failure::Network(error.to_string()))?;
    let result = run(&node, &config, cancel, progress);
    node.stop();
    result
}

fn look_up(node: &Node, keys: &Keys) -> Option<Record> {
    let window = record::window(unix_now());
    [window, window.saturating_sub(1)]
        .into_iter()
        .find_map(|window| {
            let (item, _) = node.get(
                &keys.public(Role::WakeRelay),
                &keys.salt(Role::WakeRelay, window),
            );
            item.and_then(|item| Record::from_item(keys, Role::WakeRelay, window, &item))
        })
        .filter(|record| unix_now().saturating_sub(record.time) <= RECORD_STALE)
}

fn run(
    node: &Node,
    config: &RequestConfig,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<WakeStatus, Failure> {
    let keys = Keys::new(config.secret);
    progress(Progress::LookingUp);
    let stun = {
        let (node, servers) = (node.clone(), config.stun_servers.clone());
        thread::spawn(move || {
            let answers = node.stun(&servers);
            let local = node.local_addr().ok();
            let nat = local.map_or(Nat::Unknown, |local| {
                Nat::from_mapping(stun::mapping(local, &answers))
            });
            let mut public: Vec<SocketAddr> = Vec::new();
            for answer in answers {
                if !public.contains(&answer) {
                    public.push(answer);
                }
            }
            (public, nat)
        })
    };
    let found = (!config.bootstrap.is_empty())
        .then(|| look_up(node, &keys))
        .flatten()
        .map(|record| RelayInfo {
            session: record.session,
            endpoints: record.endpoints,
            nat: record.nat,
        });
    let relay = found
        .or_else(|| config.known.clone())
        .ok_or(Failure::RelayOffline)?;
    if cancel.load(Ordering::Acquire) {
        return Err(Failure::Cancelled);
    }
    let session: [u8; 16] = crate::crypto::random();
    let (public, nat) = stun.join().unwrap_or_default();
    if !public.is_empty() && !config.bootstrap.is_empty() {
        let item = Record {
            role: Role::WakeRequester,
            time: unix_now(),
            session,
            peer_session: relay.session,
            nat,
            endpoints: public,
            ports: None,
        }
        .item(&keys, record::window(unix_now()), crate::crypto::random());
        let publishing = node.clone();
        let _ = thread::Builder::new()
            .name("wake request publish".into())
            .spawn(move || publishing.put(&item));
    }
    progress(Progress::Punching);
    let path = keys.path(&relay.session, &session);
    let sealer = Arc::new(Sealer::new(&path.client_to_host, path.tag, session));
    let (opened_tx, opened) = mpsc::channel::<(Kind, Vec<u8>, SocketAddr)>();
    let opener = Mutex::new(Opener::new(&path.host_to_client, path.tag));
    {
        let (answering, sealer) = (node.clone(), sealer.clone());
        node.set_handler(Box::new(move |datagram, from| {
            let Some(opened) = opener.lock().unwrap().open(datagram) else {
                return;
            };
            if opened.kind == Kind::Punch {
                answering.send(from, &sealer.seal(Kind::PunchAck, 0, &[]));
            }
            let _ = opened_tx.send((opened.kind, opened.payload, from));
        }));
    }
    let now = node.now();
    let mut puncher = Puncher::new(punch::candidates(&relay.endpoints, relay.nat), now);
    puncher.hold_until(punch::device_deadline(now));
    let request = WakeRequest::new(&config.target_id, config.device, config.epoch, unix_now())
        .encode(&config.secret);
    let nonce = WakeRequest::parse(&request).expect("its own request").nonce;
    let mut sends = 0;
    let mut last_send: Option<Instant> = None;
    let mut sent_burst: Option<(WakeStatus, Instant)> = None;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(Failure::Cancelled);
        }
        let now = node.now();
        for to in puncher.poll(now) {
            node.send(to, &sealer.seal(Kind::Punch, 0, &wire::punch_stamp(unix_now())));
        }
        if puncher.state() == punch::State::Failed {
            return Err(Failure::RelayUnreachable);
        }
        while let Ok((kind, payload, from)) = opened.try_recv() {
            match kind {
                Kind::Punch => puncher.on_punch(from),
                Kind::PunchAck => puncher.on_ack(from),
                Kind::WakeStatus => {
                    let Some(status) = WakeStatus::decode(&payload).filter(|s| s.nonce == nonce)
                    else {
                        continue;
                    };
                    progress(Progress::Status(status));
                    if status.status.is_final() {
                        return Ok(status);
                    }
                    sent_burst.get_or_insert((status, Instant::now()));
                }
                _ => {}
            }
        }
        if let punch::State::Connected(relay) = puncher.state() {
            match (&sent_burst, last_send) {
                (Some((status, at)), _) => {
                    if at.elapsed() >= FINAL_WAIT {
                        return Ok(*status);
                    }
                }
                (None, Some(at)) if sends >= SENDS && at.elapsed() >= ANSWER_WAIT => {
                    return Err(Failure::RelayUnreachable);
                }
                (None, at) if sends < SENDS && at.is_none_or(|at| at.elapsed() >= RESEND_EVERY) => {
                    if sends == 0 {
                        progress(Progress::Requested);
                    }
                    // The same request each time: the relay answers a repeat, never wakes twice.
                    node.send(relay, &sealer.seal(Kind::WakeRequest, 0, &request));
                    sends += 1;
                    last_send = Some(Instant::now());
                }
                _ => {}
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}
