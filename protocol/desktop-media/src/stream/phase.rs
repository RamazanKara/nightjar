//! Display vblank phase lock, steady reporting, and fractional V-Sync hold anchors.

use rubylight_protocol::phase_lock::{Report, SlackWindow};
use sdl3_sys::everything::*;

#[path = "phase/linux.rs"]
mod linux;
#[cfg(target_os = "linux")]
pub(crate) use linux::{
    clear_frame_egl_sync, egl_swap_check, egl_swap_presented, egl_sync_sample,
    egl_sync_sample_for_present, glass_phase_timing, note_external_present, timing_with_source,
    TimingSource,
};
#[cfg(not(target_os = "linux"))]
pub(crate) use linux::clear_frame_egl_sync;
pub use linux::reset_display_timing;

/// Windows in a row a new place of the frames takes to reach the host of a stream with no refreshes
/// to spare (`SteadyReports`): 2 s.
pub const STEADY_WINDOWS: u32 = 4;
/// Windows of a new lock that go out as they are: it has to find its place first.
const STEADY_AFTER: u32 = 6;

/// Keeps a passing change of place from the host, on a stream with no refreshes to spare.
///
/// A source faster than the stream takes frames a picture off their place now and then, for a second
/// or two, and they come back by themselves. The host answers a report from there by taking a later
/// picture, and it learns a false drift that moves them later again: they land past the desktop's
/// deadline, where a picture a refresh has none to spare and each miss repeats one and loses one. So a
/// report from a new place repeats the last place until `windows` in a row said the same. Frames that
/// came early only wait a little longer, and a refresh that missed goes through at once.
#[derive(Debug, Default)]
pub struct SteadyReports {
    kept: Option<(i32, u32)>,
    seen: u32,
    away: u32,
    missed: bool,
}

impl SteadyReports {
    /// A refresh repeated a picture that was late for it.
    pub fn missed(&mut self) { self.missed = true; }

    /// What to send for this window's `report`.
    pub fn report(&mut self, report: Report, windows: u32) -> Report {
        let missed = std::mem::take(&mut self.missed);
        self.seen = self.seen.saturating_add(1);
        let period = i64::from(report.period_ns);
        let moved = |kept: i32| (i64::from(report.lead_ns) - i64::from(kept)).rem_euclid(period);
        let held = self.kept.filter(|&(lead, _)| {
            let moved = moved(lead);
            let away = moved.min(period - moved) > period / 5 || i64::from(report.spread_ns) > period / 4;
            away && !missed && self.seen > STEADY_AFTER && self.away + 1 < windows
        });
        if let Some((lead_ns, spread_ns)) = held {
            self.away += 1;
            return Report { lead_ns, spread_ns, ..report };
        }
        self.away = 0;
        self.kept = Some((report.lead_ns, report.spread_ns));
        report
    }
}

#[derive(Default)]
pub struct Phase {
    window: SlackWindow,
    last_report: u64,
    last_frame: u64,
    pub lead_us: Option<i32>,
    steady: SteadyReports,
}

impl Phase {
    /// A refresh repeated a picture that was late for it.
    pub fn missed(&mut self) { self.steady.missed(); }

    /// `steady_windows`: how long a stream with no refreshes to spare keeps a new place from the
    /// host (`SteadyReports`), 0 for any other.
    pub fn sample(&mut self, ready_us: u64, now_us: u64, timing: Option<(i64, i64)>, steady_windows: u32) -> Option<Report> {
        let Some((latch_us, period_ns)) = timing.filter(|(_, period)| (2_000_000..=41_700_000).contains(period)) else {
            self.reset(); return None;
        };
        if now_us.saturating_sub(ready_us) > nightjar_policy::vsync::MAX_QUEUED_FRAME_US { self.reset(); return None; }
        // A place kept before the stream stopped skipping is stale once it skips again.
        if steady_windows == 0 { self.steady = SteadyReports::default(); }
        if self.last_frame == 0 || now_us.saturating_sub(self.last_frame) > 250_000 { self.reset(); self.last_report = now_us; }
        self.last_frame = now_us;
        self.window.push((latch_us - ready_us as i64) * 1000, period_ns);
        let elapsed_us = now_us.saturating_sub(self.last_report);
        if elapsed_us < 500_000 { return None; }
        self.last_report = now_us;
        // The window's length tells the report whether a frame came every refresh.
        let report = self.window.report_over(10, elapsed_us.min(u32::MAX as u64) as i64 * 1000)?;
        self.lead_us = Some(report.lead_ns / 1000);
        Some(if steady_windows > 0 { self.steady.report(report, steady_windows) } else { report })
    }

