//! The relay's security rules, each on its own: who may wake, what, how often, and where the
//! magic packets may go.

use super::*;
use crate::wire::{self, Kind, Opener, Sealer};

const NOW: u64 = 1_800_000_000;
const TARGET: &str = "0123456789ABCDEF";
const MAC: [u8; 6] = [0x10, 0x7b, 0x44, 0x01, 0x02, 0x03];

fn from() -> SocketAddr {
    "203.0.113.9:40000".parse().unwrap()
}

fn lan() -> Vec<Interface> {
    vec![Interface {
        ip: Ipv4Addr::new(192, 168, 1, 20),
        prefix: 24,
    }]
}

fn device() -> DeviceId {
    device_id("-----BEGIN CERTIFICATE-----\nphone\n-----END CERTIFICATE-----")
}

fn enrollment() -> Enrollment {
    Enrollment {
        target_id: TARGET.into(),
        target_name: "Gaming PC".into(),
        epoch: 3,
        secret: [42; 32],
        macs: vec![MAC],
        broadcasts: vec![Ipv4Addr::new(192, 168, 1, 255)],
        address: Some(Ipv4Addr::new(192, 168, 1, 30)),
        http_port: 47989,
        devices: vec![device()],
    }
}

/// A request as the paired device makes it, from the secret the PC handed it.
fn request(epoch: u64, time: u64) -> (WakeRequest, [u8; REQUEST_BYTES]) {
    let secret = device_secret(&[42; 32], TARGET, &device(), epoch);
    let request = WakeRequest::new(TARGET, device(), epoch, time);
    (request, request.encode(&secret))
}

fn status(decision: &Decision) -> Option<Status> {
    match decision {
        Decision::Silent => None,
        Decision::Reply(reply) | Decision::Wake { reply, .. } => Some(reply.status),
    }
}

fn bursts(decision: &Decision) -> usize {
    match decision {
        Decision::Wake { burst, .. } => burst.packets.len(),
        _ => 0,
    }
}

#[test]
fn a_paired_devices_request_sends_one_bounded_burst_to_the_registered_mac_on_the_lan() {
    let mut relay = Relay::new();
    let (sent, bytes) = request(3, NOW);
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    let Decision::Wake { reply, burst } = &decision else {
        panic!("{decision:?}");
    };
    assert_eq!(reply.status, Status::PacketSent);
    assert_eq!(reply.nonce, sent.nonce);
    // Three repeats to the subnet's broadcast and the limited broadcast: nothing else.
    assert_eq!(burst.packets.len(), BURST_REPEATS * 2);
    assert!(burst.packets.len() <= MAX_BURST_PACKETS);
    for (destination, packet) in &burst.packets {
        assert!(
            *destination.ip() == Ipv4Addr::new(192, 168, 1, 255)
                || *destination.ip() == Ipv4Addr::BROADCAST
        );
        assert_eq!(destination.port(), WOL_PORT);
        assert_eq!(packet, &magic_packet(&MAC));
    }
}

#[test]
fn the_magic_packet_is_six_ff_then_sixteen_copies_of_the_mac() {
    let packet = magic_packet(&MAC);
    assert_eq!(packet.len(), 102);
    assert_eq!(&packet[..6], &[0xff; 6]);
    assert!(packet[6..].chunks(6).all(|copy| copy == MAC));
}

#[test]
fn a_tampered_signature_is_rejected_and_sends_nothing() {
    let mut relay = Relay::new();
    let (_, mut bytes) = request(3, NOW);
    bytes[REQUEST_BYTES - 1] ^= 1;
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
    assert_eq!(bursts(&decision), 0);
    // A field changed under a valid signature is as bad.
    let (_, mut bytes) = request(3, NOW);
    bytes[30] ^= 1;
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
}

