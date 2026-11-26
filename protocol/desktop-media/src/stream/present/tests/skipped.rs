//! Frame pacing skip rules, `SkippedPicture`, `MissWatch`, and held margin/deadline tests.

use super::super::*;

#[test]
fn frame_pacing_other_than_lowest_latency_waits_for_the_refresh_without_vrr_only() {
    let mut settings = crate::config::Settings { vrr: false, vsync: false, ..Default::default() };
    assert_eq!(settings.frame_pacing, FramePacing::Latency, "the default keeps today's pacing");
    assert_eq!(pacing(&settings, true).0, PacingMode::Immediate, "lowest latency follows the V-Sync setting");
    for mode in [FramePacing::Balanced, FramePacing::CapFps, FramePacing::Smoothness] {
        settings.frame_pacing = mode;
        assert_eq!(pacing(&settings, true).0, PacingMode::Vsync, "{mode:?}");
    }
    let vrr = crate::config::Settings { frame_pacing: FramePacing::Smoothness, ..Default::default() };
    assert_eq!(pacing(&vrr, true), (PacingMode::HostTimes, true), "VRR ignores frame pacing, as on Android");
    assert_eq!(frame_us(60), 16_666);
    assert_eq!(frame_us(0), 250_000, "an unknown rate holds at most 250 ms");
}

/// The report: 120 fps on a 120 Hz panel took 22 ms from arrival to the display, 7 ms when it
/// skips the pictures that would wait, and a 240 Hz panel never waited.
#[test]
fn a_stream_with_no_refreshes_to_spare_skips_instead_of_waiting() {
    let skips = |pacing, fps, hz| skips_instead_of_waiting(PacingMode::Vsync, pacing, frame_us(fps), hz);
    for pacing in [FramePacing::Latency, FramePacing::Balanced] {
        for (fps, hz) in [(120, 120.0), (60, 60.0), (60, 59.94), (60, 60.5), (240, 240.0), (120, 60.0), (144, 120.0)] {
            assert!(skips(pacing, fps, hz), "{pacing:?} {fps} fps at {hz} Hz has none to spare");
        }
        for (fps, hz) in [(120, 240.0), (60, 120.0), (90, 120.0), (120, 144.0), (30, 60.0), (115, 120.0)] {
            assert!(!skips(pacing, fps, hz), "{pacing:?} {fps} fps at {hz} Hz drains by itself, and keeps every picture");
        }
        assert!(!skips(pacing, 120, 0.0), "no refresh rate known");
    }
    for pacing in [FramePacing::CapFps, FramePacing::Smoothness] {
        assert!(!skips(pacing, 120, 120.0), "{pacing:?} keeps its pictures in order");
    }
    for mode in [PacingMode::Immediate, PacingMode::HostTimes, PacingMode::BeamRace] {
        assert!(!skips_instead_of_waiting(mode, FramePacing::Latency, frame_us(120), 120.0), "{mode:?} does not wait");
    }
    let pacer = Pacer::new(PacingMode::Vsync, 8_333).with_frame_pacing(FramePacing::Latency, 120);
    assert!(pacer.skips_instead_of_waiting(120.0));
    assert!(!pacer.skips_instead_of_waiting(240.0));
}

/// A stream dragged to a faster display, or changed by automatic quality, follows its new rate.
#[test]
fn a_new_frame_rate_decides_whether_a_stream_skips() {
    let mut pacer = Pacer::new(PacingMode::Vsync, 16_666).with_frame_pacing(FramePacing::Latency, 60);
    assert!(!pacer.skips_instead_of_waiting(120.0), "60 fps has refreshes to spare at 120 Hz");
    pacer.set_fps(120);
    assert!(pacer.skips_instead_of_waiting(120.0), "120 fps has none");
    assert_eq!(pacer.frame_us(), frame_us(120));
    pacer.set_fps(60);
    assert!(!pacer.skips_instead_of_waiting(120.0));
}

