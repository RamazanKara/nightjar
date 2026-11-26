//! Away wake relay for Nightjar (`docs/designs/15-away-wake-relay.md`): what the PC hands out
//! over the native API (`GET /wake/relay`, `POST /wake/relay/enroll`) turned into what
//! `rubylight-traverse` runs with. A device that is away asks a relay with a
//! [`RequestConfig`] from its [`WakeRelayBundle`]; a relay serves the PCs it enrolled with, one
//! [`Enrollment`] per [`RelayAssignment`]. Both apps share these, so they read the PC's answer
//! the same way and refuse the same malformed ones.
use crate::http::native_api::wake_relay::{
    RelayAssignment, RelayCredential, WakeRelayBundle,
};
use rubylight_traverse::wake_relay::{Enrollment, parse_hex, parse_mac};
use rubylight_traverse::wake_relay_node::requester::RequestConfig;
use std::net::{Ipv4Addr, SocketAddr};

/// The relay's own side: which PCs it asks, and what it keeps of their answers.
pub mod driver;
/// The relay's saved assignments, `wake-relay.json`.
pub mod saved;

/// The enrollment a relay runs for one PC, from its assignment. `None` when the secret or a
/// device id is malformed; MACs and addresses that do not parse, or that a relay must not
/// send to (multicast MACs, public addresses), are left out.
pub fn enrollment(assignment: &RelayAssignment) -> Option<Enrollment> {
    let secret = parse_hex::<32>(&assignment.secret)?;
    let devices = assignment
        .devices
        .iter()
        .map(|id| parse_hex::<32>(id))
        .collect::<Option<Vec<_>>>()?;
    Some(
        Enrollment {
            target_id: assignment.target_id.clone(),
            target_name: assignment.target_name.clone(),
            epoch: assignment.epoch,
            secret,
            macs: assignment.macs.iter().filter_map(|mac| parse_mac(mac)).collect(),
            broadcasts: parse_ipv4s(&assignment.broadcasts),
            address: assignment
                .address
                .as_deref()
                .and_then(|address| address.parse::<Ipv4Addr>().ok()),
            http_port: assignment.http_port,
            devices,
        }
        .sanitized(),
    )
}

fn parse_ipv4s(addresses: &[String]) -> Vec<Ipv4Addr> {
    addresses
        .iter()
        .filter_map(|address| address.parse().ok())
        .collect()
}

/// What this device asks `relay` with, from its bundle. `None` when the bundle is malformed.
/// `bind` is where its socket binds (port 0 for any); the DHT and STUN servers are the defaults
/// unless given.
pub fn request_config(
    bundle: &WakeRelayBundle,
    relay: &RelayCredential,
    bind: SocketAddr,
    stun_servers: Vec<String>,
    bootstrap: Vec<String>,
) -> Option<RequestConfig> {
    Some(RequestConfig {
        target_id: bundle.target_id.clone(),
        device: parse_hex::<32>(&bundle.device)?,
        epoch: bundle.epoch,
        secret: parse_hex::<32>(&relay.secret)?,
        known: None,
        bind,
        stun_servers,
        bootstrap,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubylight_traverse::wake_relay::{device_id, device_secret, hex};

    fn assignment() -> RelayAssignment {
        RelayAssignment {
            target_id: "PC-UNIQUE".into(),
            target_name: "Desktop".into(),
            epoch: 3,
            secret: hex(&[7; 32]),
            macs: vec![
                "10:7B:44:00:00:01".into(),
                "01:00:5E:00:00:01".into(),
                "nonsense".into(),
            ],
            broadcasts: vec!["192.168.4.255".into(), "8.8.8.255".into(), "x".into()],
            address: Some("192.168.4.20".into()),
            http_port: 47989,
            devices: vec![hex(&device_id("phone"))],
            refresh_seconds: 300,
        }
    }

    #[test]
    fn a_relay_keeps_only_unicast_macs_and_private_addresses() {
        let enrollment = enrollment(&assignment()).unwrap();
        assert_eq!(enrollment.macs, [[0x10, 0x7b, 0x44, 0, 0, 1]]);
        assert_eq!(enrollment.broadcasts, [Ipv4Addr::new(192, 168, 4, 255)]);
        assert_eq!(enrollment.address, Some(Ipv4Addr::new(192, 168, 4, 20)));
        assert_eq!(enrollment.devices, [device_id("phone")]);
        let mut public = assignment();
        public.address = Some("8.8.8.8".into());
        assert_eq!(enrollment_of(&public).address, None);
    }

    fn enrollment_of(assignment: &RelayAssignment) -> Enrollment {
        enrollment(assignment).unwrap()
    }

    #[test]
    fn a_malformed_secret_or_device_is_no_enrollment() {
        let mut short = assignment();
        short.secret = "abcd".into();
        assert!(enrollment(&short).is_none());
        let mut device = assignment();
        device.devices.push("not hex".into());
        assert!(enrollment(&device).is_none());
    }

    #[test]
    fn the_device_asks_with_the_key_the_relay_derives() {
        let relay_secret = [7; 32];
        let device = device_id("phone");
        let key = device_secret(&relay_secret, "PC-UNIQUE", &device, 3);
        let bundle = WakeRelayBundle {
            target_id: "PC-UNIQUE".into(),
            epoch: 3,
            device: hex(&device),
            relays: vec![RelayCredential {
                id: "relay".into(),
                name: "NUC".into(),
                secret: hex(&key),
            }],
        };
        let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let config =
            request_config(&bundle, &bundle.relays[0], bind, Vec::new(), Vec::new()).unwrap();
        assert_eq!(config.secret, enrollment_of(&assignment()).device_secret(&device));
        assert_eq!(config.device, device);
        let mut broken = bundle.clone();
        broken.device = "zz".into();
        assert!(request_config(&broken, &bundle.relays[0], bind, Vec::new(), Vec::new()).is_none());
    }
}