    pub fn reset(&mut self) { *self = Self::default(); }
}

/// The latch moved `aim_us` ahead of the refresh: slack is reported to that point.
pub fn aimed(timing: Option<(i64, i64)>, aim_us: i64) -> Option<(i64, i64)> {
    timing.map(|(latch_us, period_ns)| (latch_us - aim_us, period_ns))
}

/// How long pictures in a row have to come `EARLY_SPARE_US` and a refresh ahead of the refresh that
/// opens their turn before the turns move a refresh earlier (`HeldAnchor::hold`). Longer than the
/// host's phase lock takes to move them later itself (a report or two), so the two never take turns.
/// See `EARLY_FOR_US`.
pub use nightjar_policy::vsync::{EARLY_FOR_US, EARLY_SPARE_US};

/// How long after the refresh that opens a held picture's turn its present goes out, µs: the
/// picture is offered to the swap chain right after that refresh, so the chain has all but this guard
/// of the refresh to latch the flip into the turn. Aimed at the composed deadline instead (5 ms before
/// the refresh at 120 Hz) every present raced it: a picture that came a little late missed its turn,
/// and the anchor then counted the next one from the refresh it aimed at or from the one the display
/// showed, depending on the statistics, which gave holds of 3 and 1 in turn (3.4% of the frames at
/// 60 fps on 120 Hz). The guard keeps the present out of the scan of the refresh before the turn.
pub const HELD_LATCH_GUARD_US: u64 = 400;

/// The guard a held picture's present goes out after the refresh that opens its turn, µs:
/// `HELD_LATCH_GUARD_US` unless a measurement run sets `NIGHTJAR_HELD_GUARD_US` (read once when the
/// stream starts, `held_guard_from_env`), and a guard past half a refresh is clamped there at use
/// (`held_present_lead_us`, `HeldAnchor::hold`). The whole aim moves with it, so a run can A/B where
/// the present lands in its turn without a rebuild.
pub fn held_guard_us(set: Option<u64>) -> u64 { set.unwrap_or(HELD_LATCH_GUARD_US) }

/// `NIGHTJAR_HELD_GUARD_US` for a measurement run, read once when the stream starts: `held_guard_us`
/// of it, with one line in the log either way. A value that is not a number of microseconds is left
/// out and the default is used, so a typo cannot turn the hold into something else silently.
pub fn held_guard_from_env() -> u64 {
    let Some(set) = std::env::var_os("NIGHTJAR_HELD_GUARD_US") else { return held_guard_us(None); };
    let parsed = set.to_str().and_then(|us| us.parse().ok());
    match parsed {
        Some(us) => eprintln!("nightjar: NIGHTJAR_HELD_GUARD_US: fractional V-Sync's held presents go out {us} us after the refresh that opens their turn"),
        None => eprintln!("nightjar: NIGHTJAR_HELD_GUARD_US: {set:?} is not a number of microseconds; using {HELD_LATCH_GUARD_US} us"),
    }
    held_guard_us(parsed)
}

/// How long before the refresh that shows a held picture the picture must be ready, µs: its present
/// goes out `guard_us` after the refresh that opens the turn, and the report shifts its latch back
/// by this (`aimed`), so the host delivers the picture before the hold. It is the hold's own target
/// (`HeldAnchor::hold`): a steering lock delivers before it, the hold waits it out, and the present
/// goes out just after the opening refresh. Aimed at the desktop's deadline instead, the host
/// delivered after the present target and the hold never waited. A guard past half the refresh is
/// clamped to half, so the swap chain keeps about half a refresh to latch the flip and the aim never
/// reaches the turn.
pub fn held_present_lead_us(period_us: u64, guard_us: u64) -> u64 {
    period_us.saturating_sub(guard_us.min(period_us / 2))
}

