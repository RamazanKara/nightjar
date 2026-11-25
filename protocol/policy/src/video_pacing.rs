//! Video frame pacing, Balanced vsync cadence, refresh-slot planning (`RefreshSlots`), and
//! display frame-rate / ARR / window-mode policies (`DisplayFrameRatePolicy`), shared with
//! Android via `NightjarPolicyJni` (`client/protocol/android/src/policy_video_pacing.rs`).

use crate::vsync;
pub use crate::vsync::{first_vsync_ns, refreshes_per_frame, report_refreshes};

pub const MAX_OUTPUT_DRAIN: i32 = 64;
pub const PYROWAVE_HOST_TIMES: i32 = 4;
pub const LIMIT_ABOVE_SCREEN: i32 = -1;
pub const LIMIT_BOGUS_SCREEN: i32 = -2;
pub const REFRESH_SLOT_RATE_TOLERANCE_PERCENT: i64 = 2;
pub const MIN_FPS_MILLI_HZ: i32 = 1_000;
pub const MAX_FPS_MILLI_HZ: i32 = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum FramePacingMode {
    MinLatency = 0,
    Balanced = 1,
    CapFps = 2,
    MaxSmoothness = 3,
}

impl FramePacingMode {
    pub fn from_ordinal(n: i32) -> Option<Self> {
        match n {
            0 => Some(Self::MinLatency),
            1 => Some(Self::Balanced),
            2 => Some(Self::CapFps),
            3 => Some(Self::MaxSmoothness),
            _ => None,
        }
    }
    pub fn forces_vsync(self) -> bool {
        !matches!(self, Self::MinLatency)
    }
}

/// Java `Math.round(float)`: exact round-half-toward-`+inf`, `NaN -> 0`, saturating to `i32`.
pub fn java_round_f32(x: f32) -> i32 {
    if x.is_nan() {
        0
    } else if x.abs() >= 8_388_608.0 {
        x as i32
    } else {
        (f64::from(x) + 0.5_f64).floor() as i32
    }
}
pub fn pyro_wave_mode(req: i32, vrr: bool, host_times: bool, arr: bool) -> i32 {
    if !vrr {
        req
    } else if host_times && arr {
        PYROWAVE_HOST_TIMES
    } else {
        0
    }
}
pub fn uses_pyro_wave_display_clock(req: i32, vrr: bool, local: bool) -> bool {
    !vrr && local && FramePacingMode::from_ordinal(req).is_some_and(FramePacingMode::forces_vsync)
}
pub fn drop_queued_frames(mode: i32) -> bool {
    !matches!(
        FramePacingMode::from_ordinal(mode),
        Some(FramePacingMode::MaxSmoothness | FramePacingMode::CapFps)
    )
}
pub fn is_frame_due(due_ns: i64, vsync_ns: i64, iv_ns: i64) -> bool {
    i128::from(vsync_ns) + i128::from(iv_ns / 2) >= i128::from(due_ns)
}
pub fn frame_interval_ns(fps_mhz: i32) -> Option<i64> {
    (MIN_FPS_MILLI_HZ..=MAX_FPS_MILLI_HZ)
        .contains(&fps_mhz)
        .then(|| 1_000_000_000_000_i64 / i64::from(fps_mhz))
}
pub fn balanced_paces_on_vsync(lock: bool, drop_late: bool, mhz: i32, iv: i64) -> Option<bool> {
    if !lock || !drop_late || iv <= 0 || iv > 1_000_000_000 {
        return Some(true);
    }
    Some(i128::from(frame_interval_ns(mhz)?) * 20 > i128::from(iv) * 21)
}
pub fn next_frame_time_ns(due_ns: i64, vsync_ns: i64, fps: i32) -> Option<i64> {
    next_frame_time_ns_milli_hz(due_ns, vsync_ns, fps.saturating_mul(1_000))
}
pub fn next_frame_time_ns_milli_hz(due_ns: i64, vsync_ns: i64, fps_mhz: i32) -> Option<i64> {
    let iv = frame_interval_ns(fps_mhz)?;
    if due_ns == 0 {
        return Some(vsync_ns.saturating_add(iv));
    }
    // Truncating division toward zero matches Java `/`.
    let step = (i128::from(vsync_ns) - i128::from(due_ns)) / i128::from(iv) + 1;
    Some(
        (i128::from(due_ns) + step * i128::from(iv))
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64,
    )
}
pub fn limit_refresh_hz(ask: f32, start: f32, fps: i32, sav: bool, hot: bool, peak: f32) -> f32 {
    let rest = java_round_f32(start) == fps
        && refreshes_per_frame(f64::from(ask), f64::from(start), false, true) > 1;
    if rest && !sav && !hot && !(peak > 0.0 && ask > peak + 0.5_f32) {
        ask
    } else {
        start
    }
}
pub fn limited_frame_rate(fps: i32, hz: f32) -> i32 {
    let scr = java_round_f32(hz);
    if fps < scr {
        fps
    } else if fps > scr.saturating_add(3) {
        LIMIT_ABOVE_SCREEN
    } else if scr <= 49 {
        LIMIT_BOGUS_SCREEN
    } else {
        scr - 1
    }
}
pub fn negotiated_frame_rate_milli_hz(req_mhz: i32, neg_fps: i32) -> i32 {
    if (MIN_FPS_MILLI_HZ..=MAX_FPS_MILLI_HZ).contains(&req_mhz)
        && (req_mhz + 500) / 1_000 == neg_fps
    {
        req_mhz
    } else {
        neg_fps.saturating_mul(1_000)
    }
}
pub fn schedules_frames(host_times: bool, pacing: i32) -> bool {
    host_times && !FramePacingMode::from_ordinal(pacing).is_some_and(FramePacingMode::forces_vsync)
}
pub fn recovers_decoder_debt(host_times: bool, pacing: i32) -> bool {
    !schedules_frames(host_times, pacing) && !drop_queued_frames(pacing)
}

