//! Unit tests for streaming HTTP uploads, downloads, 100-continue, and cancellation.

use super::*;

#[test]
fn an_upload_streams_exactly_its_length_in_chunks() {
    let sent = file(5 * 1024 * 1024 + 7);
    let (port, server) = host(|mut socket| {
        let head = read_head_of(&mut socket);
        let length: usize = head.lines().find_map(|l| l.strip_prefix("Content-Length: ")).unwrap().parse().unwrap();
        let mut body = vec![0; length];
        socket.read_exact(&mut body).unwrap();
        socket.write_all(b"HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}").unwrap();
        (head, body)
    });
    let host_name = Host::parse("127.0.0.1").unwrap();
    let mut calls = Vec::new();
    // A reader with more than `length`: only `length` bytes go out.
    let mut reader = std::io::Cursor::new([sent.as_slice(), b"not sent"].concat());
    let response = upload(&transfer(&host_name, port, "files?name=a"), &Cancel::default(),
        Upload { reader: &mut reader, length: sent.len() as u64, content_type: "application/octet-stream", method: "POST" },
        &mut recorder(&mut calls), &Cancel::default()).unwrap();
    assert_eq!((response.status, response.body.as_slice()), (201, &b"{}"[..]));
    let (head, body) = server.join().unwrap();
    assert!(head.starts_with("POST /files?name=a HTTP/1.1\r\n"), "{head}");
    assert!(head.contains("Content-Type: application/octet-stream\r\nContent-Length: 5242887\r\n"), "{head}");
    assert!(head.ends_with("\r\nExpect: 100-continue\r\n\r\n"), "{head}");
    assert!(body == sent);
    assert_progress(&calls, sent.len() as u64);
}

#[test]
fn an_upload_refused_by_its_head_sends_no_body_and_returns_the_refusal() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let refuse = |answer: &'static [u8]| host(move |mut socket| {
        read_head_only(&mut socket);
        socket.write_all(answer).unwrap();
        read_rest(&mut socket).len()
    });
    let send = |port| upload(&transfer(&host_name, port, "files"), &Cancel::default(),
        Upload { reader: &mut &file(1024 * 1024)[..], length: 1024 * 1024, content_type: "image/png", method: "POST" },
        &mut |_, _| {}, &Cancel::default());
    // Interim answers other than 100 are skipped.
    let (port, server) = refuse(b"HTTP/1.1 103 Early Hints\r\n\r\nHTTP/1.1 413 Payload Too Large\r\nContent-Length: 3\r\n\r\nbig");
    let response = send(port).unwrap();
    assert_eq!((response.status, response.body.as_slice()), (413, &b"big"[..]));
    assert_eq!(server.join().unwrap(), 0);
    // A success for a body it never got is not one.
    let (port, server) = refuse(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
    let result = send(port);
    assert!(matches!(&result, Err(Error::Malformed(m)) if m.contains("before the upload was sent")), "{result:?}");
    assert_eq!(server.join().unwrap(), 0);
}

#[test]
fn a_host_that_never_says_continue_gets_the_body_after_the_wait() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let sent = file(300 * 1024);
    let length = sent.len() as u64;
    for tls in [false, true] {
        fn serve(socket: &mut (impl Read + Write), length: usize) -> Vec<u8> {
            let head = read_head_only(socket);
            assert!(head.contains("\r\nExpect: 100-continue\r\n"), "{head}");
            let mut body = vec![0; length];
            socket.read_exact(&mut body).unwrap();
            socket.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\n\r\n{}").unwrap();
            socket.flush().unwrap();
            body
        }
        let (port, config, server) = if tls {
            let (port, config, server) = tls_host(move |mut tls| serve(&mut tls, length as usize));
            (port, Some(config), server)
        } else {
            let (port, server) = host(move |mut socket| serve(&mut socket, length as usize));
            (port, None, server)
        };
        let request = Request { tls: config, ..transfer(&host_name, port, "files") };
        let started = std::time::Instant::now();
        let response = upload(&request, &Cancel::default(),
            Upload { reader: &mut &sent[..], length, content_type: "application/octet-stream", method: "PUT" },
            &mut |_, _| {}, &Cancel::default()).unwrap();
        let took = started.elapsed();
        assert_eq!((response.status, response.body.as_slice()), (201, &b"{}"[..]), "tls {tls}");
        assert!(server.join().unwrap() == sent, "tls {tls}");
        // After the wait, and well inside the 10 s read timeout.
        assert!(took >= CONTINUE_WAIT - Duration::from_millis(100) && took < Duration::from_secs(5), "tls {tls}: {took:?}");
    }
}

