use super::{
    Error, Feature, HostClient, MAX_XML_RESPONSE_BYTES, SHORT_CONNECT_TIMEOUT, ServerInfo, pace,
    transport, xml,
};
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

impl HostClient {
    /// Strict pinned HTTPS serverinfo with a whole-call deadline and explicit byte limit
    /// (for remote access probing, without HTTP fallback).
    pub fn server_info_https_strict(
        &self,
        deadline: Duration,
        limit: usize,
    ) -> Result<ServerInfo, Error> {
        let port = self.https_port_cached();
        if port == 0 {
            return Err(Error::NotPaired);
        }
        let target = self.url_target("serverinfo", None);
        let response = transport::probe(
            &transport::Request {
                host: &self.host,
                port,
                target: &target,
                tls: Some(self.tls()?),
                timeouts: transport::Timeouts {
                    connect: SHORT_CONNECT_TIMEOUT.min(deadline),
                    read: Some(deadline),
                },
                body_limit: limit,
            },
            &self.cancel,
            deadline,
            &mut |_, _, _| Ok(()),
        )?;
        match response.status {
            200..=299 => {
                let raw = String::from_utf8(response.body)
                    .map_err(|_| Error::Malformed("host reply is not UTF-8".into()))?;
                ServerInfo::parse(&raw, self.http_port)
            }
            404 => Err(Error::NotFound("/serverinfo".into())),
            code => Err(Error::Status {
                code: i32::from(code),
                message: response.reason,
            }),
        }
    }

    pub fn probe_bandwidth(&self, info: &ServerInfo) -> Result<f64, Error> {
        self.probe_bandwidth_with_progress(info, &mut |_| {})
    }

    pub fn probe_bandwidth_with_progress(
        &self,
        info: &ServerInfo,
        progress: &mut dyn FnMut(u8),
    ) -> Result<f64, Error> {
        let bytes = super::capabilities::PYROWAVE_BANDWIDTH_PROBE_BYTES as usize;
        if !info.paired
            || !info.capabilities.has(Feature::PyroWaveBandwidthProbe)
            || info.pyrowave_bandwidth_probe_bytes as usize != bytes
        {
            return Err(Error::InvalidArgument(
                "This paired PC does not advertise the bandwidth test.".into(),
            ));
        }
        self.probe_bandwidth_raw_with_progress(bytes, progress)
    }

    pub fn probe_bandwidth_raw_with_progress(
        &self,
        bytes: usize,
        progress: &mut dyn FnMut(u8),
    ) -> Result<f64, Error> {
        let target = self.url_target("pyrowave-bandwidth-probe", None);
        let started = Instant::now();
        let mut total: usize = 0;
        let mut prev_percent: Option<u8> = None;
        let received = transport::probe_stream(
            &transport::Request {
                host: &self.host,
                port: self.https_port(true)?,
                target: &target,
                tls: Some(self.tls()?),
                timeouts: transport::Timeouts {
                    connect: Duration::from_secs(5),
                    read: Some(Duration::from_secs(7)),
                },
                body_limit: bytes,
            },
            &self.cancel,
            Duration::from_secs(30),
            &mut |status, headers, _| {
                status_ok(status)?;
                let get = |name: &str| {
                    headers
                        .iter()
                        .find(|(n, _)| n.eq_ignore_ascii_case(name))
                        .map(|(_, v)| v.as_str())
                };
                if !get("Content-Type").is_some_and(|s| {
                    s.split(';')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .eq_ignore_ascii_case("application/octet-stream")
                }) || get("Content-Length")
                    .is_some_and(|s| s.parse::<usize>().ok() != Some(bytes))
                {
                    return Err(Error::Malformed(
                        "Unexpected bandwidth test type or size.".into(),
                    ));
                }
                Ok(())
            },
            &mut |chunk| {
                total += chunk.len();
                let percent = ((total as u64 * 100) / bytes as u64).min(100) as u8;
                if prev_percent != Some(percent) {
                    progress(percent);
                    prev_percent = Some(percent);
                }
                Ok(())
            },
        )?;
        if received as usize != bytes {
            return Err(Error::Malformed(
                "Bandwidth test ended before the advertised size.".into(),
            ));
        }
        Ok(bytes as f64 * 8.0 / started.elapsed().as_secs_f64().max(0.000001) / 1_000_000.0)
    }

    /// Mbit/s of one `GET /bandwidth-probe` of [`LINK_PROBE_BYTES`] (`Feature::BandwidthProbe`, every
    /// codec), timed from the response headers to the end of the body. The host stops writing after
    /// 3 s, so a body that ends early on a slow link is still a measurement; an empty one is not.
    pub fn probe_link(&self, info: &ServerInfo) -> Result<f64, Error> {
        if !info.paired || !info.capabilities.has(Feature::BandwidthProbe) {
            return Err(Error::InvalidArgument(
                "This paired PC does not offer the bandwidth test.".into(),
            ));
        }
        self.probe_link_raw()
    }