/// The next picture replaces a refused one, which is shown after all only if none comes.
#[test]
fn a_kept_picture_gives_way_to_a_newer_one_and_is_drawn_once_when_none_comes() {
    let mut kept = Some(1);
    assert_eq!(take_over_kept(None, &mut kept, false), (None, 0), "not yet");
    assert_eq!(kept, Some(1));
    assert_eq!(take_over_kept(None, &mut kept, true), (Some(1), 0), "drawn, so not replaced");
    assert_eq!(kept, None);
    assert_eq!(take_over_kept(None, &mut kept, true), (None, 0), "once");

    let mut kept = Some(1);
    assert_eq!(take_over_kept(Some(2), &mut kept, false), (Some(2), 1), "replaced");
    assert_eq!(kept, None);
    let mut kept = Some(1);
    assert_eq!(take_over_kept(Some(2), &mut kept, true), (Some(2), 1), "the newer one wins when both are due");
    let mut kept = None;
    assert_eq!(take_over_kept(Some(2), &mut kept, false), (Some(2), 0), "nothing to replace");
}

/// A still desktop sends no next picture, and the screen must not stay one behind.
#[test]
fn a_skipped_picture_is_drawn_after_all_unless_a_newer_one_reaches_the_display() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    assert!(!skipped.take_due(u64::MAX));
    skipped.skipped(1_000, 8_333, 8_333);
    assert_eq!(skipped.due_us(), Some(13_499), "after the next picture would have come");
    assert!(!skipped.take_due(13_498));
    assert!(!skipped.must_wait());
    assert!(skipped.take_due(13_499));
    assert!(skipped.must_wait(), "nothing later would take its place, so it gets its turn");
    assert!(!skipped.take_due(u64::MAX), "once");
    skipped.shown();
    assert!(!skipped.must_wait());
    skipped.skipped(1_000, 8_333, 8_333);
    skipped.shown();
    assert_eq!(skipped.due_us(), None);
    assert!(!skipped.take_due(u64::MAX), "a newer one showed");
}

/// A GPU that is behind gets its work held back: pictures nobody sees would pile up on it.
#[test]
fn presents_refused_again_and_again_wait_once() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..REFUSED_IN_A_ROW {
        assert!(!skipped.must_wait());
        skipped.skipped(1_000, 8_333, 8_333);
    }
    assert!(skipped.must_wait());
    skipped.shown();
    assert!(!skipped.must_wait());
}

/// At 60 Hz the stream arrived 1.6 ms before the refresh: no hold showed 37 pictures a second,
/// a 1 ms hold 50, 3 ms 60. A display whose deadline is earlier must not stay at half the rate.
#[test]
fn too_short_a_hold_grows_until_every_other_picture_stops_being_refused() {
    let (frame, refresh) = (16_666, 16_666);
    let mut skipped = SkippedPicture::new(std::time::Duration::ZERO);
    let mut grew = 0;
    for _ in 0..4_000 {
        if skipped.hold_us >= 3_000 { break; }
        if skipped.skipped(0, frame, refresh).is_some() { grew += 1; }
        skipped.shown();
    }
    assert_eq!((skipped.hold_us, grew), (3_000, 6), "half the pictures refused: it grows step by step");
}

#[test]
fn a_hold_that_grew_comes_back_once_the_pictures_stop_being_refused() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    while skipped.skipped(0, 8_333, 8_333).is_none() {}
    assert_eq!(skipped.hold_us, 5_500);
    let left = PICTURES_SHORTEN - skipped.pictures;
    for _ in 0..left - 1 { skipped.shown(); }
    assert_eq!(skipped.hold_us, 5_500);
    skipped.shown();
    assert_eq!(skipped.hold(), REFUSED_HOLD, "a step shorter");
    for _ in 0..PICTURES_SHORTEN { skipped.shown(); }
    assert_eq!(skipped.hold(), REFUSED_HOLD, "never below where it started");
}

/// A late picture is refused now and then for good, or the hold would only ever grow.
#[test]
fn late_pictures_now_and_then_do_not_keep_a_hold_long() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    while skipped.skipped(0, 8_333, 8_333).is_none() {}
    assert_eq!(skipped.hold_us, 5_500);
    for n in 0..2 * PICTURES_SHORTEN {
        if n % 100 == 0 { skipped.skipped(0, 8_333, 8_333); } else { skipped.shown(); }
    }
    assert_eq!(skipped.hold(), REFUSED_HOLD, "1 in 100 refused");
    // More than that, and it stays: this display needs it.
    while skipped.skipped(0, 8_333, 8_333).is_none() {}
    assert_eq!(skipped.hold_us, 5_500);
    for n in 0..2 * PICTURES_SHORTEN {
        if n % 16 == 0 { skipped.skipped(0, 8_333, 8_333); } else { skipped.shown(); }
    }
    assert_eq!(skipped.hold_us, 5_500, "1 in 16 refused");
}