/// Which refresh the last picture of a held stream was shown at (fractional V-Sync), and where the
/// next one's turn is. The swap chain's statistics name the last present shown, which may be a
/// redraw without a picture (the menu, the overlay, Frame Warp's shift): its turn would put the next
/// picture a hold behind it. So the picture's own present (`presented`) is watched until the
/// statistics show it, and until then its turn counts from the refresh its hold aimed it at, plus the
/// refreshes the display showed the last pictures after their aim (`late`).
#[derive(Debug, Default)]
pub struct HeldAnchor {
    picture: u32,
    shown_at: Option<u32>,
    /// The refresh the last picture's hold aimed it at, and the next picture's, until its present.
    aimed: Option<u32>,
    pending: Option<u32>,
    /// Refreshes the display showed the last picture after the refresh its hold aimed it at, 0 or 1
    /// (the composed pipeline's own refresh; a bigger difference is a stall, which moves the grid by
    /// at most this one refresh). A display whose composition lands a refresh later than aimed would
    /// otherwise count one picture's turn from the aim and the next one's from the display, and every
    /// other hold would land a refresh off: holds of 3 and 1 in turn, the residual 3% at 60 fps.
    late: u32,
    /// Since when pictures came a refresh early for their turn, in the clock's ticks.
    early_since: Option<u64>,
}

impl HeldAnchor {
    /// A picture went out as the swap chain's present `count`.
    pub fn presented(&mut self, count: u32) {
        (self.picture, self.shown_at, self.aimed) = (count, None, self.pending.take());
    }

    /// Another refresh rate or stream rate (`Streaming::sync_vrr`): the turns count on a new grid,
    /// and a display whose mode changed may restart its refresh count, so the aims and lateness of
    /// the old one say nothing about the turns now. The next picture counts from the first one shown
    /// after the change.
    pub fn restart(&mut self) { *self = Self::default(); }

    /// A new rate (`refreshes`, `VrrPacing::refreshes` after `Streaming::sync_vrr` read it) against
    /// the one the turns count on (`before`) starts them over when the two differ: the old grid's
    /// aim and lateness say nothing about the new one, and a display mode may restart its refresh
    /// count. The same rate keeps them, which is every present of a stream that did not change.
    /// Returns whether the turns started over.
    pub fn restart_on_rate_change(&mut self, before: u32, refreshes: u32) -> bool {
        if refreshes == before { return false; }
        self.restart();
        true
    }

    /// The refresh the last picture shows at, from the last present shown (`present_count` at
    /// `present_refresh`) and the latest refresh, which the statistics cover unless `stale` (their
    /// vblank time is a refresh or more in the past): the one it was shown at, else the refresh its
    /// hold aimed it at plus the lateness the display showed (`late`), while it waits or once a
    /// redraw passed it unseen, else the latest refresh it may have reached, and None when it is not
    /// known (no picture yet, or an unaimed one a redraw passed unseen).
    #[cfg_attr(not(windows), allow(dead_code))] // Only Windows counts refreshes.
    pub fn last_refresh(&mut self, present_count: u32, present_refresh: u32, sync_refresh: u32, stale: bool) -> Option<u32> {
        if self.picture == 0 || present_refresh == 0 { return None; }
        if present_count == self.picture {
            self.shown_at = Some(present_refresh);
            self.late = self.aimed.map_or(0, |aimed| (present_refresh.wrapping_sub(aimed) as i32).clamp(0, 1) as u32);
        }
        if self.shown_at.is_some() { return self.shown_at; }
        let aimed = self.aimed.map(|aimed| aimed.wrapping_add(self.late));
        let waiting = (self.picture.wrapping_sub(present_count) as i32) > 0;
        if !waiting { return aimed; }
        // Still waiting: the refresh its aim names, plus the lateness the display showed, and past
        // that the latest refresh it may have reached; a picture without an aim shows at the next.
        // The aim counts while it is still ahead, and the latest refresh only where the statistics
        // are a refresh stale, so the picture may have been shown at one they do not cover. The
        // latest refresh they do cover is not a picture's: a present they leave out of
        // `present_count` was not shown at it, so a missed turn counts from the next refresh the
        // statistics cover, or from the latest one they name when they are stale, never from the one
        // it missed (that made the next turn a refresh early and the one after it a refresh short,
        // once a miss; holds of 3 and 1 in turn, the residual 3% at 60 fps).
        let next = sync_refresh.wrapping_add(1);
        Some(aimed.map_or(next, |aimed| {
            if (aimed.wrapping_sub(sync_refresh) as i32) > 0 { aimed } else if stale { sync_refresh } else { next }
        }))
    }

