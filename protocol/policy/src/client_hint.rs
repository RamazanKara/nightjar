//! Client-hint bitrate adaptation over 0x553C rate changes: [`Bitrate`] is Android's
//! `ClientHintBitrateController.java`.
//!
//! Adapted from Moonlight V+'s local controller (`AdaptiveBitrateService.tickLocal`, balanced
//! mode, GPL-3.0; originally ported into Rubylight by RamazanKara): loss cuts the bitrate by a
//! step that grows with the loss, and five clean seconds probe it back up by 2–5 %, with two
//! seconds between changes. The client hints the host over the control stream instead of HTTP,
//! so a change reaches the encoder on its next frame without a keyframe.
//!
//! Beyond V+: the ceiling never exceeds the user's configured `initial_kbps` (so clean links
//! never over-probe above the chosen bitrate or preempt host congestion control); a probe that
//! meets loss holds at the last proven clean rate with exponential backoff instead of
//! sawtoothing; device heat holds (warm) or steps down (hot) the bitrate; no decision runs
//! without recent video; one request is outstanding at a time; a host cap lowers the ceiling;
//! a refusal stops the controller; and a standing network queue holds upward probes.

use serde_json::Value;

use crate::stats::{Stats, known};
use crate::trace::{Args, Subject};

/// Time between two changes (V+'s balanced cooldown).
pub const COOLDOWN_MS: u64 = 2000;
/// Time after the start before the first decision (V+'s start delay).
pub const START_DELAY_MS: u64 = 3000;
/// Clean samples before a probe up (V+'s balanced threshold).
pub const STABLE_SAMPLES: u32 = 5;
/// A network queue at or above this holds probes.
pub const QUEUE_HOLD_MS: f32 = 20.0;
/// Consecutive hot (`thermal_level >= 2`) samples before stepping bitrate down.
pub const THERMAL_HOT_SAMPLES: u32 = 3;
/// Initial hold at the pre-probe ceiling after an upward probe meets frame loss.
pub const INITIAL_CEILING_HOLD_MS: u64 = 30_000;
/// Maximum backoff hold at a learned loss ceiling.
pub const MAX_CEILING_HOLD_MS: u64 = 300_000;

/// The bounded range below a starting bitrate: 0.3x (at least 3 Mbps where `initial_kbps`
/// allows it) up to `initial_kbps` itself, clamped within 0.5–800 Mbps.
pub fn range(initial_kbps: u32) -> (u32, u32) {
    let baseline = initial_kbps.clamp(500, 800_000);
    let low = 3000.max((f64::from(baseline) * 0.3) as u32);
    (baseline.min(low).max(500), baseline)
}

/// Steers the stream's bitrate from this client's loss and heat, one 0x553C request at a time.
#[derive(Clone, Debug, PartialEq)]
pub struct Bitrate {
    min_kbps: u32,
    max_kbps: u32,
    /// This client's cap, lowered to a cap the host answered with; 0 for none.
    limit_kbps: u32,
    current_kbps: u32,
    started_ms: u64,
    last_change_ms: Option<u64>,
    stable_samples: u32,
    loss_streak: u32,
    hot_samples: u32,
    thermal_level: u32,
    last_down: bool,
    probed_from_kbps: Option<u32>,
    ceiling_hold_kbps: Option<u32>,
    ceiling_until_ms: Option<u64>,
    probe_backoff_ms: u64,
    pending_kbps: Option<u32>,
    client_owned: bool,
    stopped: bool,
    /// A cut the stream before an automatic reconnect had made ([`Self::resume_from`]).
    resume_kbps: Option<u32>,
}

impl Bitrate {
    /// After an automatic reconnect: the first decision returns to the rate the stream before had
    /// cut to, inside the range of the stream's own bitrate, which it may climb back to.
    pub fn resume_from(&mut self, kbps: u32) {
        self.resume_kbps = (kbps > 0 && kbps < self.current_kbps).then_some(kbps);
    }

    /// Updates the device thermal level (`0` normal, `1` warm holds, `>= 2` hot steps down).
    pub fn set_thermal_level(&mut self, level: u32) {
        self.thermal_level = level;
    }

