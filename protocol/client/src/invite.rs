//! Opening a friend's invite (`rubylight://invite#...`, `rubylight_protocol::invite_uri`), shared
//! by desktop and Android: reach the PC over the internet with the rendezvous secret the link
//! carries, pair with the one-time PIN it carries while the PC's certificate must hash to the
//! link's fingerprint, then fetch this device's own internet play details over the pinned
//! HTTPS, so the PC is saved like any paired PC and reached again after the link is spent.
//!
//! The PC pairs an invited device as a guest only: it may join the game being played when the
//! player lets it in, and nothing else, until the invite's time ends.

use crate::http::{ClientIdentity, Error, HostClient, InternetPlay, PairResult};
use crate::internet::{self, InternetPath};
use rubylight_protocol::{invite_uri::InviteUri, pairing_uri::PairingUri};
use rubylight_traverse::invite::{pairing_pin, rendezvous_secret, PASSPHRASE};
use serde_json::json;
use std::{net::SocketAddr, sync::Arc};

/// The words for a link that is no invite.
pub const NOT_AN_INVITE: &str = "This is not a Nightjar invite link";
/// The words for an invite whose time has passed.
pub const EXPIRED: &str = "This invite has expired. Ask for a new one.";
/// The words for an invite the PC refused: used, revoked or past its time.
pub const REFUSED: &str = "The PC did not accept this invite: it was already used, taken back or has expired. Ask for a new one.";
/// The words for this device's own internet setting being off.
pub const INTERNET_OFF: &str =
    "Turn on Play over the internet in Nightjar's settings, then open the invite again.";

/// The words for a PC that paired and then said it is another PC than the link names.
pub const ANOTHER_PC: &str =
    "The PC that answered is not the one this invite names, so Nightjar did not keep it.";

/// How often the device's own internet details are asked for after pairing.
const DETAILS_TRIES: usize = 3;

/// A PC reached and paired through an invite.
#[derive(Clone, PartialEq, Eq)]
pub struct Redeemed {
    /// The PC's `uniqueid`.
    pub unique_id: String,
    /// The PC's name as it answered.
    pub name: String,
    /// Where the PC answered: its IPv6 address, its forwarded IPv4 address, or this device's
    /// loopback end of a punched path.
    pub address: SocketAddr,
    pub path: InternetPath,
    pub https_port: u16,
    /// The PC's certificate (PEM), checked against the link and pinned.
    pub server_certificate_pem: String,
    /// This device's own internet play reply, as `/internet-play/v1` answered it: keep it as a
    /// paired PC's reply is kept.
    pub reply: String,
}

/// Leaves out the reply, which holds this device's secret.
impl std::fmt::Debug for Redeemed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redeemed")
            .field("unique_id", &self.unique_id)
            .field("name", &self.name)
            .field("address", &self.address)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// Reads a link the user opened or pasted.
pub fn parse(text: &str) -> Result<InviteUri, &'static str> {
    InviteUri::parse(text).ok_or(NOT_AN_INVITE)
}

