//! Away wake relay (`docs/designs/15-away-wake-relay.md`): a device away from home asks an
//! always-on device on the PC's network, one the PC enrolled as its relay, to send Wake-on-LAN
//! to that PC, over a punched path like internet play's. This is the part both ends share, and
//! it decides without a socket:
//!
//! - the keys: when a relay enrolls, the PC gives it a secret of its own; every paired device
//!   gets, per relay, a secret derived from it and bound to the PC, the device and the PC's key
//!   epoch ([`device_secret`]). The relay derives the same for each device the PC lists, so a
//!   device the PC no longer lists, or a secret of an older epoch, opens nothing. Each device's
//!   secret keys its records ([`crate::record::Role::WakeRelay`] and
//!   [`crate::record::Role::WakeRequester`]) and the punched path, as a paired device's secret
//!   does for internet play.
//! - the request a device sends ([`WakeRequest`], 148 bytes, signed) and the status a relay
//!   answers ([`WakeStatus`], at most 88 bytes: shorter than any request, so a relay amplifies
//!   nothing it is sent).
//! - the relay's checks, in order ([`Relay::handle`]): the device is listed, the signature, the
//!   device, the PC and the epoch the request names, the clock (within two minutes), the nonce
//!   (never seen in that time), and the rate (one burst per ten seconds). Then exactly one
//!   bounded burst of magic packets, only to the MACs the PC registered, and only to broadcast
//!   addresses of private subnets this relay is on itself ([`lan_destinations`]).
//!
//! A datagram that fails the path's authentication never gets here: the path drops it without
//! a byte in answer ([`crate::wire::Opener`], [`crate::wake_relay_node`]).

