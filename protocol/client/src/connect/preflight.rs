//! Shared preflight probes and sizing helpers: jumbo UDP probe, link-speed
//! first-bitrate test, PyroWave bitrate/link notices, and route packet sizes.

use std::net::IpAddr;

use nightjar_policy::http::{AppControl, AppRole, Feature, ServerInfo};
use nightjar_policy::pyrowave::{self, LinkNotice};
use rubylight_protocol::video_packet::{MAX_VIDEO_PACKET, STOCK_MAX_VIDEO_PACKET};

use crate::http::{
    self, HostClient, JUMBO_PROBE_FPS, JUMBO_PROBE_KBPS, jumbo_probe_passes, jumbo_probe_size,
};
use crate::sdp::VIDEO_FORMAT_MASK_PYROWAVE;
use crate::session;

/// Route selection mode for preflight packet sizing (`Auto`, `Local`, `Remote`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteLocation {
    Auto,
    Local,
    Remote,
}

/// Whether a stream asks for jumbo packets (`--packet-size` above 1400, or the setting).
pub fn asks_for_jumbo(packet_size: u32, jumbo_frames: bool) -> bool {
    packet_size > u32::from(STOCK_MAX_VIDEO_PACKET) || (packet_size == 0 && jumbo_frames)
}

/// Candidate jumbo size (including 32-byte encryption header) capped by host offer and protocol max.
pub fn jumbo_config_size(offered: u16, packet_size: u32) -> Option<u16> {
    if offered <= STOCK_MAX_VIDEO_PACKET {
        return None;
    }
    let wanted = if packet_size > u32::from(STOCK_MAX_VIDEO_PACKET) {
        packet_size
            .min(u32::from(offered))
            .min(u32::from(MAX_VIDEO_PACKET)) as u16
    } else {
        offered
    };
    Some(wanted.min(MAX_VIDEO_PACKET))
}

/// What preflight should do for jumbo video packets before launching.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumboPreflight {
    Probe { candidate: u16 },
    Skip(JumboSkip),
}

/// Why a jumbo request is skipped without probing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumboSkip {
    NotAsked,
    Resume,
    Remote,
    NotOffered,
    NoProbe,
}

impl JumboSkip {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NotAsked => "not asked for",
            Self::Resume => "not probed on a resume, which keeps the running session's packets",
            Self::Remote => "not tried over a remote or public route",
            Self::NotOffered => "asked for but this PC offers no more than stock packets",
            Self::NoProbe => "asked for but this PC cannot test the path",
        }
    }
}

/// Evaluates whether to run the jumbo UDP probe before launching.
#[allow(clippy::too_many_arguments)]
pub fn jumbo_preflight(
    location: RouteLocation,
    address: Option<IpAddr>,
    offered: u16,
    burst_probe: bool,
    packet_size: u32,
    jumbo_frames: bool,
    auto_reconnect: bool,
) -> JumboPreflight {
    if !asks_for_jumbo(packet_size, jumbo_frames) {
        return JumboPreflight::Skip(JumboSkip::NotAsked);
    }
    let local = match location {
        RouteLocation::Local => true,
        RouteLocation::Remote => false,
        RouteLocation::Auto => address.is_some_and(session::is_private),
    };
    if !local {
        return JumboPreflight::Skip(JumboSkip::Remote);
    }
    if auto_reconnect {
        return JumboPreflight::Skip(JumboSkip::Resume);
    }
    let Some(candidate) = jumbo_config_size(offered, packet_size) else {
        return JumboPreflight::Skip(JumboSkip::NotOffered);
    };
    if !burst_probe {
        return JumboPreflight::Skip(JumboSkip::NoProbe);
    }
    JumboPreflight::Probe { candidate }
}

/// Packet size after a failed or skipped jumbo preflight.
pub fn reset_packet_size(route: i32, requested: u32) -> i32 {
    if requested > 0 && requested <= u32::from(STOCK_MAX_VIDEO_PACKET) {
        requested as i32
    } else {
        route
    }
}

/// Packet size remembered in session `Facts` for automatic reconnects.
pub fn facts_packet_size(requested: i32, _announced: i32) -> i32 {
    requested.max(0)
}

/// Packet size asked on `/resume` (keeps the negotiated size when known).
pub fn resume_packet_size(negotiated: u32, route: i32, requested: u32) -> i32 {
    if negotiated > 0 {
        negotiated as i32
    } else {
        reset_packet_size(route, requested)
    }
}

