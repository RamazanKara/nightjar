//! Wake-on-LAN socket sender (`if_addrs` + `nightjar_core::wol`), re-exporting pure
//! MAC/target/wait types from `nightjar_core::wol`.

pub use nightjar_core::wol::{WakeStep as Step, WakeWait, Wakes, targets};

use nightjar_core::wol::{Links, WakeHost, plan, send_udp};
use nightjar_desktop_config::config::HostRecord;

fn read_links() -> Links {
    let mut links = Links::default();
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => {
            for interface in interfaces
                .iter()
                .filter(|i| i.is_oper_up() && !i.is_loopback())
            {
                match &interface.addr {
                    if_addrs::IfAddr::V4(v4) => links.broadcasts.extend(v4.broadcast),
                    if_addrs::IfAddr::V6(_) => links.ipv6.extend(interface.index),
                }
            }
        }
        Err(e) => eprintln!("nightjar: Wake-on-LAN cannot list the network interfaces: {e}"),
    }
    links
}

/// Sends the magic packets for the PC (blocks). Fails when the PC has no usable MAC, or when no
/// packet could be sent at all.
pub fn wake(host: &HostRecord) -> Result<(), String> {
    let endpoints = collect_endpoints(host);
    let wake_host = WakeHost {
        mac: host.mac.as_deref(),
        endpoints: &endpoints,
        wake_macs: &host.wake_macs,
        wake_broadcasts: &host.wake_broadcasts,
        own_wake_macs: &host.own_wake_macs,
        fallback_http_port: host.http_port,
    };
    let plan = plan(&wake_host, &read_links())?;
    send_udp(&plan)
}

fn collect_endpoints(host: &HostRecord) -> Vec<(String, u16)> {
    let mut out = Vec::new();
    let mut push = |addr: &str, port: u16| {
        let trimmed = addr.trim();
        if trimmed.is_empty() {
            return;
        }
        let port = if port == 0 { nightjar_core::http::DEFAULT_HTTP_PORT } else { port };
        if !out.iter().any(|(a, p): &(String, u16)| a.eq_ignore_ascii_case(trimmed) && *p == port) {
            out.push((trimmed.to_owned(), port));
        }
    };
    push(&host.address, host.http_port);
    for addr in &host.other_addresses {
        push(addr, host.http_port);
    }
    if let Some(remote) = &host.remote_address {
        push(remote, host.http_port);
    }
    if let Some(pinned) = &host.pinned_address {
        push(pinned, host.http_port);
    }
    for slot in [
        &host.local_endpoint,
        &host.manual_endpoint,
        &host.ipv6_endpoint,
        &host.remote_endpoint,
        &host.pinned_endpoint,
    ]
    .into_iter()
    .flatten()
    {
        push(&slot.address, slot.port);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::collect_endpoints;
    use nightjar_desktop_config::config::HostRecord;
    use nightjar_policy::hosts::EndpointSlot;

    #[test]
    fn collect_endpoints_includes_all_address_slots_and_deduplicates() {
        let host = HostRecord {
            address: "192.168.1.10".into(),
            http_port: 47989,
            other_addresses: vec!["10.0.0.10".into(), "192.168.1.10".into()],
            remote_address: Some("203.0.113.5".into()),
            pinned_address: Some("198.51.100.7".into()),
            local_endpoint: Some(EndpointSlot { address: "192.168.1.11".into(), port: 47989 }),
            manual_endpoint: Some(EndpointSlot { address: "10.0.0.11".into(), port: 47990 }),
            ipv6_endpoint: Some(EndpointSlot { address: "2001:db8::1".into(), port: 47989 }),
            remote_endpoint: Some(EndpointSlot { address: "203.0.113.5".into(), port: 47989 }),
            pinned_endpoint: Some(EndpointSlot { address: "198.51.100.8".into(), port: 48010 }),
            ..Default::default()
        };
        let got = collect_endpoints(&host);
        assert_eq!(
            got,
            vec![
                ("192.168.1.10".into(), 47989),
                ("10.0.0.10".into(), 47989),
                ("203.0.113.5".into(), 47989),
                ("198.51.100.7".into(), 47989),
                ("192.168.1.11".into(), 47989),
                ("10.0.0.11".into(), 47990),
                ("2001:db8::1".into(), 47989),
                ("198.51.100.8".into(), 48010),
            ]
        );
    }
}