use crate::crypto::{self, Key};
use std::{
    collections::{HashMap, VecDeque},
    fmt,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

pub const VERSION: u8 = 1;
pub const REQUEST_BYTES: usize = 148;
/// A status without its optional detail; this version sends none.
pub const STATUS_BYTES: usize = 24;
pub const DETAIL_MAX_BYTES: usize = 64;
pub const STATUS_MAX_BYTES: usize = STATUS_BYTES + DETAIL_MAX_BYTES;
const _: () = assert!(STATUS_MAX_BYTES < REQUEST_BYTES);
/// How far a request's clock may be from the relay's.
pub const CLOCK_SKEW_SECONDS: u64 = 120;
/// The most nonces a relay remembers; when all are still fresh, it takes no new request.
pub const REPLAY_ENTRIES: usize = 512;
/// At most one burst per device and PC this often, and per PC from any device.
pub const BURST_GAP_SECONDS: u64 = 10;
pub const BURST_WINDOW_SECONDS: u64 = 600;
/// At most this many bursts per device and PC in [`BURST_WINDOW_SECONDS`],
pub const BURSTS_PER_WINDOW: usize = 10;
/// and per PC from all its devices together.
pub const TARGET_BURSTS_PER_WINDOW: usize = 20;
pub const MAX_MACS: usize = 4;
pub const MAX_BROADCASTS: usize = 4;
/// Most devices one enrollment lists.
pub const MAX_DEVICES: usize = 64;
/// A burst repeats each magic packet this often, as Nightjar's own LAN wake does.
pub const BURST_REPEATS: usize = 3;
pub const WOL_PORT: u16 = 9;
pub const MAGIC_BYTES: usize = 102;
/// The most datagrams one burst sends: per MAC, the registered broadcasts and the limited one.
pub const MAX_BURST_PACKETS: usize = MAX_MACS * (MAX_BROADCASTS + 1) * BURST_REPEATS;
/// How long a relay watches for the PC to come up after a burst.
pub const BOOT_WAIT_SECONDS: u64 = 45;
/// How often one request's status is sent again when the device repeats it (a lost answer).
pub const MAX_RESENDS: u8 = 3;
const SIGNED: &[u8] = b"rl-wake-v1";

/// A paired device: the SHA-256 of its certificate, as the host names devices.
pub type DeviceId = [u8; 32];
/// A PC: the first 16 bytes of the SHA-256 of its unique id.
pub type TargetHash = [u8; 16];

/// The id the host gives a paired device: the SHA-256 of its PEM certificate, trimmed.
pub fn device_id(certificate: &str) -> DeviceId {
    crypto::sha256(&[certificate.trim().as_bytes()])
}

pub fn target_hash(target_id: &str) -> TargetHash {
    let digest = crypto::sha256(&[target_id.trim().as_bytes()]);
    let mut hash = [0; 16];
    hash.copy_from_slice(&digest[..16]);
    hash
}

/// The wake secret of `device` for the relay holding `relay_secret`, for the PC `target_id` in
/// key epoch `epoch`. The PC hands it to the device; the relay derives it again.
pub fn device_secret(relay_secret: &Key, target_id: &str, device: &DeviceId, epoch: u64) -> Key {
    let target = target_id.trim().as_bytes();
    let length = u16::try_from(target.len()).unwrap_or(u16::MAX);
    let mut context = Vec::with_capacity(2 + target.len() + 32 + 8);
    context.extend_from_slice(&length.to_be_bytes());
    context.extend_from_slice(&target[..usize::from(length)]);
    context.extend_from_slice(device);
    context.extend_from_slice(&epoch.to_be_bytes());
    crypto::derive_key(relay_secret, b"wake relay device secret", &context)
}

fn signing_seed(secret: &Key) -> Key {
    crypto::derive_key(secret, b"wake request signing", &[])
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn parse_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let text = text.trim().as_bytes();
    if text.len() != N * 2 {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = [0; N];
    for (index, [high, low]) in text.as_chunks::<2>().0.iter().enumerate() {
        out[index] = digit(*high)? << 4 | digit(*low)?;
    }
    Some(out)
}

/// A request to wake a PC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakeRequest {
    pub epoch: u64,
    /// The device's clock, unix seconds.
    pub time: u64,
    /// Fresh per request; repeating one asks again for its status, never for another burst.
    pub nonce: [u8; 16],
    pub device: DeviceId,
    pub target: TargetHash,
}

const BODY_BYTES: usize = 84;

impl WakeRequest {
    pub fn new(target_id: &str, device: DeviceId, epoch: u64, time: u64) -> Self {
        Self {
            epoch,
            time,
            nonce: crypto::random(),
            device,
            target: target_hash(target_id),
        }
    }

    fn body(&self) -> [u8; BODY_BYTES] {
        let mut body = [0; BODY_BYTES];
        body[0] = VERSION;
        // body[1] flags and body[2..4] reserved stay 0 in this version.
        body[4..12].copy_from_slice(&self.epoch.to_le_bytes());
        body[12..20].copy_from_slice(&self.time.to_le_bytes());
        body[20..36].copy_from_slice(&self.nonce);
        body[36..68].copy_from_slice(&self.device);
        body[68..84].copy_from_slice(&self.target);
        body
    }

    /// The request signed with the device's wake secret for this relay.
    pub fn encode(&self, secret: &Key) -> [u8; REQUEST_BYTES] {
        let body = self.body();
        let signature = crypto::sign(&signing_seed(secret), &[SIGNED, &body].concat());
        let mut out = [0; REQUEST_BYTES];
        out[..BODY_BYTES].copy_from_slice(&body);
        out[BODY_BYTES..].copy_from_slice(&signature);
        out
    }

    /// The fields of a request of this version, before its signature is checked.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != REQUEST_BYTES || bytes[0] != VERSION {
            return None;
        }
        let field = |range: std::ops::Range<usize>| &bytes[range];
        Some(Self {
            epoch: u64::from_le_bytes(field(4..12).try_into().unwrap()),
            time: u64::from_le_bytes(field(12..20).try_into().unwrap()),
            nonce: field(20..36).try_into().unwrap(),
            device: field(36..68).try_into().unwrap(),
            target: field(68..84).try_into().unwrap(),
        })
    }

    /// Whether `bytes` carry a valid signature by the holder of `secret`.
    pub fn signed_by(bytes: &[u8], secret: &Key) -> bool {
        if bytes.len() != REQUEST_BYTES {
            return false;
        }
        let public = crypto::public_key(&signing_seed(secret));
        let message = [SIGNED, &bytes[..BODY_BYTES]].concat();
        crypto::verify(&public, &message, &bytes[BODY_BYTES..])
    }
}

/// What a relay did with a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The magic packets went out on the PC's network.
    PacketSent = 1,
    /// The relay watches for the PC to come up.
    WaitingForBoot = 2,
    /// The PC answers on its network.
    HostAwake = 3,
    /// The PC did not answer within [`BOOT_WAIT_SECONDS`].
    TimedOut = 4,
    /// Too many wakes lately; try again after `retry_after` seconds.
    RateLimited = 5,
    /// The relay does not take this device's request: it is not (or no longer) allowed to wake
    /// the PC, or its wake access is from an older key epoch.
    RejectedAuth = 6,
    /// A request seen before, or a clock more than two minutes off.
    RejectedReplay = 7,
    /// The relay is not on the PC's network.
    TargetNotOnLan = 8,
}