/// Location and base packet size for a route (`1024` over VPN/tunnel, else `1392`).
pub fn location_for_route(location: RouteLocation, vpn_or_tunnel: bool) -> (RouteLocation, i32) {
    match location {
        RouteLocation::Auto if vpn_or_tunnel => (RouteLocation::Remote, 1024),
        other => (other, 1392),
    }
}

/// Android's `GameStreamUiSetup` default `maxPacketSize` before route detection.
pub const ANDROID_DEFAULT_PACKET_SIZE: u32 = 1392;

/// Treats Android's stock default `1392` `maxPacketSize` (`GameStreamUiSetup` /
/// `MicOnlyService`) as unset (`0`) so `location_for_route` resolves `Auto` + VPN/tunnel
/// to `1024`, `jumbo_preflight` probes `8192` on LAN when `jumbo_frames` is enabled, and
/// non-jumbo or explicit `Remote` routes fall back to `1392`, while preserving custom
/// user packet sizes (such as `1200`).
pub const fn effective_android_requested_packet_size(requested: u32) -> u32 {
    if requested == ANDROID_DEFAULT_PACKET_SIZE {
        0
    } else {
        requested
    }
}

/// Result of running the jumbo UDP probe against a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JumboProbeOutcome {
    pub packet_size: i32,
    pub passed: bool,
    pub fallback_notice: bool,
}

/// Runs the jumbo UDP probe + optional 1392-byte control probe using `probe_fn`.
pub fn run_jumbo_probe(
    candidate: u16,
    route_packet_size: i32,
    requested_packet_size: u32,
    jumbo_frames: bool,
    mut probe_fn: impl FnMut(u32, u32, u32, u32) -> Result<http::pace::Measurement, http::Error>,
) -> JumboProbeOutcome {
    let probe_size = jumbo_probe_size(candidate);
    let jumbo = probe_fn(
        JUMBO_PROBE_KBPS,
        probe_size,
        JUMBO_PROBE_FPS,
        JUMBO_PROBE_KBPS,
    );
    match jumbo {
        Ok(measurement) => {
            let control = if probe_size == 1392 {
                None
            } else {
                probe_fn(JUMBO_PROBE_KBPS, 1392, JUMBO_PROBE_FPS, JUMBO_PROBE_KBPS)
                    .ok()
                    .filter(|m| m.measured())
            };
            if jumbo_probe_passes(&measurement, control.as_ref()) {
                eprintln!(
                    "nightjar: jumbo video packets: asking for {candidate} bytes ({probe_size} probed, {:.2}% loss)",
                    measurement.loss
                );
                JumboProbeOutcome {
                    packet_size: i32::from(candidate),
                    passed: true,
                    fallback_notice: false,
                }
            } else {
                let control_loss = control.as_ref().map_or_else(
                    || "no smaller control".to_owned(),
                    |c| format!("{:.2}% at 1392", c.loss),
                );
                eprintln!(
                    "nightjar: jumbo video packets failed the field test ({:.2}% loss at {probe_size}, {control_loss}); streaming normal packets",
                    measurement.loss
                );
                JumboProbeOutcome {
                    packet_size: reset_packet_size(route_packet_size, requested_packet_size),
                    passed: false,
                    fallback_notice: jumbo_frames,
                }
            }
        }
        Err(err) => {
            eprintln!(
                "nightjar: jumbo video packets could not be tested ({err}); streaming normal packets"
            );
            JumboProbeOutcome {
                packet_size: reset_packet_size(route_packet_size, requested_packet_size),
                passed: false,
                fallback_notice: jumbo_frames,
            }
        }
    }
}

/// Runs [`run_jumbo_probe`] directly through a [`HostClient`].
pub fn run_jumbo_probe_with_client(
    client: &HostClient,
    candidate: u16,
    route_packet_size: i32,
    requested_packet_size: u32,
    jumbo_frames: bool,
) -> JumboProbeOutcome {
    run_jumbo_probe(
        candidate,
        route_packet_size,
        requested_packet_size,
        jumbo_frames,
        |kbps, size, fps, pace| client.probe_pace(kbps, size, fps, pace),
    )
}

/// Whether to send `x-rl-video[0].clientWireless` (only hosts supporting it read it).
pub fn tells_host_wireless(
    setting: bool,
    host_supports_wireless_hint: bool,
    wireless: bool,
) -> bool {
    setting && wireless && host_supports_wireless_hint
}

