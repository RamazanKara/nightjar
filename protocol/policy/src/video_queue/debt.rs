//! Decoder residency governor and decoder debt recovery state machines (Nightjar issue 7).

use super::gate::{DisplayGate, RecentLevel};

/// Residency governor: tracks the minimum input-to-output latency over a sliding 10-second
/// window and requests dropping an older decoded output when surplus residency persists and
/// dropping outputs measurably shortens the queue.
#[derive(Debug, Clone)]
pub struct Residency {
    window_min_ns: [i64; Self::WINDOWS],
    windows: usize,
    slot: usize,
    window_start_ns: i64,
    current_min_ns: i64,
    last_i2o_ns: i64,
    last_excess_ns: i64,
    check_excess_ns: Option<i64>,
    post_drop_min_excess_ns: i64,
    post_drop_min_i2o_ns: i64,
    streak: i32,
    since_drop: i32,
}

impl Residency {
    pub const FRAMES: i32 = 30;
    pub const PERIOD: i32 = 4;
    pub const WINDOWS: usize = 10;
    pub const WINDOW_NS: i64 = 1_000_000_000;

    pub fn new() -> Self {
        Self {
            window_min_ns: [0; Self::WINDOWS],
            windows: 0,
            slot: 0,
            window_start_ns: 0,
            current_min_ns: i64::MAX,
            last_i2o_ns: 0,
            last_excess_ns: 0,
            check_excess_ns: None,
            post_drop_min_excess_ns: i64::MAX,
            post_drop_min_i2o_ns: i64::MAX,
            streak: 0,
            since_drop: Self::PERIOD,
        }
    }

    /// One decoder output: input-to-output time and clock reading. Returns the excess over the ten
    /// second minimum (zero for the first frame or an unknown time).
    pub fn on_output(&mut self, input_to_output_ns: i64, now_ns: i64) -> i64 {
        if input_to_output_ns <= 0 {
            self.last_i2o_ns = 0;
            return 0;
        }
        self.last_i2o_ns = input_to_output_ns;
        if self.window_start_ns == 0 {
            self.window_start_ns = now_ns;
        }
        if i128::from(now_ns) - i128::from(self.window_start_ns) >= i128::from(Self::WINDOW_NS) {
            self.window_min_ns[self.slot] = self.current_min_ns;
            self.slot = (self.slot + 1) % Self::WINDOWS;
            self.windows = (self.windows + 1).min(Self::WINDOWS);
            self.current_min_ns = i64::MAX;
            self.window_start_ns = now_ns;
        }
        self.current_min_ns = self.current_min_ns.min(input_to_output_ns);
        let mut baseline = self.current_min_ns;
        for &w in self.window_min_ns.iter().take(self.windows) {
            baseline = baseline.min(w);
        }
        input_to_output_ns - baseline
    }

    /// Counts the output for the streak and the drop period; call after [`Self::on_output`], once
    /// per output. When a previous governor drop failed to shorten residency over [`Self::PERIOD`]
    /// outputs (e.g. slower decoder clocks or heavier pictures rather than a queued buffer),
    /// re-anchors the sliding minimum so it stops dropping 1 in 4 frames for 10 seconds.
    pub fn track(&mut self, excess_ns: i64, content_interval_ns: i64) {
        self.last_excess_ns = excess_ns;
        self.streak = if excess_ns > content_interval_ns / 2 {
            self.streak + 1
        } else {
            0
        };
        if self.since_drop < Self::PERIOD {
            self.since_drop += 1;
            self.post_drop_min_excess_ns = self.post_drop_min_excess_ns.min(excess_ns);
            if self.last_i2o_ns > 0 {
                self.post_drop_min_i2o_ns = self.post_drop_min_i2o_ns.min(self.last_i2o_ns);
            }
            if self.since_drop == Self::PERIOD
                && let Some(before_excess) = self.check_excess_ns.take()
            {
                let min_gain = (content_interval_ns.max(0) * 2) / 5;
                let shortened = self.post_drop_min_excess_ns <= content_interval_ns / 2
                    || i128::from(before_excess) - i128::from(self.post_drop_min_excess_ns)
                        >= i128::from(min_gain);
                if !shortened && self.post_drop_min_i2o_ns < i64::MAX {
                    let floor = self.post_drop_min_i2o_ns;
                    if self.current_min_ns < i64::MAX {
                        self.current_min_ns = self.current_min_ns.max(floor);
                    }
                    for w in self.window_min_ns.iter_mut().take(self.windows) {
                        *w = (*w).max(floor);
                    }
                    self.streak = 0;
                }
            }
        }
    }