impl Status {
    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            1 => Status::PacketSent,
            2 => Status::WaitingForBoot,
            3 => Status::HostAwake,
            4 => Status::TimedOut,
            5 => Status::RateLimited,
            6 => Status::RejectedAuth,
            7 => Status::RejectedReplay,
            8 => Status::TargetNotOnLan,
            _ => return None,
        })
    }

    /// Whether the relay sends nothing more for the request after this.
    pub fn is_final(self) -> bool {
        !matches!(self, Status::PacketSent | Status::WaitingForBoot)
    }

    /// Its name in JSON and across JNI.
    pub fn key(self) -> &'static str {
        match self {
            Status::PacketSent => "packet_sent",
            Status::WaitingForBoot => "waiting_for_boot",
            Status::HostAwake => "host_awake",
            Status::TimedOut => "timed_out",
            Status::RateLimited => "rate_limited",
            Status::RejectedAuth => "rejected_auth",
            Status::RejectedReplay => "rejected_replay",
            Status::TargetNotOnLan => "target_not_on_lan",
        }
    }
}

/// A relay's answer to a request, naming its nonce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WakeStatus {
    pub status: Status,
    pub retry_after: u16,
    pub nonce: [u8; 16],
    /// Since the magic packets went out.
    pub elapsed_ms: u32,
}

impl WakeStatus {
    pub fn new(status: Status, nonce: [u8; 16]) -> Self {
        Self {
            status,
            retry_after: 0,
            nonce,
            elapsed_ms: 0,
        }
    }

    pub fn encode(&self) -> [u8; STATUS_BYTES] {
        let mut out = [0; STATUS_BYTES];
        out[0] = VERSION;
        out[1] = self.status as u8;
        out[2..4].copy_from_slice(&self.retry_after.to_le_bytes());
        out[4..20].copy_from_slice(&self.nonce);
        out[20..24].copy_from_slice(&self.elapsed_ms.to_le_bytes());
        out
    }

    /// A status of this version; a detail after it, which later versions may send, is skipped.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if !(STATUS_BYTES..=STATUS_MAX_BYTES).contains(&bytes.len()) || bytes[0] != VERSION {
            return None;
        }
        Some(Self {
            status: Status::from_byte(bytes[1])?,
            retry_after: u16::from_le_bytes([bytes[2], bytes[3]]),
            nonce: bytes[4..20].try_into().unwrap(),
            elapsed_ms: u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
        })
    }
}

/// One of the relay's own IPv4 interfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interface {
    pub ip: Ipv4Addr,
    pub prefix: u8,
}

/// A private or link-local address: one a home network uses, never one on the internet.
pub fn is_lan(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_link_local()
}

impl Interface {
    /// The subnet's directed broadcast, for a private subnet of a home network's size.
    pub fn broadcast(&self) -> Option<Ipv4Addr> {
        if !(8..=30).contains(&self.prefix) || !is_lan(self.ip) {
            return None;
        }
        let host_bits = u32::MAX >> self.prefix;
        Some(Ipv4Addr::from(u32::from(self.ip) | host_bits))
    }

    pub fn contains(&self, other: Ipv4Addr) -> bool {
        (1..=32).contains(&self.prefix) && {
            let mask = u32::MAX << (32 - u32::from(self.prefix));
            u32::from(self.ip) & mask == u32::from(other) & mask
        }
    }
}

/// Where a burst may go: each broadcast address the PC registered that is the broadcast of a
/// private subnet this relay is on itself, and then the limited broadcast, which never leaves
/// the link. Nothing when the relay shares no such subnet with the PC: never a unicast, never a
/// public address, and never an address the request named.
pub fn lan_destinations(registered: &[Ipv4Addr], interfaces: &[Interface]) -> Vec<SocketAddrV4> {
    let mut out: Vec<SocketAddrV4> = Vec::new();
    for broadcast in interfaces.iter().filter_map(Interface::broadcast) {
        let destination = SocketAddrV4::new(broadcast, WOL_PORT);
        if registered.iter().take(MAX_BROADCASTS).any(|r| *r == broadcast)
            && !out.contains(&destination)
            && out.len() < MAX_BROADCASTS
        {
            out.push(destination);
        }
    }
    if !out.is_empty() {
        out.push(SocketAddrV4::new(Ipv4Addr::BROADCAST, WOL_PORT));
    }
    out
}