    /// Fractional V-Sync: how long a picture waits before its present, which then goes out at the
    /// next refresh, so it is shown at its turn: `held` refreshes after the refresh the last picture
    /// shows at (`last`, `last_refresh`). The present goes out `guard` into the refresh that opens
    /// the turn, so the swap chain has all but the guard of the refresh to latch the flip; a guard
    /// past half the refresh is clamped to half, so it keeps about half a refresh to latch it.
    /// `held_present_lead_us`
    /// (the same `period` and `guard`) is the target the phase lock aims the host's frames at, so a
    /// steering lock delivers the picture before its hold. `margin` is the desktop's deadline before
    /// a refresh, which only says which turn a picture that comes late takes (the first whose
    /// deadline it still makes, at most `held` refreshes and one more behind a last picture still
    /// waiting). Nothing for a picture later than that, which the next ones then count from, or with
    /// no hold or no known last picture. A swap chain's own sync interval holds the picture too, but
    /// a picture early for its turn waits in the swap chain's queue, where each one after it waits as
    /// well and the phase report, which counts within the hold, never sees it: 60 fps on 120 Hz went
    /// from 9 to 36 ms that way.
    ///
    /// Pictures that come `early_spare` and a refresh ahead of their turn's open for `early_for` in a
    /// row move the turns a refresh earlier, once: a source whose clock runs ahead of the display brings
    /// them earlier and earlier, each waiting longer, up to the whole hold, until a host's phase lock
    /// moves them later, and a host without one never does (0 and 1 show the move only).
    ///
    /// Ticks of the clock of `now` and `sync_ticks`, the latest refresh `sync_refresh`'s time, and of
    /// `period`, `margin`, `guard`, `early_spare` and `early_for`. The refresh it aims the picture at
    /// goes to the next `presented`.
    #[allow(clippy::too_many_arguments)]
    pub fn hold(&mut self, now: u64, sync_ticks: u64, sync_refresh: u32, last: Option<u32>, held: u32, period: u64, margin: u64,
        guard: u64, early_spare: u64, early_for: u64) -> u64 {
        let Some(last) = last.filter(|_| held > 1 && period > 0) else {
            (self.pending, self.early_since) = (None, None);
            return 0;
        };
        let (period, margin) = (period.min(i64::MAX as u64) as i64, margin.min(i64::MAX as u64) as i64);
        // A guard past half the refresh would leave the swap chain less than half a refresh to latch
        // the flip: clamp it, as the report's aim does.
        let guard = guard.min((period / 2) as u64) as i64;
        let mut ahead = i64::from(last.wrapping_add(held).wrapping_sub(sync_refresh) as i32);
        // The present goes out just after the refresh before the turn: the swap chain latches the
        // flip into the turn with all but the guard of a refresh to spare. The phase lock aims at the
        // same point (`held_present_lead_us`), so the picture is ready before this wait starts.
        let lead = held_present_lead_us(period as u64, guard as u64) as i64;
        let wait = |ahead: i64| sync_ticks as i64 + ahead * period - lead - now as i64;
        // A refresh earlier is still after the last picture's: `held` is 2 or more.
        if wait(ahead - 1) >= early_spare.min(i64::MAX as u64) as i64 {
            let since = *self.early_since.get_or_insert(now);
            if now.saturating_sub(since) >= early_for {
                ahead -= 1;
                self.early_since = None;
            }
        } else {
            self.early_since = None;
        }
        // The refresh it shows at: its turn, or the first whose deadline it still makes.
        let first = (now as i64 + margin - sync_ticks as i64 + period - 1).div_euclid(period);
        self.pending = Some(sync_refresh.wrapping_add(ahead.max(first) as u32));
        wait(ahead).clamp(0, i64::from(held + 1) * period) as u64
    }
}