/// Frame statistics for a picture on `refresh`, read on the same refresh.
fn on_display(count: u32, refresh: u32, at_us: u64) -> crate::timing_csv::Flip {
    crate::timing_csv::Flip { present_count: count.wrapping_add(1), stat_present_count: count, stat_present_refresh: refresh, stat_sync_refresh: refresh, stat_sync_us: at_us, ..Default::default() }
}

/// The report: a picture presented 4 ms into a refresh reached the display two refreshes later.
#[test]
fn a_picture_a_refresh_late_counts_as_missed_and_a_gap_in_the_stream_does_not() {
    const REFRESH: u64 = 8_333;
    let mut watch = MissWatch::default();
    watch.presented(10, 1_000);
    assert!(!watch.seen(on_display(10, 100, 8_333), REFRESH), "the first statistics have nothing before them");
    watch.presented(11, 9_400);
    assert!(!watch.seen(on_display(11, 101, 16_666), REFRESH), "the next refresh");
    assert!(!watch.seen(on_display(11, 101, 16_666), REFRESH), "the same picture again");
    watch.presented(12, 20_700);
    assert!(watch.seen(on_display(12, 103, 33_332), REFRESH), "presented 12.6 ms before it was shown, and a refresh repeated");
    // The stream paused: the next picture came just before its refresh.
    watch.presented(13, 78_000);
    assert!(!watch.seen(on_display(13, 109, 83_330), REFRESH), "a gap, not a miss");
    // Read a refresh after the flip.
    watch.presented(14, 92_000);
    let read_later = crate::timing_csv::Flip { stat_sync_refresh: 112, stat_sync_us: 108_329, ..on_display(14, 111, 0) };
    assert!(!watch.seen(read_later, REFRESH), "under a refresh from the present to the flip, whenever it was read");
    // Statistics that skipped a picture still add up.
    let mut watch = MissWatch::default();
    assert!(!watch.seen(on_display(20, 200, 8_333), REFRESH));
    watch.presented(22, 0);
    assert!(!watch.seen(on_display(22, 202, 25_000), REFRESH), "two pictures on two refreshes");
    // The counters wrap.
    let mut watch = MissWatch::default();
    assert!(!watch.seen(on_display(u32::MAX, u32::MAX, 8_333), REFRESH));
    watch.presented(1, 0);
    assert!(watch.seen(on_display(1, 2, 33_000), REFRESH), "two pictures on three refreshes, across the wrap");
    // A renderer without statistics.
    assert!(!MissWatch::default().seen(Default::default(), REFRESH));
}

#[test]
fn many_missed_refreshes_make_each_picture_wait_its_turn() {
    const REFRESH: u64 = 8_333;
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..2 * MISSED_WAIT { assert_eq!(skipped.missed(REFRESH, REFRESH), None, "one bad spell passes"); }
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..MISSED_WAIT - 1 { assert_eq!(skipped.missed(REFRESH, REFRESH), None); }
    assert_eq!(skipped.hold(), REFUSED_HOLD, "now and then is the price of no queue");
    assert_eq!(skipped.missed(REFRESH, REFRESH), Some(std::time::Duration::from_micros(REFRESH + HOLD_STEP_US)));
    assert!(skipped.hold() > std::time::Duration::from_micros(REFRESH), "a whole refresh always gets its turn");
    for _ in 0..2 * MISSED_WAIT { assert_eq!(skipped.missed(REFRESH, REFRESH), None, "it holds no longer than that"); }
    // It comes back, and tries again.
    let mut looks = 0;
    while skipped.hold() > REFUSED_HOLD {
        for _ in 0..PICTURES_SHORTEN { skipped.shown(); }
        looks += 1;
    }
    assert_eq!(looks, 8, "half a minute a step at 120 Hz");
}

#[test]
fn misses_spread_over_many_pictures_leave_the_hold_as_it_is() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..20 {
        for _ in 0..MISSED_WAIT - 1 { assert_eq!(skipped.missed(8_333, 8_333), None); }
        for _ in 0..MISSED_OF { skipped.shown(); }
    }
    assert_eq!(skipped.hold(), REFUSED_HOLD);
}