/// Link speed and pacing hints communicated to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkHints {
    pub link_mbps: u32,
    pub sent_link_mbps: u32,
    pub pace_mbps: u32,
}

pub fn link_hints(
    tell: bool,
    routed_mbps: u32,
    link_override: Option<u32>,
    measured_pace: u32,
    pace_override: Option<u32>,
) -> LinkHints {
    let link_mbps = link_override.unwrap_or(routed_mbps);
    LinkHints {
        link_mbps,
        sent_link_mbps: if tell || link_override.is_some() {
            link_mbps
        } else {
            0
        },
        pace_mbps: pace_override.unwrap_or(measured_pace),
    }
}

/// Whether a new stream runs a link-bandwidth probe before `/launch`.
#[allow(clippy::too_many_arguments)]
pub fn probes_first_bitrate(
    link_probe_enabled: bool,
    automatic_bitrate: bool,
    resume_only: bool,
    control: AppControl,
    role: AppRole,
    small_stream_input_only: bool,
    negotiated_video_formats: u32,
    info: &ServerInfo,
) -> bool {
    link_probe_enabled
        && automatic_bitrate
        && !resume_only
        && control == AppControl::None
        && role != AppRole::InputOnly
        && !small_stream_input_only
        && (negotiated_video_formats & VIDEO_FORMAT_MASK_PYROWAVE) == 0
        && info.paired
        && info.capabilities.has(Feature::BandwidthProbe)
}

/// Computes starting bitrate when PyroWave is negotiated, respecting any mobile/client cap.
#[allow(clippy::too_many_arguments)]
pub fn pyro_wave_start_kbps(
    negotiated_video_formats: u32,
    configured_bitrate_kbps: u32,
    automatic_bitrate: bool,
    width: u32,
    height: u32,
    fps: u32,
    link_mbps: u32,
    bitrate_limit_kbps: u32,
) -> u32 {
    if (negotiated_video_formats & VIDEO_FORMAT_MASK_PYROWAVE) == 0 {
        return configured_bitrate_kbps;
    }
    let start = pyrowave::start_kbps(
        configured_bitrate_kbps,
        automatic_bitrate,
        width,
        height,
        fps,
        link_mbps,
    );
    if bitrate_limit_kbps > 0 {
        start.min(bitrate_limit_kbps.max(configured_bitrate_kbps))
    } else {
        start
    }
}

/// Link notice for a PyroWave stream, if the mode asks for more than the link carries.
pub fn pyro_wave_link_notice(
    negotiated_video_formats: u32,
    width: u32,
    height: u32,
    fps: u32,
    link_mbps: u32,
    configured_bitrate_kbps: u32,
    automatic_bitrate: bool,
) -> Option<LinkNotice> {
    if (negotiated_video_formats & VIDEO_FORMAT_MASK_PYROWAVE) == 0 {
        return None;
    }
    let configured = if automatic_bitrate {
        0
    } else {
        configured_bitrate_kbps
    };
    pyrowave::link_notice(width, height, fps, link_mbps, configured)
}

