//! RTSP message parsing, responses, and DESCRIBE formatting shared by host and client.
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;

pub const MAX_MESSAGE: usize = 65536;

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub target: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
    pub cseq: u32,
}

pub fn complete_length(b: &[u8]) -> Result<Option<usize>> {
    if b.len() > MAX_MESSAGE {
        bail!("RTSP message too large");
    }
    let Some(i) = b.windows(4).position(|b| b == b"\r\n\r\n") else {
        return Ok(None);
    };
    let total = message_length(b, i)?;
    Ok(if b.len() >= total { Some(total) } else { None })
}

fn message_length(b: &[u8], i: usize) -> Result<usize> {
    let head = std::str::from_utf8(&b[..i])?;
    let mut length = 0;
    let mut seen = false;
    for l in head.lines().skip(1) {
        if let Some((k, v)) = l.split_once(':')
            && k.eq_ignore_ascii_case("Content-Length")
        {
            if seen {
                bail!("duplicate Content-Length");
            }
            seen = true;
            length = v.trim().parse::<usize>()?;
        }
    }
    let total = (i + 4)
        .checked_add(length)
        .context("RTSP length overflow")?;
    if total > MAX_MESSAGE {
        bail!("RTSP message too large");
    }
    Ok(total)
}

/// Incremental framing for one RTSP request. Search new header bytes only and
/// parse Content-Length once, even when the peer fragments a large request.
#[derive(Default)]
pub struct MessageBuffer {
    bytes: Vec<u8>,
    expected: Option<usize>,
}

