//! Control thread wake-and-flush loopback tests.

use super::*;
use std::sync::mpsc;

#[test]
fn with_input_wakes_control_thread_and_flushes_immediately() {
    let settings = HostSettings {
        peer_limit: 1,
        channel_limit: channel::COUNT,
        ..Default::default()
    };
    let mut host = Host::new(UdpSocket::bind("127.0.0.1:0").unwrap(), settings).unwrap();
    let host_addr = host.socket().local_addr().unwrap();
    let caps = HostCapabilities {
        app_version: [7, 1, 431, -1],
        feature_flags: 3,
        encryption_enabled: crate::control::negotiation::encryption::CONTROL_V2,
        control_messages: Vec::new(),
        client_roaming: false,
    };
    let config = DriverConfig {
        connect_timeout: Duration::from_secs(2),
        ..DriverConfig::new(host_addr)
    };
    let connecting =
        std::thread::spawn(move || connect(config, caps, &[0x42; 16], SessionOptions::default()));
    let mut host_peer = None;
    let deadline = Instant::now() + Duration::from_secs(2);
    while !connecting.is_finished() || host_peer.is_none() {
        assert!(Instant::now() < deadline, "ENet connect timed out");
        if let Some(Event::Connect { peer, .. }) = host.service().unwrap() {
            host_peer = Some(peer.id());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let _host_peer = host_peer.expect("host accepted peer");
    let (client, handle) = connecting.join().unwrap().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let running = std::thread::spawn(move || {
        let outcome = client.run(|_| {});
        let _ = done_tx.send(outcome);
    });

    // Wait for initial start/haptics packets to drain and settle so the control thread sleeps.
    let settle_until = Instant::now() + Duration::from_millis(40);
    while Instant::now() < settle_until {
        let _ = host.service().unwrap();
        std::thread::sleep(Duration::from_millis(1));
    }

    // Push a mouse move while the control thread is idle; it must wake and flush without waiting
    // for any incoming host packet or MAX_IDLE_WAIT.
    let pushed_at = Instant::now();
    handle
        .with_input(|q| q.send_mouse_move(7, -3))
        .expect("queue accepts mouse move");
    let mut got_input = false;
    let recv_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < recv_deadline {
        if let Some(Event::Receive { channel_id, .. }) = host.service().unwrap()
            && channel_id == channel::MOUSE
        {
            got_input = true;
            break;
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    assert!(got_input, "mouse packet never arrived");
    let elapsed = pushed_at.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "wake + flush stalled for {elapsed:?}"
    );

    handle.stop();
    let stop_deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < stop_deadline {
        let _ = host.service().unwrap();
        if done_rx.try_recv().is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    let _ = running.join();
}