    /// `frames_behind`: outputs still in the decoder behind this one.
    pub fn should_drop(&self, frames_behind: i32) -> bool {
        frames_behind >= 1 && self.streak >= Self::FRAMES && self.since_drop >= Self::PERIOD
    }

    pub fn on_dropped(&mut self) {
        self.since_drop = 0;
        self.check_excess_ns = Some(self.last_excess_ns);
        self.post_drop_min_excess_ns = i64::MAX;
        self.post_drop_min_i2o_ns = i64::MAX;
    }

    pub fn reset(&mut self) {
        self.windows = 0;
        self.slot = 0;
        self.window_start_ns = 0;
        self.current_min_ns = i64::MAX;
        self.last_i2o_ns = 0;
        self.last_excess_ns = 0;
        self.check_excess_ns = None;
        self.post_drop_min_excess_ns = i64::MAX;
        self.post_drop_min_i2o_ns = i64::MAX;
        self.streak = 0;
        self.since_drop = Self::PERIOD;
    }
}

impl Default for Residency {
    fn default() -> Self {
        Self::new()
    }
}

/// Decoder debt recovery in Smoothest and the frame rate limit (Nightjar issue 7).
#[derive(Debug, Clone)]
pub struct Debt {
    sample_ns: [i64; Self::CAPACITY],
    sample_over: [bool; Self::CAPACITY],
    head: usize,
    count: usize,
    over_count: usize,
    window_start_ns: i64,

    min_ns: i64,
    learned_ns: i64,
    interval_ns: i64,
    excess_ns: i64,
    over: bool,
    recent_ns: [i64; 3],
    state: i32,
    drops: i32,
    waits: i32,
    settle: i32,
    settle_ns: [i64; Self::SETTLE as usize],
    dropped_level_ns: i64,
    start_level_ns: i64,
    ready_since_ns: i64,
    under: i32,
    event: Option<String>,

    watches_display: bool,
    gate: DisplayGate,
    recent_wait: RecentLevel,
    gate_recovery: bool,
    nothing_behind: bool,
    dropped_alone: bool,
    last_output_ns: i64,
    gate_check_ns: i64,
    gate_off: bool,
    gate_drops: i32,
    gate_level_ns: i64,
    gate_base_ns: i64,
    gate_backoff_ns: i64,
    gate_quiet_until_ns: i64,
    unlearned_level_ns: i64,
}

impl Debt {
    pub const WINDOW_NS: i64 = 1_500_000_000;
    pub const SHARE_PERCENT: i32 = 80;
    pub const MIN_OUTPUTS: usize = 20;
    pub const SETTLE: i32 = 3;
    pub const MAX_DROPS: i32 = 8;
    pub const WAIT_US: i64 = 2_000;
    pub const MAX_WAITS: i32 = 60;
    pub const MAX_LEARNED_FRAMES: i32 = 3;
    pub const GATE_EXCESS_NS: i64 = 1_000_000;
    pub const GATE_CHECK_NS: i64 = 5_000_000_000;
    pub const GATE_BACKOFF_NS: i64 = 600_000_000_000;
    pub const MAX_GATE_DROPS: i32 = 24;
    const GATE_CHECK_BUCKETS: i32 = 3;
    const GATE_CHECK_SLOPE: f64 = 0.5;
    const CAPACITY: usize = 1024;
    const WATCH: i32 = 0;
    const RECOVER: i32 = 1;
    const PAUSED: i32 = 2;

