//! Wake-on-LAN planning (with DNS resolution), UDP burst sending, and persistent
//! `wake_targets.json` storage shared across Nightjar clients.

pub mod store;
#[cfg(test)]
mod tests;

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV6, ToSocketAddrs, UdpSocket};
use std::time::Duration;

pub use nightjar_policy::wol::{
    Links, MAX_BROADCASTS, MAX_MACS, NO_MAC, Plan, WAKE_POLL_INTERVAL_MS, WakeLaunch, WakeStep,
    WakeTargetsPollState, WakeWait, Wakes, Waking, build_plan, build_plan_sockets, extract_targets,
    magic_packet, normal_mac, parse_ipv4_literal, parse_macs, parse_scoped_ipv6_literal, ports,
    targets,
};
pub use store::{WakeTargetEntry, WakeTargetsStore};

/// Host inputs needed to plan a Wake-on-LAN burst.
pub struct WakeHost<'a> {
    pub mac: Option<&'a str>,
    pub endpoints: &'a [(String, u16)],
    pub wake_macs: &'a [String],
    pub wake_broadcasts: &'a [String],
    pub own_wake_macs: &'a [String],
    pub fallback_http_port: u16,
}

#[cfg(unix)]
fn scope_id_from_name(name: &str) -> u32 {
    let Ok(c_name) = std::ffi::CString::new(name) else {
        return 0;
    };
    // SAFETY: `c_name` is a valid NUL-terminated string passed to POSIX `if_nametoindex`.
    unsafe { libc::if_nametoindex(c_name.as_ptr()) }
}

#[cfg(not(unix))]
fn scope_id_from_name(_name: &str) -> u32 {
    0
}

/// Parses an endpoint IP literal (IPv4, IPv6, `[IPv6]`, or `IPv6%<scope>` / `[IPv6%<scope>]`),
/// resolving numeric scope IDs directly and interface-name scopes on Unix (falling back
/// gracefully to `scope_id = 0` so link-local IPv6 destinations are never silently dropped).
pub fn resolve_endpoint_literal(address: &str, port: u16) -> Option<SocketAddr> {
    let trimmed = address.trim();
    if let Ok(ip) = trimmed.parse::<std::net::IpAddr>() {
        return Some(SocketAddr::new(ip, port));
    }
    if let Some((ip, num_scope, name_scope)) = parse_scoped_ipv6_literal(trimmed) {
        let scope_id = num_scope
            .or_else(|| name_scope.map(scope_id_from_name))
            .unwrap_or(0);
        return Some(SocketAddr::V6(SocketAddrV6::new(ip, port, 0, scope_id)));
    }
    None
}

/// Resolves `host.endpoints` via IP literal parsing (preserving IPv6 `%scope_id`) or DNS and
/// delegates to [`build_plan_sockets`].
pub fn plan(host: &WakeHost<'_>, links: &Links) -> Result<Plan, String> {
    let macs: Vec<&str> = host
        .mac
        .into_iter()
        .chain(host.wake_macs.iter().map(String::as_str))
        .chain(host.own_wake_macs.iter().map(String::as_str))
        .collect();

    let mut resolved_endpoints = Vec::new();
    let mut endpoint_ports = Vec::new();
    for (address, port) in host.endpoints {
        endpoint_ports.push(*port);
        if let Some(socket) = resolve_endpoint_literal(address, *port) {
            resolved_endpoints.push(socket);
            continue;
        }
        match (address.as_str(), 9).to_socket_addrs() {
            Ok(resolved) => {
                for mut addr in resolved {
                    addr.set_port(*port);
                    resolved_endpoints.push(addr);
                }
            }
            Err(e) => eprintln!("nightjar: Wake-on-LAN skips {address}: {e}"),
        }
    }
    if endpoint_ports.is_empty() {
        endpoint_ports.push(host.fallback_http_port);
    }

    let wake_broadcasts: Vec<&str> = host.wake_broadcasts.iter().map(String::as_str).collect();
    build_plan_sockets(
        &macs,
        &resolved_endpoints,
        &endpoint_ports,
        &wake_broadcasts,
        links,
    )
}

/// Sends `plan` in a 3-burst loop using `send`; fails only when no primary destination
/// succeeded (`best_effort` IPv6 all-nodes failures alone do not fail the wake).
pub fn send(
    plan: &Plan,
    send: impl FnMut(&[u8], SocketAddr) -> io::Result<()>,
) -> Result<(), String> {
    send_with_gap(plan, Duration::ZERO, send)
}

fn send_with_gap(
    plan: &Plan,
    burst_gap: std::time::Duration,
    mut send: impl FnMut(&[u8], SocketAddr) -> io::Result<()>,
) -> Result<(), String> {
    let mut sent = false;
    let mut error = String::from("No Wake-on-LAN destination could be reached.");
    let mut best_effort_failures = 0usize;
    for burst in 0..3 {
        if burst > 0 && !burst_gap.is_zero() {
            std::thread::sleep(burst_gap);
        }
        for payload in &plan.payloads {
            for destination in &plan.destinations {
                match send(payload, *destination) {
                    Ok(()) => sent = true,
                    Err(e) => error = e.to_string(),
                }
            }
            for destination in &plan.best_effort {
                if send(payload, *destination).is_err() {
                    best_effort_failures += 1;
                }
            }
        }
    }
    if best_effort_failures > 0 {
        eprintln!("nightjar: Wake-on-LAN: {best_effort_failures} IPv6 all-nodes sends failed");
    }
    if sent { Ok(()) } else { Err(error) }
}

/// Binds IPv4 (broadcast enabled) and optional IPv6 UDP sockets and transmits `plan`.
pub fn send_udp(plan: &Plan) -> Result<(), String> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    let ipv6 = UdpSocket::bind("[::]:0").ok();
    send_with_gap(plan, std::time::Duration::from_millis(10), |payload, destination| {
        let sock = if destination.is_ipv4() {
            &socket
        } else {
            ipv6.as_ref()
                .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "no IPv6 socket"))?
        };
        sock.send_to(payload, destination).map(|_| ())
    })
}