#[test]
fn another_devices_secret_or_a_request_naming_another_device_is_rejected() {
    let mut relay = Relay::new();
    let other = device_id("another certificate");
    let mut listed = enrollment();
    listed.devices.push(other);
    // Signed by the other device's secret, sent over this device's path.
    let request = WakeRequest::new(TARGET, other, 3, NOW);
    let bytes = request.encode(&device_secret(&[42; 32], TARGET, &other, 3));
    let decision = relay.handle(&listed, &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
    // Signed by this device's secret but naming the other device.
    let bytes = request.encode(&device_secret(&[42; 32], TARGET, &device(), 3));
    let decision = relay.handle(&listed, &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
}

#[test]
fn a_device_the_pc_no_longer_lists_cannot_wake_it() {
    let mut relay = Relay::new();
    let (_, bytes) = request(3, NOW);
    let mut unpaired = enrollment();
    unpaired.devices.clear();
    let decision = relay.handle(&unpaired, &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
    assert_eq!(bursts(&decision), 0);
}

#[test]
fn a_revoked_epoch_is_rejected() {
    let mut relay = Relay::new();
    // The device kept its secret of epoch 2; the PC rotated to 3.
    let (_, bytes) = request(2, NOW);
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
    assert_eq!(bursts(&decision), 0);
}

#[test]
fn a_request_for_another_pc_is_rejected() {
    let mut relay = Relay::new();
    let secret = device_secret(&[42; 32], TARGET, &device(), 3);
    let bytes = WakeRequest::new("another PC", device(), 3, NOW).encode(&secret);
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RejectedAuth));
}

#[test]
fn a_clock_more_than_two_minutes_off_is_rejected_either_way() {
    for skew in [-121i64, 121] {
        let mut relay = Relay::new();
        let (_, bytes) = request(3, NOW.saturating_add_signed(skew));
        let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
        assert_eq!(status(&decision), Some(Status::RejectedReplay), "{skew}");
        assert_eq!(bursts(&decision), 0);
    }
    for skew in [-120i64, 120] {
        let mut relay = Relay::new();
        let (_, bytes) = request(3, NOW.saturating_add_signed(skew));
        let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
        assert_eq!(status(&decision), Some(Status::PacketSent), "{skew}");
    }
}

#[test]
fn a_repeated_nonce_never_sends_a_second_burst() {
    let mut relay = Relay::new();
    let (_, bytes) = request(3, NOW);
    let first = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    assert_eq!(status(&first), Some(Status::PacketSent));
    // The device asks again (its answer was lost): the same answer, no burst, a few times.
    for _ in 0..MAX_RESENDS {
        let again = relay.handle(&enrollment(), &device(), from(), &bytes, NOW + 1, &lan());
        assert_eq!(status(&again), Some(Status::PacketSent));
        assert_eq!(bursts(&again), 0);
    }
    let silent = relay.handle(&enrollment(), &device(), from(), &bytes, NOW + 2, &lan());
    assert_eq!(silent, Decision::Silent);
    // The same request replayed from elsewhere is a replay.
    let elsewhere: SocketAddr = "198.51.100.7:1234".parse().unwrap();
    let replayed = relay.handle(&enrollment(), &device(), elsewhere, &bytes, NOW + 30, &lan());
    assert_eq!(status(&replayed), Some(Status::RejectedReplay));
    assert_eq!(bursts(&replayed), 0);
    // Remembered as long as the clock check could pass it.
    let late = relay.handle(&enrollment(), &device(), elsewhere, &bytes, NOW + 120, &lan());
    assert_eq!(status(&late), Some(Status::RejectedReplay));
}

#[test]
fn one_burst_per_ten_seconds_and_ten_per_ten_minutes() {
    let mut relay = Relay::new();
    let (_, bytes) = request(3, NOW);
    assert_eq!(
        bursts(&relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan())),
        BURST_REPEATS * 2
    );
    let (_, bytes) = request(3, NOW + 4);
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW + 4, &lan());
    assert_eq!(bursts(&decision), 0);
    let Decision::Reply(reply) = decision else {
        panic!("{decision:?}");
    };
    assert_eq!(reply.status, Status::RateLimited);
    assert_eq!(reply.retry_after, 6);
    // Every ten seconds one more, until ten in ten minutes.
    let mut sent = 1;
    let mut at = NOW;
    while sent < BURSTS_PER_WINDOW {
        at += BURST_GAP_SECONDS;
        let (_, bytes) = request(3, at);
        assert!(bursts(&relay.handle(&enrollment(), &device(), from(), &bytes, at, &lan())) > 0);
        sent += 1;
    }
    at += BURST_GAP_SECONDS;
    let (_, bytes) = request(3, at);
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, at, &lan());
    assert_eq!(status(&decision), Some(Status::RateLimited));
    assert_eq!(bursts(&decision), 0);
    // Ten minutes after the first, one more.
    let at = NOW + BURST_WINDOW_SECONDS;
    let (_, bytes) = request(3, at);
    assert!(bursts(&relay.handle(&enrollment(), &device(), from(), &bytes, at, &lan())) > 0);
}