/// A MAC as the host and Nightjar write it: six hex pairs, with `:` or `-` or nothing between.
pub fn parse_mac(text: &str) -> Option<[u8; 6]> {
    let digits: String = text
        .trim()
        .chars()
        .filter(|c| *c != ':' && *c != '-')
        .collect();
    parse_hex::<6>(&digits)
}

/// A network card's own address: not zero, not a group (multicast or broadcast) address.
pub fn is_unicast_mac(mac: &[u8; 6]) -> bool {
    *mac != [0; 6] && mac[0] & 1 == 0
}

pub fn magic_packet(mac: &[u8; 6]) -> [u8; MAGIC_BYTES] {
    let mut packet = [0xff; MAGIC_BYTES];
    for copy in packet[6..].as_chunks_mut::<6>().0 {
        *copy = *mac;
    }
    packet
}

/// The datagrams of one wake, at most [`MAX_BURST_PACKETS`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Burst {
    pub packets: Vec<(SocketAddrV4, [u8; MAGIC_BYTES])>,
}

pub fn burst(macs: &[[u8; 6]], destinations: &[SocketAddrV4]) -> Burst {
    let macs: Vec<&[u8; 6]> = macs
        .iter()
        .filter(|mac| is_unicast_mac(mac))
        .take(MAX_MACS)
        .collect();
    let mut packets = Vec::new();
    for _ in 0..BURST_REPEATS {
        for mac in &macs {
            for destination in destinations.iter().take(MAX_BROADCASTS + 1) {
                packets.push((*destination, magic_packet(mac)));
            }
        }
    }
    packets.truncate(MAX_BURST_PACKETS);
    Burst { packets }
}

/// What a relay keeps of a PC that enrolled it.
#[derive(Clone, PartialEq, Eq)]
pub struct Enrollment {
    pub target_id: String,
    pub target_name: String,
    pub epoch: u64,
    /// This relay's secret for the PC.
    pub secret: Key,
    pub macs: Vec<[u8; 6]>,
    /// The broadcast addresses of the PC's own subnets.
    pub broadcasts: Vec<Ipv4Addr>,
    /// The PC's address on its network, to see it come up.
    pub address: Option<Ipv4Addr>,
    pub http_port: u16,
    /// The devices the PC lets wake it.
    pub devices: Vec<DeviceId>,
}

impl fmt::Debug for Enrollment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Enrollment")
            .field("target_id", &self.target_id)
            .field("target_name", &self.target_name)
            .field("epoch", &self.epoch)
            .field("macs", &self.macs.len())
            .field("broadcasts", &self.broadcasts)
            .field("address", &self.address)
            .field("devices", &self.devices.len())
            .finish_non_exhaustive()
    }
}

impl Enrollment {
    pub fn device_secret(&self, device: &DeviceId) -> Key {
        device_secret(&self.secret, &self.target_id, device, self.epoch)
    }

    pub fn target_hash(&self) -> TargetHash {
        target_hash(&self.target_id)
    }

    /// Only what a relay may act on: unicast MACs, private broadcast and PC addresses, each
    /// list bounded.
    pub fn sanitized(mut self) -> Self {
        self.macs.retain(is_unicast_mac);
        self.macs.dedup();
        self.macs.truncate(MAX_MACS);
        self.broadcasts.retain(|ip| is_lan(*ip));
        self.broadcasts.dedup();
        self.broadcasts.truncate(MAX_BROADCASTS);
        self.address = self.address.filter(|ip| is_lan(*ip));
        self.devices.dedup();
        self.devices.truncate(MAX_DEVICES);
        self
    }
}

/// What a relay does with an authenticated datagram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Nothing, not even an answer.
    Silent,
    Reply(WakeStatus),
    /// Send the burst once, then the reply.
    Wake { reply: WakeStatus, burst: Burst },
}

struct Seen {
    nonce: [u8; 16],
    device: DeviceId,
    from: SocketAddr,
    /// The relay's unix time after which the request could no longer pass the clock check.
    until: u64,
    status: WakeStatus,
    resends: u8,
}

/// A relay's memory of requests and bursts.
#[derive(Default)]
pub struct Relay {
    seen: VecDeque<Seen>,
    bursts: HashMap<(DeviceId, TargetHash), VecDeque<u64>>,
    target_bursts: HashMap<TargetHash, VecDeque<u64>>,
}