    /// `initial_kbps` is the stream's bitrate and `limit_kbps` this client's own cap (0 for
    /// none), which the range never exceeds.
    pub fn new(initial_kbps: u32, limit_kbps: u32, now_ms: u64) -> Self {
        let (min_kbps, max_kbps) = range(initial_kbps);
        let max_kbps = if limit_kbps > 0 {
            max_kbps.min(limit_kbps)
        } else {
            max_kbps
        };
        Self {
            min_kbps: min_kbps.min(max_kbps),
            max_kbps,
            limit_kbps,
            current_kbps: initial_kbps,
            started_ms: now_ms,
            last_change_ms: None,
            stable_samples: 0,
            loss_streak: 0,
            hot_samples: 0,
            thermal_level: 0,
            last_down: false,
            probed_from_kbps: None,
            ceiling_hold_kbps: None,
            ceiling_until_ms: None,
            probe_backoff_ms: 0,
            pending_kbps: None,
            client_owned: false,
            stopped: false,
            resume_kbps: None,
        }
    }

    /// Call once per second. Returns the bitrate to request, which stays pending until
    /// [`Self::applied`] or [`Self::refused`], or `None` to keep the current one.
    pub fn sample(&mut self, stats: &Stats) -> Option<u32> {
        let now_ms = stats.now_ms;
        if self.stopped || self.pending_kbps.is_some() {
            return None;
        }
        if self.ceiling_until_ms.is_some_and(|until| now_ms >= until) {
            self.ceiling_until_ms = None;
            self.ceiling_hold_kbps = None;
        }
        // Without video the loss is not current.
        if !stats.recent_video {
            self.stable_samples = 0;
            self.loss_streak = 0;
            self.hot_samples = 0;
            return None;
        }
        if now_ms < self.started_ms + START_DELAY_MS
            || self
                .last_change_ms
                .is_some_and(|at| now_ms < at + COOLDOWN_MS)
        {
            return None;
        }
        if let Some(kbps) = self.resume_kbps.take() {
            let kbps = kbps.clamp(self.min_kbps, self.max_kbps);
            if kbps < self.current_kbps {
                self.pending_kbps = Some(kbps);
                return Some(kbps);
            }
        }
        // Unknown loss is no decision, and no clean sample.
        let Some(loss) = known(stats.loss_percent) else {
            self.stable_samples = 0;
            return None;
        };
        let thermal = stats.thermal_level.max(self.thermal_level);
        let current = f64::from(self.current_kbps);
        let mut target = current;
        let mut loss_cut = false;
        if loss > 5.0 {
            target = current * 0.7;
            self.stable_samples = 0;
            self.loss_streak += 1;
            self.hot_samples = 0;
            loss_cut = true;
        } else if loss > 2.0 {
            self.stable_samples = 0;
            self.loss_streak += 1;
            self.hot_samples = 0;
            if self.loss_streak >= 2 {
                target = current * 0.9;
                loss_cut = true;
            }
        } else if loss > 0.5 {
            self.loss_streak += 1;
            self.stable_samples = 0;
            self.hot_samples = 0;
            if self.loss_streak >= 4 {
                target = current * 0.95;
                loss_cut = true;
            }
        } else if thermal >= 2 {
            self.loss_streak = 0;
            self.stable_samples = 0;
            self.hot_samples += 1;
            if self.hot_samples >= THERMAL_HOT_SAMPLES {
                target = current * 0.9;
                self.hot_samples = 0;
            }
        } else if thermal == 1 || known(stats.queue_ms).is_some_and(|queue| queue >= QUEUE_HOLD_MS)
        {
            self.loss_streak = 0;
            self.hot_samples = 0;
            self.stable_samples = 0;
        } else {
            self.loss_streak = 0;
            self.hot_samples = 0;
            self.stable_samples += 1;
            let probe_ceiling = self
                .ceiling_hold_kbps
                .map_or(self.max_kbps, |hold| hold.min(self.max_kbps));
            if self.stable_samples >= STABLE_SAMPLES {
                if self.client_owned && self.current_kbps < probe_ceiling {
                    let step = if self.last_down
                        || (self.probe_backoff_ms > 0 && self.ceiling_hold_kbps.is_none())
                    {
                        1.02
                    } else {
                        1.05
                    };
                    target = (current * step).min(f64::from(probe_ceiling));
                } else {
                    if self.ceiling_hold_kbps.is_some() && self.ceiling_until_ms.is_none() {
                        self.ceiling_until_ms = Some(now_ms.saturating_add(self.probe_backoff_ms));
                    }
                    self.probed_from_kbps = None;
                }
                self.stable_samples = 0;
            }
        }
        if loss_cut && let Some(safe_kbps) = self.probed_from_kbps.take() {
            self.ceiling_hold_kbps = Some(safe_kbps);
            self.probe_backoff_ms = if self.probe_backoff_ms == 0 {
                INITIAL_CEILING_HOLD_MS
            } else {
                (self.probe_backoff_ms * 2).min(MAX_CEILING_HOLD_MS)
            };
            self.ceiling_until_ms = None;
            target = target.min(f64::from(safe_kbps));
        }
        let target = (target as u32).clamp(self.min_kbps, self.max_kbps);
        if target == self.current_kbps || (!self.client_owned && target > self.current_kbps) {
            return None;
        }
        self.pending_kbps = Some(target);
        Some(target)
    }

