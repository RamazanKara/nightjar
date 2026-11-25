//! Loopback TLS (`rustls::ServerConnection` + SAN test CA), header, framing, redirect and
//! checksum-first download tests for `nightjar-core::update`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nightjar_policy::http::TransferCancel;
use nightjar_policy::update::Release;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};

use super::net::{
    Response, Transport, Url, WebPkiTransport, api_host, build_request_head, download_host,
    format_host_header, open,
};
use super::{
    UpdateError, UpdateTarget, download_android_apk_with, fetch_latest, fetch_latest_with,
    sha256_hex,
};
use crate::http::transport::Host;

fn temp_dir(name: &str) -> std::path::PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "nj-core-update-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Minimal DER/X.509 v3 CA + leaf SAN cert generator for loopback `rustls::ServerConnection` tests.
fn make_san_cert_and_key() -> (
    CertificateDer<'static>,
    CertificateDer<'static>,
    PrivateKeyDer<'static>,
) {
    use ring::rand::SystemRandom;
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};

    let rng = SystemRandom::new();
    // ECDSA-with-SHA256 OID: 1.2.840.10045.4.3.2
    let alg = der_seq(&[&der_oid(&[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02])]);
    let validity = der_seq(&[&der_utc(b"240101000000Z"), &der_utc(b"350101000000Z")]);

    let ca_pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
    let ca_key =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, ca_pkcs8.as_ref(), &rng).unwrap();
    let ca_name = der_seq(&[&der_set(&[&der_seq(&[
        &der_oid(&[0x55, 0x04, 0x03]),
        &der_utf8("Nightjar Test CA"),
    ])])]);
    let ca_spki = der_spki_p256(ca_key.public_key().as_ref());
    let ca_bc = der_seq(&[
        &der_oid(&[0x55, 0x1d, 0x13]),
        &[0x01, 0x01, 0xff],
        &der_octet(&der_seq(&[&[0x01, 0x01, 0xff]])),
    ]);
    // KeyUsage: keyCertSign (bit 5 -> 0x04 with 2 unused bits).
    let ca_ku = der_seq(&[
        &der_oid(&[0x55, 0x1d, 0x0f]),
        &[0x01, 0x01, 0xff],
        &der_octet(&[0x03, 0x02, 0x02, 0x04]),
    ]);
    let ca_tbs = der_seq(&[
        &der_elem(0xa0, &der_int(&[2])),
        &der_int(&[1]),
        &alg,
        &ca_name,
        &validity,
        &ca_name,
        &ca_spki,
        &der_elem(0xa3, &der_seq(&[&ca_bc, &ca_ku])),
    ]);
    let ca_sig = ca_key.sign(&rng, &ca_tbs).unwrap();
    let ca_der = der_seq(&[&ca_tbs, &alg, &der_bit_string(ca_sig.as_ref())]);

    let leaf_pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
    let leaf_key =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, leaf_pkcs8.as_ref(), &rng)
            .unwrap();
    let leaf_name = der_seq(&[&der_set(&[&der_seq(&[
        &der_oid(&[0x55, 0x04, 0x03]),
        &der_utf8("localhost"),
    ])])]);
    let leaf_spki = der_spki_p256(leaf_key.public_key().as_ref());
    // BasicConstraints cA=FALSE (DER DEFAULT boolean is omitted -> empty SEQUENCE).
    let leaf_bc = der_seq(&[
        &der_oid(&[0x55, 0x1d, 0x13]),
        &[0x01, 0x01, 0xff],
        &der_octet(&der_seq(&[])),
    ]);
    // KeyUsage: digitalSignature (bit 0 -> 0x80 with 7 unused bits).
    let leaf_ku = der_seq(&[
        &der_oid(&[0x55, 0x1d, 0x0f]),
        &[0x01, 0x01, 0xff],
        &der_octet(&[0x03, 0x02, 0x07, 0x80]),
    ]);
    // ExtendedKeyUsage: id-kp-serverAuth (1.3.6.1.5.5.7.3.1).
    let leaf_eku = der_seq(&[
        &der_oid(&[0x55, 0x1d, 0x25]),
        &der_octet(&der_seq(&[&der_oid(&[
            0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01,
        ])])),
    ]);
    let san_value = der_seq(&[
        &der_ctx_prim(2, b"localhost"),
        &der_ctx_prim(7, &[127, 0, 0, 1]),
    ]);
    let leaf_san = der_seq(&[&der_oid(&[0x55, 0x1d, 0x11]), &der_octet(&san_value)]);
    let leaf_tbs = der_seq(&[
        &der_elem(0xa0, &der_int(&[2])),
        &der_int(&[2]),
        &alg,
        &ca_name,
        &validity,
        &leaf_name,
        &leaf_spki,
        &der_elem(0xa3, &der_seq(&[&leaf_bc, &leaf_ku, &leaf_eku, &leaf_san])),
    ]);
    let leaf_sig = ca_key.sign(&rng, &leaf_tbs).unwrap();
    let leaf_der = der_seq(&[&leaf_tbs, &alg, &der_bit_string(leaf_sig.as_ref())]);

    (
        CertificateDer::from(ca_der),
        CertificateDer::from(leaf_der),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_pkcs8.as_ref().to_vec())),
    )
}

