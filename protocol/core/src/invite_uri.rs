//! The link a host hands out to invite a friend over the internet:
//! `rubylight://invite#v=1&k=&id=&fp=&name=&exp=&perm=&ports=` and optional path hints.
//!
//! Everything is in the fragment, so no web server ever receives the link, and the custom scheme
//! never leaves the device that opens it. `k` is the invite's 32-byte secret: the client derives
//! the rendezvous secret and the pairing PIN from it (`rubylight_traverse::invite`). `fp` is the
//! SHA-256 of the host certificate's DER: a client must refuse to pair when the certificate the
//! host presents does not hash to it. `name`, `exp` and `perm` are shown in the confirmation only;
//! the host keeps and enforces its own copy. The optional `s`, `e`, `nat`, `v6`, `ip4` and
//! `mapped` let the first path start before the DHT lookup ends.
//!
//! A link can come from any web page or message, so whatever opens one asks first, showing
//! [`InviteUri::shown_name`].

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::{self, Write};
use core::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::pairing_uri::{decode, encode, fingerprint, hex_digit, shown_text};

/// Scheme and path of every invite link; the rest is the fragment.
pub const PREFIX: &str = "rubylight://invite#";
/// The same link as Windows' shell hands it to its handler: it adds the empty path's slash.
const SHELL_PREFIX: &str = "rubylight://invite/#";
/// The link version this module reads and writes.
pub const VERSION: u32 = 1;
/// The longest link read, in bytes.
pub const MAX_LEN: usize = crate::pairing_uri::MAX_LEN;
/// What a guest may do, as the link and the host's API spell it.
pub const PERMISSIONS: [&str; 3] = ["view", "controller", "controller-keyboard-mouse"];
/// At most this many public endpoints and IPv6 addresses are read.
const MAX_ENDPOINTS: usize = 4;
const MAX_IPV6: usize = 2;
/// The longest host ID read.
const MAX_ID_LEN: usize = 64;

/// The host's streaming ports, in the order of `ports=`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InvitePorts {
    pub http: u16,
    pub https: u16,
    pub rtsp: u16,
    pub video: u16,
    pub audio: u16,
    pub control: u16,
    /// 0 when the host takes no microphone.
    pub mic: u16,
}

/// A friend's invitation.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteUri {
    /// The invite's secret.
    pub secret: [u8; 32],
    /// The host's `uniqueid`.
    pub unique_id: String,
    /// SHA-256 of the host certificate's DER.
    pub cert_sha256: [u8; 32],
    /// The host's name, for display.
    pub name: String,
    /// Unix seconds the link stops working, for display.
    pub expires: u64,
    /// One of [`PERMISSIONS`], for display.
    pub permission: String,
    pub ports: InvitePorts,
    /// The host's rendezvous session when the link was made.
    pub session: Option<[u8; 16]>,
    /// The host's public endpoints as STUN saw them.
    pub endpoints: Vec<SocketAddr>,
    /// `cone`, `symmetric` or empty when unknown.
    pub nat: String,
    /// The host's global IPv6 addresses.
    pub ipv6: Vec<Ipv6Addr>,
    /// The router's public IPv4 address.
    pub external_ipv4: Option<Ipv4Addr>,
    /// Every streaming port is forwarded under its own number.
    pub mapped_ports: bool,
}

/// Leaves out the secret, so no log can carry it.
impl fmt::Debug for InviteUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InviteUri")
            .field("unique_id", &self.unique_id)
            .field("name", &self.name)
            .field("expires", &self.expires)
            .field("permission", &self.permission)
            .finish_non_exhaustive()
    }
}

fn push_hex(out: &mut String, bytes: &[u8]) {
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
}

