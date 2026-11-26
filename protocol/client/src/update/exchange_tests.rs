//! Live loopback TLS tests for [`WebPkiTransport::exchange`] covering the 24 MiB body cap,
//! 5-redirect hop limit, HTTPS->HTTP downgrade rejection, `POST`->`GET` 302 conversion, and
//! `Retry-After` parsing.

use super::net::{MAX_EXCHANGE_BYTES, Url, WebPkiTransport};
use super::tests::{TlsFixture, TlsReply};

#[test]
fn exchange_enforces_24_mib_body_cap_exact_and_plus_one() {
    let fixture = TlsFixture::start(|path, _| match path {
        "/exact" => {
            let mut out = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {MAX_EXCHANGE_BYTES}\r\nConnection: close\r\n\r\n"
            )
            .into_bytes();
            out.resize(out.len() + MAX_EXCHANGE_BYTES, b'a');
            TlsReply::Raw(out)
        }
        "/over" => {
            let over = MAX_EXCHANGE_BYTES + 1;
            let mut out = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {over}\r\nConnection: close\r\n\r\n"
            )
            .into_bytes();
            out.resize(out.len() + over, b'b');
            TlsReply::Raw(out)
        }
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });

    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let exact_url = Url::parse(&fixture.url("/exact")).unwrap();
    let res = trusted.exchange("GET", &exact_url, &[], None, false).unwrap();
    assert_eq!(res.status, 200);
    assert_eq!(res.body.len(), MAX_EXCHANGE_BYTES);

    let over_url = Url::parse(&fixture.url("/over")).unwrap();
    let err = trusted
        .exchange("GET", &over_url, &[], None, false)
        .unwrap_err();
    assert_eq!(err, "response exceeded 24 MiB limit");
}

#[test]
fn exchange_follows_five_redirects_and_rejects_six() {
    let fixture = TlsFixture::start(|path, port| match path {
        "/r0" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/r1\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/r1" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/r2\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/r2" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/r3\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/r3" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/r4\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/r4" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/r5\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/r5" => TlsReply::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nok5".to_vec()),
        "/s0" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/r0\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });

    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let five_url = Url::parse(&fixture.url("/r0")).unwrap();
    let ok = trusted.exchange("GET", &five_url, &[], None, true).unwrap();
    assert_eq!(ok.status, 200);
    assert_eq!(ok.body, b"ok5");

    let six_url = Url::parse(&fixture.url("/s0")).unwrap();
    let err = trusted
        .exchange("GET", &six_url, &[], None, true)
        .unwrap_err();
    assert_eq!(err, "too many redirects (5)");
}