/// The refresh a picture held for `held` refreshes is due at, and that many refreshes as the
/// period (fractional V-Sync): its turns count from the refresh the last picture was shown at
/// (`last`, `HeldAnchor::last_refresh`), so the latch is the latest refresh of that grid,
/// `sync_refresh` being the latest refresh at `sync_ticks`. The refresh itself, and its period, for
/// `held` 1 or no known refresh of the last picture. Ticks and periods in the clock's own units.
#[cfg_attr(not(windows), allow(dead_code))] // Only Windows counts refreshes.
pub fn held_latch(sync_ticks: u64, sync_refresh: u32, last: Option<u32>, held: u32, period: u64) -> (u64, u64) {
    let Some(last) = last.filter(|_| held > 1) else { return (sync_ticks, period) };
    // Signed: the last picture may still wait for a refresh after the latest one (`last_refresh`),
    // and a remainder of the unsigned difference is right only where `held` divides 2^32.
    let since = i64::from(sync_refresh.wrapping_sub(last) as i32).rem_euclid(i64::from(held)) as u64;
    (sync_ticks.saturating_sub(since * period), period * u64::from(held))
}

/// Fractional V-Sync where the display tells when a picture reached it as a time rather than a
/// refresh count: Wayland's presentation feedback, or a KMS commit, which returns at the refresh
/// that shows it. The turns are `HeldAnchor::hold`'s, counted in refreshes of the display's period
/// from the last picture's time (`shown`), or from the time its hold aimed it at until that comes.
/// Times on the session clock, µs.
#[derive(Debug, Default)]
pub struct HeldClock {
    picture: Option<u32>,
    /// The picture before `picture`, whose feedback may still come after `picture` went out.
    before: Option<u32>,
    shown_us: Option<u64>,
    aimed_us: Option<u64>,
    pending_us: Option<u64>,
    /// The last hold's refreshes and refresh period, µs.
    held: u32,
    period_us: u64,
    turns: HeldAnchor,
}

impl HeldClock {
    /// The picture of stream frame `frame` went out.
    pub fn presented(&mut self, frame: u32) {
        self.before = self.picture;
        (self.picture, self.shown_us, self.aimed_us) = (Some(frame), None, self.pending_us.take());
    }

    /// Frame `frame` reached the display at `at_us`. The last picture's time is where the next turn
    /// counts from. The feedback of the picture before it may come only after the last one went out
    /// (the compositor answers at the refresh, the poll runs after a present): with nothing else
    /// known of the last picture, it shows a hold after that one.
    pub fn shown(&mut self, frame: u32, at_us: u64) {
        if self.picture == Some(frame) {
            self.shown_us = Some(at_us);
        } else if self.before == Some(frame) && self.shown_us.is_none() && self.aimed_us.is_none() && self.held > 1 {
            self.aimed_us = Some(at_us + u64::from(self.held) * self.period_us);
        }
    }

    fn last_us(&self) -> Option<u64> { self.shown_us.or(self.aimed_us) }

    /// `HeldAnchor::hold` for the next picture, `period_us` a refresh: how long it waits before its
    /// present, `margin_us` the desktop's deadline and `guard_us` (`held_guard_us`, µs) how long after
    /// the refresh that opens its turn the present goes out, whose lead the phase report aims at
    /// (`held_present_lead_us`).
    pub fn hold(&mut self, now_us: u64, held: u32, period_us: u64, margin_us: u64, guard_us: u64) -> u64 {
        (self.held, self.period_us) = (held, period_us);
        let Some(last) = self.last_us().filter(|_| held > 1 && period_us > 0) else {
            self.pending_us = None;
            return self.turns.hold(now_us, 0, 0, None, held, period_us, margin_us, guard_us, EARLY_SPARE_US, EARLY_FOR_US);
        };
        let wait = self.turns.hold(now_us, last, 0, Some(0), held, period_us, margin_us, guard_us, EARLY_SPARE_US, EARLY_FOR_US);
        self.pending_us = self.turns.pending.take().map(|refresh| last + u64::from(refresh) * period_us);
        wait
    }

