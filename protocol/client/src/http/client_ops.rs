//! Endpoint operations on [`HostClient`]: serverinfo, pairing, app list, cancel, clipboard, and bitrate.

use super::*;

impl HostClient {
    /// Serverinfo: over pinned HTTPS when paired, falling back to HTTP when the host does not
    /// know this client (401) or shows another certificate, so the client can pair again.
    pub fn server_info(&self, likely_online: bool) -> Result<ServerInfo, Error> {
        self.server_info_with_cert_check(likely_online)
            .map(|(info, _)| info)
    }

    /// Serverinfo like [`HostClient::server_info`], also reporting [`InfoPath::CertificateMismatch`]
    /// when it fell back to HTTP after another certificate answered at the pinned HTTPS port.
    pub fn server_info_with_cert_check(
        &self,
        likely_online: bool,
    ) -> Result<(ServerInfo, InfoPath), Error> {
        match self.server_info_checked(likely_online) {
            Ok((info, path)) => Ok((info, path)),
            Err(Error::CertificateMismatch) => self
                .server_info_http(likely_online)
                .map(|info| (info, InfoPath::CertificateMismatch)),
            Err(e) => Err(e),
        }
    }

    /// Serverinfo and how it was reached ([`InfoPath`]), which says what its `PairStatus` is
    /// worth. Over pinned HTTPS when paired, over HTTP after the pinned host refused this client
    /// (401) or when nothing is pinned. Another certificate at this address is
    /// [`Error::CertificateMismatch`], with no HTTP fallback: that answer would be another
    /// host's, not this one's.
    pub fn server_info_checked(
        &self,
        likely_online: bool,
    ) -> Result<(ServerInfo, InfoPath), Error> {
        let connect = if likely_online {
            LONG_CONNECT_TIMEOUT
        } else {
            SHORT_CONNECT_TIMEOUT
        };
        let paired = self.state.lock().unwrap().server_certificate.is_some();
        if paired {
            let https = self
                .request_text(
                    Scheme::Https,
                    "serverinfo",
                    None,
                    connect,
                    Some(READ_TIMEOUT),
                )
                .and_then(|raw| ServerInfo::parse(&raw, self.http_port));
            match https {
                Ok(info) => return Ok((info, InfoPath::Pinned)),
                Err(Error::Status { code: 401, .. }) => {
                    return self
                        .server_info_http(likely_online)
                        .map(|info| (info, InfoPath::Refused));
                }
                Err(e) => return Err(e),
            }
        }
        self.server_info_http(likely_online)
            .map(|info| (info, InfoPath::Plain))
    }

    /// Serverinfo over plain HTTP; hosts report `PairStatus` 0 there.
    pub fn server_info_http(&self, likely_online: bool) -> Result<ServerInfo, Error> {
        let connect = if likely_online {
            LONG_CONNECT_TIMEOUT
        } else {
            SHORT_CONNECT_TIMEOUT
        };
        let raw = self.request_text(
            Scheme::Http,
            "serverinfo",
            None,
            connect,
            Some(READ_TIMEOUT),
        )?;
        let info = ServerInfo::parse(&raw, self.http_port)?;
        let mut state = self.state.lock().unwrap();
        if state.https_port == 0 {
            state.https_port = info.https_port;
        }
        Ok(info)
    }

    /// Serverinfo over pinned HTTPS only, without fallback (verifies the paired host).
    pub fn server_info_https(&self) -> Result<ServerInfo, Error> {
        let raw = self.request_text(
            Scheme::Https,
            "serverinfo",
            None,
            SHORT_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )?;
        ServerInfo::parse(&raw, self.http_port)
    }

    /// Pairs with the PIN the user types on the host. Blocks until the host answers, up to
    /// minutes; [`HostClient::cancel_pending_requests`] ends the wait. On success the host
    /// certificate is pinned; store it with the identity.
    pub fn pair(&self, server_info: &ServerInfo, pin: &str) -> Result<PairResult, Error> {
        self.pair_with(server_info, pin, None)
    }