#[test]
fn exchange_rejects_https_downgrade_converts_post_302_and_parses_retry_after() {
    let fixture = TlsFixture::start(|path, port| match path {
        "/downgrade" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{port}/plain\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/post-redirect" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/target\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/target" => TlsReply::Raw(
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After:   7 \r\nContent-Length: 2\r\n\r\nhi"
                .to_vec(),
        ),
        "/bad-retry" => TlsReply::Raw(
            b"HTTP/1.1 429 Too Many Requests\r\nRetry-After: Wed, 21 Oct 2026 07:28:00 GMT\r\nContent-Length: 0\r\n\r\n"
                .to_vec(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });

    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());

    let downgrade_url = Url::parse(&fixture.url("/downgrade")).unwrap();
    assert_eq!(
        trusted
            .exchange("GET", &downgrade_url, &[], None, true)
            .unwrap_err(),
        "redirect downgraded off HTTPS"
    );

    let post_url = Url::parse(&fixture.url("/post-redirect")).unwrap();
    let res = trusted
        .exchange("POST", &post_url, &[], Some(b"payload"), true)
        .unwrap();
    assert_eq!(res.status, 429);
    assert_eq!(res.retry_after, Some(7));
    assert_eq!(res.body, b"hi");
    {
        let seen = fixture.seen_requests.lock().unwrap();
        let target_req = seen
            .iter()
            .find(|req| req.contains(" /target HTTP/1.1\r\n"))
            .expect("redirected request to /target");
        assert!(target_req.starts_with("GET /target HTTP/1.1\r\n"));
        assert!(!target_req.contains("Content-Length:"));
    }

    let bad_retry_url = Url::parse(&fixture.url("/bad-retry")).unwrap();
    let bad_res = trusted
        .exchange("GET", &bad_retry_url, &[], None, false)
        .unwrap();
    assert_eq!(bad_res.status, 429);
    assert_eq!(bad_res.retry_after, None);
}

#[test]
fn url_strips_fragments_and_post_without_body_sets_content_length_zero() {
    let fixture = TlsFixture::start(|path, port| match path {
        "/items" => TlsReply::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec()),
        "/redir" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: https://127.0.0.1:{port}/items#done\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });
    let parsed = Url::parse("https://Example.COM/path?a=1#frag").unwrap();
    assert_eq!(parsed.host, "example.com");
    assert_eq!(parsed.path, "/path?a=1");
    let resolved = parsed.resolve("/rel?x=1#section").unwrap();
    assert_eq!(resolved.path, "/rel?x=1");
    assert!(Url::parse("https://example.com#only-frag").unwrap().path == "/");

    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let post_url = Url::parse(&fixture.url("/items#ignore")).unwrap();
    let post_res = trusted.exchange("POST", &post_url, &[], None, false).unwrap();
    assert_eq!(post_res.status, 200);
    let redir_url = Url::parse(&fixture.url("/redir")).unwrap();
    assert_eq!(
        trusted.exchange("GET", &redir_url, &[], None, true).unwrap().status,
        200
    );

    let seen = fixture.seen_requests.lock().unwrap();
    assert!(seen[0].starts_with("POST /items HTTP/1.1\r\n"));
    assert!(seen[0].contains("Content-Length: 0\r\n"));
    assert!(!seen[0].contains("#ignore"));
    assert!(seen[2].starts_with("GET /items HTTP/1.1\r\n"));
    assert!(!seen[2].contains("#done"));
}

#[test]
fn exchange_rejects_cross_origin_307_with_body_and_strips_unlisted_headers() {
    let second = TlsFixture::start(|path, _| match path {
        "/dest" => TlsReply::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok".to_vec()),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });
    let dest_url = second.url("/dest");
    let dest_302 = dest_url.clone();
    let dest_307 = dest_url;
    let first = TlsFixture::start(move |path, _| match path {
        "/302" => TlsReply::Raw(
            format!("HTTP/1.1 302 Found\r\nLocation: {dest_302}\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        "/307" => TlsReply::Raw(
            format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: {dest_307}\r\nContent-Length: 0\r\n\r\n")
                .into_bytes(),
        ),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });
    let mut roots = rustls::RootCertStore::empty();
    roots.add(first.ca_cert.clone()).unwrap();
    roots.add(second.ca_cert.clone()).unwrap();
    let trusted = WebPkiTransport::with_roots_for_test("Nightjar/test".into(), roots);

    let ok = trusted
        .exchange(
            "GET",
            &Url::parse(&first.url("/302")).unwrap(),
            &[
                ("X-Api-Key".into(), "secret-unlisted".into()),
                ("Cookie".into(), "session=xyz".into()),
                ("Accept".into(), "application/json".into()),
            ],
            None,
            true,
        )
        .unwrap();
    assert_eq!(ok.status, 200);
    let second_req = &second.seen_requests.lock().unwrap()[0];
    assert!(!second_req.contains("secret-unlisted"));
    assert!(!second_req.contains("session=xyz"));
    assert!(second_req.contains("Accept: application/json\r\n"));

    let err = trusted
        .exchange(
            "POST",
            &Url::parse(&first.url("/307")).unwrap(),
            &[],
            Some(b"body"),
            true,
        )
        .unwrap_err();
    assert!(err.contains("307"), "unexpected 307 error: {err}");
}

#[test]
fn download_to_path_uses_distinct_part_files_and_get_skips_100_continue() {
    use super::net::Transport;

    let fixture = TlsFixture::start(|path, _| match path {
        "/a.png" => TlsReply::Raw(
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\npng"
                .to_vec(),
        ),
        "/a.jpg" => TlsReply::Raw(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\njpg".to_vec()),
        _ => TlsReply::Raw(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_vec()),
    });
    let trusted =
        WebPkiTransport::with_roots_for_test("Nightjar/test".into(), fixture.roots.clone());
    let resp = trusted
        .get(&Url::parse(&fixture.url("/a.png")).unwrap(), false)
        .unwrap();
    assert_eq!(resp.status, 200);

    let dir = std::env::temp_dir().join("nightjar-update-part-ext-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Pre-create `a.png.part` as a directory so `a.png` would fail if `.part` collides with `a.jpg`,
    // proving `a.jpg` uses `a.jpg.part` instead of `a.part`.
    std::fs::create_dir_all(dir.join("a.png.part")).unwrap();
    let jpg = dir.join("a.jpg");
    assert_eq!(
        trusted
            .download_to_path(&Url::parse(&fixture.url("/a.jpg")).unwrap(), &jpg, false)
            .unwrap(),
        200
    );
    assert_eq!(std::fs::read(&jpg).unwrap(), b"jpg");
    let _ = std::fs::remove_dir_all(&dir);
}