#[test]
fn two_devices_together_still_get_one_burst_per_ten_seconds() {
    let mut relay = Relay::new();
    let other = device_id("another certificate");
    let mut listed = enrollment();
    listed.devices.push(other);
    let (_, bytes) = request(3, NOW);
    assert!(bursts(&relay.handle(&listed, &device(), from(), &bytes, NOW, &lan())) > 0);
    let bytes =
        WakeRequest::new(TARGET, other, 3, NOW).encode(&listed.device_secret(&other));
    let decision = relay.handle(&listed, &other, from(), &bytes, NOW + 1, &lan());
    assert_eq!(status(&decision), Some(Status::RateLimited));
    assert_eq!(bursts(&decision), 0);
}

#[test]
fn a_full_replay_memory_refuses_rather_than_forgets() {
    let mut relay = Relay::new();
    let mut bytes = request(3, NOW).1;
    for _ in 0..REPLAY_ENTRIES {
        bytes = request(3, NOW).1;
        relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &lan());
    }
    let (_, fresh) = request(3, NOW);
    let decision = relay.handle(&enrollment(), &device(), from(), &fresh, NOW, &lan());
    assert_eq!(status(&decision), Some(Status::RateLimited));
    // The newest remembered request is still a replay.
    let elsewhere: SocketAddr = "198.51.100.7:1234".parse().unwrap();
    let replayed = relay.handle(&enrollment(), &device(), elsewhere, &bytes, NOW, &lan());
    assert_eq!(status(&replayed), Some(Status::RejectedReplay));
}

#[test]
fn magic_packets_go_only_to_private_subnets_the_relay_and_the_pc_share() {
    let registered = [
        Ipv4Addr::new(192, 168, 1, 255),
        Ipv4Addr::new(10, 0, 0, 255),
        Ipv4Addr::new(203, 0, 113, 255),
    ];
    let interfaces = [
        Interface {
            ip: Ipv4Addr::new(192, 168, 1, 20),
            prefix: 24,
        },
        // A public subnet, even one the PC listed, gets nothing.
        Interface {
            ip: Ipv4Addr::new(203, 0, 113, 20),
            prefix: 24,
        },
        // A private subnet the PC is not on gets nothing.
        Interface {
            ip: Ipv4Addr::new(172, 16, 5, 5),
            prefix: 16,
        },
    ];
    assert_eq!(
        lan_destinations(&registered, &interfaces),
        vec![
            SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 255), WOL_PORT),
            SocketAddrV4::new(Ipv4Addr::BROADCAST, WOL_PORT),
        ]
    );
    // A relay that shares no private subnet with the PC sends nothing at all.
    let away = [Interface {
        ip: Ipv4Addr::new(10, 9, 9, 9),
        prefix: 24,
    }];
    assert!(lan_destinations(&registered, &away).is_empty());
    let mut relay = Relay::new();
    let (_, bytes) = request(3, NOW);
    let decision = relay.handle(&enrollment(), &device(), from(), &bytes, NOW, &away);
    assert_eq!(status(&decision), Some(Status::TargetNotOnLan));
    assert_eq!(bursts(&decision), 0);
}