impl Relay {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decides on `payload`, a [`crate::wire::Kind::WakeRequest`] that came over the path of
    /// `device` from `from`, for `enrollment`, at unix time `now`, on a relay with `interfaces`.
    pub fn handle(
        &mut self,
        enrollment: &Enrollment,
        device: &DeviceId,
        from: SocketAddr,
        payload: &[u8],
        now: u64,
        interfaces: &[Interface],
    ) -> Decision {
        let Some(request) = WakeRequest::parse(payload) else {
            return Decision::Silent;
        };
        let reply = |status| WakeStatus::new(status, request.nonce);
        let authentic = enrollment.devices.contains(device)
            && WakeRequest::signed_by(payload, &enrollment.device_secret(device))
            && request.device == *device
            && request.target == enrollment.target_hash()
            && request.epoch == enrollment.epoch;
        if !authentic {
            return Decision::Reply(reply(Status::RejectedAuth));
        }
        if now.abs_diff(request.time) > CLOCK_SKEW_SECONDS {
            return Decision::Reply(reply(Status::RejectedReplay));
        }
        self.forget(now);
        if let Some(seen) = self.seen.iter_mut().find(|seen| seen.nonce == request.nonce) {
            // The same device asking again from where it asked: its answer was lost. Anyone or
            // anywhere else: a replay.
            if seen.device != *device || seen.from != from {
                return Decision::Reply(reply(Status::RejectedReplay));
            }
            if seen.resends >= MAX_RESENDS {
                return Decision::Silent;
            }
            seen.resends += 1;
            return Decision::Reply(seen.status);
        }
        if self.seen.len() >= REPLAY_ENTRIES {
            // Forgetting a fresh nonce would let its request through again.
            let wait = self.seen.iter().map(|seen| seen.until).min().unwrap_or(now);
            let mut status = reply(Status::RateLimited);
            status.retry_after = retry(wait.saturating_sub(now));
            return Decision::Reply(status);
        }
        let key = (*device, request.target);
        let destinations = lan_destinations(&enrollment.broadcasts, interfaces);
        let burst = burst(&enrollment.macs, &destinations);
        let decision = if let Some(wait) = self.limited(&key, now) {
            let mut status = reply(Status::RateLimited);
            status.retry_after = wait;
            Decision::Reply(status)
        } else if burst.packets.is_empty() {
            Decision::Reply(reply(Status::TargetNotOnLan))
        } else {
            self.take(&key, now);
            Decision::Wake {
                reply: reply(Status::PacketSent),
                burst,
            }
        };
        let status = match &decision {
            Decision::Reply(status) | Decision::Wake { reply: status, .. } => *status,
            Decision::Silent => unreachable!("a decision above always answers"),
        };
        self.seen.push_back(Seen {
            nonce: request.nonce,
            device: *device,
            from,
            until: request.time.saturating_add(CLOCK_SKEW_SECONDS + 1),
            status,
            resends: 0,
        });
        decision
    }

    fn forget(&mut self, now: u64) {
        self.seen.retain(|seen| now < seen.until);
        let recent = |times: &mut VecDeque<u64>| {
            times.retain(|at| now.saturating_sub(*at) < BURST_WINDOW_SECONDS);
            !times.is_empty()
        };
        self.bursts.retain(|_, times| recent(times));
        self.target_bursts.retain(|_, times| recent(times));
    }

    /// Seconds until another burst for `key` may go, when one may not now.
    fn limited(&self, key: &(DeviceId, TargetHash), now: u64) -> Option<u16> {
        let wait = |times: Option<&VecDeque<u64>>, most: usize| -> Option<u64> {
            let times = times?;
            let last = *times.back()?;
            // A clock that went back counts as no time passed.
            let since = now.saturating_sub(last);
            if since < BURST_GAP_SECONDS || now < last {
                return Some(BURST_GAP_SECONDS - since.min(BURST_GAP_SECONDS));
            }
            let recent: Vec<u64> = times
                .iter()
                .copied()
                .filter(|at| now.saturating_sub(*at) < BURST_WINDOW_SECONDS)
                .collect();
            (recent.len() >= most).then(|| BURST_WINDOW_SECONDS - (now - recent[0]))
        };
        wait(self.bursts.get(key), BURSTS_PER_WINDOW)
            .or_else(|| wait(self.target_bursts.get(&key.1), TARGET_BURSTS_PER_WINDOW))
            .map(retry)
    }

    fn take(&mut self, key: &(DeviceId, TargetHash), now: u64) {
        self.bursts.entry(*key).or_default().push_back(now);
        self.target_bursts.entry(key.1).or_default().push_back(now);
    }
}

fn retry(seconds: u64) -> u16 {
    u16::try_from(seconds.max(1)).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod sim_tests;