    pub fn new() -> Self {
        Self {
            sample_ns: [0; Self::CAPACITY],
            sample_over: [false; Self::CAPACITY],
            head: 0,
            count: 0,
            over_count: 0,
            window_start_ns: 0,
            min_ns: i64::MAX,
            learned_ns: 0,
            interval_ns: 0,
            excess_ns: 0,
            over: false,
            recent_ns: [0; 3],
            state: Self::WATCH,
            drops: 0,
            waits: 0,
            settle: 0,
            settle_ns: [0; Self::SETTLE as usize],
            dropped_level_ns: 0,
            start_level_ns: 0,
            ready_since_ns: 0,
            under: 0,
            event: None,
            watches_display: false,
            gate: DisplayGate::new(),
            recent_wait: RecentLevel::new(),
            gate_recovery: false,
            nothing_behind: false,
            dropped_alone: false,
            last_output_ns: 0,
            gate_check_ns: 0,
            gate_off: false,
            gate_drops: 0,
            gate_level_ns: 0,
            gate_base_ns: 0,
            gate_backoff_ns: 0,
            gate_quiet_until_ns: 0,
            unlearned_level_ns: 0,
        }
    }

    pub fn watch_display(&mut self, watch: bool) {
        self.watches_display = watch;
        if !watch {
            self.gate_recovery = false;
        }
    }

    pub fn watches_display(&self) -> bool {
        self.watches_display
    }

    pub fn on_output(
        &mut self,
        input_to_output_ns: i64,
        submit_to_output_ns: i64,
        now_ns: i64,
        content_interval_ns: i64,
        frames_behind: i32,
    ) -> i64 {
        if input_to_output_ns <= 0 || content_interval_ns <= 0 {
            self.over = false;
            self.nothing_behind = false;
            return 0;
        }
        self.interval_ns = content_interval_ns;
        self.min_ns = self.min_ns.min(input_to_output_ns);
        let excess = (i128::from(input_to_output_ns) - self.floor_ns())
            .max(0)
            .min(i128::from(i64::MAX)) as i64;
        self.excess_ns = excess;
        self.over = self.is_over(self.excess_ns);
        self.nothing_behind = frames_behind == 0;
        self.last_output_ns = now_ns;
        self.recent_ns[0] = self.recent_ns[1];
        self.recent_ns[1] = self.recent_ns[2];
        self.recent_ns[2] = self.excess_ns;
        if self.watches_display && submit_to_output_ns > 0 {
            self.watch_display_sample(submit_to_output_ns, now_ns, content_interval_ns);
        }
        if self.state == Self::WATCH {
            self.add(now_ns, self.over);
            let share = self.window_share(now_ns, true);
            if share
                || (self.watches_display
                    && !self.gate_off
                    && now_ns >= self.gate_quiet_until_ns
                    && self.gate.held())
            {
                self.state = Self::RECOVER;
                self.gate_recovery = !share;
                self.drops = 0;
                self.waits = 0;
                self.settle = 0;
                self.under = 0;
                self.ready_since_ns = now_ns;
                self.start_level_ns =
                    median3(self.recent_ns[0], self.recent_ns[1], self.recent_ns[2]);
            }
        } else if self.state == Self::RECOVER {
            self.recover(now_ns);
        } else {
            self.add(now_ns, self.over);
            if self.window_share(now_ns, false) {
                self.state = Self::WATCH;
                self.clear();
            }
        }
        self.excess_ns
    }

    pub fn should_drop(&self) -> bool {
        self.state == Self::RECOVER
            && self.settle == 0
            && if self.gate_recovery {
                self.nothing_behind
            } else {
                self.over
            }
            && self.drops < Self::MAX_DROPS
    }

    pub fn on_dropped(&mut self) {
        self.drops += 1;
        self.settle = Self::SETTLE;
        self.dropped_level_ns = median3(self.recent_ns[0], self.recent_ns[1], self.recent_ns[2]);
        self.dropped_alone = self.nothing_behind;
        if self.gate_recovery {
            self.gate_check_ns = self.last_output_ns.saturating_add(Self::GATE_CHECK_NS);
            self.gate_level_ns = self.recent_wait.last_ns();
            self.gate_base_ns = self.recent_wait.low_ns();
            self.gate_drops += 1;
            if self.gate_drops >= Self::MAX_GATE_DROPS {
                self.gate_off = true;
                self.event = Some(format!(
                    "Decoder debt: {} drops for the display since the last flush, half a frame is what is left",
                    self.gate_drops
                ));
            }
        }
    }

