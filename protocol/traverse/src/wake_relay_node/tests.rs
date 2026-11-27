//! The relay and a requester on real sockets, all on 127.0.0.1 (no wildcard bind, no DHT, no
//! STUN): the requester knows the relay's session and address, as a DHT record would tell it.

use super::*;
use crate::wake_relay::{device_id, device_secret, WakeRequest};
use std::net::{IpAddr, Ipv4Addr};

const TARGET: &str = "GAMING-PC";
const MAC: [u8; 6] = [0x10, 0x7b, 0x44, 0x0a, 0x0b, 0x0c];

#[derive(Default)]
struct FakeLan {
    bursts: Mutex<Vec<Burst>>,
    up: AtomicBool,
}

impl Lan for FakeLan {
    fn interfaces(&self) -> Vec<Interface> {
        vec![Interface {
            ip: Ipv4Addr::new(192, 168, 1, 10),
            prefix: 24,
        }]
    }
    fn send(&self, burst: &Burst) -> usize {
        self.bursts.lock().unwrap().push(burst.clone());
        burst.packets.len()
    }
    fn is_up(&self, _: SocketAddrV4) -> bool {
        self.up.load(Ordering::Acquire)
    }
}

fn loopback() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

fn device() -> DeviceId {
    device_id("phone certificate")
}

fn enrollment() -> Enrollment {
    Enrollment {
        target_id: TARGET.into(),
        target_name: "Gaming PC".into(),
        epoch: 2,
        secret: [21; 32],
        macs: vec![MAC],
        broadcasts: vec![Ipv4Addr::new(192, 168, 1, 255)],
        address: Some(Ipv4Addr::new(192, 168, 1, 30)),
        http_port: 47989,
        devices: vec![device()],
    }
}

fn relay(lan: Arc<FakeLan>) -> WakeRelayPeer {
    let config = RelayConfig {
        bind: loopback(),
        stun_servers: Vec::new(),
        bootstrap: Vec::new(),
    };
    let peer = WakeRelayPeer::start(config, lan).unwrap();
    peer.set_enrollments(vec![enrollment()]);
    peer
}

fn request(peer: &WakeRelayPeer, epoch: u64) -> RequestConfig {
    RequestConfig {
        target_id: TARGET.into(),
        device: device(),
        epoch,
        secret: device_secret(&[21; 32], TARGET, &device(), epoch),
        known: Some(RelayInfo {
            session: peer.session(),
            endpoints: vec![peer.local_addr().unwrap()],
            nat: Nat::None,
        }),
        bind: loopback(),
        stun_servers: Vec::new(),
        bootstrap: Vec::new(),
    }
}

#[test]
fn a_paired_device_wakes_the_pc_through_the_relay_and_hears_it_came_up() {
    let lan = Arc::new(FakeLan::default());
    let peer = relay(lan.clone());
    lan.up.store(true, Ordering::Release);
    let mut seen = Vec::new();
    let result = request_wake(request(&peer, 2), &AtomicBool::new(false), &mut |p| {
        seen.push(p)
    });
    peer.stop();
    assert_eq!(result.map(|status| status.status), Ok(Status::HostAwake));
    assert!(seen.contains(&Progress::Requested));
    assert!(seen
        .iter()
        .any(|p| matches!(p, Progress::Status(s) if s.status == Status::PacketSent)));
    // One request, one burst.
    let bursts = lan.bursts.lock().unwrap();
    assert_eq!(bursts.len(), 1);
    assert!(bursts[0].packets.len() <= crate::wake_relay::MAX_BURST_PACKETS);
    assert_eq!(peer.status().last_wake.map(|w| w.2), Some(Status::HostAwake));
}

#[test]
fn a_second_wake_right_after_is_rate_limited_without_a_burst() {
    let lan = Arc::new(FakeLan::default());
    let peer = relay(lan.clone());
    lan.up.store(true, Ordering::Release);
    let cancel = AtomicBool::new(false);
    let first = request_wake(request(&peer, 2), &cancel, &mut |_| {});
    let second = request_wake(request(&peer, 2), &cancel, &mut |_| {});
    peer.stop();
    assert_eq!(first.map(|s| s.status), Ok(Status::HostAwake));
    let second = second.unwrap();
    assert_eq!(second.status, Status::RateLimited);
    assert!(second.retry_after > 0);
    assert_eq!(lan.bursts.lock().unwrap().len(), 1);
}