fn der_elem(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    if body.len() < 0x80 {
        out.push(body.len() as u8);
    } else {
        let len_bytes = body.len().to_be_bytes();
        let skip = len_bytes.iter().take_while(|&&b| b == 0).count();
        out.push(0x80 | (len_bytes.len() - skip) as u8);
        out.extend_from_slice(&len_bytes[skip..]);
    }
    out.extend_from_slice(body);
    out
}
fn der_seq(parts: &[&[u8]]) -> Vec<u8> {
    der_elem(0x30, &parts.concat())
}
fn der_set(parts: &[&[u8]]) -> Vec<u8> {
    der_elem(0x31, &parts.concat())
}
fn der_oid(b: &[u8]) -> Vec<u8> {
    der_elem(0x06, b)
}
fn der_int(b: &[u8]) -> Vec<u8> {
    der_elem(0x02, b)
}
fn der_utf8(s: &str) -> Vec<u8> {
    der_elem(0x0c, s.as_bytes())
}
fn der_utc(b: &[u8]) -> Vec<u8> {
    der_elem(0x17, b)
}
fn der_octet(b: &[u8]) -> Vec<u8> {
    der_elem(0x04, b)
}
fn der_ctx_prim(tag_num: u8, b: &[u8]) -> Vec<u8> {
    der_elem(0x80 | tag_num, b)
}
fn der_bit_string(b: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(b.len() + 1);
    v.push(0);
    v.extend_from_slice(b);
    der_elem(0x03, &v)
}
fn der_spki_p256(pub_bytes: &[u8]) -> Vec<u8> {
    let alg = der_seq(&[
        &der_oid(&[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01]),
        &der_oid(&[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07]),
    ]);
    der_seq(&[&alg, &der_bit_string(pub_bytes)])
}

#[derive(Clone)]
pub(super) enum TlsReply {
    Raw(Vec<u8>),
    SlowBody {
        head: Vec<u8>,
        first: Vec<u8>,
        pause: Duration,
        rest: Vec<u8>,
    },
}

pub(super) struct TlsFixture {
    pub(super) port: u16,
    pub(super) ca_cert: CertificateDer<'static>,
    pub(super) roots: RootCertStore,
    pub(super) seen_requests: Arc<Mutex<Vec<String>>>,
}