    /// The host's result for the pending request: the stream now runs at `kbps`. Below the
    /// request, the host capped it, and the ceiling follows.
    pub fn applied(&mut self, kbps: u32, now_ms: u64) {
        let Some(requested) = self.pending_kbps.take() else {
            return;
        };
        self.client_owned = true;
        if kbps < requested {
            self.limit_kbps = kbps;
            self.max_kbps = self.max_kbps.min(kbps);
            self.min_kbps = self.min_kbps.min(self.max_kbps);
        }
        if kbps > self.current_kbps {
            self.probed_from_kbps = Some(self.current_kbps);
            if kbps >= self.max_kbps {
                self.probe_backoff_ms = 0;
            }
        } else if kbps < self.current_kbps {
            self.probed_from_kbps = None;
        }
        if kbps != self.current_kbps {
            self.last_down = kbps < self.current_kbps;
        }
        self.current_kbps = kbps;
        if let Some(hold) = self.ceiling_hold_kbps
            && self.current_kbps >= hold
            && self.ceiling_until_ms.is_none()
        {
            self.ceiling_until_ms = Some(now_ms.saturating_add(self.probe_backoff_ms));
        }
        self.last_change_ms = Some(now_ms);
    }

    /// The host refused the pending request: the controller stops for this stream.
    pub fn refused(&mut self) {
        self.pending_kbps = None;
        self.stopped = true;
    }

    /// The stream runs at `kbps` the user set by hand: the range follows it, as a new start
    /// under the same caps.
    pub fn overridden(&mut self, kbps: u32, now_ms: u64) {
        *self = Self {
            last_change_ms: Some(now_ms),
            client_owned: true,
            stopped: self.stopped,
            thermal_level: self.thermal_level,
            ..Self::new(kbps, self.limit_kbps, self.started_ms)
        };
    }

    /// Prefer bitrate reductions before lowering stream resolution for network congestion.
    pub fn has_room_before_quality_reduction(&self) -> bool {
        !self.stopped && self.current_kbps > self.min_kbps.max(self.max_kbps * 65 / 100)
    }

    /// Syncs the pre-takeover baseline with a host `0x5538` link notice while this client has not
    /// yet written a bitrate on the session (`!client_took_bitrate`), preventing the first client
    /// loss or thermal cut from jumping above a rate the host already cut to without probing up on
    /// clean samples before the client owns the bitrate.
    pub fn sync_host_link(&mut self, host_current_kbps: u32, host_ceiling_kbps: u32) {
        if self.stopped
            || self.client_owned
            || self.pending_kbps.is_some()
            || host_ceiling_kbps == 0
        {
            return;
        }
        let scaled = if host_current_kbps >= host_ceiling_kbps {
            self.max_kbps
        } else {
            ((u64::from(self.max_kbps) * u64::from(host_current_kbps))
                / u64::from(host_ceiling_kbps)) as u32
        };
        self.current_kbps = scaled.clamp(500, self.max_kbps);
    }

    pub fn current_kbps(&self) -> u32 {
        self.current_kbps
    }

    pub fn min_kbps(&self) -> u32 {
        self.min_kbps
    }

    pub fn max_kbps(&self) -> u32 {
        self.max_kbps
    }

    /// A request is waiting for the host's result.
    pub fn pending(&self) -> bool {
        self.pending_kbps.is_some()
    }

    /// A refusal stopped the controller.
    pub fn stopped(&self) -> bool {
        self.stopped
    }
}

pub(crate) fn traced(args: &Value) -> Result<Box<dyn Subject>, String> {
    let a = Args::new(args, 3)?;
    Ok(Box::new(Bitrate::new(a.int(0)?, a.int(1)?, a.int(2)?)))
}

// Args reads only integers and text; samples also carry floats and a flag. A float reaches
// the controller as Java passes it, -1 for a missing measurement included.
fn float(args: &Value, i: usize) -> Result<Option<f32>, String> {
    args[i]
        .as_f64()
        .map(|v| Some(v as f32))
        .ok_or_else(|| format!("argument {i} is not a number: {}", args[i]))
}