    pub fn probe_link_raw(&self) -> Result<f64, Error> {
        let bytes = LINK_PROBE_BYTES as usize;
        let target = self.url_target("bandwidth-probe", Some(&format!("bytes={bytes}")));
        let mut headers_at = None;
        let response = transport::probe(
            &transport::Request {
                host: &self.host,
                port: self.https_port(true)?,
                target: &target,
                tls: Some(self.tls()?),
                timeouts: transport::Timeouts {
                    connect: Duration::from_secs(5),
                    read: Some(Duration::from_secs(5)),
                },
                body_limit: bytes,
            },
            &self.cancel,
            Duration::from_secs(10),
            &mut |status, headers, _| {
                if !(200..=299).contains(&status) {
                    return Err(Error::Status {
                        code: status as i32,
                        message: "The PC refused the bandwidth test.".into(),
                    });
                }
                let content_type = headers
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case("Content-Type"))
                    .map(|(_, v)| v.as_str());
                if !content_type.is_some_and(|s| {
                    s.split(';')
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .eq_ignore_ascii_case("application/octet-stream")
                }) {
                    return Err(Error::Malformed("Unexpected bandwidth test type.".into()));
                }
                headers_at = Some(Instant::now());
                Ok(())
            },
        )?;
        let elapsed = headers_at.map_or(Duration::ZERO, |at| at.elapsed());
        if response.body.is_empty() {
            return Err(Error::Malformed("The bandwidth test sent nothing.".into()));
        }
        Ok(response.body.len() as f64 * 8.0 / elapsed.as_secs_f64().max(0.000001) / 1_000_000.0)
    }

    pub fn probe_pace(
        &self,
        kbps: u32,
        packet_size: u32,
        fps: u32,
        pace_kbps: u32,
    ) -> Result<pace::Measurement, Error> {
        let max_packet = u32::from(rubylight_protocol::video_packet::MAX_VIDEO_PACKET);
        if !(5000..=3_000_000).contains(&kbps)
            || !(256..=max_packet).contains(&packet_size)
            || !(10..=500).contains(&fps)
            || pace_kbps < kbps
            || pace_kbps > 9_999_999
        {
            return Err(Error::InvalidArgument(
                "Invalid PyroWave probe settings.".into(),
            ));
        }
        let tls = self.tls()?;
        let (socket, dual) = match Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP)) {
            Ok(socket) if socket.set_only_v6(false).is_ok() => (socket, true),
            _ => (
                Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?,
                false,
            ),
        };
        socket.set_recv_buffer_size(8 * 1024 * 1024)?;
        socket.bind(
            &if dual { "[::]:0" } else { "0.0.0.0:0" }
                .parse::<SocketAddr>()
                .unwrap()
                .into(),
        )?;
        let socket: UdpSocket = socket.into();
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        let token = transport::random_uuid().replace('-', "");
        let query = format!(
            "kbps={kbps}&port={}&packetsize={packet_size}&token={token}&handshake=1&fps={fps}&pacekbps={pace_kbps}",
            socket.local_addr()?.port()
        );
        let target = self.url_target("pyrowave-udp-probe", Some(&query));
        let expected = pace::expected_packets(kbps, packet_size);
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let mut receiver = None;
            let mut ipv6 = false;
            let response = transport::probe(
                &transport::Request {
                    host: &self.host,
                    port: self.https_port(true)?,
                    target: &target,
                    tls: Some(tls),
                    timeouts: transport::Timeouts {
                        connect: Duration::from_secs(5),
                        read: Some(Duration::from_secs(6)),
                    },
                    body_limit: MAX_XML_RESPONSE_BYTES,
                },
                &self.cancel,
                Duration::from_secs(10),
                &mut |status, headers, peer| {
                    status_ok(status)?;
                    let port = headers
                        .iter()
                        .find(|(name, _)| name.eq_ignore_ascii_case("X-PyroWave-Udp-Port"))
                        .and_then(|(_, value)| udp_port(value))
                        .ok_or_else(|| {
                            Error::Malformed(
                                "The PC did not announce a valid UDP probe port.".into(),
                            )
                        })?;
                    ipv6 = peer.is_ipv6();
                    let mut peer = peer;
                    peer.set_port(port);
                    if dual && let SocketAddr::V4(v4) = peer {
                        peer = SocketAddr::new(v4.ip().to_ipv6_mapped().into(), port);
                    }
                    socket.connect(peer)?;
                    receiver = Some(scope.spawn(|| {
                        receive(
                            &socket,
                            token.as_bytes(),
                            packet_size + 48,
                            expected,
                            &stop,
                            &self.cancel,
                        )
                    }));
                    Ok(())
                },
            );
            if response.is_ok() && !self.cancel.is_cancelled() {
                std::thread::sleep(Duration::from_millis(100));
            }
            stop.store(true, Ordering::Release);
            let arrived = receiver.map(|worker| {
                worker
                    .join()
                    .map_err(|_| Error::Malformed("UDP probe receiver stopped.".into()))
            });
            let response = response?;
            let arrived = arrived.ok_or_else(|| {
                Error::Malformed("The PC did not start the UDP handshake.".into())
            })???;
            let raw = std::str::from_utf8(&response.body)
                .map_err(|_| Error::Malformed("UDP probe result is not UTF-8.".into()))?;
            let mut result = parse_result(raw, expected)?;
            pace::summarize(
                &mut result,
                &arrived,
                fps,
                pace::burst_group(pace_kbps, packet_size, ipv6),
            );
            Ok(result)
        })
    }
}