/// Whatever arrives without a listed device's keys gets not one byte back, and sends nothing on
/// the LAN: no amplification, no reflection, nothing for a scanner to see.
#[test]
fn unauthenticated_datagrams_get_no_answer_at_all() {
    let lan = Arc::new(FakeLan::default());
    let peer = relay(lan.clone());
    let relay_address = peer.local_addr().unwrap();
    let probe = UdpSocket::bind(loopback()).unwrap();
    probe
        .set_read_timeout(Some(Duration::from_millis(600)))
        .unwrap();
    let mut datagrams: Vec<Vec<u8>> = vec![
        b"hello".to_vec(),
        vec![0; 200],
        // A DHT query and a STUN request: other protocols on the same socket.
        b"d1:ad2:id20:aaaaaaaaaaaaaaaaaaaae1:q4:ping1:t2:aa1:y1:qe".to_vec(),
        [0x00, 0x01, 0x00, 0x00, 0x21, 0x12, 0xa4, 0x42].repeat(3),
    ];
    // Well-formed punches and requests under keys the relay does not hold: an unknown device,
    // a device of an older epoch (revoked), and the right tag with the wrong secret.
    let stamp = wire::punch_stamp(unix_now());
    let session = [9; 16];
    let stranger = Keys::new([1; 32]).path(&peer.session(), &session);
    let revoked = Keys::new(device_secret(&[21; 32], TARGET, &device(), 1))
        .path(&peer.session(), &session);
    let listed = Keys::new(enrollment().device_secret(&device())).path(&peer.session(), &session);
    for keys in [stranger, revoked] {
        let sealer = Sealer::new(&keys.client_to_host, keys.tag, session);
        datagrams.push(sealer.seal(Kind::Punch, 0, &stamp));
        datagrams.push(sealer.seal(Kind::WakeRequest, 0, &[0; 148]));
    }
    let wrong_key = Sealer::new(&stranger.client_to_host, listed.tag, session);
    datagrams.push(wrong_key.seal(Kind::Punch, 0, &stamp));
    // The listed device's own punch, but stale: a capture replayed later.
    let stale = Sealer::new(&listed.client_to_host, listed.tag, session);
    datagrams.push(stale.seal(Kind::Punch, 0, &wire::punch_stamp(unix_now() - 600)));
    for datagram in &datagrams {
        probe.send_to(datagram, relay_address).unwrap();
    }
    let mut buffer = [0u8; 2048];
    let answer = probe.recv_from(&mut buffer);
    peer.stop();
    assert!(answer.is_err(), "the relay answered {:?}", answer.map(|a| a.0));
    assert!(lan.bursts.lock().unwrap().is_empty());
}

#[test]
fn a_device_the_pc_dropped_is_refused_at_once_on_its_open_path() {
    let lan = Arc::new(FakeLan::default());
    let peer = relay(lan.clone());
    let relay_address = peer.local_addr().unwrap();
    let probe = UdpSocket::bind(loopback()).unwrap();
    probe
        .set_read_timeout(Some(Duration::from_millis(600)))
        .unwrap();
    let session = [7; 16];
    let keys = Keys::new(enrollment().device_secret(&device())).path(&peer.session(), &session);
    let sealer = Sealer::new(&keys.client_to_host, keys.tag, session);
    probe
        .send_to(
            &sealer.seal(Kind::Punch, 0, &wire::punch_stamp(unix_now())),
            relay_address,
        )
        .unwrap();
    let mut buffer = [0u8; 2048];
    let (length, _) = probe.recv_from(&mut buffer).unwrap();
    let mut opener = Opener::new(&keys.host_to_client, keys.tag);
    assert_eq!(opener.open(&buffer[..length]).unwrap().kind, Kind::PunchAck);
    // The PC unpairs the device: its path closes, and its request gets nothing.
    let mut dropped = enrollment();
    dropped.devices.clear();
    peer.set_enrollments(vec![dropped]);
    let secret = enrollment().device_secret(&device());
    let request = WakeRequest::new(TARGET, device(), 2, unix_now()).encode(&secret);
    probe
        .send_to(&sealer.seal(Kind::WakeRequest, 0, &request), relay_address)
        .unwrap();
    let answer = probe.recv_from(&mut buffer);
    peer.stop();
    assert!(answer.is_err());
    assert!(lan.bursts.lock().unwrap().is_empty());
}

