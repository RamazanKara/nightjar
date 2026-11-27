//! Unit tests for automatic quality ladder decisions and host `0x5538` link-notice bitrate sync.

use super::*;

#[test]
fn a_stored_floor_reads_as_the_nearest_choice() {
    assert!(
        MIN_SHORT_EDGE_CHOICES.contains(&DEFAULT_MIN_SHORT_EDGE)
            && MIN_FPS_CHOICES.contains(&DEFAULT_MIN_FPS)
    );
    // Desktop's old steppers (256-2160 px, 15-240 fps) and Android's lists.
    assert_eq!(nearest_floor(&MIN_SHORT_EDGE_CHOICES, 256), 360);
    assert_eq!(nearest_floor(&MIN_SHORT_EDGE_CHOICES, 480), 540);
    assert_eq!(
        nearest_floor(&MIN_SHORT_EDGE_CHOICES, 630),
        540,
        "a tie takes the lower floor"
    );
    assert_eq!(nearest_floor(&MIN_SHORT_EDGE_CHOICES, 2160), 1440);
    assert_eq!(nearest_floor(&MIN_FPS_CHOICES, 15), 30);
    assert_eq!(nearest_floor(&MIN_FPS_CHOICES, 50), 45);
    assert_eq!(nearest_floor(&MIN_FPS_CHOICES, 75), 60);
    assert_eq!(nearest_floor(&MIN_FPS_CHOICES, 240), 120);
    for choices in [&MIN_SHORT_EDGE_CHOICES[..], &MIN_FPS_CHOICES[..]] {
        for &c in choices {
            assert_eq!(nearest_floor(choices, c), c);
        }
    }
}

const BOUNDS: Bounds = Bounds {
    min_short_edge: 720,
    min_fps: 30,
    align_width: 2,
    align_height: 2,
};

fn hot(quality: &mut AdaptiveQualityController, now_ms: u64) -> Option<Target> {
    (0..4)
        .map(|i| {
            let stats = Stats {
                now_ms: now_ms + i * 1000,
                recent_video: true,
                decode_ms: Some(2.0),
                thermal_level: 2,
                ..Stats::default()
            };
            quality.sample(&stats, true)
        })
        .last()
        .flatten()
}

/// An automatic reconnect returns to the step the stream before had reached, once.
#[test]
fn a_resumed_ladder_returns_to_its_step_once() {
    let mut quality =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    let step = hot(&mut quality, 15_000).unwrap();
    let mut resumed =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    resumed.resume_at(step);
    let tick = Stats {
        now_ms: 1000,
        recent_video: true,
        ..Stats::default()
    };
    assert_eq!(resumed.sample(&tick, true), Some(step));
    resumed.applied(step, 1000).unwrap();
    assert_eq!(resumed.current(), step);
    assert_eq!(
        resumed.ceiling(),
        quality.ceiling(),
        "the same ceiling to climb back to"
    );
    assert_eq!(
        resumed.sample(
            &Stats {
                now_ms: 2000,
                ..tick
            },
            true
        ),
        None,
        "once"
    );
    let mut top = AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    top.resume_at(top.ceiling());
    assert_eq!(top.sample(&tick, true), None, "the top is no step down");
}

#[test]
fn a_tick_with_nothing_measured_never_recovers() {
    let mut quality =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    let lower = hot(&mut quality, 15_000).unwrap();
    quality.applied(lower, 18_000).unwrap();
    let tick = |second: u64| Stats {
        now_ms: second * 1000,
        recent_video: true,
        ..Stats::default()
    };
    assert!((19..120).all(|second| quality.sample(&tick(second), true).is_none()));
    // Measured health recovers within the same time.
    let healthy = |second: u64| Stats {
        loss_percent: Some(0.0),
        decode_ms: Some(2.0),
        jitter_ms: Some(1.0),
        queue_ms: Some(0.0),
        ..tick(second)
    };
    let recovered = (120..240).find_map(|second| quality.sample(&healthy(second), true));
    assert_eq!(recovered, Some(quality.ceiling()));
}

#[test]
fn invalid_streams_are_refused() {
    for (width, height, fps) in [(0, 1080, 60), (1920, 0, 60), (1920, 1080, 0)] {
        let result = AdaptiveQualityController::new(Mode::Motion, width, height, fps, BOUNDS, 0);
        assert_eq!(result.err(), Some(QualityError::InvalidStream));
    }
    let mut quality =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    let lower = hot(&mut quality, 15_000).unwrap();
    quality.applied(lower, 18_000).unwrap();
    // A refused resize leaves the ladder and the level alone.
    assert_eq!(
        quality.resize_ceiling(0, 904, 120, 20_000),
        Err(QualityError::InvalidStream)
    );
    assert_eq!(quality.current(), lower);
    assert!(quality.ceiling().matches(1920, 1080, 120));
}

#[test]
fn targets_off_the_ladder_are_refused() {
    let mut quality =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    let foreign = Target {
        width: 1000,
        height: 1000,
        fps: 120,
    };
    assert_eq!(
        quality.applied(foreign, 1000),
        Err(QualityError::ForeignTarget)
    );
    assert!(quality.current().matches(1920, 1080, 120));
}

#[test]
fn requests_use_millihertz_and_modes_round_trip() {
    let target = Target {
        width: 1280,
        height: 720,
        fps: 60,
    };
    assert_eq!(target.request(), [1280, 720, 60_000]);
    assert_eq!(parse_target(&target.to_string()), Some(target));
    for mode in [Mode::Off, Mode::Motion, Mode::Detail, Mode::Battery] {
        assert_eq!(Mode::parse(Some(mode.preference())), mode);
    }
    assert_eq!(Mode::parse(None), Mode::Off);
}