impl TlsFixture {
    pub(super) fn start(route: impl Fn(&str, u16) -> TlsReply + Send + Sync + 'static) -> Self {
        let (ca_cert, leaf_cert, key) = make_san_cert_and_key();
        let mut roots = RootCertStore::empty();
        roots.add(ca_cert.clone()).unwrap();
        let ca_cert_saved = ca_cert.clone();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server_cfg = Arc::new(
            ServerConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .unwrap()
                .with_no_client_auth()
                .with_single_cert(vec![leaf_cert, ca_cert], key)
                .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen_requests = Arc::new(Mutex::new(Vec::new()));
        let seen = seen_requests.clone();
        let route = Arc::new(route);
        std::thread::spawn(move || {
            for tcp in listener.incoming().flatten() {
                let cfg = server_cfg.clone();
                let seen = seen.clone();
                let route = route.clone();
                std::thread::spawn(move || {
                    let Ok(conn) = ServerConnection::new(cfg) else {
                        return;
                    };
                    let mut tls = StreamOwned::new(conn, tcp);
                    let mut req_buf = Vec::new();
                    let mut b = [0u8; 1];
                    while !req_buf.ends_with(b"\r\n\r\n") {
                        if tls.read(&mut b).unwrap_or(0) == 0 {
                            return;
                        }
                        req_buf.push(b[0]);
                    }
                    let req_str = String::from_utf8_lossy(&req_buf).into_owned();
                    let path = req_str.split_whitespace().nth(1).unwrap_or("").to_owned();
                    let content_length = req_str
                        .lines()
                        .find_map(|line| {
                            let (k, v) = line.split_once(':')?;
                            k.trim()
                                .eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    let mut remaining = content_length;
                    let mut body_buf = [0u8; 256];
                    while remaining > 0 {
                        let want = remaining.min(body_buf.len());
                        let n = tls.read(&mut body_buf[..want]).unwrap_or(0);
                        if n == 0 {
                            break;
                        }
                        remaining -= n;
                    }
                    seen.lock().unwrap().push(req_str);
                    match route(&path, port) {
                        TlsReply::Raw(bytes) => {
                            let _ = tls.write_all(&bytes);
                            let _ = tls.flush();
                        }
                        TlsReply::SlowBody {
                            head,
                            first,
                            pause,
                            rest,
                        } => {
                            let _ = tls.write_all(&head);
                            let _ = tls.write_all(&first);
                            let _ = tls.flush();
                            std::thread::sleep(pause);
                            let _ = tls.write_all(&rest);
                            let _ = tls.flush();
                        }
                    }
                    tls.conn.send_close_notify();
                    let _ = tls.flush();
                });
            }
        });
        Self {
            port,
            ca_cert: ca_cert_saved,
            roots,
            seen_requests,
        }
    }

    pub(super) fn url(&self, path: &str) -> String {
        format!("https://127.0.0.1:{}{path}", self.port)
    }
}

fn allow_localhost(url: &Url) -> bool {
    url.secure && (url.host == "127.0.0.1" || url.host == "localhost")
}

#[test]
fn host_header_omits_default_ports_for_s3_signed_urls() {
    let h = Host::parse("objects.githubusercontent.com").unwrap();
    assert_eq!(
        format_host_header(&h, 443, true),
        "objects.githubusercontent.com"
    );
    assert_eq!(
        format_host_header(&h, 8443, true),
        "objects.githubusercontent.com:8443"
    );
    let url =
        Url::parse("https://objects.githubusercontent.com/a/b?X-Amz-SignedHeaders=host").unwrap();
    let head = build_request_head(&url, &h, "Nightjar/1.0", false);
    assert!(head.contains("Host: objects.githubusercontent.com\r\n"));
    assert!(!head.contains("objects.githubusercontent.com:443"));
}

#[test]
fn url_parser_and_host_allowlists_match_desktop_and_android_rules() {
    assert!(download_host(
        &Url::parse("https://github.com/RamazanKara/nightjar/releases/download/v1/x").unwrap()
    ));
    assert!(download_host(
        &Url::parse("https://release-assets.githubusercontent.com/x?sig=1").unwrap()
    ));
    assert!(download_host(
        &Url::parse("https://objects.githubusercontent.com/x").unwrap()
    ));
    assert!(!download_host(&Url::parse("http://github.com/x").unwrap()));
    assert!(!download_host(
        &Url::parse("https://github.com:8443/x").unwrap()
    ));
    assert!(!download_host(
        &Url::parse("https://evil.example/x").unwrap()
    ));
    assert!(!download_host(
        &Url::parse("https://api.github.com/x").unwrap()
    ));
    assert!(api_host(&Url::parse("https://api.github.com/x").unwrap()));
    assert!(!api_host(&Url::parse("https://github.com/x").unwrap()));
    let from = Url::parse("https://github.com/a").unwrap();
    assert_eq!(from.resolve("//evil.example/x"), None);
    assert_eq!(from.resolve("/b?c").unwrap().path, "/b?c");
}

#[test]
fn webpki_tls_verifies_ca_handles_chunked_api_json_and_rate_limits() {
    let fixture = TlsFixture::start(|path, _| match path {
        "/latest" => {
            let json = r#"{"tag_name":"v1.2.3","body":"Hello"}"#;
            let chunk = format!("{:x}\r\n{json}\r\n0\r\n\r\n", json.len());
            TlsReply::Raw(
                format!(
                    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{chunk}"
                )
                .into_bytes(),
            )
        }
        "/ratelimit" => {
            TlsReply::Raw(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n".to_vec())
        }
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });

    // Untrusted default roots must reject our test CA certificate.
    let untrusted = WebPkiTransport::new("Nightjar/test".into());
    assert!(fetch_latest_with(&untrusted, &fixture.url("/latest"), allow_localhost).is_err());

    // Trusted root succeeds and decodes chunked JSON.
    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let release = fetch_latest_with(&trusted, &fixture.url("/latest"), allow_localhost).unwrap();
    assert_eq!(release.version(), "1.2.3");
    assert_eq!(release.body, "Hello");
    let seen = fixture.seen_requests.lock().unwrap();
    assert!(seen[0].contains(&format!("Host: 127.0.0.1:{}\r\n", fixture.port)));
    assert!(seen[0].contains("Accept: application/vnd.github+json\r\n"));
    drop(seen);

    // 403 maps to RateLimited.
    assert_eq!(
        fetch_latest_with(&trusted, &fixture.url("/ratelimit"), allow_localhost),
        Err(UpdateError::RateLimited)
    );
}

#[test]
fn redirect_to_untrusted_host_is_rejected_before_connecting() {
    let fixture = TlsFixture::start(|path, _| {
        match path {
        "/start" => TlsReply::Raw(
            b"HTTP/1.1 302 Found\r\nLocation: https://evil.example/payload\r\nContent-Length: 0\r\n\r\n"
                .to_vec(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec()),
    }
    });
    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let res = open(&trusted, &fixture.url("/start"), allow_localhost, false);
    assert!(matches!(res, Err(super::net::OpenError::Address(ref h)) if h == "evil.example"));
}

#[test]
fn download_android_apk_checks_sums_first_verifies_sha256_size_and_cancel() {
    let apk_bytes: Vec<u8> = (0..16_384u32).map(|i| (i % 251) as u8).collect();
    let expected_sha = sha256_hex(&apk_bytes);
    let apk_hits = Arc::new(AtomicUsize::new(0));
    let apk_hits_srv = apk_hits.clone();
    let sums_body = format!("{expected_sha}  nightjar-2.0.0.apk\n");
    let sums_len = sums_body.len();
    let apk_body = apk_bytes.clone();

    let fixture = TlsFixture::start(move |path, port| {
        match path {
        "/sums" => TlsReply::Raw(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{sums_body}",
                sums_body.len()
            )
            .into_bytes(),
        ),
        "/apk-redirect" => TlsReply::Raw(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/apk-data\r\nContent-Length: 0\r\n\r\n"
            )
            .into_bytes(),
        ),
        "/apk-data" => {
            apk_hits_srv.fetch_add(1, Ordering::SeqCst);
            let mut out = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", apk_body.len())
                .into_bytes();
            out.extend_from_slice(&apk_body);
            TlsReply::Raw(out)
        }
        "/apk-tampered" => {
            let mut bad = apk_body.clone();
            bad[0] ^= 0xff;
            let mut out =
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bad.len()).into_bytes();
            out.extend_from_slice(&bad);
            TlsReply::Raw(out)
        }
        "/apk-slow" => {
            let (first, rest) = apk_body.split_at(2048);
            TlsReply::SlowBody {
                head: format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", apk_body.len())
                    .into_bytes(),
                first: first.to_vec(),
                pause: Duration::from_secs(5),
                rest: rest.to_vec(),
            }
        }
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    }
    });

    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let release_json = format!(
        r#"{{"tag_name":"v2.0.0","assets":[{{"name":"nightjar-2.0.0.apk","size":{},"browser_download_url":"{}"}},{{"name":"SHA256SUMS-2.0.0.txt","size":{},"browser_download_url":"{}"}}]}}"#,
        apk_bytes.len(),
        fixture.url("/apk-redirect"),
        sums_len,
        fixture.url("/sums")
    );
    let release = Release::parse(&release_json).unwrap();
    let dir = temp_dir("good");
    let saw_progress = Arc::new(AtomicBool::new(false));
    let saw_clone = saw_progress.clone();
    let out = download_android_apk_with(
        &trusted,
        allow_localhost,
        &release,
        false,
        &dir,
        None,
        &mut |done, total| {
            if done > 0 && total == apk_bytes.len() as u64 {
                saw_clone.store(true, Ordering::SeqCst);
            }
        },
    )
    .unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), apk_bytes);
    assert!(saw_progress.load(Ordering::SeqCst));
    assert!(!dir.join("nightjar-2.0.0.apk.part").exists());