/// The internet play reply a link stands for until the PC gives this device its own: the PC's
/// ports and addresses from the link, and the rendezvous secret derived from the link's secret.
pub fn reply_of(link: &InviteUri) -> String {
    let p = link.ports;
    let mut reply = json!({
        "version": 1,
        "enabled": true,
        "portMapping": link.mapped_ports,
        "externalIpv4": link.external_ipv4.map(|ip| ip.to_string()),
        "mappedPorts": link.mapped_ports,
        "ipv6": link.ipv6.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "ports": {
            "http": p.http, "https": p.https, "rtsp": p.rtsp, "video": p.video,
            "audio": p.audio, "control": p.control, "mic": p.mic,
        },
        "secret": hex(&rendezvous_secret(&link.secret)),
        "endpoints": link.endpoints.iter().map(ToString::to_string).collect::<Vec<_>>(),
        "nat": if link.nat.is_empty() { "unknown" } else { link.nat.as_str() },
    });
    if let Some(session) = link.session {
        reply["session"] = json!(hex(&session));
    }
    reply.to_string()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The pairing a link stands for: its one-time PIN and the PC certificate's fingerprint, at
/// `address`.
fn pairing(link: &InviteUri, address: SocketAddr) -> PairingUri {
    PairingUri {
        host: address.ip().to_string(),
        port: address.port(),
        otp: pairing_pin(&link.secret),
        passphrase: PASSPHRASE.to_owned(),
        unique_id: link.unique_id.clone(),
        cert_sha256: link.cert_sha256,
        name: link.name.clone(),
    }
}

fn pair_error(error: &Error) -> String {
    match error {
        Error::CertificateMismatch => {
            "The PC that answered is not the one that made this invite".to_owned()
        }
        Error::Status { code: 400, .. } => REFUSED.to_owned(),
        other => format!("Pairing over the internet failed: {other}"),
    }
}

/// Opens `link` (blocking, up to the punched path's wait and a pairing): the PC answers on the
/// first path that works, pairs this device as an invited guest and gives it its own internet
/// play details. `now` is the time in unix seconds.
pub fn redeem(
    link: &InviteUri,
    identity: Arc<ClientIdentity>,
    device_name: &str,
    now: u64,
) -> Result<Redeemed, String> {
    if link.expired(now) {
        return Err(EXPIRED.to_owned());
    }
    if !internet::enabled() {
        return Err(INTERNET_OFF.to_owned());
    }
    // A tunnel this device already has to that PC is the PC's own, and stays. One this redeem
    // starts is made from the link's details, which anyone can write, and is kept under the ID
    // the link names: unless the invite works, it must not stay there, where a PC of the list
    // with that ID would be looked for through it until Nightjar restarts.
    let own_tunnel = internet::active(&link.unique_id);
    let redeemed = redeem_over(link, identity, device_name);
    if redeemed.is_err() && !own_tunnel {
        internet::stop(&link.unique_id);
    }
    redeemed
}

/// [`redeem`] once the link may be tried: reaches the PC, pairs and fetches this device's own
/// internet play details.
fn redeem_over(
    link: &InviteUri,
    identity: Arc<ClientIdentity>,
    device_name: &str,
) -> Result<Redeemed, String> {
    let info = InternetPlay::parse(&reply_of(link)).map_err(|e| e.to_string())?;
    let mut answered = None;
    let (path, address) = internet::reach(&link.unique_id, &[], Some(&info), true, |at| {
        let found = HostClient::new(&at.ip().to_string(), at.port(), identity.clone())
            .and_then(|client| client.server_info_http(true));
        match found {
            // Another PC at that address is no answer.
            Ok(server) if server.unique_id == link.unique_id => {
                answered = Some(server);
                true
            }
            _ => false,
        }
    })?;
    let server = answered.ok_or("the PC did not answer")?;
    let client = HostClient::new(&address.ip().to_string(), address.port(), identity.clone())
        .map_err(|e| e.to_string())?
        .with_device_name(device_name);
    let certificate = match client.pair_with_link(&server, &pairing(link, address)) {
        Ok(PairResult::Paired {
            server_certificate_pem,
        }) => server_certificate_pem,
        Ok(PairResult::PinWrong) => return Err(REFUSED.to_owned()),
        Ok(PairResult::AlreadyInProgress) => {
            return Err(
                "Another device is pairing with this PC. Try the invite again in a minute."
                    .to_owned(),
            )
        }
        Ok(PairResult::Failed(failure)) => return Err(format!("Pairing failed: {failure:?}")),
        Err(error) => return Err(pair_error(&error)),
    };
    let paired = HostClient::new(&address.ip().to_string(), address.port(), identity)
        .map_err(|e| e.to_string())?
        .with_https_port(server.https_port)
        .with_server_certificate_pem(&certificate)
        .map_err(|e| e.to_string())?;
    let mut last = String::new();
    let mut details = None;
    for attempt in 0..DETAILS_TRIES {
        let result = paired
            .server_info_https()
            .map_err(|e| e.to_string())
            .and_then(|info| {
                paired
                    .internet_play(&info.capabilities)
                    .map_err(|e| e.to_string())
                    .map(|(text, own)| (info, text, own))
            });
        match result {
            Ok(found) => {
                details = Some(found);
                break;
            }
            Err(error) => {
                last = error;
                if attempt + 1 < DETAILS_TRIES {
                    std::thread::sleep(std::time::Duration::from_millis(
                        200 * (attempt as u64 + 1),
                    ));
                }
            }
        }
    }
    let (info, reply, own) = details.ok_or_else(|| {
        format!("The PC paired this device but did not give it its internet details: {last}")
    })?;
    // What answers with the paired certificate is the PC the link names and nothing else: the
    // apps keep a PC under this ID, and one that names another PC here would be kept as that PC.
    if !names_the_links_pc(&info.unique_id, link) {
        return Err(ANOTHER_PC.to_owned());
    }
    // The link's rendezvous ends soon after it is spent: carry on with this device's own.
    if path == InternetPath::Punched {
        internet::stop(&link.unique_id);
        let _ = internet::tunnel(&link.unique_id, &own);
    }
    Ok(Redeemed {
        unique_id: link.unique_id.clone(),
        name: info.hostname.clone(),
        address,
        path,
        https_port: info.https_port,
        server_certificate_pem: certificate,
        reply,
    })
}

/// Whether the ID a paired PC gives over its pinned connection is the link's. The link's ID is the
/// one the player was shown the PC under and the one the lookup asked for; the paired answer's
/// is whatever that PC says.
fn names_the_links_pc(answered: &str, link: &InviteUri) -> bool {
    answered == link.unique_id
}

/// What a confirmation shows before anything is sent: `{name, expires, permission, expired}`.
pub fn summary(link: &InviteUri, now: u64) -> serde_json::Value {
    json!({
        "name": link.shown_name(),
        "uniqueId": link.unique_id,
        "expires": link.expires,
        "permission": link.permission,
        "expired": link.expired(now),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubylight_protocol::invite_uri::InvitePorts;

    fn link() -> InviteUri {
        InviteUri {
            secret: [3; 32],
            unique_id: "HOST-1".into(),
            cert_sha256: [9; 32],
            name: "Den PC".into(),
            expires: 2_000_000_000,
            permission: "controller".into(),
            ports: InvitePorts {
                http: 47989,
                https: 47984,
                rtsp: 48010,
                video: 47998,
                audio: 48000,
                control: 47999,
                mic: 48002,
            },
            session: Some([4; 16]),
            endpoints: vec!["203.0.113.5:40000".parse().unwrap()],
            nat: "cone".into(),
            ipv6: vec!["2001:db8::2".parse().unwrap()],
            external_ipv4: None,
            mapped_ports: false,
        }
    }

    #[test]
    fn the_link_reads_as_an_internet_play_reply_with_the_derived_secret() {
        let reply = InternetPlay::parse(&reply_of(&link())).unwrap();
        assert!(reply.enabled);
        assert_eq!(reply.secret, Some(rendezvous_secret(&[3; 32])));
        assert_ne!(reply.secret, Some([3; 32]), "never the link's own secret");
        assert_eq!(reply.session, Some([4; 16]));
        assert_eq!(reply.ports.http, 47989);
        assert_eq!(reply.ports.mic, 48002);
        assert_eq!(reply.ipv6, link().ipv6);
        assert_eq!(reply.endpoints, link().endpoints);
        assert!(!reply.mapped_ports);
        let bare = InviteUri {
            session: None,
            nat: String::new(),
            endpoints: Vec::new(),
            ..link()
        };
        let reply = InternetPlay::parse(&reply_of(&bare)).unwrap();
        assert_eq!(reply.session, None);
        assert!(reply.endpoints.is_empty());
    }

    #[test]
    fn the_pairing_carries_the_derived_pin_and_the_link_fingerprint() {
        let at: SocketAddr = "127.78.1.2:47989".parse().unwrap();
        let pairing = pairing(&link(), at);
        assert_eq!(pairing.otp, pairing_pin(&[3; 32]));
        assert_eq!(pairing.otp.len(), 32);
        assert_eq!(pairing.passphrase, PASSPHRASE);
        assert_eq!(pairing.cert_sha256, [9; 32]);
        assert_eq!(pairing.host, "127.78.1.2");
        assert_eq!(pairing.port, 47989);
    }

    #[test]
    fn an_expired_link_or_the_setting_off_stops_before_any_network() {
        let identity = Arc::new(ClientIdentity::generate().unwrap());
        let error = redeem(&link(), identity.clone(), "Phone", 2_000_000_000).unwrap_err();
        assert_eq!(error, EXPIRED);
        // The setting is off until an app turns it on; no test here turns it on.
        if !internet::enabled() {
            let error = redeem(&link(), identity, "Phone", 1_000).unwrap_err();
            assert_eq!(error, INTERNET_OFF);
        }
    }

    #[test]
    fn a_pc_without_a_tunnel_has_none_to_keep() {
        // What decides whether a failed redeem ends the tunnel under the link's ID.
        assert!(!internet::active("HOST-1"));
        internet::stop("HOST-1");
        assert!(!internet::active("HOST-1"));
    }

    #[test]
    fn a_paired_pc_that_names_another_pc_is_not_the_links() {
        assert!(names_the_links_pc("HOST-1", &link()));
        // An invite's PC that answers, once paired, with the ID of a PC the player already has.
        assert!(!names_the_links_pc("HOME-PC", &link()));
        assert!(!names_the_links_pc("host-1", &link()));
        assert!(!names_the_links_pc("", &link()));
    }

    #[test]
    fn refusals_read_in_plain_words() {
        assert_eq!(
            pair_error(&Error::Status {
                code: 400,
                message: "no one-time pairing code is live".into()
            }),
            REFUSED
        );
        assert!(pair_error(&Error::CertificateMismatch).contains("not the one"));
        let summary = summary(&link(), 2_000_000_001);
        assert_eq!(summary["name"], "Den PC");
        assert_eq!(summary["expired"], true);
        assert_eq!(parse("rubylight://pair?v=1").unwrap_err(), NOT_AN_INVITE);
    }
}