    /// The phase report's latch and period for a held stream (`timing`'s): the latest refresh of the
    /// grid the turns count on, at or before `now_us`, and `held` refreshes. None without a hold or a
    /// last picture, or with one long gone.
    pub fn timing(&self, now_us: u64, held: u32, period_us: u64) -> Option<(i64, i64)> {
        let last = self.last_us().filter(|_| held > 1 && period_us > 0)?;
        let grid = u64::from(held) * period_us;
        let latch = if now_us >= last { last + (now_us - last) / grid * grid } else { last.checked_sub((last - now_us).div_ceil(grid) * grid)? };
        (now_us - latch <= 100_000 + grid).then_some((latch as i64, (grid * 1000) as i64))
    }
}

/// Whether the display tells when it showed a picture, which fractional V-Sync's turns count from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldTimes {
    /// It does: pictures are held for their turns.
    Told,
    /// Not known yet (Wayland's feedback before the first answer was due): nothing held, nothing said.
    Unknown,
    /// It does not (X11, a compositor without timed feedback, swap chain statistics failing): each
    /// picture shows at the next refresh, and the log says why once.
    Untold,
}

/// How long a stream's first feedback request may wait for a timed answer before the display
/// counts as giving none (`glass_times`), µs.
pub const FEEDBACK_DUE_US: u64 = 1_000_000;

/// `HeldTimes` of Wayland's presentation feedback: `offered` None before the first request,
/// Some(false) where it is not offered or carries no times, else timed answers since
/// `last_timed_us`, the first request at `first_request_us`. Times on the session clock, µs.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // Only Linux has the feedback.
pub fn glass_times(offered: Option<bool>, first_request_us: Option<u64>, last_timed_us: Option<u64>, now_us: u64) -> HeldTimes {
    match offered {
        None => HeldTimes::Unknown,
        Some(false) => HeldTimes::Untold,
        Some(true) if last_timed_us.is_some_and(|at| now_us.saturating_sub(at) < FEEDBACK_DUE_US) => HeldTimes::Told,
        // Answers that stopped are as overdue as a first one that never came.
        Some(true) if now_us.saturating_sub(last_timed_us.or(first_request_us).unwrap_or(now_us)) < FEEDBACK_DUE_US => HeldTimes::Unknown,
        Some(true) => HeldTimes::Untold,
    }
}

/// How long a swap chain whose statistics failed holds nothing before it is tried again
/// (`StatsWatch`), µs.
pub const STATS_RETRY_US: u64 = 2_000_000;

/// Whether the swap chain's statistics, which fractional V-Sync's turns count from, can be read. A
/// single failure passes (the first read after a mode change may fail); two in a row hold nothing
/// for `STATS_RETRY_US`, then the next held picture tries again.
#[derive(Debug, Default)]
pub struct StatsWatch {
    fails: u32,
    since_us: Option<u64>,
}

impl StatsWatch {
    /// A held picture read the statistics (`ok`) or could not.
    pub fn read(&mut self, ok: bool, now_us: u64) {
        if ok { *self = Self::default(); return; }
        self.fails = self.fails.saturating_add(1);
        // A failed try after the wait starts a new one.
        if self.since_us.is_none_or(|since| now_us.saturating_sub(since) >= STATS_RETRY_US) { self.since_us = Some(now_us); }
    }

    #[cfg_attr(not(windows), allow(dead_code))] // Only Windows reads the statistics.
    pub fn times(&self, now_us: u64) -> HeldTimes {
        match self.since_us {
            Some(since) if self.fails >= 2 && now_us.saturating_sub(since) < STATS_RETRY_US => HeldTimes::Untold,
            _ => HeldTimes::Told,
        }
    }
}

/// Whether a swap chain's statistics are a refresh stale: the vblank they name (`SyncQPCTime`, QPC)
/// is `period` or more in the past, so a later refresh may have passed that they do not cover
/// (`HeldAnchor::last_refresh`). Callers check the time is positive.
#[cfg(windows)]
fn stats_stale(sync_qpc_time: i64, period: u64) -> bool {
    // SAFETY: a plain query of the clock.
    let now = unsafe { SDL_GetPerformanceCounter() };
    now.saturating_sub(sync_qpc_time as u64) > period
}

