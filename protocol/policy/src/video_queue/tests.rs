//! Unit tests for `nightjar_policy::video_queue` state machines and pure helpers.

use super::*;

const MS: i64 = 1_000_000;
const CONTENT_NS: i64 = 11_111_111;
const FRAME_NS: i64 = 16_666_667;
const REFRESH_NS: i64 = 16_683_000;

fn feed_residency(r: &mut Residency, count: usize, i2o_ns: i64, mut now_ns: i64) -> i64 {
    for _ in 0..count {
        let excess = r.on_output(i2o_ns, now_ns);
        r.track(excess, CONTENT_NS);
        now_ns += CONTENT_NS;
    }
    now_ns
}

#[test]
fn depth_and_decoder_family_rules() {
    assert_eq!(depth(10, 7, 2), 1);
    assert_eq!(depth(5, 7, 2), 0);

    for family in [
        "MTK",
        "Exynos",
        "Nvidia",
        "Brcm",
        "TI",
        "Allwinner",
        "Amlogic",
        "Intel",
    ] {
        assert_eq!(
            output_limit(&format!("OMX.{family}.avc.decoder"), true, false, false),
            1
        );
    }
    assert_eq!(output_limit("c2.qti.av1.decoder", false, false, false), 1);
    assert_eq!(output_limit("c2.qti.av1.decoder", true, false, false), 2);
    assert_eq!(output_limit("c2.qti.av1.decoder", true, true, false), 1);
    assert_eq!(output_limit("c2.qti.av1.decoder", true, false, true), 1);

    assert!(watches_display_queue("OMX.Nvidia.h264.decode"));
    assert!(watches_display_queue("c2.nvidia.hevc.decoder"));
    assert!(!watches_display_queue("c2.qti.hevc.decoder"));
}

#[test]
fn residency_streak_period_and_window_expiry() {
    let mut r = Residency::new();
    assert_eq!(r.on_output(-1, 1_000_000_000), 0);
    assert_eq!(r.on_output(0, 1_000_000_000), 0);
    assert_eq!(r.on_output(9 * MS, 1_000_000_000), 0);
    assert_eq!(r.on_output(12 * MS, 1_000_000_000 + CONTENT_NS), 3 * MS);

    r.reset();
    let mut now = feed_residency(&mut r, 20, 4 * MS, 1_000_000_000);
    assert!(!r.should_drop(3));
    for _ in 0..(Residency::FRAMES - 1) {
        now = feed_residency(&mut r, 1, 22 * MS, now);
        assert!(!r.should_drop(2));
    }
    now = feed_residency(&mut r, 1, 22 * MS, now);
    assert!(r.should_drop(1));
    assert!(!r.should_drop(0));
    r.on_dropped();
    // Real two-frame backlog: first drop shortens residency from 22 ms -> 11 ms, so after
    // PERIOD outputs it may drop once more to drain the remaining queued buffer.
    for _ in 1..Residency::PERIOD {
        now = feed_residency(&mut r, 1, 11 * MS, now);
        assert!(!r.should_drop(1));
    }
    now = feed_residency(&mut r, 1, 11 * MS, now);
    assert!(r.should_drop(1));

    // Slower decode compute (4 ms -> 10 ms) where a drop does not shorten residency:
    // re-anchors the baseline after one test drop instead of dropping 1 in 4 frames for 10 s.
    r.reset();
    now = feed_residency(&mut r, 20, 4 * MS, now);
    now = feed_residency(&mut r, Residency::FRAMES as usize, 10 * MS, now);
    assert!(r.should_drop(1));
    r.on_dropped();
    for _ in 0..120 {
        now = feed_residency(&mut r, 1, 10 * MS, now);
        assert!(
            !r.should_drop(1),
            "unhelpful drop must stop 1-in-4 frame drops"
        );
    }
    // A real queue building on top of the 10 ms compute floor (10 -> 21 ms) still triggers,
    // and returning to 4 ms immediately re-learns the faster baseline.
    now = feed_residency(&mut r, Residency::FRAMES as usize, 21 * MS, now);
    assert!(r.should_drop(1));
    let _ = feed_residency(&mut r, 1, 4 * MS, now);
    assert_eq!(r.on_output(4 * MS, now + CONTENT_NS), 0);

    // 10s window expiry ages out old floor
    r.reset();
    let mut t = feed_residency(&mut r, 90, 4 * MS, 1_000_000_000);
    let mut early = 0;
    for i in 0..120 {
        let excess = r.on_output(10 * MS, t);
        if i == 0 {
            early = excess;
        }
        t += 100 * MS;
    }
    assert_eq!(early, 6 * MS);
    assert_eq!(r.on_output(10 * MS, t), 0);
}