impl MessageBuffer {
    pub fn append(&mut self, bytes: &[u8]) -> Result<bool> {
        if bytes.len() > MAX_MESSAGE - self.bytes.len() {
            bail!("RTSP message too large");
        }
        let scan_from = self.bytes.len().saturating_sub(3);
        self.bytes.extend_from_slice(bytes);
        if self.expected.is_none()
            && let Some(offset) = self.bytes[scan_from..]
                .windows(4)
                .position(|b| b == b"\r\n\r\n")
        {
            self.expected = Some(message_length(&self.bytes, scan_from + offset)?);
        }
        if let Some(total) = self.expected {
            if self.bytes.len() > total {
                bail!("unexpected RTSP trailing data");
            }
            return Ok(self.bytes.len() == total);
        }
        Ok(false)
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl Request {
    pub fn parse(b: &[u8]) -> Result<Self> {
        let n = complete_length(b)?.context("incomplete RTSP message")?;
        if n != b.len() {
            bail!("multiple messages passed to RTSP parser");
        }
        let i = b.windows(4).position(|b| b == b"\r\n\r\n").unwrap();
        let head = std::str::from_utf8(&b[..i])?;
        let mut lines = head.lines();
        let parts: Vec<_> = lines
            .next()
            .context("empty request")?
            .split_whitespace()
            .collect();
        if parts.len() != 3 || parts[2] != "RTSP/1.0" {
            bail!("invalid RTSP request line");
        }
        let mut headers = BTreeMap::new();
        for l in lines {
            let (k, v) = l.split_once(':').context("invalid RTSP header")?;
            if headers
                .insert(k.to_ascii_lowercase(), v.trim().to_owned())
                .is_some()
            {
                bail!("duplicate RTSP header");
            }
        }
        let cseq = headers.get("cseq").context("missing CSeq")?.parse()?;
        Ok(Self {
            method: parts[0].into(),
            target: parts[1].into(),
            headers,
            body: b[i + 4..].to_vec(),
            cseq,
        })
    }
}

pub fn response(
    cseq: u32,
    code: u16,
    reason: &str,
    headers: &[(&str, String)],
    body: &[u8],
) -> Vec<u8> {
    let mut s = format!("RTSP/1.0 {code} {reason}\r\nCSeq: {cseq}\r\n");
    for (k, v) in headers {
        s.push_str(&format!("{k}: {v}\r\n"));
    }
    if !body.is_empty() {
        s.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    s.push_str("\r\n");
    let mut b = s.into_bytes();
    b.extend_from_slice(body);
    b
}

/// The Rubylight control messages this host takes, by id: phase lock reports
/// (0x5530), display luminance (0x5531), reconfiguration (0x5532) and USB
/// controller speaker, microphone and status (0x5533–0x5535), and host timing
/// requests (0x5536), which it answers with reports, and clock sync probes (0x5537),
/// which it answers at once, and link notice requests (0x5538), after which it tells the
/// client when it lowers the video bitrate for the network.
pub const CONTROL_MESSAGES: &str =
    "a=x-rl-control:0x5530,0x5531,0x5532,0x5533,0x5534,0x5535,0x5536,0x5537,0x5538\r\n";

/// Control messages offered when the host's network bitrate adaptation (`bitrate_adaptation = off`)
/// is disabled: `0x5538` link notices are omitted so clients do not expect host bitrate cuts.
pub const CONTROL_MESSAGES_WITHOUT_LINK_NOTICE: &str =
    "a=x-rl-control:0x5530,0x5531,0x5532,0x5533,0x5534,0x5535,0x5536,0x5537\r\n";

/// The `DESCRIBE` answer. `mic` is set when the host takes a client's
/// microphone: its encryption is then supported and requested, and the
/// caller ends the answer with `mic::sdp`, whose `m=` line must come last.
/// `roaming` offers client roaming (`config.client_roaming()`).
#[allow(clippy::too_many_arguments)]
pub fn describe(
    feature_flags: u32,
    supported: u32,
    requested: u32,
    hevc: bool,
    av1: bool,
    pyrowave: bool,
    mic: bool,
    roaming: bool,
) -> String {
    describe_with_link_notice(
        feature_flags,
        supported,
        requested,
        hevc,
        av1,
        pyrowave,
        mic,
        roaming,
        true,
    )
}

/// Like [`describe`], advertising `0x5538` link notices only when `link_notice` is set.
#[allow(clippy::too_many_arguments)]
pub fn describe_with_link_notice(
    feature_flags: u32,
    supported: u32,
    requested: u32,
    hevc: bool,
    av1: bool,
    pyrowave: bool,
    mic: bool,
    roaming: bool,
    link_notice: bool,
) -> String {
    let (supported, requested) = if mic {
        (
            supported | crate::mic::ENCRYPTION,
            requested | crate::mic::ENCRYPTION,
        )
    } else {
        (supported, requested)
    };
    let mut s = format!(
        "a=x-ss-general.featureFlags:{feature_flags}\r\na=x-ss-general.encryptionSupported:{supported}\r\na=x-ss-general.encryptionRequested:{requested}\r\n"
    );
    // Rubylight's own control messages (rubylight-protocol); a client sends them
    // only to a host that lists them.
    s.push_str(if link_notice {
        CONTROL_MESSAGES
    } else {
        CONTROL_MESSAGES_WITHOUT_LINK_NOTICE
    });
    // Client roaming, wire format v1; clients that don't know it ignore it.
    if roaming {
        s.push_str(rubylight_protocol::roaming::SDP_ATTRIBUTE);
        s.push_str("\r\n");
    }
    if hevc {
        s.push_str("sprop-parameter-sets=AAAAAU\r\n");
    }
    if av1 {
        s.push_str("a=rtpmap:98 AV1/90000\r\n");
    }
    if pyrowave {
        s.push_str("a=rtpmap:99 PYROWAVE/90000\r\na=x-ss-pyrowave.bitstream:186f0393\r\n");
    }
    s.push_str("a=fmtp:97 surround-params=21101\r\na=fmtp:97 surround-params=642014523\r\na=fmtp:97 surround-params=85301456723\r\na=fmtp:97 surround-params=21101\r\na=fmtp:97 surround-params=660014523\r\na=fmtp:97 surround-params=88001456723\r\n");
    s
}