impl Subject for Bitrate {
    fn call(&mut self, method: &str, args: &Value) -> Result<Value, String> {
        match method {
            "sample" => {
                let a = Args::new(args, 4)?;
                let stats = Stats {
                    now_ms: a.int(0)?,
                    recent_video: args[1]
                        .as_bool()
                        .ok_or_else(|| format!("argument 1 is not a boolean: {}", args[1]))?,
                    loss_percent: float(args, 2)?,
                    queue_ms: float(args, 3)?,
                    ..Stats::default()
                };
                Ok(self.sample(&stats).unwrap_or(0).into())
            }
            "resume_from" => {
                let a = Args::new(args, 1)?;
                self.resume_from(a.int(0)?);
                Ok(Value::Null)
            }
            "set_thermal_level" => {
                let a = Args::new(args, 1)?;
                self.set_thermal_level(a.int(0)?);
                Ok(Value::Null)
            }
            "applied" => {
                let a = Args::new(args, 2)?;
                self.applied(a.int(0)?, a.int(1)?);
                Ok(Value::Null)
            }
            "refused" => {
                Args::new(args, 0)?;
                self.refused();
                Ok(Value::Null)
            }
            "overridden" => {
                let a = Args::new(args, 2)?;
                self.overridden(a.int(0)?, a.int(1)?);
                Ok(Value::Null)
            }
            "current_kbps"
            | "min_kbps"
            | "max_kbps"
            | "pending"
            | "stopped"
            | "has_room_before_quality_reduction" => {
                Args::new(args, 0)?;
                Ok(match method {
                    "current_kbps" => self.current_kbps.into(),
                    "min_kbps" => self.min_kbps.into(),
                    "max_kbps" => self.max_kbps.into(),
                    "pending" => self.pending().into(),
                    "has_room_before_quality_reduction" => {
                        self.has_room_before_quality_reduction().into()
                    }
                    _ => self.stopped.into(),
                })
            }
            _ => Err(format!("unknown method {method}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(second: u64, loss: f32) -> Stats {
        Stats {
            now_ms: second * 1000,
            recent_video: true,
            loss_percent: Some(loss),
            ..Stats::default()
        }
    }

    #[test]
    fn the_range_is_capped_at_the_starting_bitrate() {
        assert_eq!(range(20_000), (6000, 20_000));
        assert_eq!(range(5000), (3000, 5000));
        assert_eq!(range(100), (500, 500));
        assert_eq!(range(100_000), (30_000, 100_000));
        assert_eq!(range(500_000), (150_000, 500_000));
        assert_eq!(range(u32::MAX), (240_000, 800_000));

        // Clean links stay quiet at the configured bitrate instead of over-probing to 2x.
        let mut clean = Bitrate::new(40_000, 0, 0);
        for second in 0..60 {
            assert_eq!(clean.sample(&tick(second, 0.0)), None, "second {second}");
        }
    }

    /// An automatic reconnect starts again from the cut, below the same ceiling.
    #[test]
    fn a_resumed_controller_returns_to_the_cut_and_may_climb_back() {
        let mut c = Bitrate::new(20_000, 0, 0);
        c.resume_from(9_000);
        assert_eq!(c.sample(&tick(2, 0.0)), None, "the start delay holds");
        assert_eq!(c.sample(&tick(3, 0.0)), Some(9_000));
        c.applied(9_000, 3000);
        assert_eq!(c.max_kbps(), 20_000, "the ceiling is the stream's own");
        let climbed = (5..30).find_map(|s| c.sample(&tick(s, 0.0)));
        assert!(climbed.is_some_and(|kbps| kbps > 9_000), "{climbed:?}");
        let mut higher = Bitrate::new(20_000, 0, 0);
        higher.resume_from(25_000);
        assert_eq!(higher.sample(&tick(3, 0.0)), None, "no cut to return to");
    }

    #[test]
    fn a_failed_upward_probe_holds_at_the_proven_ceiling_with_backoff() {
        let mut c = Bitrate::new(40_000, 0, 0);
        assert_eq!(c.sample(&tick(3, 10.0)), Some(28_000));
        c.applied(28_000, 3000);
        // 5 clean seconds probe +2% from 28_000 -> 28_560.
        for s in 5..9 {
            assert_eq!(c.sample(&tick(s, 0.0)), None);
        }
        assert_eq!(c.sample(&tick(9, 0.0)), Some(28_560));
        c.applied(28_560, 9000);
        // Loss immediately after the probe cuts; recovery climbs back to 28_000 and then holds
        // there for the full 30 s (`INITIAL_CEILING_HOLD_MS`) before cautiously re-probing +2%.
        assert_eq!(c.sample(&tick(11, 10.0)), Some(19_992));
        c.applied(19_992, 11_000);
        let mut reached_at = None;
        for now_s in 13..80 {
            if let Some(req) = c.sample(&tick(now_s, 0.0)) {
                assert!(
                    req <= 28_000,
                    "must not probe past learned 28_000 during hold: {req} at {now_s}s"
                );
                c.applied(req, now_s * 1000);
                if req == 28_000 && reached_at.is_none() {
                    reached_at = Some(now_s);
                }
            }
        }
        let reached_s = reached_at.expect("climbs back to learned 28_000 ceiling");
        assert_eq!(c.current_kbps(), 28_000);
        // Still held just before `reached_s + 30`, then cautious +2% probing resumes after.
        assert_eq!(c.sample(&tick(reached_s + 29, 0.0)), None);
        let reprobe = ((reached_s + 30)..(reached_s + 40)).find_map(|s| c.sample(&tick(s, 0.0)));
        assert_eq!(reprobe, Some(28_560));
    }

    #[test]
    fn thermal_warm_holds_probes_and_hot_steps_down_while_has_room_tracks_65_percent() {
        let mut c = Bitrate::new(20_000, 0, 0);
        assert!(c.has_room_before_quality_reduction());
        c.set_thermal_level(2);
        assert_eq!(c.sample(&tick(3, 0.0)), None);
        assert_eq!(c.sample(&tick(4, 0.0)), None);
        assert_eq!(c.sample(&tick(5, 0.0)), Some(18_000));
        c.applied(18_000, 5000);
        assert!(c.has_room_before_quality_reduction());

        // Warm holds upward recovery even on clean loss=0 samples.
        c.set_thermal_level(1);
        for s in 7..25 {
            assert_eq!(c.sample(&tick(s, 0.0)), None);
        }

        // Below 65% of max_kbps (13_000), Automatic quality may step down stream size.
        assert_eq!(c.sample(&tick(26, 10.0)), Some(12_600));
        c.applied(12_600, 26_000);
        assert!(!c.has_room_before_quality_reduction());
    }

    #[test]
    fn one_request_at_a_time_and_nothing_after_a_refusal() {
        let mut c = Bitrate::new(20_000, 0, 0);
        assert_eq!(c.sample(&tick(3, 10.0)), Some(14_000));
        assert!(c.pending());
        assert_eq!(c.sample(&tick(4, 10.0)), None, "the first is still pending");
        c.applied(14_000, 4000);
        assert_eq!(c.sample(&tick(5, 10.0)), None, "cooldown");
        assert_eq!(c.sample(&tick(6, 10.0)), Some(9800));
        c.refused();
        assert!(c.stopped() && !c.pending() && !c.has_room_before_quality_reduction());
        assert!((7..60).all(|s| c.sample(&tick(s, 10.0)).is_none()));
        c.overridden(30_000, 60_000);
        assert!(
            c.stopped(),
            "a manual change does not restart a refused controller"
        );
    }

    #[test]
    fn a_client_limit_caps_the_range_without_raising_above_initial() {
        let c = Bitrate::new(20_000, 25_000, 0);
        assert_eq!((c.min_kbps(), c.max_kbps()), (6000, 20_000));
        let c = Bitrate::new(20_000, 15_000, 0);
        assert_eq!((c.min_kbps(), c.max_kbps()), (6000, 15_000));
        let c = Bitrate::new(20_000, 1000, 0);
        assert_eq!((c.min_kbps(), c.max_kbps()), (1000, 1000));
    }

    #[test]
    fn moderate_loss_streak_resets_stable_samples() {
        let mut c = Bitrate::new(10_000, 0, 0);
        c.resume_from(7_000);
        assert_eq!(c.sample(&tick(3, 0.0)), Some(7_000));
        c.applied(7_000, 3000);
        for second in 5..9 {
            assert_eq!(c.sample(&tick(second, 0.0)), None);
        }
        // Single > 2% loss sample resets the clean-sample streak even before a cut.
        assert_eq!(c.sample(&tick(9, 3.0)), None);
        assert_eq!(c.sample(&tick(10, 0.0)), None);
    }
}