    /// One-time PIN pairing (Apollo, Vibepollo, Rubylight): the host already knows the PIN
    /// and passphrase, so no one types anything.
    pub fn pair_with_passphrase(
        &self,
        server_info: &ServerInfo,
        pin: &str,
        passphrase: &str,
    ) -> Result<PairResult, Error> {
        self.pair_with(server_info, pin, Some(passphrase))
    }

    /// Pairs with a host's QR pairing link (Experimental): one-time PIN pairing, refused with
    /// [`Error::CertificateMismatch`] before anything is sent back when the host's certificate
    /// does not hash to the link's fingerprint. The client must already point at the link's
    /// host and port.
    pub fn pair_with_link(
        &self,
        server_info: &ServerInfo,
        link: &rubylight_protocol::pairing_uri::PairingUri,
    ) -> Result<PairResult, Error> {
        self.pair_checked(
            server_info,
            &link.otp,
            Some(&link.passphrase),
            Some(link.cert_sha256),
        )
    }

    fn pair_with(
        &self,
        server_info: &ServerInfo,
        pin: &str,
        passphrase: Option<&str>,
    ) -> Result<PairResult, Error> {
        self.pair_checked(server_info, pin, passphrase, None)
    }

    pub fn pair_checked(
        &self,
        server_info: &ServerInfo,
        pin: &str,
        passphrase: Option<&str>,
        fingerprint: Option<[u8; 32]>,
    ) -> Result<PairResult, Error> {
        let major = server_info.major_version()?;
        let previous = {
            let mut state = self.state.lock().unwrap();
            if state.https_port == 0 {
                state.https_port = server_info.https_port;
            }
            state.server_certificate.clone()
        };
        let identity = self
            .identity
            .as_deref()
            .ok_or_else(|| Error::InvalidIdentity("missing client certificate".into()))?;
        // A failed attempt keeps the previous pin (the Java client left the new one set).
        let mut transport = PairingClient {
            client: self,
            fingerprint,
        };
        let result = pairing::pair(&mut transport, identity, major, pin, passphrase);
        if !matches!(result, Ok(PairResult::Paired { .. })) {
            self.pin(previous);
        }
        result
    }

    /// Unpairs this client; over HTTPS when paired, else HTTP.
    pub fn unpair(&self) -> Result<(), Error> {
        let scheme = if self.state.lock().unwrap().server_certificate.is_some() {
            Scheme::Https
        } else {
            Scheme::Http
        };
        self.request_text(
            scheme,
            "unpair",
            None,
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )?;
        Ok(())
    }

    /// The app list; `capabilities` (from serverinfo) decides which entries are control tiles.
    pub fn app_list(&self, capabilities: &HostCapabilities) -> Result<Vec<App>, Error> {
        let raw = self.app_list_raw()?;
        parse_app_list(&raw, capabilities)
    }

    /// The app list reply as received, for callers that cache it.
    pub fn app_list_raw(&self) -> Result<String, Error> {
        self.request_text(
            Scheme::Https,
            "applist",
            None,
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )
    }

    fn box_art_query(app_id: i32) -> String {
        format!("appid={app_id}&AssetType=2&AssetIdx=0")
    }

    /// The box art image (PNG or JPEG bytes as the host sends them).
    pub fn box_art(&self, app_id: i32) -> Result<Vec<u8>, Error> {
        self.request(
            Scheme::Https,
            "appasset",
            Some(&Self::box_art_query(app_id)),
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
            MAX_BOX_ART_BYTES,
        )
    }

    /// The box art URL; fetching it needs this client's certificate and the pinned host's.
    pub fn box_art_url(&self, app_id: i32) -> Result<String, Error> {
        let port = self.https_port(true)?;
        Ok(format!(
            "https://{}:{port}/{}",
            self.host.url_host(),
            self.url_target("appasset", Some(&Self::box_art_query(app_id)))
        ))
    }

    /// Quits the running app. Ok(false) when the host declined; a 599 status when the host
    /// still runs a game afterwards (it belongs to another client).
    pub fn cancel(&self) -> Result<bool, Error> {
        let raw = self.request_text(
            Scheme::Https,
            "cancel",
            None,
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )?;
        if xml::parse_reply(&raw)?.required_text("cancel")? == "0" {
            return Ok(false);
        }
        if self.server_info(true)?.current_game != 0 {
            return Err(Error::Status {
                code: 599,
                message: String::new(),
            });
        }
        Ok(true)
    }

