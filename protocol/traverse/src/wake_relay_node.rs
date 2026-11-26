//! The away wake relay over sockets; [`crate::wake_relay`] decides, this drives it.
//!
//! [`WakeRelayPeer`] runs on an always-on device on the PC's network that the PC enrolled: per
//! PC and per device the PC lists, it publishes a [`Role::WakeRelay`] record (its public
//! endpoints and session) under that device's wake secret, looks for the device's
//! [`Role::WakeRequester`] record, punches back, and answers a sealed request with one burst of
//! magic packets on its own network ([`Lan`]) and the statuses that follow. A datagram that does
//! not open with a listed device's keys gets no answer at all. [`request_wake`]
//! ([`requester`]) is the device's side.
//!
//! It shares nothing with the internet play host's service ([`crate::host`]): no tunnel, no
//! port of the PC's, one socket of its own.

use crate::{
    node::{unix_now, Node},
    punch::{self, Puncher},
    record::{self, Keys, Nat, PathKeys, Record, Role},
    stun,
    wake_relay::{Burst, Decision, DeviceId, Enrollment, Interface, Relay, Status, WakeStatus},
    wire::{self, Kind, Opener, Opened, Sealer},
    Millis,
};
use std::{
    collections::HashMap,
    io,
    net::{SocketAddr, SocketAddrV4, TcpStream, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

pub mod requester;
pub use requester::{request_wake, Failure, Progress, RelayInfo, RequestConfig};

/// STUN again this often, as the internet play host does.
const STUN_EVERY: Millis = 5 * 60_000;
const STUN_RETRY: Millis = 30_000;
/// Republish a relay record this often, well inside the two hours DHT nodes keep it.
pub const PUBLISH_EVERY: Millis = 30 * 60_000;
const PUBLISH_RETRY: Millis = 60_000;
/// Look for each device's request this often: within a requester's punch budget.
const POLL_EVERY: Millis = punch::HOST_POLL_IDLE;
const MAX_POLLS: usize = 4;
/// A path nobody speaks on for this long is dropped.
const PATH_IDLE: Millis = (crate::wake_relay::BOOT_WAIT_SECONDS + 30) * 1000;
/// How often the relay looks whether the PC came up after a burst.
#[cfg(not(test))]
const PROBE_EVERY: Duration = Duration::from_secs(2);
#[cfg(test)]
const PROBE_EVERY: Duration = Duration::from_millis(50);
const PROBE_CONNECT: Duration = Duration::from_millis(800);

/// What the relay needs of the network it is on.
pub trait Lan: Send + Sync {
    /// This device's IPv4 interfaces that are up.
    fn interfaces(&self) -> Vec<Interface>;
    /// Sends the burst once; how many datagrams went.
    fn send(&self, burst: &Burst) -> usize;
    /// Whether the PC answers at `address` (its HTTP port takes a connection).
    fn is_up(&self, address: SocketAddrV4) -> bool;
}

/// A [`Lan`] on real sockets. Listing interfaces takes a crate each caller already has.
pub struct UdpLan {
    list: Box<dyn Fn() -> Vec<Interface> + Send + Sync>,
}

impl UdpLan {
    pub fn new(list: impl Fn() -> Vec<Interface> + Send + Sync + 'static) -> Self {
        Self {
            list: Box::new(list),
        }
    }
}

impl Lan for UdpLan {
    fn interfaces(&self) -> Vec<Interface> {
        (self.list)()
    }

    /// Each directed broadcast leaves from the interface on its subnet, bound to that
    /// interface's own address (never a wildcard); the limited broadcast leaves from each.
    fn send(&self, burst: &Burst) -> usize {
        let interfaces = self.interfaces();
        let mut sockets: HashMap<std::net::Ipv4Addr, UdpSocket> = HashMap::new();
        for (destination, _) in &burst.packets {
            if let Some(interface) = interfaces
                .iter()
                .find(|interface| interface.broadcast() == Some(*destination.ip()))
                && !sockets.contains_key(&interface.ip)
                && let Ok(socket) = UdpSocket::bind(SocketAddrV4::new(interface.ip, 0))
                && socket.set_broadcast(true).is_ok()
            {
                sockets.insert(interface.ip, socket);
            }
        }
        let per_repeat = burst
            .packets
            .len()
            .div_ceil(crate::wake_relay::BURST_REPEATS)
            .max(1);
        let mut sent = 0;
        for (index, (destination, packet)) in burst.packets.iter().enumerate() {
            if index > 0 && index % per_repeat == 0 {
                thread::sleep(Duration::from_millis(10));
            }
            let from: Vec<&UdpSocket> = if destination.ip().is_broadcast() {
                sockets.values().collect()
            } else {
                interfaces
                    .iter()
                    .filter(|interface| interface.broadcast() == Some(*destination.ip()))
                    .filter_map(|interface| sockets.get(&interface.ip))
                    .collect()
            };
            for socket in from {
                if socket.send_to(packet, destination).is_ok() {
                    sent += 1;
                }
            }
        }
        sent
    }

    fn is_up(&self, address: SocketAddrV4) -> bool {
        TcpStream::connect_timeout(&address.into(), PROBE_CONNECT).is_ok()
    }
}

#[derive(Clone, Debug)]
pub struct RelayConfig {
    /// Where the relay's socket binds (IPv4, port 0 for any).
    pub bind: SocketAddr,
    pub stun_servers: Vec<String>,
    pub bootstrap: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelayStatus {
    /// The socket's public addresses, as STUN servers see them.
    pub public: Vec<SocketAddr>,
    pub nat: Nat,
    pub stun_error: Option<String>,
    pub dht_error: Option<String>,
    /// PCs that enrolled this relay, and the devices they let wake them.
    pub targets: usize,
    pub devices: usize,
    /// Devices whose relay record the DHT took.
    pub published: usize,
    /// The last request's PC, unix time and outcome.
    pub last_wake: Option<(String, u64, Status)>,
}

struct DeviceState {
    target: String,
    device: DeviceId,
    keys: Keys,
    tag: [u8; 8],
    published: Option<(u64, Millis, Vec<SocketAddr>)>,
    publish_retry_at: Option<Millis>,
    last_poll: Option<Millis>,
    polling: bool,
    answered: Option<[u8; 16]>,
}

/// A punched path to one device's session.
struct Path {
    target: String,
    device: DeviceId,
    session: [u8; 16],
    puncher: Puncher,
    opener: Opener,
    sealer: Sealer,
    peer: Option<SocketAddr>,
    heard: Millis,
}

impl Path {
    fn new(state: &DeviceState, keys: PathKeys, session: [u8; 16], now: Millis) -> Self {
        Self {
            target: state.target.clone(),
            device: state.device,
            session,
            heard: now,
            puncher: Puncher::new(Vec::new(), now),
            opener: Opener::new(&keys.client_to_host, keys.tag),
            sealer: Sealer::new(&keys.host_to_client, keys.tag, session),
            peer: None,
        }
    }
}

struct Shared {
    node: Node,
    config: RelayConfig,
    session: [u8; 16],
    lan: Arc<dyn Lan>,
    enrollments: Mutex<Vec<Enrollment>>,
    devices: Mutex<Vec<DeviceState>>,
    /// Locked before `devices` when a thread needs both.
    paths: Mutex<HashMap<[u8; 8], Path>>,
    relay: Mutex<Relay>,
    status: Mutex<RelayStatus>,
    publishing: AtomicBool,
    stop: AtomicBool,
}

#[derive(Clone)]
pub struct WakeRelayPeer {
    shared: Arc<Shared>,
}

impl WakeRelayPeer {
    pub fn start(config: RelayConfig, lan: Arc<dyn Lan>) -> io::Result<Self> {
        let node = Node::bind(config.bind, config.bootstrap.clone())?;
        let shared = Arc::new(Shared {
            node: node.clone(),
            config,
            session: crate::crypto::random(),
            lan,
            enrollments: Mutex::new(Vec::new()),
            devices: Mutex::new(Vec::new()),
            paths: Mutex::new(HashMap::new()),
            relay: Mutex::new(Relay::new()),
            status: Mutex::new(RelayStatus::default()),
            publishing: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });
        let handling = Arc::downgrade(&shared);
        node.set_handler(Box::new(move |datagram, from| {
            if let Some(shared) = handling.upgrade() {
                WakeRelayPeer { shared }.receive(datagram, from);
            }
        }));
        let peer = Self { shared };
        let running = peer.clone();
        thread::Builder::new()
            .name("wake relay".into())
            .spawn(move || running.run())?;
        Ok(peer)
    }

    /// This relay's session nonce, which its records carry.
    pub fn session(&self) -> [u8; 16] {
        self.shared.session
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.shared.node.local_addr()
    }

    /// The PCs that enrolled this relay, as they are now: a device a PC no longer lists, or a
    /// whole PC, loses its records' answers and its open path at once.
    pub fn set_enrollments(&self, enrollments: Vec<Enrollment>) {
        let enrollments: Vec<Enrollment> =
            enrollments.into_iter().map(Enrollment::sanitized).collect();
        let mut paths = self.shared.paths.lock().unwrap();
        let mut states = self.shared.devices.lock().unwrap();
        let mut next = Vec::new();
        for enrollment in &enrollments {
            for device in &enrollment.devices {
                let keys = Keys::new(enrollment.device_secret(device));
                let tag = keys.device_tag(&self.shared.session);
                match states.iter().position(|state| state.tag == tag) {
                    Some(index) => next.push(states.swap_remove(index)),
                    None => next.push(DeviceState {
                        target: enrollment.target_id.clone(),
                        device: *device,
                        keys,
                        tag,
                        published: None,
                        publish_retry_at: None,
                        last_poll: None,
                        polling: false,
                        answered: None,
                    }),
                }
            }
        }
        paths.retain(|tag, _| next.iter().any(|state| state.tag == *tag));
        let mut status = self.shared.status.lock().unwrap();
        status.targets = enrollments.len();
        status.devices = next.len();
        *states = next;
        *self.shared.enrollments.lock().unwrap() = enrollments;
    }

    pub fn status(&self) -> RelayStatus {
        self.shared.status.lock().unwrap().clone()
    }

    pub fn stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.node.stop();
    }

    fn stopped(&self) -> bool {
        self.shared.stop.load(Ordering::Acquire)
    }

    fn send(&self, path: &Path, to: SocketAddr, kind: Kind, payload: &[u8]) {
        self.shared
            .node
            .send(to, &path.sealer.seal(kind, 0, payload));
    }

    /// Takes a datagram from the socket. Only one that opens with the keys of a device some PC
    /// lists gets anything back.
    fn receive(&self, datagram: &mut [u8], from: SocketAddr) {
        let Some(tag) = wire::tag(datagram) else {
            return;
        };
        let punched = wire::punch_session(datagram);
        let mut paths = self.shared.paths.lock().unwrap();
        if let Some(path) = paths.get_mut(&tag)
            && punched.is_none_or(|session| session == path.session)
        {
            if let Some(opened) = path.opener.open(datagram) {
                self.on_datagram(&mut paths, tag, opened, from);
            }
            return;
        }
        // A punch for a session of its own: a device starting a new request.
        let Some(session) = punched else { return };
        let devices = self.shared.devices.lock().unwrap();
        let Some(state) = devices.iter().find(|state| state.tag == tag) else {
            return;
        };
        let keys = state.keys.path(&self.shared.session, &session);
        let mut opener = Opener::new(&keys.client_to_host, keys.tag);
        let Some(opened) = opener.open(datagram) else {
            return;
        };
        // Only a fresh punch opens a path: a captured one replayed later starts nothing.
        let fresh = opened.kind == Kind::Punch
            && wire::punch_time(&opened.payload)
                .is_some_and(|time| unix_now().abs_diff(time) <= record::CLIENT_FRESH_SECONDS);
        if !fresh {
            return;
        }
        let now = self.shared.node.now();
        let mut path = Path::new(state, keys, session, now);
        drop(devices);
        path.opener = opener;
        paths.insert(tag, path);
        self.on_datagram(&mut paths, tag, opened, from);
    }

    fn on_datagram(
        &self,
        paths: &mut HashMap<[u8; 8], Path>,
        tag: [u8; 8],
        opened: Opened,
        from: SocketAddr,
    ) {
        let Some(path) = paths.get_mut(&tag) else {
            return;
        };
        path.heard = self.shared.node.now();
        path.peer = Some(from);
        match opened.kind {
            Kind::Punch => {
                path.puncher.on_punch(from);
                self.send(path, from, Kind::PunchAck, &[]);
            }
            Kind::PunchAck => path.puncher.on_ack(from),
            Kind::WakeRequest => self.on_request(path, tag, &opened.payload, from),
            Kind::Close => {
                paths.remove(&tag);
            }
            _ => {}
        }
    }

    fn on_request(&self, path: &Path, tag: [u8; 8], payload: &[u8], from: SocketAddr) {
        let Some(enrollment) = self
            .shared
            .enrollments
            .lock()
            .unwrap()
            .iter()
            .find(|enrollment| enrollment.target_id == path.target)
            .cloned()
        else {
            return;
        };
        let now = unix_now();
        let interfaces = self.shared.lan.interfaces();
        let decision = self.shared.relay.lock().unwrap().handle(
            &enrollment,
            &path.device,
            from,
            payload,
            now,
            &interfaces,
        );
        let reply = match decision {
            Decision::Silent => return,
            Decision::Reply(reply) => reply,
            Decision::Wake { reply, burst } => {
                self.shared.lan.send(&burst);
                self.watch(&enrollment, tag, path.session, reply.nonce);
                reply
            }
        };
        self.shared.status.lock().unwrap().last_wake =
            Some((enrollment.target_name.clone(), now, reply.status));
        self.send(path, from, Kind::WakeStatus, &reply.encode());
    }

    /// After a burst: looks whether the PC answers on its network, and tells the device once,
    /// when it does or when [`crate::wake_relay::BOOT_WAIT_SECONDS`] ran out.
    fn watch(&self, enrollment: &Enrollment, tag: [u8; 8], session: [u8; 16], nonce: [u8; 16]) {
        let target = enrollment
            .address
            .map(|address| SocketAddrV4::new(address, enrollment.http_port));
        let name = enrollment.target_name.clone();
        let watching = self.clone();
        let _ = thread::Builder::new()
            .name("wake relay watch".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let wait = Duration::from_secs(crate::wake_relay::BOOT_WAIT_SECONDS);
                let status = loop {
                    thread::sleep(PROBE_EVERY);
                    if watching.stopped() {
                        return;
                    }
                    if target.is_some_and(|target| watching.shared.lan.is_up(target)) {
                        break Status::HostAwake;
                    }
                    if started.elapsed() >= wait {
                        break Status::TimedOut;
                    }
                };
                let mut answer = WakeStatus::new(status, nonce);
                answer.elapsed_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
                watching.shared.status.lock().unwrap().last_wake =
                    Some((name, unix_now(), status));
                let paths = watching.shared.paths.lock().unwrap();
                if let Some(path) = paths.get(&tag).filter(|path| path.session == session)
                    && let Some(peer) = path.peer
                {
                    watching.send(path, peer, Kind::WakeStatus, &answer.encode());
                }
            });
    }

    fn run(&self) {
        let mut next_stun: Millis = 0;
        while !self.stopped() {
            let now = self.shared.node.now();
            if now >= next_stun && !self.shared.config.stun_servers.is_empty() {
                next_stun = now + if self.discover() { STUN_EVERY } else { STUN_RETRY };
            }
            self.serve_devices();
            self.drive_paths();
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn discover(&self) -> bool {
        let answers = self.shared.node.stun(&self.shared.config.stun_servers);
        let local = self.shared.node.local_addr().ok();
        let mut public: Vec<SocketAddr> = Vec::new();
        for answer in &answers {
            if !public.contains(answer) {
                public.push(*answer);
            }
        }
        let nat = local.map_or(Nat::Unknown, |local| {
            Nat::from_mapping(stun::mapping(local, &answers))
        });
        let mut status = self.shared.status.lock().unwrap();
        status.stun_error = answers.is_empty().then(|| {
            "no STUN server answered: this device may have no internet connection, or a firewall blocks UDP".to_owned()
        });
        status.public = public;
        status.nat = nat;
        !answers.is_empty()
    }

    /// Punches toward devices whose request record named this relay, and drops idle paths.
    fn drive_paths(&self) {
        let now = self.shared.node.now();
        let stamp = wire::punch_stamp(unix_now());
        let mut paths = self.shared.paths.lock().unwrap();
        paths.retain(|_, path| {
            path.puncher.state() != punch::State::Failed
                && now.saturating_sub(path.heard) < PATH_IDLE
        });
        for path in paths.values_mut() {
            for to in path.puncher.poll(now) {
                self.send(path, to, Kind::Punch, &stamp);
            }
        }
    }
}

mod worker;

#[cfg(test)]
mod tests;