fn hex16(text: &str) -> Option<[u8; 16]> {
    let bytes = text.as_bytes();
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0; 16];
    for (index, pair) in bytes.chunks(2).enumerate() {
        out[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Some(out)
}

fn number<T: core::str::FromStr>(text: &str) -> Option<T> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn list<T: core::str::FromStr>(text: &str, max: usize) -> Option<Vec<T>> {
    let mut out = Vec::new();
    for part in text.split(',').filter(|part| !part.is_empty()) {
        if out.len() == max {
            break;
        }
        out.push(part.parse().ok()?);
    }
    Some(out)
}

impl InviteUri {
    pub fn to_uri(&self) -> String {
        let mut out = String::from(PREFIX);
        let _ = write!(out, "v={VERSION}&k=");
        push_hex(&mut out, &self.secret);
        out.push_str("&id=");
        encode(&mut out, &self.unique_id);
        out.push_str("&fp=");
        push_hex(&mut out, &self.cert_sha256);
        out.push_str("&name=");
        encode(&mut out, &self.name);
        let _ = write!(out, "&exp={}&perm=", self.expires);
        encode(&mut out, &self.permission);
        let p = self.ports;
        let _ = write!(
            out,
            "&ports={},{},{},{},{},{},{}",
            p.http, p.https, p.rtsp, p.video, p.audio, p.control, p.mic
        );
        if let Some(session) = self.session {
            out.push_str("&s=");
            push_hex(&mut out, &session);
        }
        let joined = |out: &mut String, key: &str, items: &mut dyn Iterator<Item = String>| {
            let mut first = true;
            for item in items {
                out.push_str(if first { key } else { "," });
                first = false;
                encode(out, &item);
            }
        };
        joined(
            &mut out,
            "&e=",
            &mut self
                .endpoints
                .iter()
                .take(MAX_ENDPOINTS)
                .map(|e| alloc::format!("{e}")),
        );
        if !self.nat.is_empty() {
            out.push_str("&nat=");
            encode(&mut out, &self.nat);
        }
        joined(
            &mut out,
            "&v6=",
            &mut self
                .ipv6
                .iter()
                .take(MAX_IPV6)
                .map(|a| alloc::format!("{a}")),
        );
        if let Some(ip) = self.external_ipv4 {
            let _ = write!(out, "&ip4={ip}");
        }
        if self.mapped_ports {
            out.push_str("&mapped=1");
        }
        out
    }

    /// Whether `text` begins with the Rubylight invite link scheme.
    pub fn is_link(text: &str) -> bool {
        let text = text.trim();
        text.starts_with(PREFIX) || text.starts_with(SHELL_PREFIX)
    }

    /// Parses a link. Another scheme, more than [`MAX_LEN`] bytes, a version other than 1, a
    /// missing `k`, `id`, `fp`, `exp`, `perm` or `ports`, a secret or fingerprint that is not 64
    /// hex digits, an ID with anything but letters, digits, `-` and `_`, a permission not in
    /// [`PERMISSIONS`], ports that are not seven numbers with nonzero HTTP and HTTPS, a bad
    /// percent escape or address, text that is not UTF-8 and a repeated key reject it. Unknown
    /// keys are ignored.
    pub fn parse(uri: &str) -> Option<Self> {
        let uri = uri.trim();
        if uri.len() > MAX_LEN {
            return None;
        }
        let fragment = uri
            .strip_prefix(PREFIX)
            .or_else(|| uri.strip_prefix(SHELL_PREFIX))?;
        let mut pairs: Vec<(&str, String)> = Vec::new();
        for part in fragment.split('&') {
            let (key, value) = part.split_once('=')?;
            if pairs.iter().any(|(seen, _)| *seen == key) {
                return None;
            }
            pairs.push((key, decode(value)?));
        }
        let take = |key: &str| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.clone())
        };
        let required = |key: &str| take(key).filter(|value| !value.is_empty());
        if required("v")? != "1" {
            return None;
        }
        let unique_id = required("id").filter(|id| {
            id.len() <= MAX_ID_LEN
                && id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        })?;
        let permission = required("perm").filter(|perm| PERMISSIONS.contains(&perm.as_str()))?;
        let ports: Vec<u16> = required("ports")?
            .split(',')
            .map(number::<u16>)
            .collect::<Option<_>>()?;
        let [http, https, rtsp, video, audio, control, mic] = ports[..] else {
            return None;
        };
        if http == 0 || https == 0 {
            return None;
        }
        let session = match take("s") {
            Some(text) => Some(hex16(&text)?),
            None => None,
        };
        let endpoints = list::<SocketAddr>(&take("e").unwrap_or_default(), MAX_ENDPOINTS)?;
        let ipv6 = list::<Ipv6Addr>(&take("v6").unwrap_or_default(), MAX_IPV6)?;
        let external_ipv4 = match take("ip4") {
            Some(text) => Some(text.parse().ok()?),
            None => None,
        };
        let nat = take("nat").unwrap_or_default();
        if !matches!(nat.as_str(), "" | "cone" | "symmetric" | "unknown" | "none") {
            return None;
        }
        Some(Self {
            secret: fingerprint(&required("k")?)?,
            unique_id,
            cert_sha256: fingerprint(&required("fp")?)?,
            name: take("name").unwrap_or_default(),
            expires: number(&required("exp")?)?,
            permission,
            ports: InvitePorts {
                http,
                https,
                rtsp,
                video,
                audio,
                control,
                mic,
            },
            session,
            endpoints,
            nat,
            ipv6,
            external_ipv4,
            mapped_ports: take("mapped").is_some_and(|value| value == "1"),
        })
    }

    /// The link's PC name for a confirmation, as [`crate::pairing_uri::PairingUri::shown_name`]
    /// cleans it. Empty when none is left.
    pub fn shown_name(&self) -> String {
        shown_text(&self.name)
    }

    /// Whether the link's own end has passed at `now` (unix seconds). The host decides; this only
    /// spares the friend a lookup that cannot succeed.
    pub fn expired(&self, now: u64) -> bool {
        now >= self.expires
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::vec;

    fn invite() -> InviteUri {
        let mut secret = [0; 32];
        let mut cert_sha256 = [0; 32];
        for index in 0..32 {
            secret[index] = index as u8;
            cert_sha256[index] = 255 - index as u8;
        }
        InviteUri {
            secret,
            unique_id: "0123ABCD-ef".into(),
            cert_sha256,
            name: "Wohnzimmer-PC ä".into(),
            expires: 1_800_000_000,
            permission: "controller".into(),
            ports: InvitePorts {
                http: 47989,
                https: 47984,
                rtsp: 48010,
                video: 47998,
                audio: 48000,
                control: 47999,
                mic: 0,
            },
            session: Some([7; 16]),
            endpoints: vec![
                "203.0.113.5:40000".parse().unwrap(),
                "[2001:db8::5]:40001".parse().unwrap(),
            ],
            nat: "cone".into(),
            ipv6: vec!["2001:db8::2".parse().unwrap()],
            external_ipv4: Some("203.0.113.5".parse().unwrap()),
            mapped_ports: true,
        }
    }

    #[test]
    fn links_round_trip_and_keep_everything_in_the_fragment() {
        let uri = invite().to_uri();
        assert!(uri.starts_with("rubylight://invite#v=1&k=000102"));
        assert!(!uri.contains('?'), "nothing before the fragment: {uri}");
        assert!(uri.contains("&name=Wohnzimmer-PC%20%C3%A4"));
        assert!(uri.contains("&ports=47989,47984,48010,47998,48000,47999,0"));
        assert_eq!(InviteUri::parse(&uri), Some(invite()));
        let bare = InviteUri {
            session: None,
            endpoints: Vec::new(),
            nat: String::new(),
            ipv6: Vec::new(),
            external_ipv4: None,
            mapped_ports: false,
            ..invite()
        };
        assert_eq!(InviteUri::parse(&bare.to_uri()), Some(bare));
    }

    #[test]
    fn a_link_reads_the_same_after_the_windows_shell_added_its_slash() {
        let uri = invite().to_uri();
        let from_shell = uri.replacen(PREFIX, SHELL_PREFIX, 1);
        assert_eq!(InviteUri::parse(&from_shell), Some(invite()));
        assert!(InviteUri::is_link(&uri) && InviteUri::is_link(&from_shell));
        assert!(InviteUri::is_link(&format!("  {uri}\n")));
        assert!(!InviteUri::is_link("rubylight://pair?v=1"));
        assert_eq!(InviteUri::parse(&uri.replacen('#', "?", 1)), None);
    }

    #[test]
    fn bad_links_are_refused() {
        let good = invite().to_uri();
        let k = "k=0001";
        let cases = [
            good.replacen("v=1", "v=2", 1),
            good.replacen("&id=0123ABCD-ef", "&id=a%2Fb", 1),
            good.replacen("&id=0123ABCD-ef", "", 1),
            good.replacen("&perm=controller", "&perm=admin", 1),
            good.replacen(",0&", ",0,1&", 1),
            good.replacen("ports=47989", "ports=0", 1),
            good.replacen("ports=47989", "ports=x", 1),
            good.replacen("&exp=1800000000", "&exp=-1", 1),
            good.replacen("&nat=cone", "&nat=open", 1),
            good.replacen("&ip4=203.0.113.5", "&ip4=host.example", 1),
            good.replacen("&s=", "&s=0", 1),
            format!("{good}&k=00"),
            format!("{good}&x"),
            good.replacen("k=0001", "k=zz01", 1),
            good.replacen("&k=", "&k=00", 1),
            good.replacen(&k[..4], "k=", 1).replacen("k=0001", "k=", 1),
        ];
        for case in &cases {
            assert_eq!(InviteUri::parse(case), None, "{case}");
        }
        assert_eq!(
            InviteUri::parse(&format!("{good}&{}", "a".repeat(MAX_LEN))),
            None
        );
        let unknown = format!("{good}&future=1");
        assert_eq!(InviteUri::parse(&unknown), Some(invite()));
    }

    #[test]
    fn debug_never_shows_the_secret() {
        let shown = format!("{:?}", invite());
        assert!(!shown.contains("000102"), "{shown}");
        assert!(!shown.contains("secret"), "{shown}");
    }

    #[test]
    fn the_shown_name_is_plain_and_expiry_is_read() {
        let named = InviteUri {
            name: "Living\u{202e}room \n PC".into(),
            ..invite()
        };
        assert_eq!(named.shown_name(), "Livingroom PC");
        assert!(!invite().expired(1_799_999_999));
        assert!(invite().expired(1_800_000_000));
    }
}
