//! Unit tests for [`HostClient`] request targets, launch timeouts, and placeholder box art.

use super::*;
use std::io::{Read, Write};

// Nightjar #88: a launch waited for the PC's answer without any limit.
#[test]
fn a_launch_the_pc_never_answers_fails_with_its_own_words() {
    assert!(LAUNCH_READ_TIMEOUT > READ_TIMEOUT);
    let timed_out = || Error::Io(std::io::ErrorKind::TimedOut.into());
    let unanswered = launch_failure(timed_out(), LAUNCH_READ_TIMEOUT, LAUNCH_READ_TIMEOUT);
    assert!(unanswered.is_launch_unanswered());
    assert!(
        unanswered.to_string().contains("within 120 seconds"),
        "{unanswered}"
    );
    assert!(
        matches!(&unanswered, Error::Io(e) if e.kind() == std::io::ErrorKind::Other),
        "not a dropped connection a retry would cure"
    );
    let would_block = Error::Io(std::io::ErrorKind::WouldBlock.into());
    assert!(
        launch_failure(would_block, LAUNCH_READ_TIMEOUT, LAUNCH_READ_TIMEOUT)
            .is_launch_unanswered()
    );
    // A connect that timed out early stays the usual failure.
    let early = launch_failure(timed_out(), Duration::from_secs(5), LAUNCH_READ_TIMEOUT);
    assert!(!early.is_launch_unanswered());
    assert!(matches!(&early, Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut));
    // Anything else is passed on.
    assert!(matches!(
        launch_failure(Error::Cancelled, LAUNCH_READ_TIMEOUT, LAUNCH_READ_TIMEOUT),
        Error::Cancelled
    ));
    assert!(!Error::Cancelled.is_launch_unanswered());
}

#[test]
fn a_detached_client_still_reaches_the_host_after_a_cancel() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let host = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = [0u8; 4096];
        let _ = socket.read(&mut request);
        socket
            .write_all(
                b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let identity = Arc::new(identity::tests::fixture());
    let client = HostClient::new("127.0.0.1", port, identity)
        .unwrap()
        .with_https_port(47984);
    client.cancel_pending_requests();
    assert!(matches!(
        client.server_info_http(true),
        Err(Error::Cancelled)
    ));
    let detached = client.detached();
    assert_eq!(detached.state.lock().unwrap().https_port, 47984);
    assert!(!matches!(
        detached.server_info_http(true),
        Err(Error::Cancelled)
    ));
    host.join().unwrap();
}

#[test]
fn only_a_second_screens_client_marks_its_requests() {
    let identity = Arc::new(identity::tests::fixture());
    let first = HostClient::new("127.0.0.1", 47989, identity).unwrap();
    let target = first.url_target("launch", Some("appid=1"));
    assert!(target.starts_with("launch?appid=1&uniqueid="), "{target}");
    assert!(!target.contains(SECOND_SCREEN_PARAMETER), "{target}");
    let second = first.detached().with_second_screen();
    for (path, query) in [
        ("launch", Some("appid=1")),
        ("cancel", None),
        ("api/abr", None),
    ] {
        let target = second.url_target(path, query);
        assert!(target.ends_with("&rlSecondScreen=1"), "{target}");
        assert_eq!(
            target.matches(SECOND_SCREEN_PARAMETER).count(),
            1,
            "{target}"
        );
    }
    assert!(
        second
            .detached()
            .url_target("bitrate", None)
            .ends_with("&rlSecondScreen=1")
    );
    assert!(
        !first
            .url_target("bitrate", None)
            .contains(SECOND_SCREEN_PARAMETER)
    );
}

#[test]
fn the_hosts_placeholder_box_art_is_no_cover() {
    let placeholder = include_bytes!("../../tests/fixtures/box-placeholder.png");
    assert!(is_placeholder_box_art(placeholder));
    let mut other = placeholder.to_vec();
    other[600] ^= 1;
    assert!(
        !is_placeholder_box_art(&other),
        "a cover of the same size is a cover"
    );
    assert!(!is_placeholder_box_art(&[]));
    // In the monorepo the fixture is the host's own placeholder.
    let host = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../rust/assets/package/box.png"
    );
    if let Ok(host) = std::fs::read(host) {
        assert_eq!(host, placeholder.to_vec());
    }
}

/// A paired PC on loopback: answers `requests` HTTPS requests, one a connection, with the status
/// and body `answer` gives for the request head.
fn https_host(
    requests: usize,
    answer: impl Fn(&str) -> (&'static str, String) + Send + 'static,
) -> (HostClient, std::thread::JoinHandle<()>) {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let identity = Arc::new(identity::tests::fixture());
    let certificate = identity.certificate_der().to_vec();
    let key = PrivatePkcs8KeyDer::from(identity.private_key_pkcs8().to_vec());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(certificate.clone())],
            PrivateKeyDer::Pkcs8(key),
        )
        .unwrap();
    let config = Arc::new(config);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for _ in 0..requests {
            let (socket, _) = listener.accept().unwrap();
            let session = rustls::ServerConnection::new(config.clone()).unwrap();
            let mut tls = rustls::StreamOwned::new(session, socket);
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                tls.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let (status, body) = answer(&String::from_utf8_lossy(&head));
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            // A client that gave up has closed: its own result says so.
            let _ = tls.write_all(reply.as_bytes());
            tls.conn.send_close_notify();
            let _ = tls.flush();
        }
    });
    let client = HostClient::new("127.0.0.1", 47989, identity)
        .unwrap()
        .with_https_port(port);
    client.pin_server_certificate_der(Some(certificate));
    (client, server)
}

#[test]
fn a_pc_without_automatic_bitrate_is_not_an_error() {
    let (client, server) = https_host(1, |_| ("404 Not Found", String::new()));
    assert!(!client.abr_capabilities().unwrap().supported);
    server.join().unwrap();
    let (client, server) = https_host(1, |_| ("503 Service Unavailable", String::new()));
    assert!(matches!(
        client.abr_capabilities(),
        Err(Error::Status { code: 503, .. })
    ));
    server.join().unwrap();
}

thread_local! {
    /// The path and timeouts of every request `HostClient::request` built on this thread.
    pub(super) static SENT: std::cell::RefCell<Vec<(String, Timeouts)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// The PC answers once the game has closed, which may take longer than any read timeout.
#[test]
fn the_cancel_request_has_no_read_timeout() {
    let (client, server) = https_host(2, |head| {
        let body = if head.starts_with("GET /cancel?") {
            "<root status_code=\"200\"><cancel>1</cancel></root>".to_owned()
        } else {
            serverinfo::tests::RUBYLIGHT.replace("<currentgame>5<", "<currentgame>0<")
        };
        ("200 OK", body)
    });
    assert!(client.cancel().unwrap());
    server.join().unwrap();
    let sent = SENT.take();
    let (_, cancel) = sent.iter().find(|(path, _)| path == "cancel").unwrap();
    assert_eq!((cancel.connect, cancel.read), (LONG_CONNECT_TIMEOUT, None));
}
