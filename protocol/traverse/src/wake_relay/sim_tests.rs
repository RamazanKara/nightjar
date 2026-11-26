//! A whole wake through the NAT simulator: a requester away from home and a relay behind its
//! home router punch through, the request crosses sealed, the relay decides and answers. No
//! socket is opened.

use super::*;
use crate::{
    record::Keys,
    sim::{Behaviour, World},
    wire::{Kind, Sealer},
};

const NOW: u64 = 1_800_000_000;
const TARGET: &str = "GAMING-PC-UUID";
const MAC: [u8; 6] = [0x10, 0x7b, 0x44, 0x0a, 0x0b, 0x0c];

fn device() -> DeviceId {
    device_id("phone certificate")
}

fn enrollment() -> Enrollment {
    Enrollment {
        target_id: TARGET.into(),
        target_name: "Gaming PC".into(),
        epoch: 1,
        secret: [11; 32],
        macs: vec![MAC],
        broadcasts: vec![Ipv4Addr::new(192, 168, 1, 255)],
        address: Some(Ipv4Addr::new(192, 168, 1, 30)),
        http_port: 47989,
        devices: vec![device()],
    }
}

/// The relay's own interface: the simulator puts it at 192.168.1.10/24.
fn lan() -> Vec<Interface> {
    vec![Interface {
        ip: Ipv4Addr::new(192, 168, 1, 10),
        prefix: 24,
    }]
}

/// Steps until peer `index` has something in its inbox, for at most two seconds.
fn wait_for(world: &mut World, index: usize) -> Option<(Kind, Vec<u8>, SocketAddr)> {
    for _ in 0..200 {
        world.step();
        if !world.peers[index].inbox.is_empty() {
            return Some(world.peers[index].inbox.remove(0));
        }
    }
    None
}

/// Punches, sends one request sealed with `requester` (the requester's own sealer if None), and
/// lets the relay decide. The relay's decision and the status the requester got, if any.
fn wake(
    relay_nat: Behaviour,
    requester_nat: Behaviour,
    requester: Option<Sealer>,
) -> Option<(Decision, Option<WakeStatus>)> {
    let enrollment = enrollment();
    let secret = enrollment.device_secret(&device());
    let mut world = World::with_secret(relay_nat, requester_nat, secret);
    if !world.punch(3_000) {
        return None;
    }
    if let Some(sealer) = requester {
        world.peers[1].sealer = sealer;
    }
    let relay_address = world.peers[1].connected()?;
    let request = WakeRequest::new(TARGET, device(), 1, NOW);
    let datagram = world.peers[1]
        .sealer
        .seal(Kind::WakeRequest, 0, &request.encode(&secret));
    world.send(1, relay_address, datagram);
    let Some((kind, payload, from)) = wait_for(&mut world, 0) else {
        return Some((Decision::Silent, None));
    };
    assert_eq!(kind, Kind::WakeRequest);
    let decision = Relay::new().handle(&enrollment, &device(), from, &payload, NOW, &lan());
    let reply = match &decision {
        Decision::Silent => return Some((decision, None)),
        Decision::Reply(reply) | Decision::Wake { reply, .. } => *reply,
    };
    let requester_address = world.peers[0].connected()?;
    let sealed = world.peers[0]
        .sealer
        .seal(Kind::WakeStatus, 0, &reply.encode());
    world.send(0, requester_address, sealed);
    let status = wait_for(&mut world, 1).and_then(|(kind, payload, _)| {
        assert_eq!(kind, Kind::WakeStatus);
        WakeStatus::decode(&payload)
    });
    assert!(status.is_none_or(|status| status.nonce == request.nonce));
    Some((decision, status))
}

#[test]
fn a_wake_gets_through_cone_and_port_restricted_nats() {
    use Behaviour::*;
    for (relay, requester) in [
        (FullCone, FullCone),
        (PortRestricted, FullCone),
        (FullCone, PortRestricted),
        (PortRestricted, PortRestricted),
        (Restricted, PortRestricted),
        // Mobile carriers' NATs: a sequential symmetric one is among the predicted ports.
        (PortRestricted, Symmetric { sequential: true }),
    ] {
        let (decision, status) = wake(relay, requester, None)
            .unwrap_or_else(|| panic!("no path: relay {relay:?}, requester {requester:?}"));
        let Decision::Wake { burst, .. } = decision else {
            panic!("relay {relay:?}, requester {requester:?}: {decision:?}");
        };
        assert!(!burst.packets.is_empty() && burst.packets.len() <= MAX_BURST_PACKETS);
        assert_eq!(
            status.map(|status| status.status),
            Some(Status::PacketSent),
            "relay {relay:?}, requester {requester:?}"
        );
    }
}

#[test]
fn no_path_between_two_networks_that_both_filter_by_port_and_hide_theirs() {
    use Behaviour::*;
    assert!(wake(PortRestricted, Symmetric { sequential: false }, None).is_none());
}

#[test]
fn a_request_sealed_under_other_keys_never_reaches_the_relays_decision() {
    // Someone with another device's secret, or none, on the path the requester opened.
    let wrong = Keys::new([99; 32]).path(&[1; 16], &[2; 16]);
    let forged = Sealer::new(&wrong.client_to_host, wrong.tag, [2; 16]);
    let (decision, status) =
        wake(Behaviour::FullCone, Behaviour::FullCone, Some(forged)).expect("punched");
    assert_eq!(decision, Decision::Silent);
    assert_eq!(status, None);
}