/// A spell on Wi-Fi: a window of many misses among clean ones.
#[test]
fn a_bad_spell_now_and_then_leaves_the_hold_as_it_is() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..10 {
        for _ in 0..2 * MISSED_WAIT { assert_eq!(skipped.missed(8_333, 8_333), None); }
        for _ in 0..2 * MISSED_OF { skipped.shown(); }
    }
    assert_eq!(skipped.hold(), REFUSED_HOLD);
}

/// Every run missed most in its first seconds, while the window was still being placed.
#[test]
fn misses_while_a_stream_settles_do_not_count() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..MISSED_OF - 1 {
        assert_eq!(skipped.missed(8_333, 8_333), None);
        skipped.shown();
    }
    assert_eq!(skipped.hold(), REFUSED_HOLD);
    skipped.shown();
    for _ in 0..MISSED_WAIT - 1 { assert_eq!(skipped.missed(8_333, 8_333), None, "the count starts here"); }
    assert_eq!(skipped.missed(8_333, 8_333), None, "the first window of many");
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..MISSED_WAIT - 1 { assert_eq!(skipped.missed(8_333, 8_333), None); }
    assert!(skipped.missed(8_333, 8_333).is_some(), "the second in a row");
}

#[test]
fn the_desktop_deadline_stays_under_most_of_a_refresh() {
    assert_eq!(deadline_before_refresh(60.0), 7_166);
    assert_eq!(deadline_before_refresh(120.0), 5_000);
    assert_eq!(deadline_before_refresh(240.0), 2_500);
    assert_eq!(deadline_before_refresh(0.0), 0, "no display timing");
}

#[test]
fn fractional_vsync_stands_aside_for_a_chosen_vrr_even_once_the_display_falls_back() {
    assert!(!fractional_vsync_stands_aside(false, PacingMode::Vsync, false), "VRR off, V-Sync pacing: held");
    assert!(fractional_vsync_stands_aside(true, PacingMode::Vsync, false), "VRR chosen, display fell back to V-Sync: the row is hidden");
    assert!(fractional_vsync_stands_aside(false, PacingMode::Immediate, false));
    assert!(fractional_vsync_stands_aside(true, PacingMode::HostTimes, false));
    assert!(fractional_vsync_stands_aside(false, PacingMode::Vsync, true), "the PC's frame times: the row is hidden");
}

#[test]
fn a_held_margin_is_the_deadline_unless_a_measurement_sets_it() {
    assert_eq!(held_base_margin_us(None, false, 120.0), 5_000, "composed at 120 Hz: the desktop's deadline");
    assert_eq!(held_base_margin_us(None, false, 60.0), 7_166, "composed at 60 Hz");
    assert_eq!(held_base_margin_us(None, true, 120.0), 2_000, "nothing composes");
    assert_eq!(held_base_margin_us(Some(6_500), false, 120.0), 6_500, "NIGHTJAR_HELD_MARGIN_US");
    assert_eq!(held_base_margin_us(Some(6_500), true, 120.0), 6_500, "set even where nothing composes");
    assert_eq!(held_base_margin_us(None, false, 0.0), 0, "no refresh rate known");
    // Wayland's Fullscreen with a mode is SDL's emulation: the compositor still schedules it.
    assert!(uncomposed_fullscreen(true, true), "exclusive fullscreen on Windows");
    assert!(!uncomposed_fullscreen(true, false), "Wayland's emulated fullscreen keeps the desktop's deadline");
    assert!(!uncomposed_fullscreen(false, true), "a composed window on Windows");
    assert_eq!(held_base_margin_us(None, uncomposed_fullscreen(true, false), 120.0), 5_000, "Wayland Fullscreen at 120 Hz");
}

