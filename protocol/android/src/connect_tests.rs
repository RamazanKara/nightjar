//! Unit tests for Android connection preflight packet sizing, metrics poller slab,
//! and XML-free `ServerInfo` construction against special-character UUIDs.

use super::*;
use nightjar_core::connect::actions::serverinfo_confirms_ended_session;
use nightjar_policy::http::{AppRole, PERMISSION_LAUNCH, PERMISSION_VIEW};

#[test]
fn metrics_poller_slab_handles_terminal_403_and_clear() {
    let handle = METRICS_POLLERS.insert(HardwareMetricsPoller::new());
    let poller = METRICS_POLLERS.get(handle).expect("poller");
    let r#gen = poller.due(1000, true).expect("due");
    let (show, _, unavail) =
        poller.finish_step_sample(r#gen, 1100, Err(HardwareStepError::Unavailable));
    assert!(!show);
    assert!(unavail);
    assert!(poller.is_unavailable());
    METRICS_POLLERS.remove(handle);
    assert!(METRICS_POLLERS.get(handle).is_none());
}

#[test]
fn build_server_info_accepts_xml_special_chars_without_panicking() {
    for bad in ["bad<uuid", "a&b", "x]]>y\"'"] {
        let info = build_server_info(true, 42, bad, false, true, true);
        assert!(info.paired);
        assert_eq!(info.current_game, 42);
        assert_eq!(info.current_game_uuid.as_deref(), Some(bad));

        let idle_info = ServerInfo::synthetic(
            true,
            0,
            bad,
            true,
            false,
            false,
            Some(PERMISSION_VIEW | PERMISSION_LAUNCH),
        );
        assert!(!serverinfo_confirms_ended_session(
            false,
            AppRole::Stream,
            false,
            &idle_info,
        ));
    }
}

#[test]
fn resolve_packet_size_auto_vpn_uses_1024_while_explicit_remote_keeps_1392() {
    let probe_unused = |_, _, _, _| -> Result<Measurement, http::Error> {
        panic!("probe should not run for remote or resume")
    };
    assert_eq!(
        resolve_packet_size_values(
            0,
            false,
            true,
            8192,
            true,
            1392,
            false,
            false,
            0,
            probe_unused
        ),
        [1024, 0]
    );
    assert_eq!(
        resolve_packet_size_values(
            0,
            false,
            true,
            8192,
            true,
            1392,
            true,
            false,
            0,
            probe_unused
        ),
        [1024, 0]
    );
    assert_eq!(
        resolve_packet_size_values(
            2,
            false,
            true,
            8192,
            true,
            1392,
            false,
            false,
            0,
            probe_unused
        ),
        [1392, 0]
    );
    assert_eq!(
        resolve_packet_size_values(
            0,
            false,
            true,
            8192,
            true,
            1200,
            false,
            false,
            0,
            probe_unused
        ),
        [1200, 0]
    );
    // Resume skips jumbo probe even on Local + jumbo_frames = true.
    assert_eq!(
        resolve_packet_size_values(
            1,
            true,
            false,
            8192,
            true,
            1392,
            true,
            true,
            0,
            probe_unused
        ),
        [1392, 0]
    );
    // Non-jumbo LAN keeps 1392 without probing, and custom 1200 stays 1200 even with jumbo on.
    assert_eq!(
        resolve_packet_size_values(
            0,
            true,
            false,
            8192,
            true,
            1392,
            false,
            false,
            0,
            probe_unused
        ),
        [1392, 0]
    );
    assert_eq!(
        resolve_packet_size_values(
            0,
            true,
            false,
            8192,
            true,
            1200,
            true,
            false,
            0,
            probe_unused
        ),
        [1200, 0]
    );
    // Explicit Remote skips jumbo even when jumbo_frames = true.
    assert_eq!(
        resolve_packet_size_values(
            2,
            true,
            false,
            8192,
            true,
            1392,
            true,
            false,
            0,
            probe_unused
        ),
        [1392, 0]
    );
}

#[test]
fn resolve_packet_size_probes_jumbo_for_android_default_1392_on_lan() {
    let passing = Measurement {
        expected: 100,
        sent: 100,
        received: 100,
        sender_ms: 2000.0,
        loss: 0.0,
        worst_window_loss: 0.0,
        frames: 120,
        damaged_frames: 0,
        excessive_loss_frames: 0,
        missing_tail_frames: 0,
        worst_frame_loss: 0.0,
    };

    for loc_ord in [0, 1] {
        let mut probed_sizes = Vec::new();
        let res = resolve_packet_size_values(
            loc_ord,
            true,
            false,
            8192,
            true,
            1392,
            true,
            false,
            0,
            |_, size, _, _| {
                probed_sizes.push(size);
                Ok(passing.clone())
            },
        );
        assert_eq!(probed_sizes, vec![8160, 1392]);
        assert_eq!(res, [8192, 0]);

        let fail_res = resolve_packet_size_values(
            loc_ord,
            true,
            false,
            8192,
            true,
            1392,
            true,
            false,
            0,
            |_, _, _, _| Err(http::Error::Io(std::io::Error::other("jumbo loss"))),
        );
        assert_eq!(fail_res, [1392, 1]);
    }
}

#[test]
fn a_resume_keeps_the_running_sessions_packet_size() {
    let probe_unused =
        |_, _, _, _| -> Result<Measurement, http::Error> { panic!("a resume does not probe") };
    // LAN, jumbo frames on: the negotiated size is asked again, 0 falls back to the route's.
    for (negotiated, expected) in [(8160, 8160), (0, 1392)] {
        assert_eq!(
            resolve_packet_size_values(
                0,
                true,
                false,
                8192,
                true,
                1392,
                true,
                true,
                negotiated,
                probe_unused
            ),
            [expected, 0]
        );
    }
    // Back over a VPN, or with jumbo frames off, the carried size is not used.
    assert_eq!(
        resolve_packet_size_values(
            0,
            false,
            true,
            8192,
            true,
            1392,
            true,
            true,
            8160,
            probe_unused
        ),
        [1024, 0]
    );
    assert_eq!(
        resolve_packet_size_values(
            0,
            true,
            false,
            8192,
            true,
            1392,
            false,
            true,
            8160,
            probe_unused
        ),
        [1392, 0]
    );
    // A fresh launch still probes, whatever an earlier session negotiated.
    let mut probed = 0;
    let fresh = resolve_packet_size_values(
        0,
        true,
        false,
        8192,
        true,
        1392,
        true,
        false,
        8160,
        |_, _, _, _| {
            probed += 1;
            Err(http::Error::Cancelled)
        },
    );
    assert_eq!((probed, fresh), (1, [1392, 1]));
}
