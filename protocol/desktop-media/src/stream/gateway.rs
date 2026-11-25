//! This device's gateway, while the PC is on the same link: what tells a PC whose server closed
//! after its goodbye from a dropout on this side ([`nightjar_core::host_loss`]). A PC behind a
//! router, a tunnel or over IPv6 gets no gateway, and then the old behaviour decides.

use std::net::{IpAddr, Ipv4Addr};

pub fn toward(host: IpAddr) -> Option<IpAddr> {
    let IpAddr::V4(host) = host else { return None };
    let interfaces = if_addrs::get_if_addrs().ok()?;
    let on_link: Vec<(&str, Ipv4Addr)> = interfaces
        .iter()
        .filter(|i| i.is_oper_up() && !i.is_loopback())
        .filter_map(|i| match &i.addr {
            if_addrs::IfAddr::V4(v4) if same_subnet(v4.ip, v4.netmask, host) => Some((i.name.as_str(), v4.ip)),
            _ => None,
        })
        .collect();
    if on_link.is_empty() {
        return None;
    }
    default_gateway_on(&on_link).filter(|gateway| *gateway != host).map(IpAddr::V4)
}

fn same_subnet(ip: Ipv4Addr, netmask: Ipv4Addr, host: Ipv4Addr) -> bool {
    let mask = u32::from(netmask);
    mask != 0 && u32::from(ip) & mask == u32::from(host) & mask
}

#[cfg(windows)]
fn interface_index_v4(addr: Ipv4Addr) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::GetBestInterfaceEx;
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR, SOCKADDR_IN};
    let mut index = 0u32;
    let mut sock: SOCKADDR_IN = unsafe { std::mem::zeroed() };
    sock.sin_family = AF_INET;
    sock.sin_addr.S_un.S_addr = u32::from_ne_bytes(addr.octets());
    // SAFETY: `sock` is initialized and `index` is valid for the duration of the query.
    let status = unsafe { GetBestInterfaceEx((&raw const sock).cast::<SOCKADDR>(), &mut index) };
    (status == 0).then_some(index)
}

#[cfg(windows)]
fn default_gateway_on(on_link: &[(&str, Ipv4Addr)]) -> Option<Ipv4Addr> {
    use windows::Win32::NetworkManagement::IpHelper::{GetBestRoute, MIB_IPFORWARDROW};
    let mut row = MIB_IPFORWARDROW::default();
    // The next hop toward any address off the link. Nothing is sent.
    // SAFETY: `row` outlives the call.
    let result = unsafe { GetBestRoute(u32::from_ne_bytes([192, 0, 2, 1]), None, &mut row) };
    if result != 0 || row.dwForwardNextHop == 0 {
        return None;
    }
    let matches_on_link = on_link
        .iter()
        .any(|(_, ip)| interface_index_v4(*ip) == Some(row.dwForwardIfIndex));
    matches_on_link.then(|| Ipv4Addr::from(row.dwForwardNextHop.to_ne_bytes()))
}

#[cfg(target_os = "linux")]
fn default_gateway_on(on_link: &[(&str, Ipv4Addr)]) -> Option<Ipv4Addr> {
    let table = std::fs::read_to_string("/proc/net/route").ok()?;
    parse_proc_route_for_interfaces(&table, on_link)
}

#[cfg(not(any(windows, target_os = "linux")))]
fn default_gateway_on(_on_link: &[(&str, Ipv4Addr)]) -> Option<Ipv4Addr> {
    None
}

/// The lowest-metric default route's `(iface, gateway)` in `/proc/net/route`, whose addresses are
/// little-endian hex.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_proc_route_entries(table: &str) -> Vec<(&str, Ipv4Addr, u32)> {
    let mut routes: Vec<(&str, Ipv4Addr, u32)> = table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let iface = fields.next()?;
            let destination = fields.next()?;
            let gateway = u32::from_str_radix(fields.next()?, 16).ok()?;
            let _flags = fields.next();
            let _refcnt = fields.next();
            let _use_count = fields.next();
            let metric = fields.next().and_then(|m| m.parse::<u32>().ok()).unwrap_or(0);
            (destination == "00000000" && gateway != 0)
                .then(|| (iface, Ipv4Addr::from(gateway.to_le_bytes()), metric))
        })
        .collect();
    routes.sort_by_key(|(_, _, metric)| *metric);
    routes
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_proc_route_for_interfaces(table: &str, on_link: &[(&str, Ipv4Addr)]) -> Option<Ipv4Addr> {
    parse_proc_route_entries(table)
        .into_iter()
        .find(|(iface, _, _)| on_link.iter().any(|(name, _)| name == iface))
        .map(|(_, gateway, _)| gateway)
}

#[cfg(test)]
fn parse_proc_route(table: &str) -> Option<Ipv4Addr> {
    parse_proc_route_entries(table).into_iter().next().map(|(_, gateway, _)| gateway)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_on_the_subnet_is_on_the_link() {
        let ip = Ipv4Addr::new(192, 168, 4, 12);
        let mask = Ipv4Addr::new(255, 255, 255, 0);
        assert!(same_subnet(ip, mask, Ipv4Addr::new(192, 168, 4, 69)));
        assert!(!same_subnet(ip, mask, Ipv4Addr::new(192, 168, 5, 69)));
        assert!(!same_subnet(ip, Ipv4Addr::UNSPECIFIED, Ipv4Addr::new(192, 168, 4, 69)));
    }

    #[test]
    fn the_default_route_gives_the_gateway() {
        let table = "Iface\tDestination\tGateway \tFlags\n\
            wlan0\t0004A8C0\t00000000\t0001\n\
            wlan0\t00000000\t0104A8C0\t0003\n";
        assert_eq!(parse_proc_route(table), Some(Ipv4Addr::new(192, 168, 4, 1)));
        assert_eq!(parse_proc_route("Iface\tDestination\tGateway\nwlan0\t0004A8C0\t00000000\n"), None);
    }

    #[test]
    fn default_route_picks_lowest_metric_and_requires_on_link_interface() {
        let table = "Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
            wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
            eth0\t00000000\t0102A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n";
        assert_eq!(parse_proc_route(table), Some(Ipv4Addr::new(192, 168, 2, 1)));
        assert_eq!(
            parse_proc_route_for_interfaces(table, &[("wlan0", Ipv4Addr::new(192, 168, 1, 20))]),
            Some(Ipv4Addr::new(192, 168, 1, 1))
        );
        assert_eq!(
            parse_proc_route_for_interfaces(table, &[("wg0", Ipv4Addr::new(10, 0, 0, 2))]),
            None,
            "a default gateway on another interface (or VPN) does not belong to the PC's link"
        );
    }

    #[test]
    fn nothing_for_ipv6_or_a_host_off_every_link() {
        assert_eq!(toward("fe80::1".parse().unwrap()), None);
        assert_eq!(toward(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 200))), None);
    }
}