#[test]
fn display_gate_distinguishes_free_vs_held_and_clear_vs_reset() {
    let mut free = DisplayGate::new();
    let mut held = DisplayGate::new();
    let mut out_free = 1_000_000_000_i64;
    let mut out_held = 1_000_000_000_i64;
    for i in 0..700 {
        let jitter = ((i % 5) - 2) * 400_000;
        let in_step = FRAME_NS + jitter;
        out_free += in_step;
        free.on_output(1_000_000 + jitter / 2, out_free, FRAME_NS, false, false);

        out_held += FRAME_NS;
        let i2o = 18_000_000 - ((i % 5) - 2) * 400_000;
        held.on_output(i2o, out_held, FRAME_NS, true, false);
    }
    assert!(!free.held());
    assert!(held.held());
    assert!(held.still_held(3, 0.5));
    held.clear();
    assert!(!held.held());
    assert!(!held.still_held(3, 0.5));
    held.reset();
    assert!(!held.held());
}

#[test]
fn recent_level_computes_percentiles_and_resets() {
    let mut rl = RecentLevel::new();
    assert_eq!(rl.low_ns(), i64::MAX);
    assert_eq!(rl.last_ns(), 0);
    for s in 0..3 {
        let base = (s + 1) * RecentLevel::SECOND_NS;
        for i in 0..20 {
            rl.add((i + 1) * MS, base + i * 20 * MS);
        }
    }
    rl.add(5 * MS, 4 * RecentLevel::SECOND_NS);
    assert_eq!(rl.last_ns(), 5 * MS);
    assert_eq!(rl.low_ns(), 5 * MS);
    rl.reset();
    assert_eq!(rl.low_ns(), i64::MAX);
    assert_eq!(rl.last_ns(), 0);
}

#[test]
fn debt_recovers_and_learns_with_exact_messages() {
    let mut debt = Debt::new();
    let mut now = 1_000_000_000_i64;
    for _ in 0..30 {
        debt.on_output(MS, 0, now, FRAME_NS, 0);
        now += FRAME_NS;
    }
    assert!(!debt.should_drop());

    // Persistent 1 frame surplus triggers recovery and drops.
    for _ in 0..120 {
        debt.on_output(MS + FRAME_NS, 0, now, FRAME_NS, 1);
        now += FRAME_NS;
    }
    assert!(debt.should_drop());
    debt.on_dropped();
    for _ in 0..Debt::SETTLE {
        debt.on_output(MS, 0, now, FRAME_NS, 0);
        now += FRAME_NS;
    }
    let event = debt.take_event().expect("recovery event");
    assert_eq!(
        event,
        "Decoder debt recovered: 1 stale output dropped, excess 16.7 -> 0.0 ms over a 1.0 ms floor"
    );
    assert!(debt.take_event().is_none());

    // Unhelpful drop learns floor.
    for _ in 0..120 {
        debt.on_output(MS + FRAME_NS, 0, now, FRAME_NS, 1);
        now += FRAME_NS;
    }
    assert!(debt.should_drop());
    debt.on_dropped();
    for _ in 0..Debt::SETTLE {
        debt.on_output(MS + FRAME_NS, 0, now, FRAME_NS, 1);
        now += FRAME_NS;
    }
    let learned_event = debt.take_event().expect("learned event");
    assert!(
        learned_event.contains(
            "Decoder debt paused, a drop did not shorten the queue: 1 stale output dropped"
        ),
        "{learned_event}"
    );
    assert!(
        learned_event.contains(", floor raised to 17.7 ms"),
        "{learned_event}"
    );
}

#[test]
fn in_flight_tracks_rendered_and_expired_slots() {
    let mut inf = InFlight::new();
    assert!(!inf.full(0));
    inf.on_released(10, 100);
    inf.on_released(11, 200);
    assert_eq!(inf.count(50), 2);
    assert!(inf.full(50));
    assert_eq!(inf.count(100), 1);
    inf.on_rendered(10);
    assert_eq!(inf.count(50), 1);
    inf.reset();
    assert_eq!(inf.count(0), 0);
}

#[test]
fn display_queue_watch_applies_detects_sustained_over_and_probes() {
    assert!(DisplayQueueWatch::applies(
        true, true, false, true, false, 60_000, REFRESH_NS
    ));
    assert!(!DisplayQueueWatch::applies(
        true, true, true, true, false, 60_000, REFRESH_NS
    ));
    assert!(!DisplayQueueWatch::applies(
        true, true, false, true, true, 60_000, REFRESH_NS
    ));
    assert!(!DisplayQueueWatch::applies(
        true, true, false, true, false, 30_000, REFRESH_NS
    ));

    let mut watch = DisplayQueueWatch::new();
    watch.configure(true, REFRESH_NS);
    let mut render_ns = 100_000 * MS;
    let step_for =
        |watch: &mut DisplayQueueWatch, seconds: usize, wait_ns: i64, t: &mut i64| -> bool {
            let frames = (seconds as f64 * 1e9 / REFRESH_NS as f64).round() as usize;
            for _ in 0..frames {
                *t += REFRESH_NS;
                let handoff = *t - wait_ns;
                if watch.take_drop(handoff) {
                    return true;
                }
                watch.on_rendered(*t - handoff, *t);
            }
            false
        };

    assert!(!step_for(&mut watch, 12, 30 * MS, &mut render_ns));
    assert!(step_for(&mut watch, 10, 48 * MS, &mut render_ns));
    let drop_msg = watch.take_event().unwrap();
    assert!(drop_msg.contains("over its lowest level"), "{drop_msg}");
    assert!(!step_for(&mut watch, 5, 30 * MS, &mut render_ns));
    let check_msg = watch.take_event().unwrap();
    assert!(check_msg.contains("held a buffer too many"), "{check_msg}");

    // Refresh change emits restart line
    watch.configure(true, 8_333_333);
    assert_eq!(
        watch.take_event().as_deref(),
        Some("Display queue: refresh now 8.33 ms, starting over")
    );
}