pub fn should_drain_output(m: i32, enq: i64, now: i64, mhz: i32, drained: i32) -> Option<bool> {
    if drained >= MAX_OUTPUT_DRAIN {
        return Some(false);
    }
    if drop_queued_frames(m) {
        return Some(true);
    }
    let max_age = frame_interval_ns(mhz)?.min(vsync::MAX_QUEUED_FRAME_NS as i64);
    Some(enq > 0 && now > enq && now - enq > max_age)
}

pub fn has_output_drain_time_remaining(elapsed_ns: i64, fps_mhz: i32) -> Option<bool> {
    if elapsed_ns < 0 {
        Some(false)
    } else {
        Some(elapsed_ns < frame_interval_ns(fps_mhz)?)
    }
}

pub fn apply_output_pacing_fallback(pyro: bool, force: bool, vsync: bool, pacing: i32) -> bool {
    !pyro && !force && !vsync && pacing == FramePacingMode::Balanced as i32
}

pub fn refresh_slots_applies(frame_iv: i64, refresh_iv: i64) -> bool {
    frame_iv > 0
        && refresh_iv > 0
        && (i128::from(frame_iv) - i128::from(refresh_iv)).abs() * 100
            <= i128::from(refresh_iv) * i128::from(REFRESH_SLOT_RATE_TOLERANCE_PERCENT)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshSlot {
    pub vsync_ns: i64,
    pub target_ns: i64,
    pub shared: bool,
    pub hold_ns: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshSlots {
    last_vsync_ns: i64,
    interval_ns: i64,
    planned: i64,
    shared: i64,
}

impl RefreshSlots {
    pub fn new() -> Self {
        Self::default()
    }
    /// Forgets the grid on a new mode or stream; keeps cumulative `planned`/`shared` counters.
    pub fn reset(&mut self) {
        self.last_vsync_ns = 0;
        self.interval_ns = 0;
    }

    fn next_claimed_vsync(&self, vsync_ns: i64, iv: i64) -> i64 {
        let (iv128, v128) = (i128::from(iv), i128::from(vsync_ns));
        let off = i128::from(self.last_vsync_ns) + iv128 - v128 + iv128 / 2;
        (v128 + off.div_euclid(iv128) * iv128).clamp(i128::from(i64::MIN), i128::from(i64::MAX))
            as i64
    }

    pub fn earliest_claimed(&self, release: i64, vsync_ns: i64, iv: i64, lead: i64) -> bool {
        if vsync_ns <= 0 || iv <= 0 || self.last_vsync_ns == 0 || iv != self.interval_ns {
            return false;
        }
        let earliest = first_vsync_ns(release, vsync_ns, iv, lead);
        let next = self.next_claimed_vsync(vsync_ns, iv);
        next.saturating_sub(iv) >= earliest
    }

    pub fn plan(&mut self, release: i64, vsync_ns: i64, iv: i64, lead: i64) -> RefreshSlot {
        if vsync_ns <= 0 || iv <= 0 {
            self.reset();
            return RefreshSlot::default();
        }
        if iv != self.interval_ns {
            self.last_vsync_ns = 0;
            self.interval_ns = iv;
        }
        let earliest = first_vsync_ns(release, vsync_ns, iv, lead);
        let (claimed, vsync) = if self.last_vsync_ns == 0 {
            (0_i64, earliest)
        } else {
            let next = self.next_claimed_vsync(vsync_ns, iv);
            (next.saturating_sub(iv), earliest.max(next))
        };
        if self.last_vsync_ns != 0 && i128::from(vsync) - i128::from(earliest) >= 2 * i128::from(iv)
        {
            self.last_vsync_ns = claimed;
            self.shared += 1;
            return RefreshSlot {
                vsync_ns: claimed,
                target_ns: claimed - iv / 2,
                shared: true,
                hold_ns: claimed.saturating_sub(earliest).max(0),
            };
        }
        self.last_vsync_ns = vsync;
        self.planned += 1;
        RefreshSlot {
            vsync_ns: vsync,
            target_ns: vsync - iv / 2,
            shared: false,
            hold_ns: vsync.saturating_sub(earliest).max(0),
        }
    }

    pub fn planned(&self) -> i64 {
        self.planned
    }
    pub fn shared(&self) -> i64 {
        self.shared
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ArrState {
    NotRequested = 0,
    InUse = 1,
    AndroidTooOld = 2,
    PanelWithoutArr = 3,
}

pub fn use_adaptive_refresh_rate(sdk: i32, vrr: bool, arr: bool) -> bool {
    vrr && sdk >= 36 && arr
}
pub fn adaptive_in_use(arr: bool, vrr: bool, fixed_timing: bool) -> bool {
    arr && (vrr || !fixed_timing)
}

pub fn arr_state(sdk: i32, vrr: bool, arr: bool) -> ArrState {
    if !vrr {
        ArrState::NotRequested
    } else if use_adaptive_refresh_rate(sdk, true, arr) {
        ArrState::InUse
    } else if sdk < 36 {
        ArrState::AndroidTooOld
    } else {
        ArrState::PanelWithoutArr
    }
}

pub fn tracks_content_rate(en: bool, reduce: bool, vrr: bool, tv: bool) -> bool {
    !tv && (vrr || (en && reduce))
}
pub fn follows_content_rate(
    sdk: i32,
    en: bool,
    sync: bool,
    arr: bool,
    vrr: bool,
    tv: bool,
) -> bool {
    (en || vrr) && sync && sdk >= 30 && !arr && !tv
}
fn rate_tol(hz: f32) -> f32 {
    0.5_f32.max(0.01_f32 * hz)
}

pub fn content_vote(content: f32, base: f32, reduce: bool, max_hz: f32) -> f32 {
    let v = if reduce { content } else { content.max(base) };
    if max_hz > 0.0 { v.min(max_hz) } else { v }
}

pub fn vrr_content_vote(content: f32, max_hz: f32, reduce: bool, seamless: &[f32]) -> f32 {
    if reduce {
        return content_vote(content, max_hz, true, max_hz);
    }
    if content <= 0.0 || max_hz <= 0.0 {
        return max_hz;
    }
    let tol = rate_tol(max_hz);
    let mult = ((max_hz + tol) / content).floor() as i32;
    let vote = content * (mult as f32);
    if mult < 1 || vote >= max_hz - tol {
        return max_hz;
    }
    let vtol = rate_tol(vote);
    seamless
        .iter()
        .copied()
        .find(|&r| (r - vote).abs() <= vtol)
        .unwrap_or(max_hz)
}

pub fn arr_vote(stream_hz: f32, max_hz: f32) -> f32 {
    if stream_hz <= 0.0 {
        max_hz
    } else if max_hz > 0.0 {
        stream_hz.min(max_hz)
    } else {
        stream_hz
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrHalfRateCheck {
    half_since_ms: i64,
    flagged: bool,
}
impl Default for ArrHalfRateCheck {
    fn default() -> Self {
        Self {
            half_since_ms: -1,
            flagged: false,
        }
    }
}

impl ArrHalfRateCheck {
    pub const HOLD_MS: i64 = 3_000;
    pub const MIN_RENDERED_FPS: f32 = 40.0;
    pub const LOW: f32 = 0.4;
    pub const HIGH: f32 = 0.6;
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_half(rend: f32, pres: f32, disp: f32, stream: f32) -> bool {
        pres >= 0.0
            && rend >= Self::MIN_RENDERED_FPS
            && disp > 0.0
            && stream > 0.0
            && pres >= rend * Self::LOW
            && pres <= rend * Self::HIGH
            && disp <= stream * Self::HIGH
    }

    pub fn update(&mut self, now_ms: i64, rend: f32, pres: f32, disp: f32, stream: f32) -> bool {
        if !Self::is_half(rend, pres, disp, stream) {
            self.reset();
            return false;
        }
        if self.half_since_ms < 0 {
            self.half_since_ms = now_ms;
        }
        self.flagged = now_ms.saturating_sub(self.half_since_ms) >= Self::HOLD_MS;
        self.flagged
    }

    pub fn flagged(&self) -> bool {
        self.flagged
    }
    pub fn reset(&mut self) {
        self.half_since_ms = -1;
        self.flagged = false;
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ContentRateVote {
    voted: f32,
    voted_at_ms: i64,
    ever_voted: bool,
}

impl ContentRateVote {
    pub const MIN_INTERVAL_MS: i64 = 2_000;
    pub const HYSTERESIS: f32 = 0.03;
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next(&mut self, now_ms: i64, mhz: i32) -> f32 {
        if mhz <= 0 {
            return 0.0;
        }
        let hz = (mhz as f32) / 1000.0;
        if (self.voted > 0.0 && (hz - self.voted).abs() <= self.voted * Self::HYSTERESIS)
            || (self.ever_voted && now_ms.saturating_sub(self.voted_at_ms) < Self::MIN_INTERVAL_MS)
        {
            return 0.0;
        }
        self.voted = hz;
        self.voted_at_ms = now_ms;
        self.ever_voted = true;
        hz
    }

    pub fn withdraw(&mut self, now_ms: i64, mhz: i32) -> bool {
        if mhz > 0
            || self.voted <= 0.0
            || now_ms.saturating_sub(self.voted_at_ms) < Self::MIN_INTERVAL_MS
        {
            return false;
        }
        self.voted = 0.0;
        self.voted_at_ms = now_ms;
        true
    }

    pub fn invalidate(&mut self) {
        self.voted = 0.0;
    }
}

pub fn pins_window_display_mode(
    sdk: i32,
    arr: bool,
    tv_v: bool,
    tv: bool,
    ext: bool,
    mod_c: bool,
    res_c: bool,
) -> bool {
    !arr && ((!tv && !ext) || (mod_c && (tv_v || sdk < 31 || res_c)))
}

pub fn syncs_window_display_mode_with_vote(arr: bool, tv_view: bool, tv: bool, ext: bool) -> bool {
    !arr && !tv_view && !tv && !ext
}

#[derive(Clone, Copy, Debug)]
pub struct PanelMode<'a> {
    pub id: i32,
    pub width: i32,
    pub height: i32,
    pub hz: f32,
    pub alternatives: &'a [f32],
}

impl PanelMode<'_> {
    pub fn same_size(&self, o: &Self) -> bool {
        self.width == o.width && self.height == o.height
    }
    pub fn switches_seamlessly_to(&self, rate: f32) -> bool {
        self.alternatives
            .iter()
            .any(|&a| (a - rate).abs() < 0.01_f32)
    }
}

pub fn holds_vote(vote_hz: f32, active_hz: f32) -> bool {
    if vote_hz <= 0.0 || active_hz <= 0.0 {
        return false;
    }
    let n = java_round_f32(active_hz / vote_hz).max(1) as f32;
    (active_hz - n * vote_hz).abs() <= 0.5_f32.max(0.01_f32 * n * vote_hz)
}

fn multiple_error(base: f32, rate: f32) -> f32 {
    (rate - (java_round_f32(rate / base).max(1) as f32) * base).abs()
}

fn better_window_mode(cand: f32, best: f32, vote_hz: f32, may_reduce: bool) -> bool {
    if (cand - best).abs() <= 0.5_f32 {
        multiple_error(vote_hz, cand) < multiple_error(vote_hz, best)
    } else if may_reduce {
        cand < best
    } else {
        cand > best
    }
}

pub fn window_mode_for_vote(
    sdk: i32,
    vote: f32,
    red: bool,
    seam: bool,
    act: &PanelMode<'_>,
    modes: &[PanelMode<'_>],
) -> Option<usize> {
    if vote <= 0.0 {
        return None;
    }
    let mut best: Option<(usize, f32)> = None;
    for (i, m) in modes.iter().enumerate() {
        if !m.same_size(act) || !holds_vote(vote, m.hz) {
            continue;
        }
        if seam && m.id != act.id && (sdk < 31 || !act.switches_seamlessly_to(m.hz)) {
            continue;
        }
        if best.is_none_or(|(_, b)| better_window_mode(m.hz, b, vote, red)) {
            best = Some((i, m.hz));
        }
    }
    best.map(|(i, _)| i)
}

pub fn max_refresh_rate(active: &PanelMode<'_>, modes: &[PanelMode<'_>], sw: i32, sh: i32) -> f32 {
    modes
        .iter()
        .filter(|m| mode_fits_stream(m.width, m.height, active.width, active.height, sw, sh))
        .fold(active.hz, |acc, m| {
            m.alternatives.iter().copied().fold(acc.max(m.hz), f32::max)
        })
}

pub fn mode_fits_stream(mw: i32, mh: i32, cw: i32, ch: i32, sw: i32, sh: i32) -> bool {
    (mw == cw && mh == ch)
        || sw <= 0
        || sh <= 0
        || (mw.max(mh) >= sw.max(sh) && mw.min(mh) >= sw.min(sh))
}

pub fn stream_frame_rate(panel_max_hz: f32) -> i32 {
    java_round_f32(panel_max_hz).clamp(1, 1000)
}
pub fn fps_follows_display(vrr: bool, fps: i32, panel_max_hz: f32) -> bool {
    vrr || fps == stream_frame_rate(panel_max_hz)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pm(id: i32, hz: f32, alts: &[f32]) -> PanelMode<'_> {
        PanelMode {
            id,
            width: 1080,
            height: 2400,
            hz,
            alternatives: alts,
        }
    }

    #[test]
    fn frame_pacing_modes_intervals_limits_and_drain_match_java() {
        assert_eq!(pyro_wave_mode(2, false, true, true), 2);
        assert_eq!(pyro_wave_mode(2, true, true, true), PYROWAVE_HOST_TIMES);
        assert_eq!(pyro_wave_mode(2, true, false, true), 0);
        assert!(uses_pyro_wave_display_clock(1, false, true));
        assert!(!uses_pyro_wave_display_clock(0, false, true));
        assert!(!uses_pyro_wave_display_clock(1, true, true));
        assert!(frame_interval_ns(999).is_none() && frame_interval_ns(1_000_001).is_none());
        assert_eq!(frame_interval_ns(60_000), Some(16_666_666));
        let b = balanced_paces_on_vsync;
        assert_eq!(b(false, true, 500, 16_666_666), Some(true));
        assert_eq!(b(true, true, 500, 16_666_666), None);
        assert_eq!(b(true, true, 120_000, 8_333_333), Some(false));
        assert_eq!(b(true, true, 60_000, 8_333_333), Some(true));
        let d = should_drain_output;
        assert_eq!(d(0, 0, 0, 500, 64), Some(false));
        assert_eq!(d(0, 0, 0, 500, 0), Some(true));
        assert_eq!(d(3, 100, 250_000_101, 1_000, 0), Some(true));
        assert_eq!(has_output_drain_time_remaining(-1, 500), Some(false));
        assert_eq!(has_output_drain_time_remaining(0, 500), None);
        let n = next_frame_time_ns_milli_hz;
        assert_eq!(
            next_frame_time_ns(1_000_000_000, 1_250_000_000, 100),
            Some(1_260_000_000)
        );
        assert_eq!(n(1_016_666_666, 1_010_000_000, 60_000), Some(1_033_333_332));
        assert_eq!(negotiated_frame_rate_milli_hz(59_940, 60), 59_940);
        assert_eq!(negotiated_frame_rate_milli_hz(59_940, 59), 59_000);
        assert_eq!(limited_frame_rate(60, 60.0), 59);
        assert_eq!(limited_frame_rate(64, 60.0), LIMIT_ABOVE_SCREEN);
        assert_eq!(limited_frame_rate(48, 48.0), LIMIT_BOGUS_SCREEN);
        assert_eq!(limited_frame_rate(0, 0.49999997), LIMIT_BOGUS_SCREEN);
        assert_eq!(limited_frame_rate(4, 0.49999997), LIMIT_ABOVE_SCREEN);
        assert_eq!(java_round_f32(0.49999997_f32), 0);
        assert_eq!(java_round_f32(0.5_f32), 1);
        assert_eq!(java_round_f32(-0.5_f32), 0);
        assert_eq!(java_round_f32(-0.50000006_f32), -1);
        assert_eq!(java_round_f32(8_388_609.0_f32), 8_388_609);
        assert_eq!(java_round_f32(-8_388_609.0_f32), -8_388_609);
        assert_eq!(java_round_f32(f32::NAN), 0);
        assert_eq!(java_round_f32(f32::INFINITY), i32::MAX);
        assert_eq!(java_round_f32(f32::NEG_INFINITY), i32::MIN);
        assert_eq!(limit_refresh_hz(120.0, 60.0, 60, false, false, 0.0), 120.0);
        assert_eq!(limit_refresh_hz(120.0, 60.0, 60, true, false, 0.0), 60.0);
        assert!(apply_output_pacing_fallback(false, false, false, 1));
        assert!(!apply_output_pacing_fallback(true, false, false, 1));
        assert!(!schedules_frames(true, 1) && schedules_frames(true, 0));
        assert!(recovers_decoder_debt(true, 3) && !recovers_decoder_debt(true, 1));
    }

    #[test]
    fn refresh_slots_119_plan_share_reset_drift_and_arr_vrr_off() {
        let (iv, vsync, lead) = (8_333_333_i64, 1_000_000_000_i64, 11_330_000_i64);
        assert!(refresh_slots_applies(8_332_800, iv));
        assert!(!refresh_slots_applies(1_000_000_000 / 144, iv));
        assert!(adaptive_in_use(true, false, false) && !adaptive_in_use(true, false, true));
        let mut slots = RefreshSlots::new();
        assert!(!slots.earliest_claimed(vsync + 1_000_000, vsync, iv, lead));
        let s1 = slots.plan(vsync + 1_000_000, vsync, iv, lead);
        assert_eq!(s1.hold_ns, 0);
        assert!(slots.earliest_claimed(vsync + 2_000_000, vsync, iv, lead));
        assert!(!slots.earliest_claimed(vsync + iv + 1_000_000, vsync, iv, lead));
        let s2 = slots.plan(vsync + 2_000_000, vsync, iv, lead);
        assert_eq!(s2.hold_ns, iv);
        assert!(slots.earliest_claimed(vsync + 3_000_000, vsync, iv, lead));
        assert!(slots.earliest_claimed(vsync + iv + 1_000_000, vsync, iv, lead));
        assert!(!slots.earliest_claimed(vsync + 2 * iv + 1_000_000, vsync, iv, lead));
        assert!(!slots.earliest_claimed(vsync + 3_000_000, vsync, 16_666_667, lead));
        let s3 = slots.plan(vsync + 3_000_000, vsync, iv, lead);
        assert_eq!(s1.vsync_ns, vsync + 2 * iv);
        assert_eq!(s2.vsync_ns, s1.vsync_ns + iv);
        assert!(!s2.shared && s3.shared && s3.vsync_ns == s2.vsync_ns && s3.hold_ns == iv);
        let invalid = slots.plan(vsync + 4_000_000, 0, iv, lead);
        assert_eq!(invalid, RefreshSlot::default());
        assert_eq!(invalid.hold_ns, 0);
        assert!(!slots.earliest_claimed(vsync + 4_000_000, vsync, iv, lead));
        assert!(!slots.earliest_claimed(vsync + 4_000_000, 0, iv, lead));
        assert!(!slots.earliest_claimed(vsync + 4_000_000, vsync, 0, lead));
        slots.reset();
        assert!(slots.planned() == 2 && slots.shared() == 1);
        let mut drift = RefreshSlots::new();
        let jitter = [0_i64, 400_000, -350_000, 250_000, -400_000, 150_000];
        for f in 0..120_i64 {
            let live = vsync + f * (iv - 50_000) + jitter[(f as usize) % jitter.len()];
            assert!(!drift.earliest_claimed(live + 1_000_000, live, iv, lead));
            let slot = drift.plan(live + 1_000_000, live, iv, lead);
            assert!(
                !slot.shared && slot.hold_ns == 0 && (slot.vsync_ns - live).rem_euclid(iv) == 0
            );
            let shifted = live + 300_000;
            assert!(drift.earliest_claimed(shifted + 1_500_000, shifted, iv, lead));
        }
    }

    #[test]
    fn display_rate_168_window_pinning_content_vote_and_arr_half_rate() {
        assert_eq!(arr_state(35, true, true), ArrState::AndroidTooOld);
        assert_eq!(arr_state(36, true, false), ArrState::PanelWithoutArr);
        assert_eq!(arr_state(36, true, true), ArrState::InUse);
        let p = pins_window_display_mode;
        assert!(p(35, false, false, false, false, false, false));
        assert!(!p(36, true, false, false, false, true, true));
        assert!(!p(35, false, false, true, false, true, false));
        let s = syncs_window_display_mode_with_vote;
        assert!(s(false, false, false, false) && !s(true, false, false, false));
        let v = vrr_content_vote;
        assert_eq!(v(90.0, 120.0, false, &[120.0, 60.0, 90.0]), 90.0);
        assert_eq!(v(50.0, 120.0, false, &[120.0, 60.0, 90.0]), 120.0);
        let modes = [
            pm(1, 60.0, &[90.0, 120.0]),
            pm(2, 90.0, &[60.0, 120.0]),
            pm(3, 120.0, &[60.0, 90.0]),
        ];
        let w = window_mode_for_vote;
        assert_eq!(w(35, 90.0, false, true, &modes[2], &modes), Some(1));
        assert_eq!(w(35, 50.0, true, true, &modes[2], &modes), None);
        assert_eq!(max_refresh_rate(&modes[0], &modes, 1920, 1080), 120.0);
        let frac = [pm(1, 59.94, &[60.0]), pm(2, 60.0, &[59.94])];
        assert_eq!(w(35, 59.94, true, false, &frac[1], &frac), Some(0));
        let mut half = ArrHalfRateCheck::new();
        assert!(!half.update(0, 120.0, 60.0, 60.0, 120.0));
        assert!(half.update(3_000, 120.0, 60.0, 60.0, 120.0));
        assert!(!half.update(4_000, 120.0, 118.0, 120.0, 120.0));
        let mut vote = ContentRateVote::new();
        assert!(vote.next(100, 60_000) == 60.0 && vote.next(300, 59_940) == 0.0);
        vote.invalidate();
        assert!(vote.next(1_000, 60_000) == 0.0 && vote.next(2_100, 60_000) == 60.0);
        assert!(vote.withdraw(4_100, 0));
    }
}