#[test]
fn a_continue_wait_cuts_only_reads_while_armed() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    let continue_by = ContinueBy::default();
    let abort = Abort::new(&socket).unwrap();
    let mut tcp = DeadlineSocket { socket, abort, deadline: None, idle: None, read_timeout: Some(Duration::from_secs(10)),
        write_timeout: None, continue_by: Some(continue_by.clone()), capped: false };
    continue_by.set(Some(std::time::Instant::now() + Duration::from_millis(200)));
    let started = std::time::Instant::now();
    let error = tcp.read(&mut [0; 4]).unwrap_err();
    assert!(is_continue_elapsed(&Error::Io(error)), "a wait that ran out is not a timeout");
    assert!(started.elapsed() < Duration::from_secs(5));
    // Disarmed, the read timeout is the socket's again.
    continue_by.set(None);
    peer.write_all(b"late").unwrap();
    let mut bytes = [0; 4];
    tcp.read_exact(&mut bytes).unwrap();
    assert_eq!(&bytes, b"late");
    assert!(!tcp.capped);
    assert_eq!(tcp.socket.read_timeout().unwrap(), Some(Duration::from_secs(10)));
}

#[test]
fn a_short_or_stopped_upload_is_an_error_not_a_short_body() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    // The file ends early.
    let (port, server) = host(|mut socket| { read_head_of(&mut socket); read_rest(&mut socket).len() });
    let result = upload(&transfer(&host_name, port, "files"), &Cancel::default(),
        Upload { reader: &mut &file(1000)[..], length: 4000, content_type: "application/octet-stream", method: "POST" },
        &mut |_, _| {}, &Cancel::default());
    assert!(matches!(&result, Err(Error::InvalidArgument(m)) if m == "the upload ended after 1000 of 4000 bytes"), "{result:?}");
    assert_eq!(server.join().unwrap(), 1000);
    // Stopped mid-transfer: no further chunk goes out.
    let (port, server) = host(|mut socket| { read_head_of(&mut socket); read_rest(&mut socket).len() });
    let stop = Cancel::default();
    let data = file(4 * 1024 * 1024);
    let mut reader = Trip::new(&data[..], 1024 * 1024, || stop.cancel());
    let result = upload(&transfer(&host_name, port, "files"), &Cancel::default(),
        Upload { reader: &mut reader, length: 4 * 1024 * 1024, content_type: "application/octet-stream", method: "POST" },
        &mut |_, _| {}, &stop);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    // The cancel also shuts the socket, so the chunk in flight may not get out either.
    assert!(server.join().unwrap() <= 1024 * 1024);
}

#[test]
fn a_cancel_ends_a_stalled_transfer_at_once_and_only_that_transfer() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let client = Cancel::default();
    // A download whose host sends half the body, then nothing: the read blocks until the
    // cancel shuts the socket, and the body cut short is a cancel, not a malformed answer.
    let (port, server) = host(|mut socket| {
        read_head_of(&mut socket);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2000\r\n\r\n").unwrap();
        socket.write_all(&file(1000)).unwrap();
        read_rest(&mut socket);
    });
    let job = Arc::new(Cancel::default());
    let canceller = cancel_later(&job, Duration::from_millis(200));
    let started = std::time::Instant::now();
    let result = download(&transfer(&host_name, port, "f"), &client, &mut Vec::new(), &mut |_, _| {}, &job);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    canceller.join().unwrap();
    server.join().unwrap();

    // An upload to a host that stops reading: the write blocks until the cancel.
    let (port, server) = host(|mut socket| {
        read_head_of(&mut socket);
        std::thread::sleep(Duration::from_secs(3));
    });
    let job = Arc::new(Cancel::default());
    let canceller = cancel_later(&job, Duration::from_millis(200));
    let started = std::time::Instant::now();
    let result = upload(&transfer(&host_name, port, "files"), &client,
        Upload { reader: &mut std::io::repeat(7).take(64 << 20), length: 64 << 20, content_type: "application/octet-stream", method: "POST" },
        &mut |_, _| {}, &job);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    canceller.join().unwrap();
    server.join().unwrap();

    // The client's own cancel is untouched: its next request goes through.
    assert!(!client.is_cancelled());
    let (port, server) = serve_download("HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\n".into(), b"abc".to_vec());
    let mut sink = Vec::new();
    download(&transfer(&host_name, port, "f"), &client, &mut sink, &mut |_, _| {}, &Cancel::default()).unwrap();
    assert_eq!(sink, b"abc");
    server.join().unwrap();
    // And a client-wide cancel still stops a transfer.
    let job = Cancel::default();
    client.cancel();
    let result = download(&transfer(&host_name, port, "f"), &client, &mut Vec::new(), &mut |_, _| {}, &job);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
}