#[test]
fn a_relay_on_several_networks_sends_each_datagram_from_one_address_only() {
    let interfaces = [
        Interface {
            ip: Ipv4Addr::new(192, 168, 1, 10),
            prefix: 24,
        },
        Interface {
            ip: Ipv4Addr::new(10, 0, 0, 5),
            prefix: 24,
        },
        // A second address on the first network: still one datagram, not two.
        Interface {
            ip: Ipv4Addr::new(192, 168, 1, 11),
            prefix: 24,
        },
    ];
    let to = |ip: [u8; 4]| SocketAddrV4::new(ip.into(), crate::wake_relay::WOL_PORT);
    let destinations = [
        to([192, 168, 1, 255]),
        to([10, 0, 0, 255]),
        to([172, 16, 0, 255]),
        to([255, 255, 255, 255]),
    ];
    let burst = crate::wake_relay::burst(&[MAC], &destinations);
    let from = sources(&burst, &interfaces);
    assert_eq!(from.len(), burst.packets.len());
    assert!(from.len() <= crate::wake_relay::MAX_BURST_PACKETS);
    for ((destination, _), source) in burst.packets.iter().zip(&from) {
        let expected = match destination.ip().octets() {
            [192, 168, 1, 255] | [255, 255, 255, 255] => Some(Ipv4Addr::new(192, 168, 1, 10)),
            [10, 0, 0, 255] => Some(Ipv4Addr::new(10, 0, 0, 5)),
            _ => None,
        };
        assert_eq!(*source, expected, "{destination}");
    }
}

/// A path's key depends only on the relay's session and the device's, so a path built again for
/// the same device session must not count from where the last one started.
#[test]
fn a_path_built_again_for_a_device_session_never_repeats_a_sequence_number() {
    let lan = Arc::new(FakeLan::default());
    let peer = relay(lan.clone());
    let relay_address = peer.local_addr().unwrap();
    let probe = UdpSocket::bind(loopback()).unwrap();
    probe
        .set_read_timeout(Some(Duration::from_millis(600)))
        .unwrap();
    let session = [5; 16];
    let keys = Keys::new(enrollment().device_secret(&device())).path(&peer.session(), &session);
    let sealer = Sealer::new(&keys.client_to_host, keys.tag, session);
    let mut buffer = [0u8; 2048];
    let mut answers = Vec::new();
    for _ in 0..3 {
        probe
            .send_to(
                &sealer.seal(Kind::Punch, 0, &wire::punch_stamp(unix_now())),
                relay_address,
            )
            .unwrap();
        // Everything the relay sends on the path (its own punches too) is under the same key;
        // a fresh opener per datagram, as a listener that kept none would.
        loop {
            let (length, _) = probe.recv_from(&mut buffer).unwrap();
            let mut opener = Opener::new(&keys.host_to_client, keys.tag);
            let kind = opener.open(&buffer[..length]).unwrap().kind;
            answers.push(u64::from_be_bytes(buffer[16..24].try_into().unwrap()));
            if kind == Kind::PunchAck {
                break;
            }
        }
        // The path goes, as when its puncher fails or the device closes it; the device's next
        // punch for the same session builds it again.
        probe
            .send_to(&sealer.seal(Kind::Close, 0, &[]), relay_address)
            .unwrap();
    }
    peer.stop();
    let mut distinct = answers.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), answers.len(), "sequence numbers {answers:?}");
}