    fn clipboard_request(&self, text: Option<&str>) -> Result<transport::Response, Error> {
        if text.is_some_and(|text| text.len() > MAX_CLIPBOARD_BYTES) {
            return Err(Error::InvalidArgument(
                "Clipboard text exceeds 1 MiB".into(),
            ));
        }
        let response = transport::send(
            &Request {
                host: &self.host,
                port: self.https_port(true)?,
                target: &self.url_target("actions/clipboard", Some("type=text")),
                tls: Some(self.tls()?),
                timeouts: Timeouts {
                    connect: LONG_CONNECT_TIMEOUT,
                    read: Some(READ_TIMEOUT),
                },
                body_limit: MAX_CLIPBOARD_BYTES,
            },
            &self.cancel,
            text.map(|text| transport::Body::text(text.as_bytes())),
        )?;
        if !(200..=299).contains(&response.status) {
            return Err(Error::Status {
                code: i32::from(response.status),
                message: response.reason,
            });
        }
        Ok(response)
    }

    pub fn clipboard(&self) -> Result<String, Error> {
        let response = self.clipboard_request(None)?;
        if !response.content_type.as_deref().is_some_and(|mime| {
            mime.split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("text/plain")
        }) {
            return Err(Error::Malformed(
                "The PC does not support text clipboard transfer".into(),
            ));
        }
        String::from_utf8(response.body)
            .map_err(|_| Error::Malformed("Clipboard text is not UTF-8".into()))
    }

    pub fn send_clipboard(&self, text: &str) -> Result<(), Error> {
        if !self.clipboard_request(Some(text))?.body.is_empty() {
            return Err(Error::Malformed(
                "The PC did not accept the clipboard text".into(),
            ));
        }
        Ok(())
    }

    pub fn set_bitrate(&self, kbps: u32) -> Result<u32, Error> {
        if !(1..=2_000_000).contains(&kbps) {
            return Err(Error::InvalidArgument(
                "Bitrate must be between 1 and 2000000 kbps".into(),
            ));
        }
        let raw = self.request_text(
            Scheme::Https,
            "bitrate",
            Some(&format!("bitrate={kbps}")),
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )?;
        xml::parse_reply(&raw)?
            .required_text("bitrate")?
            .parse::<u32>()
            .ok()
            .filter(|applied| (1..=2_000_000).contains(applied))
            .ok_or_else(|| Error::Malformed("The PC did not apply the bitrate".into()))
    }
}

struct PairingClient<'a> {
    client: &'a HostClient,
    /// The SHA-256 the host certificate must have (a QR pairing link's `fp`).
    fingerprint: Option<[u8; 32]>,
}

impl pairing::PairingTransport for PairingClient<'_> {
    fn pair_command(&mut self, query: &str, read_timeout: bool) -> Result<String, Error> {
        self.client.request_text(
            Scheme::Http,
            "pair",
            Some(&format!("updateState=1&{query}")),
            LONG_CONNECT_TIMEOUT,
            read_timeout.then_some(READ_TIMEOUT),
        )
    }

    fn pair_challenge(&mut self) -> Result<String, Error> {
        self.client.request_text(
            Scheme::Https,
            "pair",
            Some("updateState=1&phrase=pairchallenge"),
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )
    }

    fn pin_server_certificate(&mut self, der: &[u8]) -> Result<(), Error> {
        if let Some(expected) = self.fingerprint
            && crypto::sha256(der) != expected
        {
            return Err(Error::CertificateMismatch);
        }
        self.client.pin(Some(der.to_vec()));
        Ok(())
    }

    fn unpair(&mut self) -> Result<(), Error> {
        self.client.unpair()
    }

    fn cancel_pairing(&mut self) -> Result<(), Error> {
        self.client.request_text(
            Scheme::Http,
            "unpair",
            None,
            LONG_CONNECT_TIMEOUT,
            Some(READ_TIMEOUT),
        )?;
        Ok(())
    }
}