#[test]
fn a_hold_before_the_draw_adds_the_draw_time_to_its_margin() {
    let mut draws = DrawTime::default();
    assert_eq!(draws.p90_us(), None);
    assert_eq!(held_margin_us(5_000, draws.p90_us(), 8_333), 5_000, "after the draw: the deadline alone");
    for us in [900, 1_000, 1_100, 1_200, 1_000, 1_000, 1_000, 1_000, 1_000, 4_000] { draws.record(us); }
    assert_eq!(draws.p90_us(), Some(1_200), "one slow draw in ten is above the 90th percentile");
    assert_eq!(held_margin_us(5_000, draws.p90_us(), 8_333), 6_200);
    assert_eq!(held_margin_us(2_000, Some(1_200), 8_333), 3_200, "exclusive fullscreen");
    assert_eq!(held_margin_us(5_000, Some(4_000), 8_333), 6_666, "capped at 4/5 of a refresh");
    assert_eq!(held_margin_us(7_166, Some(9_000), 8_333), 7_166, "never under the deadline");
    for _ in 0..DRAW_TIMES { draws.record(500); }
    assert_eq!(draws.p90_us(), Some(500), "only the last draws count");
    draws.reset();
    assert_eq!(draws.p90_us(), None);
}

#[test]
fn the_phase_lock_aims_at_the_deadline_only_where_a_picture_has_to_make_it() {
    assert_eq!(phase_lock_aim_us(true, false, None, 120.0), 5_000);
    assert_eq!(phase_lock_aim_us(true, false, Some(1_000), 120.0), 1_000, "a measurement run's");
    assert_eq!(phase_lock_aim_us(false, false, None, 120.0), 0, "a present that waits, Linux's among them");
    assert_eq!(phase_lock_aim_us(true, true, None, 120.0), 0, "exclusive fullscreen");
}

/// 120 fps on 60 Hz waiting its turn was the 51 ms queue.
#[test]
fn a_stream_faster_than_the_display_never_waits_for_its_misses() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..4 {
        for _ in 0..2 * MISSED_WAIT { assert_eq!(skipped.missed(8_333, 16_666), None); }
        for _ in 0..MISSED_OF { skipped.shown(); }
    }
    assert_eq!(skipped.hold(), REFUSED_HOLD);
}

/// 17 ms grown at 60 Hz would be a queue at 120 Hz.
#[test]
fn another_display_starts_from_the_first_hold() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    while skipped.skipped(0, 16_666, 16_666).is_none() {}
    assert!(skipped.hold() > REFUSED_HOLD);
    skipped.restart();
    assert_eq!(skipped.hold(), REFUSED_HOLD);
    for _ in 0..REFUSED_OF_64_WIDENS - 1 { assert_eq!(skipped.skipped(0, 8_333, 8_333), None, "what it counted went too"); }
    // The new display settles as a new stream does.
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..MISSED_OF { skipped.shown(); }
    skipped.restart();
    for _ in 0..2 * MISSED_WAIT { assert_eq!(skipped.missed(8_333, 8_333), None); }
    for _ in 0..MISSED_OF { skipped.shown(); }
    for _ in 0..MISSED_WAIT { assert_eq!(skipped.missed(8_333, 8_333), None, "those did not count"); }
}

#[test]
fn a_late_picture_now_and_then_leaves_the_hold_as_it_is() {
    let (frame, refresh) = (8_333, 8_333);
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for n in 0..100_000 {
        if n % 16 == 0 {
            assert_eq!(skipped.skipped(0, frame, refresh), None, "1 in 16, more than the 0.5 to 1% of the runs");
        }
        skipped.shown();
    }
    assert_eq!(skipped.hold(), REFUSED_HOLD);
}

#[test]
fn the_hold_stops_at_a_refresh_and_never_grows_on_a_stream_faster_than_the_display() {
    let mut skipped = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..100_000 {
        skipped.skipped(0, 8_333, 8_333);
    }
    assert_eq!(skipped.hold_us, 8_500, "a hold longer than a refresh always gets its turn");
    let mut faster = SkippedPicture::new(REFUSED_HOLD);
    for _ in 0..100_000 {
        assert_eq!(faster.skipped(0, 8_333, 16_666), None, "120 fps at 60 Hz is refused by design");
    }
}