/// `HeldAnchor::hold` for the renderer's swap chain, in microseconds; None when its statistics
/// cannot be read (`StatsWatch`). `guard_us` is `held_guard_us`.
#[cfg(windows)]
pub fn held_wait_us(renderer: *mut SDL_Renderer, refresh_hz: f32, held: u32, margin_us: u64, guard_us: u64, anchor: &mut HeldAnchor) -> Option<u64> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{IDXGISwapChain, DXGI_FRAME_STATISTICS};
    if held <= 1 || refresh_hz <= 0.0 { return Some(0); }
    // SAFETY: the chain pointer comes from the live renderer and is only borrowed for these calls.
    unsafe {
        let chain = crate::render_device::live_renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_SWAPCHAIN_POINTER);
        let chain = IDXGISwapChain::from_raw_borrowed(&chain)?;
        let mut stats = DXGI_FRAME_STATISTICS::default();
        if chain.GetFrameStatistics(&mut stats).is_err() || stats.SyncQPCTime <= 0 { return None; }
        let freq = SDL_GetPerformanceFrequency() as f64;
        let ticks = |us: f64| (us * freq / 1e6) as u64;
        let period = ticks(1e6 / f64::from(refresh_hz));
        let last = anchor.last_refresh(stats.PresentCount, stats.PresentRefreshCount, stats.SyncRefreshCount,
            stats_stale(stats.SyncQPCTime, period));
        let wait = anchor.hold(SDL_GetPerformanceCounter(), stats.SyncQPCTime as u64, stats.SyncRefreshCount, last,
            held, period, ticks(margin_us as f64), ticks(guard_us as f64),
            ticks(EARLY_SPARE_US as f64), ticks(EARLY_FOR_US as f64));
        Some((wait as f64 * 1e6 / freq) as u64)
    }
}

/// The swap chain's count of the last present, for `HeldAnchor::presented`; 0 without one.
#[cfg(windows)]
pub fn last_present_count(renderer: *mut SDL_Renderer) -> u32 {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::IDXGISwapChain;
    // SAFETY: the chain pointer comes from the live renderer and is only borrowed for this call.
    unsafe {
        let chain = crate::render_device::live_renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_SWAPCHAIN_POINTER);
        IDXGISwapChain::from_raw_borrowed(&chain).and_then(|chain| chain.GetLastPresentCount().ok()).unwrap_or(0)
    }
}

/// Only Windows has the count.
#[cfg(not(windows))]
pub fn last_present_count(_: *mut SDL_Renderer) -> u32 { 0 }

/// Only Windows holds pictures for their turn.
#[cfg(not(windows))]
pub fn held_wait_us(_: *mut SDL_Renderer, _: f32, _: u32, _: u64, _: u64, _: &mut HeldAnchor) -> Option<u64> { Some(0) }

#[cfg(any(windows, test))]
fn latch_time_us(now_us: u64, counter: u64, latch: u64, freq: u64, period_ns: i64) -> Option<(i64, i64)> {
    if freq == 0 { return None; }
    let delta_ticks = counter as i128 - latch as i128;
    let age_us = delta_ticks as f64 * 1_000_000.0 / freq as f64;
    if age_us.abs() > 100_000.0 { return None; }
    Some((now_us as i64 - age_us as i64, period_ns))
}