#[test]
fn a_burst_stays_bounded_and_skips_group_macs() {
    let macs = [[0x01, 0, 0x5e, 0, 0, 1], [0; 6], [0xff; 6], MAC];
    let destinations: Vec<SocketAddrV4> = (0..20)
        .map(|n| SocketAddrV4::new(Ipv4Addr::new(10, 0, n, 255), WOL_PORT))
        .collect();
    let sent = burst(&macs, &destinations);
    assert!(sent.packets.len() <= MAX_BURST_PACKETS);
    assert!(sent.packets.iter().all(|(_, packet)| packet == &magic_packet(&MAC)));
    let many: Vec<[u8; 6]> = (0..10).map(|n| [2, 0, 0, 0, 0, n]).collect();
    assert_eq!(burst(&many, &destinations).packets.len(), MAX_BURST_PACKETS);
    // An enrollment keeps only what a relay may act on.
    let mut wide = enrollment();
    wide.macs = macs.to_vec();
    wide.broadcasts = vec![Ipv4Addr::new(8, 8, 8, 255), Ipv4Addr::new(192, 168, 1, 255)];
    wide.address = Some(Ipv4Addr::new(8, 8, 8, 8));
    let kept = wide.sanitized();
    assert_eq!(kept.macs, vec![MAC]);
    assert_eq!(kept.broadcasts, vec![Ipv4Addr::new(192, 168, 1, 255)]);
    assert_eq!(kept.address, None);
}

#[test]
fn a_status_is_shorter_than_a_request_on_the_wire_and_survives_a_round_trip() {
    let keys = crate::record::Keys::new([5; 32]).path(&[1; 16], &[2; 16]);
    let requester = Sealer::new(&keys.client_to_host, keys.tag, [2; 16]);
    let relay = Sealer::new(&keys.host_to_client, keys.tag, [2; 16]);
    let (sent, bytes) = request(3, NOW);
    let request_datagram = requester.seal(Kind::WakeRequest, 0, &bytes);
    let mut answer = WakeStatus::new(Status::HostAwake, sent.nonce);
    answer.elapsed_ms = 12_345;
    let status_datagram = relay.seal(Kind::WakeStatus, 0, &answer.encode());
    assert_eq!(request_datagram.len(), wire::OVERHEAD + REQUEST_BYTES);
    assert!(status_datagram.len() + wire::OVERHEAD + DETAIL_MAX_BYTES < request_datagram.len());
    // The relay's whole answer to one request, two statuses, still sends less than it got.
    assert!(2 * status_datagram.len() < request_datagram.len());
    let opened = Opener::new(&keys.host_to_client, keys.tag)
        .open(&status_datagram)
        .unwrap();
    assert_eq!(opened.kind, Kind::WakeStatus);
    assert_eq!(WakeStatus::decode(&opened.payload), Some(answer));
    let opened = Opener::new(&keys.client_to_host, keys.tag)
        .open(&request_datagram)
        .unwrap();
    assert_eq!(opened.kind, Kind::WakeRequest);
    assert_eq!(WakeRequest::parse(&opened.payload), Some(sent));
    // Garbage and other versions decode to nothing.
    assert_eq!(WakeStatus::decode(&[1, 99]), None);
    assert_eq!(WakeRequest::parse(&bytes[..100]), None);
    let mut later = bytes;
    later[0] = 2;
    assert_eq!(WakeRequest::parse(&later), None);
}

#[test]
fn malformed_requests_get_no_answer() {
    let mut relay = Relay::new();
    for payload in [&[][..], &[1; 20][..], &[0; REQUEST_BYTES][..]] {
        assert_eq!(
            relay.handle(&enrollment(), &device(), from(), payload, NOW, &lan()),
            Decision::Silent
        );
    }
}

#[test]
fn device_secrets_depend_on_relay_pc_device_and_epoch() {
    let base = device_secret(&[1; 32], TARGET, &device(), 1);
    assert_eq!(base, device_secret(&[1; 32], TARGET, &device(), 1));
    assert_ne!(base, device_secret(&[2; 32], TARGET, &device(), 1));
    assert_ne!(base, device_secret(&[1; 32], "other", &device(), 1));
    assert_ne!(base, device_secret(&[1; 32], TARGET, &[9; 32], 1));
    assert_ne!(base, device_secret(&[1; 32], TARGET, &device(), 2));
    assert_eq!(parse_hex::<4>(&hex(&[0xde, 0xad, 0xbe, 0xef])), Some([0xde, 0xad, 0xbe, 0xef]));
    assert_eq!(parse_hex::<2>("zz00"), None);
    assert_eq!(parse_mac("10-7B-44-01-02-03"), Some(MAC));
    assert_eq!(parse_mac("10:7b:44:01:02"), None);
    // The debug form never shows the secret.
    assert!(!format!("{:?}", enrollment()).contains("42"));
}