/// What [`HostClient::probe_link`] asks for: the host's largest probe, so a fast link is past TCP's
/// slow start for most of it.
pub const LINK_PROBE_BYTES: u32 = 4 * 1024 * 1024;
/// The share of a probed link a stream starts at: room for FEC, audio and other traffic.
pub const PROBE_START_SHARE: f64 = 0.8;
/// No probe starts a stream below this.
pub const PROBE_FLOOR_KBPS: u32 = 2000;

/// The pre-flight probe for jumbo video packets: 100 Mbps for two seconds, shaped as 60 frames
/// a second. The same rate measures the 1392-byte control, so only the packet size differs.
pub const JUMBO_PROBE_KBPS: u32 = 100_000;
pub const JUMBO_PROBE_FPS: u32 = 60;

/// The packet size a jumbo stream's pre-flight probe asks for. The host's probe frame is
/// `packetsize + 48` bytes and the stream datagram is the configured size + 64, so the probe asks
/// for the configured size less 32 and its wire size matches the stream's.
pub fn jumbo_probe_size(configured: u16) -> u32 {
    u32::from(configured).saturating_sub(32).max(256)
}

/// Whether jumbo packets passed the pre-flight: the jumbo probe was a valid measurement and lost
/// no more than half a percentage point more than the control. A control that was not measured
/// (the host refused it, or sent too little) accepts only a clean jumbo probe under 0.5%.
pub fn jumbo_probe_passes(jumbo: &pace::Measurement, control: Option<&pace::Measurement>) -> bool {
    if !jumbo.measured() {
        return false;
    }
    match control.filter(|control| control.measured()) {
        Some(control) => jumbo.loss <= control.loss + 0.5,
        None => jumbo.loss < 0.5,
    }
}

/// The first bitrate of a stream whose bitrate is automatic: `configured_kbps`, lowered to
/// [`PROBE_START_SHARE`] of a link probed at `probe_mbps`, never below [`PROBE_FLOOR_KBPS`]. A
/// probe never raises it.
pub fn probe_start_kbps(configured_kbps: u32, probe_mbps: f64) -> u32 {
    let share = (probe_mbps.max(0.0) * 1000.0 * PROBE_START_SHARE).min(f64::from(u32::MAX)) as u32;
    configured_kbps
        .min(share)
        .max(PROBE_FLOOR_KBPS.min(configured_kbps))
}

fn status_ok(status: u16) -> Result<(), Error> {
    if (200..=299).contains(&status) {
        Ok(())
    } else {
        Err(Error::Status {
            code: status as i32,
            message: "The PC refused the network test; stop any active stream and try again."
                .into(),
        })
    }
}

fn udp_port(text: &str) -> Option<u16> {
    (text.len() >= 4 && text.len() <= 5 && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse::<u16>().ok())
        .flatten()
        .filter(|&p| p >= 1024)
}

fn receive(
    socket: &UdpSocket,
    token: &[u8],
    packet_size: u32,
    expected: u32,
    stop: &AtomicBool,
    cancel: &transport::Cancel,
) -> Result<Vec<bool>, Error> {
    let mut arrived = vec![false; expected as usize];
    // The host's probe packet is the token, the sequence number and padding to `packet_size`;
    // a jumbo probe needs a buffer of that size, not stock 2048.
    let mut buffer = vec![0u8; packet_size as usize + 64];
    let mut authenticated = false;
    let mut last_hello = None;
    let mut attempts = 0;
    while !stop.load(Ordering::Acquire) && !cancel.is_cancelled() {
        if !authenticated
            && attempts < 15
            && last_hello.is_none_or(|t: Instant| t.elapsed() >= Duration::from_millis(100))
        {
            socket.send(token)?;
            last_hello = Some(Instant::now());
            attempts += 1;
        }
        match socket.recv(&mut buffer) {
            Ok(n) if n == packet_size as usize && buffer[..32] == *token => {
                authenticated = true;
                let seq = u32::from_be_bytes(buffer[32..36].try_into().unwrap()) as usize;
                if let Some(present) = arrived.get_mut(seq) {
                    *present = true;
                }
            }
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused
                ) || (cfg!(windows) && e.raw_os_error() == Some(10052)) => {}
            Err(e) => return Err(e.into()),
        }
    }
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(arrived)
    }
}