    pub fn on_no_newer(&mut self) {
        if self.state == Self::RECOVER {
            self.waits += 1;
            if self.waits >= Self::MAX_WAITS {
                self.learn("paused, no newer output came out");
            }
        }
    }

    pub fn take_event(&mut self) -> Option<String> {
        self.event.take()
    }

    pub fn reset(&mut self) {
        self.clear();
        self.min_ns = i64::MAX;
        self.learned_ns = 0;
        self.excess_ns = 0;
        self.over = false;
        self.recent_ns.fill(0);
        self.state = Self::WATCH;
        self.drops = 0;
        self.waits = 0;
        self.settle = 0;
        self.under = 0;
        self.event = None;
        self.gate.reset();
        self.recent_wait.reset();
        self.gate_recovery = false;
        self.nothing_behind = false;
        self.dropped_alone = false;
        self.last_output_ns = 0;
        self.gate_check_ns = 0;
        self.gate_off = false;
        self.gate_drops = 0;
        self.gate_backoff_ns = 0;
        self.gate_quiet_until_ns = 0;
        self.unlearned_level_ns = 0;
    }

    fn watch_display_sample(
        &mut self,
        submit_to_output_ns: i64,
        now_ns: i64,
        content_interval_ns: i64,
    ) {
        self.recent_wait.add(submit_to_output_ns, now_ns);
        let over = i128::from(submit_to_output_ns) - i128::from(self.recent_wait.low_ns())
            >= i128::from(Self::GATE_EXCESS_NS);
        self.gate.on_output(
            submit_to_output_ns,
            now_ns,
            content_interval_ns,
            over,
            !self.nothing_behind,
        );
        if self.gate_check_ns == 0 || now_ns < self.gate_check_ns {
            return;
        }
        self.gate_check_ns = 0;
        let wait_before_ns = i128::from(self.gate_level_ns) - i128::from(self.gate_base_ns);
        let wait_after_ns = i128::from(self.recent_wait.last_ns()) - i128::from(self.gate_base_ns);
        if self
            .gate
            .still_held(Self::GATE_CHECK_BUCKETS, Self::GATE_CHECK_SLOPE)
        {
            self.gate_off = true;
            self.event = Some(
                "Decoder debt: outputs still follow the display after a drop, the wait for a buffer is not it"
                    .to_string(),
            );
        } else if wait_after_ns * 2 > wait_before_ns {
            self.gate_backoff_ns = if self.gate_backoff_ns == 0 {
                Self::GATE_BACKOFF_NS
            } else {
                self.gate_backoff_ns.saturating_mul(2)
            };
            self.gate_quiet_until_ns = now_ns.saturating_add(self.gate_backoff_ns);
            self.event = Some(format!(
                "Decoder debt: a drop left {:.1} of {:.1} ms of wait, the next one waits {} minutes",
                wait_after_ns as f64 / 1e6,
                wait_before_ns as f64 / 1e6,
                self.gate_backoff_ns / 60_000_000_000
            ));
        } else {
            self.gate_backoff_ns = 0;
        }
    }

    fn recover(&mut self, now_ns: i64) {
        if self.settle > 0 {
            let idx = (Self::SETTLE - self.settle) as usize;
            self.settle_ns[idx] = self.excess_ns;
            self.settle -= 1;
            if self.settle > 0 {
                return;
            }
            let level = median3(self.settle_ns[0], self.settle_ns[1], self.settle_ns[2]);
            if self.gate_recovery || !self.is_over(level) {
                self.finish("recovered", level, Self::WATCH, false);
            } else if i128::from(level)
                > i128::from(self.dropped_level_ns) - i128::from(self.interval_ns) / 2
            {
                if self.dropped_alone {
                    self.gate_off = true;
                }
                self.learn("paused, a drop did not shorten the queue");
            } else if self.drops >= Self::MAX_DROPS {
                self.finish("paused at the drop limit", level, Self::PAUSED, false);
            } else {
                self.ready_since_ns = now_ns;
                self.waits = 0;
                self.under = 0;
            }
            return;
        }
        if !self.over && !self.gate_recovery {
            self.under += 1;
            if self.under >= 2 {
                self.finish("recovered", self.excess_ns, Self::WATCH, false);
            }
            return;
        }
        self.under = 0;
        if i128::from(now_ns) - i128::from(self.ready_since_ns) < i128::from(Self::WINDOW_NS) {
            return;
        }
        if self.waits > 0 {
            self.learn("paused, no newer output came out");
        } else {
            self.finish("ended without a drop", self.excess_ns, Self::WATCH, false);
        }
    }