/// The real swap chain: presents back to back never wait for a refresh, the ones the display
/// has no room for say so, and SDL's next draw still reaches the back buffer. Needs a display
/// that shows the window, which a hidden one is not.
#[cfg(windows)]
#[test]
#[ignore = "requires a Direct3D 11 display, and shows a small window"]
fn a_present_without_waiting_is_refused_while_the_last_picture_waits() {
    let _guard = crate::sdl::TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: This test owns SDL and all its resources on one thread.
    unsafe {
        assert!(SDL_Init(SDL_INIT_VIDEO), "{}", crate::sdl::error());
        SDL_SetHint(SDL_HINT_RENDER_DRIVER, c"direct3d11".as_ptr());
        let window = SDL_CreateWindow(c"Present test".as_ptr(), 128, 64, SDL_WindowFlags(0));
        let renderer = crate::sdl::create_renderer(window, true);
        assert!(!renderer.is_null(), "{}", crate::sdl::error());
        assert!(SDL_SetRenderVSync(renderer, 1));
        let clear = |red: u8| {
            assert!(SDL_SetRenderDrawColor(renderer, red, 0, 0, 255));
            assert!(SDL_RenderClear(renderer));
        };
        // Reading back takes longer than a refresh, so not during the burst.
        let drawn = |red: u8| {
            clear(red);
            let surface = SDL_RenderReadPixels(renderer, std::ptr::null());
            assert!(!surface.is_null(), "{}", crate::sdl::error());
            let (mut r, mut g, mut b, mut a) = (0, 0, 0, 0);
            assert!(SDL_ReadSurfacePixel(surface, 0, 0, &mut r, &mut g, &mut b, &mut a));
            SDL_DestroySurface(surface);
            assert_eq!(r, red, "the draw after a present must reach the back buffer");
        };
        let settle = |renderer| for _ in 0..5 { SDL_PumpEvents(); SDL_RenderPresent(renderer); };
        settle(renderer);
        let (mut shown, mut refused, mut longest) = (0, 0, std::time::Duration::ZERO);
        for n in 0..60u8 {
            clear(n + 1);
            let begun = std::time::Instant::now();
            match present_without_waiting(renderer, std::time::Duration::ZERO, 60.0) {
                NoWait::Shown => shown += 1,
                NoWait::Refused => refused += 1,
                NoWait::Failed | NoWait::PresentFailed => panic!("the present failed"),
            }
            longest = longest.max(begun.elapsed());
            // A burst over several refreshes: some find the last picture gone, most do not.
            SDL_DelayPrecise(1_000_000);
        }
        assert!(shown >= 1, "shown {shown}, refused {refused}");
        assert!(refused >= 1, "a picture a millisecond, the display has no room for all: shown {shown}");
        assert!(longest < std::time::Duration::from_millis(4), "a present waited {longest:?}, shown {shown}, refused {refused}");
        drawn(99);
        // With a hold as long as a refresh it gets its turn.
        let before = shown;
        for _ in 0..3 {
            drawn(200);
            if present_without_waiting(renderer, std::time::Duration::from_millis(100), 60.0) == NoWait::Shown { shown += 1; }
        }
        assert_eq!(shown, before + 3, "a hold longer than a refresh always gets its turn");
        drawn(201);
        SDL_DestroyRenderer(renderer);
        SDL_DestroyWindow(window);
        SDL_Quit();
    }
}

#[test]
fn refused_skipped_picture_keeps_uploaded_flag_across_overdue_retry() {
    let mut pacer = Pacer::new(PacingMode::Vsync, 8_333);
    pacer.accept(super::decoded(1_000));
    let (mut first, dropped) = pacer.take_due(1_000);
    assert_eq!(dropped, 0);
    let mut first = first.take().unwrap();
    assert!(!first.uploaded, "freshly accepted picture needs upload");
    first.uploaded = true;
    let mut kept = Some(first);
    let (retry, replaced) = take_over_kept(None, &mut kept, true);
    assert_eq!(replaced, 0);
    assert!(retry.unwrap().uploaded, "retried skipped_picture stays marked uploaded so upload_take is skipped");
}

#[test]
fn egl_in_order_wait_cap_tracks_two_refreshes_with_safe_bounds() {
    use std::time::Duration;
    assert_eq!(super::super::pacer::no_wait::egl_in_order_wait_cap(90.0), Duration::from_micros(22_222));
    assert_eq!(super::super::pacer::no_wait::egl_in_order_wait_cap(240.0), Duration::from_millis(20));
    assert_eq!(super::super::pacer::no_wait::egl_in_order_wait_cap(15.0), Duration::from_millis(100));
    assert_eq!(super::super::pacer::no_wait::egl_in_order_wait_cap(0.0), Duration::from_millis(40));
}