#[cfg(windows)]
pub fn timing(renderer: *mut SDL_Renderer, refresh_hz: f32, held: u32, anchor: &mut HeldAnchor) -> Option<(i64, i64)> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{IDXGISwapChain, DXGI_FRAME_STATISTICS};
    use windows::Win32::Graphics::Dwm::{DwmGetCompositionTimingInfo, DWM_TIMING_INFO};
    unsafe {
        let period_ns = (1_000_000_000.0 / f64::from(refresh_hz)) as i64;
        let chain = crate::render_device::live_renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_SWAPCHAIN_POINTER);
        let stats = IDXGISwapChain::from_raw_borrowed(&chain).and_then(|chain| {
            let mut stats = DXGI_FRAME_STATISTICS::default();
            chain.GetFrameStatistics(&mut stats).ok().map(|_| stats)
        });
        let held_stats = stats.filter(|s| s.SyncQPCTime > 0).map(|s| {
            let ticks_per_refresh = (period_ns as f64 * SDL_GetPerformanceFrequency() as f64 / 1e9) as u64;
            let stale = stats_stale(s.SyncQPCTime, ticks_per_refresh);
            let last = if held > 1 { anchor.last_refresh(s.PresentCount, s.PresentRefreshCount, s.SyncRefreshCount, stale) } else { None };
            let (latch, _) = held_latch(s.SyncQPCTime as u64, s.SyncRefreshCount, last, held, ticks_per_refresh);
            let (_, period) = held_latch(0, s.SyncRefreshCount, last, held, period_ns as u64);
            (latch, period as i64)
        });
        // DWM's clock knows no refresh of the last picture, so it keeps the refresh itself.
        let (latch, period_ns) = if let Some(held) = held_stats { held } else {
            let window = SDL_GetRenderWindow(renderer);
            // DWM exposes the primary compositor's clock, not the phase of another monitor.
            if SDL_GetDisplayForWindow(window) != SDL_GetPrimaryDisplay() { return None; }
            let mut info = DWM_TIMING_INFO { cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32, ..Default::default() };
            DwmGetCompositionTimingInfo(windows::Win32::Foundation::HWND::default(), &mut info).ok()?;
            let measured = (info.qpcRefreshPeriod as f64 * 1_000_000_000.0 / SDL_GetPerformanceFrequency() as f64) as i64;
            if (measured - period_ns).abs() > period_ns / 100 { return None; }
            (info.qpcVBlank, measured)
        };
        latch_time_us(nightjar_core::session::now_us(), SDL_GetPerformanceCounter(), latch, SDL_GetPerformanceFrequency(), period_ns)
    }
}

/// The swap chain's counters and last vblank time right after a present call, for the timing CSV
/// (`timing_csv::Flip`). Empty when the renderer has no DXGI swap chain.
#[cfg(windows)]
pub fn present_stats(renderer: *mut SDL_Renderer) -> crate::timing_csv::Flip {
    use windows::core::Interface;
    use windows::Win32::Graphics::Dxgi::{IDXGISwapChain, DXGI_FRAME_STATISTICS};
    // SAFETY: the chain pointer comes from the live renderer and is only borrowed for these calls.
    unsafe {
        let chain = crate::render_device::live_renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_SWAPCHAIN_POINTER);
        let Some(chain) = IDXGISwapChain::from_raw_borrowed(&chain) else { return Default::default() };
        let mut flip = crate::timing_csv::Flip { present_count: chain.GetLastPresentCount().unwrap_or(0), ..Default::default() };
        let mut stats = DXGI_FRAME_STATISTICS::default();
        if chain.GetFrameStatistics(&mut stats).is_ok() {
            flip.stat_present_count = stats.PresentCount;
            flip.stat_present_refresh = stats.PresentRefreshCount;
            flip.stat_sync_refresh = stats.SyncRefreshCount;
            let age = SDL_GetPerformanceCounter().checked_sub(stats.SyncQPCTime as u64).filter(|_| stats.SyncQPCTime > 0);
            if let Some(age) = age {
                let age_us = (age as f64 * 1_000_000.0 / SDL_GetPerformanceFrequency() as f64) as u64;
                flip.stat_sync_us = nightjar_core::session::now_us().saturating_sub(age_us);
            }
        }
        flip
    }
}

#[cfg(target_os = "linux")]
pub fn present_stats(renderer: *mut SDL_Renderer) -> crate::timing_csv::Flip {
    linux::present_stats(renderer)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn present_stats(_: *mut SDL_Renderer) -> crate::timing_csv::Flip { Default::default() }

#[cfg(target_os = "linux")]
pub fn timing(renderer: *mut SDL_Renderer, refresh_hz: f32, _held: u32, _anchor: &mut HeldAnchor) -> Option<(i64, i64)> {
    linux::timing_with_source(renderer, refresh_hz).map(|(timing, _)| timing)
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn timing(_: *mut SDL_Renderer, _: f32, _: u32, _: &mut HeldAnchor) -> Option<(i64, i64)> {
    None
}

#[cfg(test)]
#[path = "phase/tests.rs"]
mod tests;