#[test]
fn a_cancel_also_ends_a_write_that_reaches_the_socket_after_it() {
    // A host that reads nothing: the write below blocks once the buffers are full.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (_peer, _) = listener.accept().unwrap();
    // Bounds the test if the cancel misses the write.
    socket.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    let abort = Abort::new(&socket).unwrap();
    let mut tcp = DeadlineSocket { socket, abort: abort.clone(), deadline: None, idle: None, read_timeout: None,
        write_timeout: None, continue_by: None, capped: false };
    let cancel = Arc::new(Cancel::default());
    cancel.register(&abort).unwrap();
    let started = std::time::Instant::now();
    // The write passed its check; the cancel runs before it reaches the socket. The
    // canceller starts inside the guard, so the cancel always races a write in flight.
    let (written, ended, canceller) = abort.guard(|| {
        let canceller = {
            let cancel = cancel.clone();
            std::thread::spawn(move || { cancel.cancel(); std::time::Instant::now() })
        };
        while !abort.aborted.load(Ordering::SeqCst) { std::thread::yield_now(); }
        std::thread::sleep(Duration::from_millis(50));
        Ok(((&tcp.socket).write_all(&vec![7; 64 << 20]), std::time::Instant::now(), canceller))
    }).unwrap();
    let cancelled = canceller.join().unwrap();
    assert!(written.is_err());
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    // On Windows the cancel returns only once the write it raced has ended; elsewhere the
    // shutdown alone ends the write and the cancel does not wait for it.
    #[cfg(windows)]
    assert!(cancelled >= ended);
    #[cfg(not(windows))]
    let _ = (cancelled, ended);
    // Any later call fails before it reaches the socket.
    for error in [tcp.write(b"x").unwrap_err(), tcp.read(&mut [0; 1]).unwrap_err()] {
        assert_eq!(error.kind(), ErrorKind::ConnectionAborted, "{error}");
    }
}

#[test]
fn a_host_that_stops_reading_fails_an_upload_after_one_timeout() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let length = 64 << 20;
    // TLS buffers a write that timed out and retries it on the next write and read, a full
    // timeout each; a plain socket would wait once more for an answer.
    for tls in [true, false] {
        let stall = |_| std::thread::sleep(Duration::from_secs(3));
        let (port, config, server) = if tls {
            let (port, config, server) = tls_host(move |mut tls| { read_head_of(&mut tls); stall(tls.sock) });
            (port, Some(config), server)
        } else {
            let (port, server) = host(move |mut socket| { read_head_of(&mut socket); stall(socket) });
            (port, None, server)
        };
        let request = Request { host: &host_name, port, target: "files", tls: config,
            timeouts: Timeouts { connect: Duration::from_secs(1), read: Some(Duration::from_secs(1)) }, body_limit: 1024 };
        let mut last = 0;
        let started = std::time::Instant::now();
        let result = upload(&request, &Cancel::default(),
            Upload { reader: &mut std::io::repeat(7).take(length), length, content_type: "application/octet-stream", method: "POST" },
            &mut |done, _| last = done, &Cancel::default());
        let took = started.elapsed();
        assert!(matches!(&result, Err(Error::Io(e)) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)),
            "tls {tls}: {result:?}");
        assert!(last < length, "tls {tls}: the host took it all");
        // Under two timeouts: 1 s leaves a loaded PC room for the handshake and the rest
        // (500 ms timed the handshake out and left 450 ms, which a busy run overshot).
        assert!(took < Duration::from_millis(1900), "tls {tls}: {took:?}");
        server.join().unwrap();
    }
}

#[test]
fn only_whole_writes_and_any_read_move_the_idle_deadline() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let idle = Duration::from_secs(60);
    let start = std::time::Instant::now() + Duration::from_millis(10);
    let abort = Abort::new(&socket).unwrap();
    let mut tcp = DeadlineSocket { socket, abort, deadline: Some(start), idle: Some(idle), read_timeout: None, write_timeout: None,
        continue_by: None, capped: false };
    // What Linux returns for a write whose timeout ran out with part of it sent.
    tcp.moved(Ok(7876), 16406).unwrap();
    assert_eq!(tcp.deadline, Some(start));
    tcp.moved(Err(ErrorKind::TimedOut.into()), 1).unwrap_err();
    tcp.moved(Ok(0), 1).unwrap();
    assert_eq!(tcp.deadline, Some(start));
    tcp.moved(Ok(16406), 16406).unwrap();
    assert!(tcp.deadline.unwrap() > start + idle / 2);
    let moved = tcp.deadline;
    tcp.moved(Ok(1), 1).unwrap();
    assert!(tcp.deadline >= moved);
}