#[test]
fn present_scheduler_state_warmup_discontinuity_p97_and_overlay() {
    let mut ps = PresentSchedulerState::new();
    assert!(ps.overlay_line().is_none());
    assert!(!ps.is_late(-1, 100_000_000, FRAME_NS));

    // Fill 32 samples over > 250ms warmup with 4ms behind_us
    for i in 0..35 {
        let host_us = 1_000_000 + i * 16_667;
        let ready_ns = (host_us + 4_000) * 1000;
        assert!(!ps.is_late(host_us, ready_ns, FRAME_NS));
    }
    // Same 4ms is not late
    assert!(!ps.is_late(1_600_000, 1_604_000_000, FRAME_NS));
    // 14ms behind (> 4ms + 0.5ms + 8.33ms) is late
    assert!(ps.is_late(1_616_667, 1_630_667_000, FRAME_NS));

    // Discontinuity resets window, counters stay
    ps.on_planned(false);
    ps.on_planned(true);
    ps.on_late_dropped();
    ps.on_governor_dropped();
    ps.on_cap_dropped();
    ps.on_cap_held();
    assert!(!ps.is_late(50_000_000, 50_020_000_000, FRAME_NS));
    assert_eq!(ps.late_dropped(), 1);
    assert_eq!(ps.governor_dropped(), 1);
    assert_eq!(ps.cap_dropped(), 1);
    assert_eq!(ps.cap_held(), 1);
    assert_eq!(
        ps.overlay_line().as_deref(),
        Some(
            "Scheduler: shared vsync 50.0% | late dropped 1 | queue dropped 2 | compositor held 1"
        )
    );
    assert!(PresentSchedulerState::drop_late(true, true, false));
    assert!(!PresentSchedulerState::drop_late(true, true, true));
}

#[test]
fn decoder_recovery_backoff_and_pure_hang_helpers() {
    let mut b = DecoderRecoveryBackoff::new();
    for expected in [0, 250, 500, 1000, 2000, -1] {
        assert_eq!(b.before_recovery(0), expected);
    }
    assert_eq!(b.recoveries_without_frame(), 6);
    assert_eq!(b.before_recovery(1), 0);
    assert_eq!(b.recoveries_without_frame(), 1);

    let custom = DecoderRecoveryBackoff::with_delays(vec![0, 60_000]);
    let mut custom = custom;
    assert_eq!(custom.before_recovery(5), 0);
    assert_eq!(custom.before_recovery(5), 60_000);
    assert_eq!(custom.before_recovery(5), -1);

    let mut r = Residency::new();
    let now = feed_residency(&mut r, 20, 4 * MS, 1_000_000_000);
    feed_residency(&mut r, 30, 10 * MS, now);
    assert_eq!(
        wants_catch_up(3, now, now, 60_000, 0, 2, true, 100, 90, 0, &r),
        Some(CatchUpReason::Governor)
    );
    assert_eq!(
        wants_catch_up(3, 1, now, 60_000, 0, 2, true, 100, 90, 0, &r),
        Some(CatchUpReason::CatchUp)
    );
    assert_eq!(
        wants_catch_up(3, now, now, 60_000, 0, 2, false, 100, 90, 0, &r),
        Some(CatchUpReason::None)
    );
    assert_eq!(
        wants_catch_up(3, 0, now, 0, 0, 2, true, 100, 90, 0, &r),
        None
    );

    assert_eq!(recovery_attempts_left(10, 600, 0, 10_000, 0), (true, true));
    assert_eq!(
        recovery_attempts_left(10, 599, 0, 10_000, 0),
        (false, false)
    );
    assert!(ran_well_since_hang_reset(600, 0, 10_000, 0));
    assert!(!ran_well_since_hang_reset(599, 0, 10_000, 0));

    // Hang after prior hang reset without running well -> rebuild without keys (no refill call).
    let d1 = hung_decoder_decision(true, true, false, 10, 600, 0, 100, 20_000, 0, 15_000);
    assert_eq!(d1.action, HungDecoderAction::RebuildWithoutKeys);
    assert!(!d1.refilled_attempts);

    // Same frame count without vendor keys -> give up immediately (no refill call).
    let d2 = hung_decoder_decision(false, true, false, 10, 600, 0, 600, 20_000, 0, 0);
    assert_eq!(d2.action, HungDecoderAction::GiveUp);
    assert!(!d2.refilled_attempts);

    // First hang with vendor keys & attempts refilled -> reset with keys and refilled_attempts=true.
    let d3 = hung_decoder_decision(true, true, false, 10, 600, 0, -1, 20_000, 0, 0);
    assert_eq!(d3.action, HungDecoderAction::ResetWithKeys);
    assert!(d3.refilled_attempts);
}