/// Runs the first-bitrate bandwidth probe when allowed, falling back to `configured_kbps` on error.
pub fn first_bitrate_kbps(
    configured_kbps: u32,
    should_probe: bool,
    probe: impl FnOnce() -> Result<f64, http::Error>,
) -> u32 {
    if !should_probe {
        return configured_kbps;
    }
    match probe() {
        Ok(mbps) => {
            let first = http::probe_start_kbps(configured_kbps, mbps);
            eprintln!("nightjar: bandwidth test {mbps:.1} Mbit/s, starting at {first} kbps");
            first
        }
        Err(err) => {
            eprintln!("nightjar: bandwidth test failed, starting at {configured_kbps} kbps: {err}");
            configured_kbps
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jumbo_preflight_checks_route_resume_offer_and_probe() {
        assert_eq!(
            jumbo_preflight(RouteLocation::Local, None, 8192, true, 0, false, false),
            JumboPreflight::Skip(JumboSkip::NotAsked)
        );
        assert_eq!(
            jumbo_preflight(RouteLocation::Remote, None, 8192, true, 0, true, false),
            JumboPreflight::Skip(JumboSkip::Remote)
        );
        assert_eq!(
            jumbo_preflight(RouteLocation::Local, None, 8192, true, 0, true, true),
            JumboPreflight::Skip(JumboSkip::Resume)
        );
        assert_eq!(
            jumbo_preflight(RouteLocation::Local, None, 1400, true, 0, true, false),
            JumboPreflight::Skip(JumboSkip::NotOffered)
        );
        assert_eq!(
            jumbo_preflight(RouteLocation::Local, None, 8192, false, 0, true, false),
            JumboPreflight::Skip(JumboSkip::NoProbe)
        );
        assert_eq!(
            jumbo_preflight(RouteLocation::Local, None, 8192, true, 0, true, false),
            JumboPreflight::Probe { candidate: 8192 }
        );
    }

    #[test]
    fn first_bitrate_falls_back_on_error_and_caps_on_success() {
        assert_eq!(first_bitrate_kbps(40_000, false, || Ok(10.0)), 40_000);
        assert_eq!(
            first_bitrate_kbps(40_000, true, || Err(http::Error::Cancelled)),
            40_000
        );
        assert_eq!(first_bitrate_kbps(40_000, true, || Ok(25.0)), 20_000);
    }

    #[test]
    fn android_auto_remote_default_1392_resolves_to_1024_while_explicit_preserved() {
        let req_auto_vpn = effective_android_requested_packet_size(ANDROID_DEFAULT_PACKET_SIZE);
        assert_eq!(req_auto_vpn, 0);
        let (loc, route_pkt) = location_for_route(RouteLocation::Auto, true);
        assert_eq!(loc, RouteLocation::Remote);
        assert_eq!(route_pkt, 1024);
        assert_eq!(reset_packet_size(route_pkt, req_auto_vpn), 1024);
        assert_eq!(
            jumbo_preflight(loc, None, 8192, true, req_auto_vpn, true, false),
            JumboPreflight::Skip(JumboSkip::Remote)
        );

        // Explicit custom packet size over Auto+VPN stays respected.
        let req_custom = effective_android_requested_packet_size(1200);
        assert_eq!(reset_packet_size(route_pkt, req_custom), 1200);

        // Explicit Remote keeps 1392 and skips jumbo even with jumbo_frames = true.
        let req_remote = effective_android_requested_packet_size(ANDROID_DEFAULT_PACKET_SIZE);
        let (rem_loc, rem_pkt) = location_for_route(RouteLocation::Remote, true);
        assert_eq!(rem_loc, RouteLocation::Remote);
        assert_eq!(reset_packet_size(rem_pkt, req_remote), 1392);
        assert_eq!(
            jumbo_preflight(rem_loc, None, 8192, true, req_remote, true, false),
            JumboPreflight::Skip(JumboSkip::Remote)
        );

        // Auto + LAN and explicit Local with default 1392 probe 8192 when jumbo_frames = true
        // and stay 1392 when jumbo_frames = false.
        let lan_ip = Some("192.168.1.10".parse().unwrap());
        let (auto_lan_loc, auto_lan_pkt) = location_for_route(RouteLocation::Auto, false);
        assert_eq!(auto_lan_loc, RouteLocation::Auto);
        assert_eq!(auto_lan_pkt, 1392);
        assert_eq!(
            jumbo_preflight(auto_lan_loc, lan_ip, 8192, true, req_remote, true, false),
            JumboPreflight::Probe { candidate: 8192 }
        );
        assert_eq!(
            jumbo_preflight(auto_lan_loc, lan_ip, 8192, true, req_remote, false, false),
            JumboPreflight::Skip(JumboSkip::NotAsked)
        );
        assert_eq!(reset_packet_size(auto_lan_pkt, req_remote), 1392);

        let (local_loc, local_pkt) = location_for_route(RouteLocation::Local, false);
        assert_eq!(local_loc, RouteLocation::Local);
        assert_eq!(local_pkt, 1392);
        assert_eq!(
            jumbo_preflight(local_loc, lan_ip, 8192, true, req_remote, true, false),
            JumboPreflight::Probe { candidate: 8192 }
        );
        assert_eq!(reset_packet_size(local_pkt, req_remote), 1392);
    }

    #[test]
    fn jumbo_error_skips_redundant_1392_control_probe() {
        let mut sizes = Vec::new();
        let out = run_jumbo_probe(8192, 1392, 0, true, |_, pkt_size, _, _| {
            sizes.push(pkt_size);
            Err(http::Error::Cancelled)
        });
        assert!(!out.passed);
        assert!(out.fallback_notice);
        assert_eq!(sizes, vec![jumbo_probe_size(8192)]);
    }
}