#[test]
fn a_slow_but_steady_download_is_not_idle() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let sent = file(6000);
    let body = sent.clone();
    let (port, server) = host(move |mut socket| {
        read_head_of(&mut socket);
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", body.len()).as_bytes()).unwrap();
        for piece in body.chunks(500) {
            std::thread::sleep(Duration::from_millis(100));
            socket.write_all(piece).unwrap();
        }
    });
    let request = Request { host: &host_name, port, target: "f", tls: None,
        timeouts: Timeouts { connect: Duration::from_secs(1), read: Some(Duration::from_secs(1)) }, body_limit: 1024 };
    let mut sink = Vec::new();
    // 1.2 s or more in all, past the timeout, but a byte every 100 ms: a loaded PC that
    // stretches a gap tenfold still keeps it under the timeout (150 ms against 400 ms did not).
    download(&request, &Cancel::default(), &mut sink, &mut |_, _| {}, &Cancel::default()).unwrap();
    assert!(sink == sent);
    server.join().unwrap();
}

/// Takes `accepted` bytes, then fails every write like a reset connection.
struct Refusing {
    accepted: usize,
    answer: std::io::Cursor<Vec<u8>>,
}

impl Read for Refusing {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> { self.answer.read(bytes) }
}

impl Write for Refusing {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.accepted { return Err(ErrorKind::ConnectionReset.into()); }
        self.accepted -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

#[test]
fn a_refusal_sent_before_the_body_was_read_wins_over_the_failed_write() {
    let send = |answer: &[u8]| {
        let mut stream = Refusing { accepted: 100 * 1024, answer: std::io::Cursor::new(answer.to_vec()) };
        send_upload(&mut stream, Upload { reader: &mut &file(1024 * 1024)[..], length: 1024 * 1024,
            content_type: "application/octet-stream", method: "POST" }, 1024, &mut |_, _| {}, &Cancel::default())
    };
    let error = br#"{"error":{"code":"too_large","message":""}}"#;
    let refusal = [format!("HTTP/1.1 413 Payload Too Large\r\nContent-Length: {}\r\n\r\n", error.len()).as_bytes(), error].concat();
    let response = send(&refusal).unwrap();
    assert_eq!(response.status, 413);
    assert!(response.body.starts_with(br#"{"error":{"code":"too_large""#));
    // No answer, or a success the host cannot have meant: the write error stands.
    assert!(matches!(send(b""), Err(Error::Io(e)) if e.kind() == ErrorKind::ConnectionReset));
    assert!(matches!(send(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\n\r\n"), Err(Error::Io(e)) if e.kind() == ErrorKind::ConnectionReset));
}

#[test]
fn a_download_streams_into_the_sink_and_must_be_whole() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let sent = file(3 * 1024 * 1024 + 11);
    let (port, server) = serve_download(format!("HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n", sent.len()), sent.clone());
    let mut sink = Vec::new();
    let mut calls = Vec::new();
    let response = download(&transfer(&host_name, port, "files/content?name=a"), &Cancel::default(), &mut sink,
        &mut recorder(&mut calls), &Cancel::default()).unwrap();
    assert_eq!((response.status, response.body.len()), (200, 0));
    assert!(sink == sent);
    assert_progress(&calls, sent.len() as u64);
    assert!(server.join().unwrap().starts_with("GET /files/content?name=a HTTP/1.1\r\n"));

    // The host closes early.
    let (port, server) = serve_download("HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n".into(), file(500));
    let result = download(&transfer(&host_name, port, "f"), &Cancel::default(), &mut Vec::new(), &mut |_, _| {}, &Cancel::default());
    assert!(matches!(&result, Err(Error::Malformed(m)) if m == "the download ended after 500 of 1000 bytes"), "{result:?}");
    server.join().unwrap();

    // The host sends more than it announced, by a lot or by one byte.
    for body in [file(1500), [file(1000), file(1)].concat()] {
        let (port, server) = serve_download("HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n".into(), body);
        let result = download(&transfer(&host_name, port, "f"), &Cancel::default(), &mut Vec::new(), &mut |_, _| {}, &Cancel::default());
        assert!(matches!(&result, Err(Error::Malformed(m)) if m == "the download is longer than its 1000 bytes"), "{result:?}");
        server.join().unwrap();
    }

    // No length to check against, or one that is not plain digits.
    for head in ["HTTP/1.1 200 OK\r\n\r\n", "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 3\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: +3\r\n\r\n", "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nContent-Length: 3\r\n\r\n"] {
        let (port, server) = serve_download(head.into(), b"3\r\nabc\r\n0\r\n\r\n".to_vec());
        let mut sink = Vec::new();
        let result = download(&transfer(&host_name, port, "f"), &Cancel::default(), &mut sink, &mut |_, _| {}, &Cancel::default());
        assert!(matches!(result, Err(Error::Malformed(_))) && sink.is_empty(), "{head}: {result:?}");
        server.join().unwrap();
    }

    // A refusal is read whole for its error and never reaches the sink.
    let error = br#"{"error":{"code":"not_found","message":"No such file."}}"#;
    let (port, server) = serve_download(format!("HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n\r\n", error.len()), error.to_vec());
    let mut sink = Vec::new();
    let response = download(&transfer(&host_name, port, "f"), &Cancel::default(), &mut sink, &mut |_, _| {}, &Cancel::default()).unwrap();
    assert_eq!((response.status, response.body.as_slice(), sink.len()), (404, &error[..], 0));
    server.join().unwrap();
}

#[test]
fn a_stopped_download_ends_mid_transfer() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let length = 16 * 1024 * 1024;
    let (port, server) = serve_download(format!("HTTP/1.1 200 OK\r\nContent-Length: {length}\r\n\r\n"), file(length));
    let stop = Cancel::default();
    let mut sink = Trip::new(Vec::new(), 1024 * 1024, || stop.cancel());
    let result = download(&transfer(&host_name, port, "f"), &Cancel::default(), &mut sink, &mut |_, _| {}, &stop);
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    // It stops before the next piece it reads.
    let got = sink.inner.len();
    assert!((1024 * 1024..1024 * 1024 + 16 * 1024).contains(&got), "{got}");
    server.join().unwrap();
}

#[test]
fn late_1xx_responses_are_skipped_and_downloads_stop_at_content_length() {
    let host_name = Host::parse("127.0.0.1").unwrap();

    // Late 100 Continue arriving right before the final response after CONTINUE_WAIT elapsed.
    let (port, server) = host(|mut socket| {
        let head = read_head_of(&mut socket);
        let length: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        let mut body = vec![0; length];
        socket.read_exact(&mut body).unwrap();
        socket
            .write_all(b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .unwrap();
    });
    let response = upload(
        &transfer(&host_name, port, "files"),
        &Cancel::default(),
        Upload {
            reader: &mut &b"hello"[..],
            length: 5,
            content_type: "application/octet-stream",
            method: "POST",
        },
        &mut |_, _| {},
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!((response.status, response.body.as_slice()), (200, &b"ok"[..]));
    server.join().unwrap();

    // Keep-alive download with Content-Length returns immediately without waiting for EOF.
    let (port, server) = host(|mut socket| {
        read_head_of(&mut socket);
        socket
            .write_all(b"HTTP/1.1 103 Early Hints\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello")
            .unwrap();
        std::thread::sleep(Duration::from_secs(2));
    });
    let start = std::time::Instant::now();
    let mut sink = Vec::new();
    let res = download_optional(
        &transfer(&host_name, port, "opt"),
        &Cancel::default(),
        &mut sink,
        &mut |_, _| {},
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!((res.status, sink.as_slice()), (200, &b"hello"[..]));
    assert!(start.elapsed() < Duration::from_secs(1));
    let _ = server.join();
}

#[test]
fn classic_http_send_enforces_overall_deadline_across_trickled_chunks() {
    let host_name = Host::parse("127.0.0.1").unwrap();
    let (port, server) = host(|mut socket| {
        read_head_of(&mut socket);
        let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n");
        for _ in 0..30 {
            if socket.write_all(b"a").is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    });
    let mut req = transfer(&host_name, port, "slow");
    req.timeouts = Timeouts {
        connect: Duration::from_millis(100),
        read: Some(Duration::from_millis(200)),
    };
    let started = std::time::Instant::now();
    let err = send(&req, &Cancel::default(), None).unwrap_err();
    assert!(
        matches!(&err, Error::Io(e) if matches!(e.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock)),
        "{err:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let _ = server.join();
}