fn parse_result(raw: &str, expected: u32) -> Result<pace::Measurement, Error> {
    let root = xml::parse_reply(raw)?;
    let invalid = || Error::Malformed("Invalid PyroWave UDP probe result.".into());
    let reported = root
        .required_text("expected")?
        .parse::<u32>()
        .map_err(|_| invalid())?;
    let sent = root
        .required_text("sent")?
        .parse::<u32>()
        .map_err(|_| invalid())?;
    let sender_ms = root
        .required_text("elapsedMs")?
        .trim()
        .parse::<f64>()
        .map_err(|_| invalid())?;
    if reported != expected || sent > expected || !sender_ms.is_finite() {
        return Err(invalid());
    }
    Ok(pace::Measurement {
        expected,
        sent,
        sender_ms,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_ports_and_invalid_measurements() {
        for port in [
            "0",
            "80",
            "1023",
            "65536",
            "-1234",
            "+1234",
            "1234 ",
            "999999999999",
        ] {
            assert_eq!(udp_port(port), None);
        }
        assert_eq!(udp_port("47998"), Some(47998));
        let reply = |count, sent, elapsed| {
            format!(
                "<root status_code=\"200\"><expected>{count}</expected><sent>{sent}</sent><elapsedMs>{elapsed}</elapsedMs></root>"
            )
        };
        assert!(parse_result(&reply(100, 99, "2000"), 100).is_ok());
        assert!(parse_result(&reply(101, 100, "2000"), 100).is_err());
        assert!(parse_result(&reply(100, 101, "2000"), 100).is_err());
        assert!(parse_result(&reply(100, 100, "NaN"), 100).is_err());
    }

    #[test]
    fn a_probe_only_lowers_the_first_bitrate() {
        assert_eq!(probe_start_kbps(20_000, 500.0), 20_000);
        assert_eq!(probe_start_kbps(75_000, 50.0), 40_000);
        assert_eq!(probe_start_kbps(20_000, 1.0), PROBE_FLOOR_KBPS);
        assert_eq!(
            probe_start_kbps(1_500, 1.0),
            1_500,
            "never above the setting, also under the floor"
        );
        assert_eq!(probe_start_kbps(20_000, f64::NAN), PROBE_FLOOR_KBPS);
        assert_eq!(probe_start_kbps(u32::MAX, f64::INFINITY), u32::MAX);
    }

    fn measured(loss: f64) -> pace::Measurement {
        pace::Measurement {
            expected: 1000,
            sent: 1000,
            received: 1000,
            sender_ms: 2000.0,
            loss,
            worst_window_loss: 0.0,
            frames: 120,
            worst_frame_loss: 0.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_jumbo_probe_is_taken_at_the_announced_size_and_needs_no_worse_loss() {
        // The stream datagram is the configured size + 64; the probe packet is its size + 48,
        // so the probe asks for 32 less and has the same wire size.
        assert_eq!(jumbo_probe_size(8192), 8160);
        assert_eq!(jumbo_probe_size(1424), 1392);
        assert_eq!(jumbo_probe_size(1392), 1360);
        assert_eq!(jumbo_probe_size(0), 256);

        assert!(jumbo_probe_passes(&measured(0.2), Some(&measured(0.1))));
        assert!(
            jumbo_probe_passes(&measured(0.6), Some(&measured(0.1))),
            "0.5 points more is allowed"
        );
        assert!(!jumbo_probe_passes(&measured(0.7), Some(&measured(0.1))));
        // A control that was not measured accepts only a clean jumbo probe.
        let unmeasured = pace::Measurement {
            sender_ms: 1000.0,
            ..measured(0.0)
        };
        assert!(!unmeasured.measured());
        assert!(jumbo_probe_passes(&measured(0.2), Some(&unmeasured)));
        assert!(!jumbo_probe_passes(&measured(0.7), Some(&unmeasured)));
        // A 1424-byte offer probes at 1392 with no smaller control; only a clean probe passes.
        assert!(jumbo_probe_passes(&measured(0.2), None));
        assert!(!jumbo_probe_passes(&measured(0.7), None));
        let mut invalid = measured(0.0);
        invalid.sent = 900;
        assert!(!invalid.measured());
        assert!(
            !jumbo_probe_passes(&invalid, Some(&measured(0.0))),
            "a jumbo probe the host did not send is no pass"
        );
    }
}