    fn learn(&mut self, result: &str) {
        let level = median3(self.recent_ns[0], self.recent_ns[1], self.recent_ns[2]);
        if self.watches_display
            && i128::from(self.learned_ns) + i128::from(level)
                > i128::from(Self::MAX_LEARNED_FRAMES) * i128::from(self.interval_ns)
            && (i128::from(level) - i128::from(self.unlearned_level_ns)).abs() * 2
                >= i128::from(self.interval_ns)
        {
            self.unlearned_level_ns = level;
            self.finish(result, level, Self::PAUSED, false);
            return;
        }
        self.learned_ns = self.learned_ns.saturating_add(level);
        self.finish(result, level, Self::PAUSED, true);
    }

    fn finish(&mut self, result: &str, level_ns: i64, next: i32, learned: bool) {
        if self.drops > 0 || next == Self::PAUSED {
            let floor = self.floor_ns();
            let before_floor = floor - if learned { i128::from(level_ns) } else { 0 };
            let raised = if learned {
                format!(", floor raised to {:.1} ms", floor as f64 / 1e6)
            } else {
                String::new()
            };
            self.event = Some(format!(
                "Decoder debt {}: {} stale output{} dropped, excess {:.1} -> {:.1} ms over a {:.1} ms floor{}",
                result,
                self.drops,
                if self.drops == 1 { "" } else { "s" },
                self.start_level_ns as f64 / 1e6,
                level_ns as f64 / 1e6,
                before_floor as f64 / 1e6,
                raised
            ));
        }
        self.state = next;
        self.gate_recovery = false;
        self.gate.clear();
        self.clear();
    }

    fn floor_ns(&self) -> i128 {
        i128::from(self.min_ns) + i128::from(self.learned_ns)
    }

    fn is_over(&self, excess: i64) -> bool {
        i128::from(excess) * 2 >= i128::from(self.interval_ns)
    }

    fn add(&mut self, now_ns: i64, is_over: bool) {
        let cutoff = i128::from(now_ns) - i128::from(Self::WINDOW_NS);
        while self.count > 0
            && (self.count == Self::CAPACITY || i128::from(self.sample_ns[self.head]) < cutoff)
        {
            if self.sample_over[self.head] {
                self.over_count -= 1;
            }
            self.head = (self.head + 1) % Self::CAPACITY;
            self.count -= 1;
        }
        if self.count == 0 {
            self.window_start_ns = now_ns;
        }
        let tail = (self.head + self.count) % Self::CAPACITY;
        self.sample_ns[tail] = now_ns;
        self.sample_over[tail] = is_over;
        self.count += 1;
        if is_over {
            self.over_count += 1;
        }
    }

    fn window_share(&self, now_ns: i64, of_over: bool) -> bool {
        if self.count < Self::MIN_OUTPUTS
            || i128::from(now_ns) - i128::from(self.window_start_ns) < i128::from(Self::WINDOW_NS)
        {
            return false;
        }
        let matching = if of_over {
            self.over_count
        } else {
            self.count - self.over_count
        };
        (matching as u128) * 100 >= (self.count as u128) * (Self::SHARE_PERCENT as u128)
    }

    fn clear(&mut self) {
        self.head = 0;
        self.count = 0;
        self.over_count = 0;
    }
}

impl Default for Debt {
    fn default() -> Self {
        Self::new()
    }
}

fn median3(a: i64, b: i64, c: i64) -> i64 {
    a.min(b).max(a.max(b).min(c))
}