    // Checksum-first: missing entry in sums never touches the APK URL.
    let before_hits = apk_hits.load(Ordering::SeqCst);
    let missing_entry_json = release_json.replace("nightjar-2.0.0.apk", "nightjar-2.0.0-root.apk");
    let missing_entry_rel = Release::parse(&missing_entry_json).unwrap();
    assert_eq!(
        download_android_apk_with(
            &trusted,
            allow_localhost,
            &missing_entry_rel,
            true,
            &temp_dir("missing-sum"),
            None,
            &mut |_, _| {}
        ),
        Err(UpdateError::ChecksumEntryMissing)
    );
    assert_eq!(apk_hits.load(Ordering::SeqCst), before_hits);

    // Tampered APK -> ChecksumMismatch and `.part` cleaned up.
    let tampered_dir = temp_dir("tampered");
    let tampered_rel =
        Release::parse(&release_json.replace("/apk-redirect", "/apk-tampered")).unwrap();
    assert_eq!(
        download_android_apk_with(
            &trusted,
            allow_localhost,
            &tampered_rel,
            false,
            &tampered_dir,
            None,
            &mut |_, _| {}
        ),
        Err(UpdateError::ChecksumMismatch)
    );
    assert!(!tampered_dir.join("nightjar-2.0.0.apk.part").exists());

