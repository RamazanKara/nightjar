//! PIN pairing, the five steps of moonlight-android's PairingManager.java: the salted PIN
//! key, the server certificate, both challenges and the signed secrets, then the pair
//! challenge over pinned HTTPS.

use super::Error;
use super::crypto::{self, PairingHash};
use super::der;
use super::identity::ClientIdentity;
use super::xml;

/// How a pairing attempt ended, when the host answered every request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PairResult {
    /// Paired; keep the certificate and pin it for HTTPS to this host.
    Paired {
        server_certificate_pem: String,
    },
    /// The host's proof did not match: the PIN typed on the host was not this one.
    PinWrong,
    /// Another device is pairing (the host sent no certificate).
    AlreadyInProgress,
    Failed(PairFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairFailure {
    /// The host answered `paired` 0 at this step.
    Declined(PairStep),
    /// The host's pairing secret was not signed by its certificate's key (a man in the middle).
    ServerSignature,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairStep {
    ServerCertificate,
    ClientChallenge,
    ServerChallengeResponse,
    ClientPairingSecret,
    PairChallenge,
}

/// The requests pairing makes; `HostClient` implements them over HTTP.
pub(crate) trait PairingTransport {
    /// `pair?updateState=1&{query}` over plain HTTP; `read_timeout` false waits indefinitely.
    fn pair_command(&mut self, query: &str, read_timeout: bool) -> Result<String, Error>;
    /// `pair?updateState=1&phrase=pairchallenge` over HTTPS with the pinned certificate.
    fn pair_challenge(&mut self) -> Result<String, Error>;
    fn pin_server_certificate(&mut self, der: &[u8]) -> Result<(), Error>;
    /// Unpair over HTTPS (the client certificate is registered by then).
    fn unpair(&mut self) -> Result<(), Error>;
    /// Unpair over HTTP, which ends an unfinished pairing on the host.
    fn cancel_pairing(&mut self) -> Result<(), Error>;
}

/// The OTP hash Apollo and Artemis use: SHA-256 of PIN, salt as hex and passphrase.
pub(crate) fn one_time_pin_auth(pin: &str, salt: &[u8], passphrase: &str) -> String {
    der::hex_encode(&crypto::sha256(
        format!("{pin}{}{passphrase}", der::hex_encode(salt)).as_bytes(),
    ))
}

fn paired(reply: &str) -> Result<(xml::Element, bool), Error> {
    let root = xml::parse_reply(reply)?;
    let paired = root.required_text("paired")? == "1";
    Ok((root, paired))
}

pub(crate) fn pair(
    transport: &mut impl PairingTransport,
    identity: &ClientIdentity,
    server_major_version: i32,
    pin: &str,
    passphrase: Option<&str>,
) -> Result<PairResult, Error> {
    let hash = PairingHash::for_generation(server_major_version);
    let salt = crypto::random::<16>();
    let mut salted = salt.to_vec();
    salted.extend_from_slice(pin.as_bytes());
    let key: [u8; 16] = hash.hash(&salted)[..16].try_into().unwrap();

    // Without a passphrase this waits for the user to enter the PIN on the host.
    let mut query = format!(
        "phrase=getservercert&salt={}&clientcert={}",
        der::hex_encode(&salt),
        der::hex_encode(identity.certificate_pem().as_bytes())
    );
    if let Some(passphrase) = passphrase {
        query.push_str(&format!(
            "&otpauth={}",
            one_time_pin_auth(pin, &salt, passphrase)
        ));
    }
    let reply = match transport.pair_command(&query, passphrase.is_some()) {
        Ok(reply) => reply,
        Err(err) => {
            let _ = transport.cancel_pairing();
            return Err(err);
        }
    };
    let (root, ok) = match paired(&reply) {
        Ok(parsed) => parsed,
        Err(err) => {
            let _ = transport.cancel_pairing();
            return Err(err);
        }
    };
    if !ok {
        return Ok(PairResult::Failed(PairFailure::Declined(
            PairStep::ServerCertificate,
        )));
    }
    let result = finish(transport, identity, hash, &key, &root);
    if !matches!(result, Ok(PairResult::Paired { .. })) {
        // Cleanup must not replace the original result.
        let _ = transport.cancel_pairing();
    }
    result
}

fn finish(
    transport: &mut impl PairingTransport,
    identity: &ClientIdentity,
    hash: PairingHash,
    key: &[u8; 16],
    certificate_reply: &xml::Element,
) -> Result<PairResult, Error> {
    // Another client pairing at the same time gets no certificate.
    let Some(plain) = certificate_reply.find_text("plaincert") else {
        return Ok(PairResult::AlreadyInProgress);
    };
    let pem = String::from_utf8(der::hex_decode(&plain)?)
        .map_err(|_| Error::Malformed("Invalid pairing certificate".into()))?;
    let server_der = der::pem_decode(&pem, "CERTIFICATE")
        .map_err(|_| Error::Malformed("Invalid pairing certificate".into()))?;
    let server = der::parse_certificate(&server_der)?;
    if !crypto::verify_self_signature(&server)? {
        return Ok(PairResult::Failed(PairFailure::ServerSignature));
    }
    transport.pin_server_certificate(&server_der)?;

    let challenge = crypto::random::<16>();
    let (root, ok) = paired(&transport.pair_command(
        &format!(
            "clientchallenge={}",
            der::hex_encode(&crypto::aes_ecb(key, &challenge, true))
        ),
        true,
    )?)?;
    if !ok {
        return Ok(PairResult::Failed(PairFailure::Declined(
            PairStep::ClientChallenge,
        )));
    }
    let encrypted = der::hex_decode(&root.required_text("challengeresponse")?)?;
    if encrypted.len() != (hash.len() + 16 + 15) & !15 {
        return Err(Error::Malformed(
            "Invalid encrypted pairing challenge length".into(),
        ));
    }
    let decrypted = crypto::aes_ecb(key, &encrypted, false);
    let server_response = &decrypted[..hash.len()];
    let server_challenge = &decrypted[hash.len()..hash.len() + 16];

    let client_secret = crypto::random::<16>();
    let proof = hash.hash(
        &[
            server_challenge,
            identity.certificate_signature(),
            &client_secret,
        ]
        .concat(),
    );
    let (root, ok) = paired(&transport.pair_command(
        &format!(
            "serverchallengeresp={}",
            der::hex_encode(&crypto::aes_ecb(key, &proof, true))
        ),
        true,
    )?)?;
    if !ok {
        return Ok(PairResult::Failed(PairFailure::Declined(
            PairStep::ServerChallengeResponse,
        )));
    }
    let server_secret_reply = der::hex_decode(&root.required_text("pairingsecret")?)?;
    if server_secret_reply.len() <= 16 {
        return Err(Error::Malformed("Invalid pairing secret length".into()));
    }
    let (server_secret, server_signature) = server_secret_reply.split_at(16);
    if !crypto::verify(&server, server_secret, server_signature)? {
        return Ok(PairResult::Failed(PairFailure::ServerSignature));
    }
    let expected = hash.hash(&[&challenge[..], &server.signature, server_secret].concat());
    if expected != server_response {
        return Ok(PairResult::PinWrong);
    }

    let mut client_pairing_secret = client_secret.to_vec();
    client_pairing_secret.extend_from_slice(&identity.sign(&client_secret)?);
    let (_, ok) = paired(&transport.pair_command(
        &format!(
            "clientpairingsecret={}",
            der::hex_encode(&client_pairing_secret)
        ),
        true,
    )?)?;
    if !ok {
        return Ok(PairResult::Failed(PairFailure::Declined(
            PairStep::ClientPairingSecret,
        )));
    }

    // Hosts show the client as paired only after this first HTTPS request.
    let (_, ok) = paired(&transport.pair_challenge()?)?;
    if !ok {
        let _ = transport.unpair();
        return Ok(PairResult::Failed(PairFailure::Declined(
            PairStep::PairChallenge,
        )));
    }
    Ok(PairResult::Paired {
        server_certificate_pem: pem,
    })
}

#[cfg(test)]
mod tests {
    use super::super::identity::tests::fixture;
    use super::*;
    use std::collections::HashMap;

    /// A host side written from the Rubylight host's pairing steps, with our own crypto.
    struct Host {
        identity: ClientIdentity,
        cert_pem_override: Option<String>,
        pin: String,
        key: Option<[u8; 16]>,
        server_secret: [u8; 16],
        server_challenge: [u8; 16],
        client_hash: Vec<u8>,
        client_certificate: Option<Vec<u8>>,
        pinned: Option<Vec<u8>>,
        log: Vec<String>,
        paired: bool,
    }

    fn query(text: &str) -> HashMap<String, String> {
        text.split('&')
            .filter_map(|kv| kv.split_once('='))
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect()
    }

    fn reply(fields: &[(&str, String)]) -> String {
        let mut s = "<root status_code=\"200\">".to_owned();
        for (k, v) in fields {
            s.push_str(&format!("<{k}>{v}</{k}>"));
        }
        s + "</root>"
    }

    impl PairingTransport for Host {
        fn pair_command(&mut self, text: &str, _read_timeout: bool) -> Result<String, Error> {
            let args = query(text);
            let key = |h: &Host| h.key.unwrap();
            if args.get("phrase").map(String::as_str) == Some("getservercert") {
                self.log.push("getservercert".into());
                let salt = der::hex_decode(&args["salt"]).unwrap();
                let pem = String::from_utf8(der::hex_decode(&args["clientcert"]).unwrap()).unwrap();
                self.client_certificate = Some(der::pem_decode(&pem, "CERTIFICATE").unwrap());
                let mut salted = salt.clone();
                salted.extend_from_slice(self.pin.as_bytes());
                self.key = Some(crypto::sha256(&salted)[..16].try_into().unwrap());
                let cert_pem = self
                    .cert_pem_override
                    .as_deref()
                    .unwrap_or_else(|| self.identity.certificate_pem());
                return Ok(reply(&[
                    ("paired", "1".into()),
                    ("plaincert", der::hex_encode(cert_pem.as_bytes())),
                ]));
            }
            if let Some(v) = args.get("clientchallenge") {
                self.log.push("clientchallenge".into());
                let challenge = crypto::aes_ecb(&key(self), &der::hex_decode(v).unwrap(), false);
                let cert_der = match self.cert_pem_override.as_deref() {
                    Some(pem) => der::pem_decode(pem, "CERTIFICATE").unwrap(),
                    None => self.identity.certificate_der().to_vec(),
                };
                let signature = &der::parse_certificate(&cert_der).unwrap().signature;
                let mut response =
                    crypto::sha256(&[&challenge[..], signature, &self.server_secret].concat());
                response.extend_from_slice(&self.server_challenge);
                let encrypted = crypto::aes_ecb(&key(self), &response, true);
                return Ok(reply(&[
                    ("paired", "1".into()),
                    ("challengeresponse", der::hex_encode(&encrypted)),
                ]));
            }
            if let Some(v) = args.get("serverchallengeresp") {
                self.log.push("serverchallengeresp".into());
                self.client_hash = crypto::aes_ecb(&key(self), &der::hex_decode(v).unwrap(), false);
                let mut secret = self.server_secret.to_vec();
                secret.extend_from_slice(&self.identity.sign(&self.server_secret).unwrap());
                return Ok(reply(&[
                    ("paired", "1".into()),
                    ("pairingsecret", der::hex_encode(&secret)),
                ]));
            }
            if let Some(v) = args.get("clientpairingsecret") {
                self.log.push("clientpairingsecret".into());
                let secret = der::hex_decode(v).unwrap();
                let client =
                    der::parse_certificate(self.client_certificate.as_ref().unwrap()).unwrap();
                let expected = crypto::sha256(
                    &[&self.server_challenge[..], &client.signature, &secret[..16]].concat(),
                );
                let ok = expected == self.client_hash
                    && crypto::verify(&client, &secret[..16], &secret[16..]).unwrap();
                self.paired = ok;
                return Ok(reply(&[("paired", if ok { "1" } else { "0" }.into())]));
            }
            panic!("unexpected pairing request {text}");
        }

        fn pair_challenge(&mut self) -> Result<String, Error> {
            self.log.push("pairchallenge".into());
            let expected_der = match self.cert_pem_override.as_deref() {
                Some(pem) => der::pem_decode(pem, "CERTIFICATE").unwrap(),
                None => self.identity.certificate_der().to_vec(),
            };
            assert_eq!(self.pinned.as_deref(), Some(expected_der.as_slice()));
            Ok(reply(&[("paired", "1".into())]))
        }

        fn pin_server_certificate(&mut self, der: &[u8]) -> Result<(), Error> {
            self.pinned = Some(der.to_vec());
            Ok(())
        }

        fn unpair(&mut self) -> Result<(), Error> {
            self.log.push("unpair".into());
            Ok(())
        }

        fn cancel_pairing(&mut self) -> Result<(), Error> {
            self.log.push("cancel".into());
            Ok(())
        }
    }

    fn host_with_identity(identity: ClientIdentity, pin: &str) -> Host {
        Host {
            identity,
            cert_pem_override: None,
            pin: pin.into(),
            key: None,
            server_secret: [3; 16],
            server_challenge: [4; 16],
            client_hash: Vec::new(),
            client_certificate: None,
            pinned: None,
            log: Vec::new(),
            paired: false,
        }
    }

    fn host(pin: &str) -> Host {
        host_with_identity(fixture(), pin)
    }

    #[test]
    fn pairs_and_pins_the_host_certificate() {
        let client = fixture();
        let mut host = host("1234");
        let result = pair(&mut host, &client, 7, "1234", None).unwrap();
        assert_eq!(
            result,
            PairResult::Paired {
                server_certificate_pem: host.identity.certificate_pem().to_owned()
            }
        );
        assert!(host.paired);
        assert_eq!(
            host.log,
            [
                "getservercert",
                "clientchallenge",
                "serverchallengeresp",
                "clientpairingsecret",
                "pairchallenge"
            ]
        );
    }

    #[test]
    fn a_wrong_pin_is_reported_and_the_pairing_cancelled() {
        let mut host = host("1234");
        assert_eq!(
            pair(&mut host, &fixture(), 7, "4321", None).unwrap(),
            PairResult::PinWrong
        );
        assert_eq!(
            host.log,
            [
                "getservercert",
                "clientchallenge",
                "serverchallengeresp",
                "cancel"
            ]
        );
        assert!(!host.paired);
    }

    // Real X.509 v2 certificate emitted by moonlight-tv mkcert.c (webOS v2 client).
    const WEBOS_V2_MOONLIGHT_TV_PEM: &str = "\
-----BEGIN CERTIFICATE-----\n\
MIICvzCCAaegAwIBAQIBATANBgkqhkiG9w0BAQsFADAjMSEwHwYDVQQDDBhOVklE\n\
SUEgR2FtZVN0cmVhbSBDbGllbnQwHhcNMjYxMDA4MjIxMDQzWhcNMzYxMDA4MjIx\n\
MDQzWjAjMSEwHwYDVQQDDBhOVklESUEgR2FtZVN0cmVhbSBDbGllbnQwggEiMA0G\n\
CSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCXuOxl+Rog8hxLWZz8+UixCP5oGAKD\n\
XnR3RikufsbcGutUhWQi3OdghyGcTAFLHif0NlUSlApcOFC3qoqE9ZeQuiYSwNQP\n\
xaWDlgq0f2/iNf38fjTUwlhTzqWWc4Qemt2eVcAQbbGahsv3TQFxFEEKt3QlQH2c\n\
C4rXdUcCQDcUZSG2onJ3rxf8mOf6O7pLiHYS2Youfk94qa/p+VpYZZ9nam5g9Y+0\n\
M8rrIgSRN9NOLYDoIfNSmRv3m6XHjrqCQlIrq2sesAXTERebreiUu9j+gOQXwjHA\n\
6fsbMfJ6+MbcxBm/ZDfaTvCIOW+yij4Lblq09UvVrgtnZunOfUZt/0+zAgMBAAEw\n\
DQYJKoZIhvcNAQELBQADggEBAED8xH1C+Mc8eCQ3Z3BbKENf9yOFGI8yr6I09DVs\n\
YU0N0zyHSjrjAYlPSwWya5R7T/2KWnG3QYRfIYbzdoEopUN17lQ2sycnLO0/a3WK\n\
UB8LOHwDNsI53jZPzacPQjf0WVx9Z0tmW1vOx1w5GBvCwZPVW8T38qoZ4aWird9y\n\
8vS/RAuDqiV9wRgmoCLwdPxgzoeR/m6WIyZCWvm8Wt0aoCYsmbOgkyXZxivFn2fc\n\
C7a5vcWv8sF1u1TvkqNO34z6ni7uOfw7NiV8Ju7xW9COVM2pws6SMOWXfpKNpyfv\n\
R3XKEKmQeDg1ZT+T17u3nzoavwyJYV0BQTTeGPS4o7Oz8g0=\n\
-----END CERTIFICATE-----\n";

    fn rsa_spki(modulus: &[u8], exponent: &[u8]) -> Vec<u8> {
        let alg = der::sequence(&[
            &der::element(der::TAG_OID, der::OID_RSA_ENCRYPTION),
            &der::element(der::TAG_NULL, &[]),
        ]);
        let pub_der = der::sequence(&[
            &der::element(der::TAG_INTEGER, modulus),
            &der::element(der::TAG_INTEGER, exponent),
        ]);
        let mut bits = Vec::with_capacity(1 + pub_der.len());
        bits.push(0);
        bits.extend_from_slice(&pub_der);
        der::sequence(&[&alg, &der::element(der::TAG_BIT_STRING, &bits)])
    }

    fn build_self_signed_cert(
        cn: &str,
        pkcs8: &[u8],
        spki: &[u8],
        sig_alg_oid: &[u8],
        tbs_sig_alg_oid: &[u8],
        extensions: Option<&[u8]>,
    ) -> Vec<u8> {
        let outer_alg = der::sequence(&[
            &der::element(der::TAG_OID, sig_alg_oid),
            &der::element(der::TAG_NULL, &[]),
        ]);
        let inner_alg = der::sequence(&[
            &der::element(der::TAG_OID, tbs_sig_alg_oid),
            &der::element(der::TAG_NULL, &[]),
        ]);
        let name = der::sequence(&[&der::element(
            der::TAG_SET,
            &der::sequence(&[
                &der::element(der::TAG_OID, der::OID_COMMON_NAME),
                &der::element(der::TAG_UTF8_STRING, cn.as_bytes()),
            ]),
        )]);
        let ver = der::element(der::TAG_CONTEXT_0, &der::element(der::TAG_INTEGER, &[2]));
        let serial = der::element(der::TAG_INTEGER, &[1]);
        let validity = der::sequence(&[&der::time(1_700_000_000), &der::time(2_000_000_000)]);
        let mut tbs_parts: Vec<&[u8]> =
            vec![&ver, &serial, &inner_alg, &name, &validity, &name, spki];
        let ext_tagged;
        if let Some(ext) = extensions {
            ext_tagged = der::element(0xa3, ext);
            tbs_parts.push(&ext_tagged);
        }
        let tbs = der::sequence(&tbs_parts);
        let signature = crypto::sign(pkcs8, &tbs).unwrap();
        let mut bits = vec![0];
        bits.extend_from_slice(&signature);
        der::sequence(&[&tbs, &outer_alg, &der::element(der::TAG_BIT_STRING, &bits)])
    }

    fn spki_from_cert(cert_der: &[u8]) -> Vec<u8> {
        let parts = der::parse_certificate(cert_der).unwrap();
        let (mod_bytes, exp_bytes) = der::rsa_public_key(&parts.public_key).unwrap();
        let mut m = Vec::new();
        if (mod_bytes[0] & 0x80) != 0 {
            m.push(0);
        }
        m.extend_from_slice(mod_bytes);
        rsa_spki(&m, exp_bytes)
    }

    #[test]
    fn otp_hash_matches_the_host() {
        // Rubylight: hex::encode_upper(sha256(format!("{pin}{salt_hex}{passphrase}"))).
        let salt = [0xab; 16];
        let expected = der::hex_encode(&crypto::sha256(
            format!("1234{}secret", "AB".repeat(16)).as_bytes(),
        ));
        assert_eq!(one_time_pin_auth("1234", &salt, "secret"), expected);
    }

    #[test]
    fn forged_certificate_signature_bytes_are_rejected_before_pinning() {
        let client = fixture();
        let victim = fixture();
        let victim_parts = der::parse_certificate(victim.certificate_der()).unwrap();

        let attacker = ClientIdentity::generate().unwrap();
        let attacker_spki = spki_from_cert(attacker.certificate_der());
        let name = der::sequence(&[&der::element(
            der::TAG_SET,
            &der::sequence(&[
                &der::element(der::TAG_OID, der::OID_COMMON_NAME),
                &der::element(der::TAG_UTF8_STRING, b"Attacker Host"),
            ]),
        )]);
        let alg = der::sequence(&[
            &der::element(der::TAG_OID, der::OID_SHA256_WITH_RSA),
            &der::element(der::TAG_NULL, &[]),
        ]);
        let tbs = der::sequence(&[
            &der::element(der::TAG_CONTEXT_0, &der::element(der::TAG_INTEGER, &[2])),
            &der::element(der::TAG_INTEGER, &[7]),
            &alg,
            &name,
            &der::sequence(&[&der::time(1_700_000_000), &der::time(2_000_000_000)]),
            &name,
            &attacker_spki,
        ]);
        // Transplant victim's signature bytes onto attacker's certificate with attacker's key.
        let mut bits = vec![0];
        bits.extend_from_slice(&victim_parts.signature);
        let forged_der = der::sequence(&[&tbs, &alg, &der::element(der::TAG_BIT_STRING, &bits)]);
        let forged_pem = der::pem_encode_certificate(&forged_der);

        let mut host = host_with_identity(attacker, "1234");
        host.cert_pem_override = Some(forged_pem);
        let result = pair(&mut host, &client, 7, "1234", None).unwrap();
        assert_eq!(result, PairResult::Failed(PairFailure::ServerSignature));
        assert!(host.pinned.is_none(), "forged cert must not be pinned");
        assert!(!host.paired);
        assert_eq!(host.log, ["getservercert", "cancel"]);
    }

    #[test]
    fn tampered_tbs_and_bad_exponent_and_wrong_sig_alg_are_rejected() {
        let client = fixture();
        let real = ClientIdentity::generate().unwrap();

        // 1) Modify a byte in TBSCertificate while keeping the signature intact.
        let mut tampered_der = real.certificate_der().to_vec();
        let needle = b"NVIDIA GameStream Client";
        let pos = tampered_der
            .windows(needle.len())
            .position(|w| w == needle)
            .unwrap();
        tampered_der[pos] = b'X';
        let mut host = host_with_identity(real.clone(), "1234");
        host.cert_pem_override = Some(der::pem_encode_certificate(&tampered_der));
        assert_eq!(
            pair(&mut host, &client, 7, "1234", None).unwrap(),
            PairResult::Failed(PairFailure::ServerSignature)
        );
        assert!(host.pinned.is_none());
        assert_eq!(host.log, ["getservercert", "cancel"]);

        // 2) RSA exponent other than 65537 (e = 3) must fail verification and pairing.
        let parts = der::parse_certificate(real.certificate_der()).unwrap();
        let (mod_bytes, _) = der::rsa_public_key(&parts.public_key).unwrap();
        let mut m = Vec::new();
        if (mod_bytes[0] & 0x80) != 0 {
            m.push(0);
        }
        m.extend_from_slice(mod_bytes);
        let bad_exp_spki = rsa_spki(&m, &[3]);
        let bad_exp_der = build_self_signed_cert(
            "Bad Exp",
            real.private_key_pkcs8(),
            &bad_exp_spki,
            der::OID_SHA256_WITH_RSA,
            der::OID_SHA256_WITH_RSA,
            None,
        );
        let bad_exp_parts = der::parse_certificate(&bad_exp_der).unwrap();
        assert!(crypto::verify_self_signature(&bad_exp_parts).is_err());
        assert!(crypto::verify(&bad_exp_parts, b"msg", &bad_exp_parts.signature).is_err());

        let mut bad_host = host_with_identity(real.clone(), "1234");
        bad_host.cert_pem_override = Some(der::pem_encode_certificate(&bad_exp_der));
        assert!(pair(&mut bad_host, &client, 7, "1234", None).is_err());
        assert!(bad_host.pinned.is_none());
        assert_eq!(bad_host.log, ["getservercert", "cancel"]);

        // 3) Unsupported SHA-1 RSA OID and mismatched inner/outer signature OIDs fail.
        let good_spki = spki_from_cert(real.certificate_der());
        let sha1_oid = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x05];
        let sha1_der = build_self_signed_cert(
            "SHA1 Host",
            real.private_key_pkcs8(),
            &good_spki,
            sha1_oid,
            sha1_oid,
            None,
        );
        let sha1_parts = der::parse_certificate(&sha1_der).unwrap();
        assert!(crypto::verify_self_signature(&sha1_parts).is_err());

        let mismatch_der = build_self_signed_cert(
            "Mismatch Alg",
            real.private_key_pkcs8(),
            &good_spki,
            der::OID_SHA256_WITH_RSA,
            sha1_oid,
            None,
        );
        assert!(der::parse_certificate(&mismatch_der).is_err());
    }

    #[test]
    fn valid_moonlight_webos_v2_sunshine_and_gfe_certs_verify() {
        let client = fixture();

        // webOS v2 (moonlight-tv real X.509 v2 certificate).
        let webos_der = der::pem_decode(WEBOS_V2_MOONLIGHT_TV_PEM, "CERTIFICATE").unwrap();
        let webos_parts = der::parse_certificate(&webos_der).unwrap();
        assert!(crypto::verify_self_signature(&webos_parts).unwrap());

        // Moonlight (X.509 v3 RSA-2048 e=65537).
        let moonlight_parts = der::parse_certificate(client.certificate_der()).unwrap();
        assert!(crypto::verify_self_signature(&moonlight_parts).unwrap());

        // Sunshine-style X.509 v3 certificate with BasicConstraints extension.
        let sunshine = ClientIdentity::generate().unwrap();
        let sunshine_spki = spki_from_cert(sunshine.certificate_der());
        let bc_ext = der::sequence(&[&der::sequence(&[
            &der::element(der::TAG_OID, &[0x55, 0x1d, 0x13]),
            &der::element(0x04, &der::sequence(&[])),
        ])]);
        let sunshine_der = build_self_signed_cert(
            "Sunshine Gamestream Host",
            sunshine.private_key_pkcs8(),
            &sunshine_spki,
            der::OID_SHA256_WITH_RSA,
            der::OID_SHA256_WITH_RSA,
            Some(&bc_ext),
        );
        let sunshine_parts = der::parse_certificate(&sunshine_der).unwrap();
        assert!(crypto::verify_self_signature(&sunshine_parts).unwrap());
        let mut s_host = host_with_identity(sunshine, "5678");
        s_host.cert_pem_override = Some(der::pem_encode_certificate(&sunshine_der));
        assert!(matches!(
            pair(&mut s_host, &client, 7, "5678", None).unwrap(),
            PairResult::Paired { .. }
        ));

        // GFE-style X.509 v3 certificate.
        let gfe = ClientIdentity::generate().unwrap();
        let gfe_spki = spki_from_cert(gfe.certificate_der());
        let gfe_der = build_self_signed_cert(
            "NVIDIA GameStream Server",
            gfe.private_key_pkcs8(),
            &gfe_spki,
            der::OID_SHA256_WITH_RSA,
            der::OID_SHA256_WITH_RSA,
            None,
        );
        let gfe_parts = der::parse_certificate(&gfe_der).unwrap();
        assert!(crypto::verify_self_signature(&gfe_parts).unwrap());
        let mut g_host = host_with_identity(gfe, "9012");
        g_host.cert_pem_override = Some(der::pem_encode_certificate(&gfe_der));
        assert!(matches!(
            pair(&mut g_host, &client, 7, "9012", None).unwrap(),
            PairResult::Paired { .. }
        ));
    }

    #[test]
    fn accepts_rubylight_generated_host_certificate_pem() {
        // Fixture produced by butterpollo_crypto::crypto::Identity::generate().certificate.
        const RUBYLIGHT_GENERATED_HOST_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIC2DCCAcCgAwIBAgIURskXZ6DZB+S7zhoSx0R/0A0+TQ8wDQYJKoZIhvcNAQEL
BQAwEzERMA8GA1UEAwwIU3Vuc2hpbmUwIBcNNzUwMTAxMDAwMDAwWhgPNDA5NjAx
MDEwMDAwMDBaMBMxETAPBgNVBAMMCFN1bnNoaW5lMIIBIjANBgkqhkiG9w0BAQEF
AAOCAQ8AMIIBCgKCAQEA4R6ST75rtq+dPO+ZWEGlqrr0ukLp1PB60Np7V7v+d9bI
14VEHlhL0oPSGAAQLJWZ47ZjOObXwbE/rEzSq0KzWETSN5ZevXDXopo9duOLhx/u
ZZktg+HakiyROrwzmuhhR+jZbIZbT5U7F5Qi/V4Rn7qyssPXhWtf3mIyCuaimhUa
9DXgtGjjaC3DkVBvA+VVqYyz2QN2qZFSYalODyKnUyN4cAsz65YJRY4eINwCkN6i
wB9utap1Y9vJPKGGWWnwSsW5CiittZFB0B42g/mZ8rCawVsku+yh6Xe5kQ2rPFNb
wjqtGwwicCaAv93or0nnIqQFHGK8r6vtyRM28L30XQIDAQABoyIwIDAeBgNVHREE
FzAVgghTdW5zaGluZYIJbG9jYWxob3N0MA0GCSqGSIb3DQEBCwUAA4IBAQBHLtCh
mNy7lgDLwuXqLN6AXVbAya6O4edBhr7EhjZfcQwbZo/uohsO1wNI9Q5etZWZQftg
80UavvrSV1Zlo7IhOApGvKVpAgG6FRyVazKl3vG5Mbh5SGTOnvNCaGKDtLbtn8zA
Ucb9gluV3qxyFM6S+NvHFgiSG52imFjWWLzHFHDK7VL7vgfCHa2TI0fH9I7sHTaj
htQitUlx1UI7Zcd/aD28FbRZOffcXcqCP2t5SPeiRp+9OBLOyfaUrqIagzR4yDqK
CLQzC0xFJ4S7pwr42xacnLzZlotgPuPrCJDHK1T7GqDkrBkbgGjCht2DaRNsyd59
nDqm3G8+csIK/YjR
-----END CERTIFICATE-----
";
        let cert_der = der::pem_decode_certificate(RUBYLIGHT_GENERATED_HOST_CERT_PEM).unwrap();
        let parts = der::parse_certificate(&cert_der).unwrap();
        assert!(crypto::verify_self_signature(&parts).unwrap());
    }
}