#[test]
fn automatic_quality_leaves_pc_link_controller_in_charge_when_0x5538_is_offered() {
    // Non-0x5538 host: Automatic quality turns on client HTTP `/bitrate` when nothing else holds it.
    assert!(quality_takes_bitrate(
        false, true, false, false, false, false
    ));
    assert!(!quality_takes_bitrate(
        true, true, false, false, false, false
    ));
    assert!(!quality_takes_bitrate(
        false, false, false, false, false, false
    ));
    assert!(!quality_takes_bitrate(
        false, true, true, false, false, false
    ));
    assert!(!quality_takes_bitrate(
        false, true, false, true, false, false
    ));
    assert!(!quality_takes_bitrate(
        false, true, false, false, true, false
    ));

    // 0x5538 host (`host_link_notice == true`): Automatic quality never starts client `/bitrate`,
    // which would call `Session::take_client_bitrate` and kill the PC's LinkRate + Congestion.
    assert!(!quality_takes_bitrate(
        false, true, false, false, false, true
    ));

    // Stream start (`own_lowering_at_start`):
    // - "pc" + Automatic quality on non-0x5538 host -> starts client ABR (`true`).
    assert!(own_lowering_at_start(false, false, false, true, false));
    // - "pc" + Automatic quality on 0x5538 host -> lets PC steer (`false`).
    assert!(!own_lowering_at_start(false, false, false, true, true));
    // - "sooner" (`adaptive_bitrate == true`) still starts device lowering unless Never/manual.
    assert!(own_lowering_at_start(false, false, true, false, true));
    assert!(own_lowering_at_start(false, false, true, true, true));
    // - "keep" (`keep_bitrate == true`) or manual bitrate never starts device lowering.
    assert!(!own_lowering_at_start(false, true, true, true, false));
    assert!(!own_lowering_at_start(true, false, true, true, false));
}

#[test]
fn host_link_room_waits_for_pc_bitrate_cuts_before_stepping_down_quality() {
    assert!(host_steers_link_bitrate(
        true, false, false, false, false, false
    ));
    assert!(!host_steers_link_bitrate(
        false, false, false, false, false, false
    ));
    assert!(!host_steers_link_bitrate(
        true, true, false, false, false, false
    ));
    assert!(!host_steers_link_bitrate(
        true, false, true, false, false, false
    ));
    assert!(!host_steers_link_bitrate(
        true, false, false, true, false, false
    ));
    assert!(!host_steers_link_bitrate(
        true, false, false, false, true, false
    ));
    assert!(!host_steers_link_bitrate(
        true, false, false, false, false, true
    ));

    // Full (including before any LinkNotice arrives) always has room.
    assert!(host_link_has_room(LinkState::Full, 0, 0));
    assert!(host_link_has_room(LinkState::Full, 50_000, 50_000));

    // Limited has room strictly above 65% of ceiling (32_500 for 50_000 kbps).
    assert!(host_link_has_room(LinkState::Limited, 32_501, 50_000));
    assert!(!host_link_has_room(LinkState::Limited, 32_500, 50_000));
    assert!(!host_link_has_room(LinkState::Limited, 20_000, 50_000));
    assert!(!host_link_has_room(LinkState::Limited, 10_000, 0));
    // Overflow safety near u32::MAX.
    assert!(host_link_has_room(LinkState::Limited, u32::MAX, u32::MAX));
    assert!(!host_link_has_room(LinkState::Limited, 1_000_000, u32::MAX));

    // Floor never has room, even when ceiling started near the floor.
    assert!(!host_link_has_room(LinkState::Floor, 2_000, 2_500));
    assert!(!host_link_has_room(LinkState::Floor, 5_000, 50_000));

    // End-to-end ladder coordination under sustained network loss.
    let mut quality =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    let lossy = |second: u64| Stats {
        now_ms: second * 1000,
        recent_video: true,
        poor_connection: true,
        loss_percent: Some(8.0),
        decode_ms: Some(2.0),
        jitter_ms: Some(1.0),
        queue_ms: Some(30.0),
        ..Stats::default()
    };
    for s in 15..25 {
        let room = host_link_has_room(LinkState::Full, 50_000, 50_000);
        assert_eq!(quality.sample(&lossy(s), room), None);
    }
    for s in 25..35 {
        let room = host_link_has_room(LinkState::Limited, 40_000, 50_000);
        assert_eq!(quality.sample(&lossy(s), room), None);
    }
    // Host cuts to 65% (32_500 kbps): 8 samples of persistent loss step down resolution.
    let cut_room = host_link_has_room(LinkState::Limited, 32_500, 50_000);
    assert!(!cut_room);
    for s in 35..42 {
        assert_eq!(quality.sample(&lossy(s), cut_room), None);
    }
    assert!(quality.sample(&lossy(42), cut_room).is_some());

    // Lossless standing queue / `poor_connection` (`loss == 0%`, where loss-only controllers
    // never cut `bitrate_has_room`) waits 8 s for any delay-aware controller, then counts 8 s
    // of network pressure and steps down instead of stalling forever.
    let mut bloated =
        AdaptiveQualityController::new(Mode::Motion, 1920, 1080, 120, BOUNDS, 0).unwrap();
    let queue_only = |second: u64| Stats {
        now_ms: second * 1000,
        recent_video: true,
        loss_percent: Some(0.0),
        decode_ms: Some(2.0),
        jitter_ms: Some(1.0),
        queue_ms: Some(40.0),
        ..Stats::default()
    };
    for s in 15..30 {
        assert_eq!(bloated.sample(&queue_only(s), true), None, "second {s}");
    }
    assert!(bloated.sample(&queue_only(30), true).is_some());
}