    // Size mismatch (including 0 / negative / missing) -> SizeMismatch before downloading.
    let bad_size_rel = Release::parse(
        &release_json.replace(&format!("\"size\":{}", apk_bytes.len()), "\"size\":999"),
    )
    .unwrap();
    assert_eq!(
        download_android_apk_with(
            &trusted,
            allow_localhost,
            &bad_size_rel,
            false,
            &temp_dir("size"),
            None,
            &mut |_, _| {}
        ),
        Err(UpdateError::SizeMismatch)
    );
    let zero_size_rel = Release::parse(
        &release_json.replace(&format!("\"size\":{}", apk_bytes.len()), "\"size\":0"),
    )
    .unwrap();
    assert_eq!(
        download_android_apk_with(
            &trusted,
            allow_localhost,
            &zero_size_rel,
            false,
            &temp_dir("zero-size"),
            None,
            &mut |_, _| {}
        ),
        Err(UpdateError::SizeMismatch)
    );

    // Cancel aborts stalled TLS read promptly without waiting 5 seconds.
    let cancel = Arc::new(TransferCancel::new());
    let cancel_bg = cancel.clone();
    let slow_transport =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone())
            .with_cancel(cancel.clone());
    let slow_rel = Release::parse(&release_json.replace("/apk-redirect", "/apk-slow")).unwrap();
    let slow_dir = temp_dir("cancel");
    let started = std::time::Instant::now();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(120));
        cancel_bg.cancel();
    });
    let res = download_android_apk_with(
        &slow_transport,
        allow_localhost,
        &slow_rel,
        false,
        &slow_dir,
        Some(&cancel),
        &mut |_, _| {},
    );
    assert_eq!(res, Err(UpdateError::Cancelled));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancel aborted stalled read promptly"
    );
    assert!(!slow_dir.join("nightjar-2.0.0.apk.part").exists());
}

#[test]
#[ignore = "asks GitHub over the network using webpki-roots"]
fn asks_github_over_webpki() {
    let net = WebPkiTransport::new("Nightjar/test".into());
    let release = fetch_latest(&net, UpdateTarget::Nightjar).unwrap();
    assert!(!release.version().is_empty());
}

#[test]
fn mock_transport_handles_non_200_and_rate_limit() {
    struct Mock(u16);
    impl Transport for Mock {
        fn get(&self, _: &Url, _: bool) -> Result<Response, String> {
            Ok(Response {
                status: self.0,
                location: None,
                length: Some(0),
                body: Box::new(std::io::Cursor::new(Vec::<u8>::new())),
            })
        }
    }
    assert_eq!(
        fetch_latest(&Mock(429), UpdateTarget::Rubylight),
        Err(UpdateError::RateLimited)
    );
    assert_eq!(
        fetch_latest(&Mock(502), UpdateTarget::Nightjar),
        Err(UpdateError::Network("GitHub answered 502".into()))
    );
}

#[test]
fn download_android_apk_rejects_unsafe_asset_name_before_network_or_cleanup() {
    struct FailIfCalled(Arc<AtomicUsize>);
    impl Transport for FailIfCalled {
        fn get(&self, _: &Url, _: bool) -> Result<Response, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err("transport must not be called".into())
        }
    }

    let hits = Arc::new(AtomicUsize::new(0));
    let net = FailIfCalled(hits.clone());
    let dir = temp_dir("unsafe-name");
    let sentinel = dir.join("keep.txt");
    std::fs::write(&sentinel, b"keep").unwrap();

    for bad_name in [
        "../nightjar-evil.apk",
        "sub\\nightjar-evil.apk",
        "C:nightjar-evil.apk",
        "nightjar-evil..apk",
    ] {
        let rel_json = format!(
            r#"{{"tag_name":"v2.0.0","assets":[{{"name":{},"size":10,"browser_download_url":"https://github.com/RamazanKara/nightjar/releases/download/v2.0.0/x.apk"}},{{"name":"SHA256SUMS-2.0.0.txt","size":10,"browser_download_url":"https://github.com/RamazanKara/nightjar/releases/download/v2.0.0/SHA256SUMS-2.0.0.txt"}}]}}"#,
            serde_json::to_string(bad_name).unwrap()
        );
        let rel = Release::parse(&rel_json).unwrap();
        let res = download_android_apk_with(
            &net,
            allow_localhost,
            &rel,
            false,
            &dir,
            None,
            &mut |_, _| {},
        );
        assert!(
            matches!(res, Err(UpdateError::Malformed(_))),
            "expected Malformed for {bad_name:?}, got {res:?}"
        );
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0);
    assert!(sentinel.exists(), "clean_dir must not run for unsafe names");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exchange_and_download_to_path_enforce_tls_headers_and_retry_after() {
    let fixture = TlsFixture::start(|path, _| match path {
        "/post" => TlsReply::Raw(
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: 42\r\nContent-Length: 4\r\n\r\nwait"
                .to_vec(),
        ),
        "/bin" => TlsReply::Raw(
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n\x00\x01\x02\x03\x04".to_vec(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });

    let trusted = WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let post_url = Url::parse(&fixture.url("/post")).unwrap();
    let res = trusted
        .exchange(
            "POST",
            &post_url,
            &[("Authorization".into(), "Bearer tok".into())],
            Some(br#"{"hello":1}"#),
            false,
        )
        .unwrap();
    assert_eq!(res.status, 429);
    assert_eq!(res.retry_after, Some(42));
    assert_eq!(res.body, b"wait");
    let seen = fixture.seen_requests.lock().unwrap();
    assert!(seen[0].contains("POST /post HTTP/1.1\r\n"));
    assert!(seen[0].contains("Authorization: Bearer tok\r\n"));
    drop(seen);

    let dir = std::env::temp_dir().join("nightjar-update-dl-test");
    let _ = std::fs::remove_dir_all(&dir);
    let target = dir.join("art.bin");
    let bin_url = Url::parse(&fixture.url("/bin")).unwrap();
    assert_eq!(trusted.download_to_path(&bin_url, &target, false).unwrap(), 200);
    assert_eq!(std::fs::read(&target).unwrap(), vec![0, 1, 2, 3, 4]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn exchange_strips_auth_headers_on_cross_origin_redirect() {
    let second = TlsFixture::start(|path, _| match path {
        "/dest" => TlsReply::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec()),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });
    let dest_url = second.url("/dest");
    let dest_for_first = dest_url.clone();
    let first = TlsFixture::start(move |path, _| match path {
        "/start" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: {dest_for_first}\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });
    let mut roots = rustls::RootCertStore::empty();
    roots.add(first.ca_cert.clone()).unwrap();
    roots.add(second.ca_cert.clone()).unwrap();
    let trusted = WebPkiTransport::with_roots_for_test("Nightjar/test".into(), roots);
    let url = Url::parse(&first.url("/start")).unwrap();
    let res = trusted
        .exchange(
            "GET",
            &url,
            &[
                ("Authorization".into(), "Bearer secret-key".into()),
                ("Client-ID".into(), "secret-client".into()),
                ("Accept".into(), "application/json".into()),
            ],
            None,
            true,
        )
        .unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.body, b"ok");
    let first_req = &first.seen_requests.lock().unwrap()[0];
    assert!(first_req.contains("Authorization: Bearer secret-key\r\n"));
    assert!(first_req.contains("Client-ID: secret-client\r\n"));
    let second_req = &second.seen_requests.lock().unwrap()[0];
    assert!(!second_req.contains("secret-key"), "Authorization leaked across origins: {second_req}");
    assert!(!second_req.contains("secret-client"), "Client-ID leaked across origins: {second_req}");
    assert!(second_req.contains("Accept: application/json\r\n"));
}
