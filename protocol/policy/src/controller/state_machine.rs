//! Pure Android/Nightjar controller state machines extracted from `ControllerHandler`:
//! quit and stream-menu combos, 25 ms tap minimum, 750 ms Start mouse-mode toggle,
//! `GuideHold` and USB Guide-hold gesture, controller mouse emulation, and USB
//! `steerMenu`/`capturePad`/`heldFromMenu` state.

use std::collections::BTreeMap;

use crate::gamepad_constants::button;

use super::deadzone::{fixed_step_delta, fixed_step_next, mouse_stick_curve};
use super::mapping::keycode;

/// Hold time (ms) on Start to toggle controller mouse emulation.
pub const START_DOWN_TIME_MOUSE_MODE_MS: i64 = 750;

/// Minimum press duration (ms) for short button/Guide taps so games observe them.
pub const MINIMUM_BUTTON_DOWN_TIME_MS: i64 = 25;

/// Period (ms) between stick mouse-emulation ticks (nominally 100 Hz).
pub const MOUSE_EMULATION_REPORT_PERIOD_MS: i32 = 10;

/// Ratio from the 50 ms base tick to [`MOUSE_EMULATION_REPORT_PERIOD_MS`] so travel speed stays constant.
pub const MOUSE_EMULATION_TIME_COMPENSATION: f32 =
    MOUSE_EMULATION_REPORT_PERIOD_MS as f32 / 50.0;

/// Default controller mouse sensitivity percentage.
pub const DEFAULT_MOUSE_SENSITIVITY_PERCENT: i32 = 100;

/// Minimum controller mouse sensitivity percentage.
pub const MIN_MOUSE_SENSITIVITY_PERCENT: i32 = 25;

/// Maximum controller mouse sensitivity percentage.
pub const MAX_MOUSE_SENSITIVITY_PERCENT: i32 = 200;

/// Clamps controller mouse sensitivity percentage to `[MIN_MOUSE_SENSITIVITY_PERCENT, MAX_MOUSE_SENSITIVITY_PERCENT]`.
pub fn clamp_mouse_sensitivity_percent(percent: i32) -> i32 {
    percent.clamp(MIN_MOUSE_SENSITIVITY_PERCENT, MAX_MOUSE_SENSITIVITY_PERCENT)
}

/// Stream-menu combo (`Back | Start | LB | RB`), held back from Android's quit-combo list.
pub const MENU_COMBO_MASK: u32 = button::BACK | button::PLAY | button::LB | button::RB;

/// Buttons forwarded to `PadCapture` while the on-screen PC keyboard owns a USB pad.
pub const CAPTURED_BUTTONS: [u32; 11] = [
    button::UP,
    button::DOWN,
    button::LEFT,
    button::RIGHT,
    button::A,
    button::B,
    button::X,
    button::Y,
    button::LB,
    button::RB,
    button::PLAY,
];

/// Mapping from newly pressed USB pad flags to Android `KeyEvent` codes when steering the stream menu.
pub const STEER_MENU_MAP: [(u32, i32); 10] = [
    (button::UP, keycode::DPAD_UP),
    (button::DOWN, keycode::DPAD_DOWN),
    (button::LEFT, keycode::DPAD_LEFT),
    (button::RIGHT, keycode::DPAD_RIGHT),
    (button::A, keycode::DPAD_CENTER),
    (button::B, keycode::BUTTON_B),
    (button::Y, keycode::BUTTON_Y),
    (button::X, keycode::BUTTON_X),
    (button::LB, keycode::BUTTON_L1),
    (button::RB, keycode::BUTTON_R1),
];

/// Android quit-combo button mask; `"off"`, `""`, `"start_select_lb_rb"`, and unknown strings map to `0`
/// because `Back | Start | LB | RB` opens Android's stream menu.
pub fn quit_combo_mask(combo: &str) -> u32 {
    let shoulders = button::LB | button::RB;
    match combo {
        "select_lb_rb_y" => shoulders | button::BACK | button::Y,
        "start_lb_rb_a" => shoulders | button::PLAY | button::A,
        "start_lb_rb_b" => shoulders | button::PLAY | button::B,
        "lb_rb_x_y" => shoulders | button::X | button::Y,
        "lb_rb_a_b" => shoulders | button::A | button::B,
        _ => 0,
    }
}

/// Physical Android `KeyEvent` keycode mapped to its quit-combo flag (`0` if unused by combos).
pub fn quit_button_flag(key_code: i32) -> u32 {
    match key_code {
        keycode::BUTTON_START | keycode::MENU => button::PLAY,
        keycode::BUTTON_SELECT | keycode::BACK => button::BACK,
        keycode::BUTTON_L1 => button::LB,
        keycode::BUTTON_R1 => button::RB,
        keycode::BUTTON_A => button::A,
        keycode::BUTTON_B => button::B,
        keycode::BUTTON_X => button::X,
        keycode::BUTTON_Y => button::Y,
        keycode::BUTTON_THUMBL => button::LS_CLK,
        keycode::BUTTON_THUMBR => button::RS_CLK,
        keycode::BUTTON_MODE => button::SPECIAL,
        keycode::DPAD_UP => button::UP,
        keycode::DPAD_DOWN => button::DOWN,
        keycode::DPAD_LEFT => button::LEFT,
        keycode::DPAD_RIGHT => button::RIGHT,
        _ => 0,
    }
}

/// Arms `quit_combo_pending` when `combo` is active and the physical `buttons` match it exactly.
pub fn hold_quit_combo(pending: bool, buttons: u32, combo: &str) -> bool {
    let mask = quit_combo_mask(combo);
    pending || (mask != 0 && buttons == mask)
}

/// Returns `(new_pending, should_leave)` once all physical buttons have been released.
pub fn release_quit_combo(pending: bool, buttons: u32) -> (bool, bool) {
    if pending && buttons == 0 {
        (false, true)
    } else {
        (pending, false)
    }
}

/// Whether the currently held buttons arm Android's stream-menu combo.
pub fn should_arm_menu_combo(held_buttons: u32) -> bool {
    held_buttons == MENU_COMBO_MASK
}

/// Whether releasing buttons after `pending_exit` should open the stream menu now.
pub fn should_open_menu_on_release(
    pending_exit: bool,
    input_map: u32,
    pressed_empty: bool,
) -> bool {
    pending_exit && input_map == 0 && pressed_empty
}

/// Remaining milliseconds to hold a short press so games see at least `MINIMUM_BUTTON_DOWN_TIME_MS`.
pub fn tap_hold_remaining_ms(down_time_ms: i64, event_time_ms: i64) -> i64 {
    MINIMUM_BUTTON_DOWN_TIME_MS.saturating_sub(event_time_ms.saturating_sub(down_time_ms))
}

/// Whether releasing Start after `start_down_time_ms` toggles mouse emulation (`> 750 ms`).
pub fn start_long_press_toggles_mouse(
    start_down_time_ms: i64,
    event_time_ms: i64,
    mouse_emulation_enabled: bool,
) -> bool {
    mouse_emulation_enabled
        && event_time_ms.saturating_sub(start_down_time_ms) > START_DOWN_TIME_MOUSE_MODE_MS
}

/// Outcome of releasing Guide under `GuideHold`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideRelease {
    Pass = 0,
    Tap = 1,
    Swallow = 2,
}

/// Operation code passed across JNI to [`guide_hold_step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuideOp {
    Down = 0,
    Expire = 1,
    Chord = 2,
    Up = 3,
    CancelPending = 4,
    Reset = 5,
}

impl GuideOp {
    /// Decodes a JNI `jint` opcode.
    pub fn from_i32(op: i32) -> Option<Self> {
        match op {
            0 => Some(Self::Down),
            1 => Some(Self::Expire),
            2 => Some(Self::Chord),
            3 => Some(Self::Up),
            4 => Some(Self::CancelPending),
            5 => Some(Self::Reset),
            _ => None,
        }
    }
}

/// Hold-Guide state for one controller (`pending` waiting on timer, `swallowing` until release).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuideHoldState {
    pub pending: bool,
    pub swallowing: bool,
}

impl GuideHoldState {
    pub fn to_bits(self) -> u8 {
        u8::from(self.pending) | (u8::from(self.swallowing) << 1)
    }

    pub fn from_bits(bits: u8) -> Self {
        Self {
            pending: (bits & 1) != 0,
            swallowing: (bits & 2) != 0,
        }
    }

    pub fn active(self) -> bool {
        self.pending || self.swallowing
    }

    pub fn down(&mut self, hold_ms: i64, repeat: bool) -> bool {
        if repeat && self.active() {
            return true;
        }
        self.reset();
        if hold_ms <= 0 {
            return false;
        }
        self.pending = true;
        true
    }

    pub fn expire(&mut self) -> bool {
        if !self.pending {
            return false;
        }
        self.pending = false;
        self.swallowing = true;
        true
    }

    pub fn chord(&mut self) -> bool {
        if !self.pending {
            return false;
        }
        self.pending = false;
        true
    }

    pub fn up(&mut self) -> GuideRelease {
        if self.pending {
            self.pending = false;
            GuideRelease::Tap
        } else if self.swallowing {
            self.swallowing = false;
            GuideRelease::Swallow
        } else {
            GuideRelease::Pass
        }
    }

    pub fn cancel_pending(&mut self) -> bool {
        if !self.pending {
            return false;
        }
        self.pending = false;
        self.swallowing = true;
        true
    }

    pub fn reset(&mut self) {
        self.pending = false;
        self.swallowing = false;
    }
}

/// Executes one [`GuideOp`] on packed `state_bits`, returning `(new_bits, op_result)`.
pub fn guide_hold_step(state_bits: u8, op: GuideOp, hold_ms: i64, repeat: bool) -> (u8, i32) {
    let mut state = GuideHoldState::from_bits(state_bits);
    let result = match op {
        GuideOp::Down => i32::from(state.down(hold_ms, repeat)),
        GuideOp::Expire => i32::from(state.expire()),
        GuideOp::Chord => i32::from(state.chord()),
        GuideOp::Up => state.up() as i32,
        GuideOp::CancelPending => i32::from(state.cancel_pending()),
        GuideOp::Reset => {
            state.reset();
            0
        }
    };
    (state.to_bits(), result)
}

/// Result of stepping USB Guide-hold logic on one driver report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbGuideStep {
    pub guide_down: bool,
    pub guide_hold_bits: u8,
    pub guide_tapping: bool,
    pub out_buttons: u32,
    pub schedule_hold: bool,
    pub cancel_hold: bool,
}

/// Updates USB Guide-hold state and filters `SPECIAL` on one USB report.
pub fn usb_guide_hold_step(
    mut guide_down: bool,
    guide_hold_bits: u8,
    mut guide_tapping: bool,
    button_flags: u32,
    hold_ms: i64,
    can_hold: bool,
) -> UsbGuideStep {
    let mut hold = GuideHoldState::from_bits(guide_hold_bits);
    let guide = (button_flags & button::SPECIAL) != 0;
    let mut schedule_hold = false;
    let mut cancel_hold = false;

    if guide != guide_down {
        guide_down = guide;
        if guide {
            if can_hold {
                if hold.down(hold_ms, false) {
                    schedule_hold = true;
                } else {
                    cancel_hold = true;
                }
            }
        } else {
            cancel_hold = true;
            if hold.up() == GuideRelease::Tap {
                guide_tapping = true;
            }
        }
    }

    if guide && (button_flags & !button::SPECIAL) != 0 && hold.chord() {
        schedule_hold = false;
        cancel_hold = true;
    }

    let mut out_buttons = button_flags;
    if hold.active() {
        out_buttons &= !button::SPECIAL;
    }
    if guide_tapping {
        out_buttons |= button::SPECIAL;
    }

    UsbGuideStep {
        guide_down,
        guide_hold_bits: hold.to_bits(),
        guide_tapping,
        out_buttons,
        schedule_hold,
        cancel_hold,
    }
}

/// Which stick scrolls in controller mouse mode (`PreferenceConfiguration.AnalogStickForScrolling`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollStick {
    None = 0,
    Right = 1,
    Left = 2,
}

impl ScrollStick {
    pub fn from_i32(mode: i32) -> Self {
        match mode {
            1 => Self::Right,
            2 => Self::Left,
            _ => Self::None,
        }
    }
}

/// Whether controller mouse emulation is silenced because `Controllers` is Off for a physical pad.
pub fn mouse_silenced(send_controllers: bool, is_default_context: bool) -> bool {
    !send_controllers && !is_default_context
}

/// Picks the pad index to turn mouse mode on for (`None` if any context is already active or none
/// are eligible assigned pads).
pub fn pick_menu_mouse_target(contexts: &[(bool, i16, bool)]) -> Option<usize> {
    if contexts.iter().any(|&(_, _, active)| active) {
        return None;
    }
    let mut best: Option<(usize, i16)> = None;
    for (idx, &(eligible_assigned, number, _)) in contexts.iter().enumerate() {
        if !eligible_assigned {
            continue;
        }
        if best.is_none_or(|(_, best_num)| number < best_num) {
            best = Some((idx, number));
        }
    }
    best.map(|(idx, _)| idx)
}

/// Button/scroll/step actions emitted by one mouse-emulation controller packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MousePacketPlan {
    pub last_input_map: u32,
    pub fixed_step_held: bool,
    pub step: i32,
    pub step_changed: bool,
    /// `1` = press, `-1` = release, `0` = unchanged.
    pub left_btn: i8,
    /// `1` = press, `-1` = release, `0` = unchanged.
    pub right_btn: i8,
    pub scroll_up: bool,
    pub scroll_down: bool,
    pub hscroll_right: bool,
    pub hscroll_left: bool,
}

/// Computes mouse-button, discrete D-pad scroll, and fixed-step actions for `input_map`.
pub fn mouse_packet_step(
    input_map: u32,
    last_input_map: u32,
    fixed_step_enabled: bool,
    current_step: i32,
) -> MousePacketPlan {
    let changed = input_map ^ last_input_map;
    let a_down = (input_map & button::A) != 0;
    let b_down = (input_map & button::B) != 0;

    let fixed_step_held = fixed_step_enabled && (input_map & button::X) != 0;
    let step_changed = fixed_step_enabled && (changed & input_map & button::Y) != 0;
    let step = if step_changed {
        fixed_step_next(current_step)
    } else {
        current_step
    };

    let left_btn = if (changed & button::A) != 0 {
        if a_down { 1 } else { -1 }
    } else {
        0
    };
    let right_btn = if (changed & button::B) != 0 {
        if b_down { 1 } else { -1 }
    } else {
        0
    };

    MousePacketPlan {
        last_input_map: input_map,
        fixed_step_held,
        step,
        step_changed,
        left_btn,
        right_btn,
        scroll_up: (changed & input_map & button::UP) != 0,
        scroll_down: (changed & input_map & button::DOWN) != 0,
        hscroll_right: (changed & input_map & button::RIGHT) != 0,
        hscroll_left: (changed & input_map & button::LEFT) != 0,
    }
}

/// Pointer moves and high-res scroll deltas emitted on one mouse-emulation tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseTickPlan {
    pub has_move1: bool,
    pub move1_x: f32,
    pub move1_y: f32,
    pub has_move2: bool,
    pub move2_x: f32,
    pub move2_y: f32,
    pub has_scroll: bool,
    pub scroll_v: f32,
    pub scroll_h: f32,
}

fn stick_move(x: i16, y: i16, fixed_step: i32, sensitivity_percent: i32) -> (bool, f32, f32) {
    let (vx, vy) = mouse_stick_curve(x, y);
    if vx == 0.0 && vy == 0.0 {
        return (false, 0.0, 0.0);
    }
    let scale = f64::from(MOUSE_EMULATION_TIME_COMPENSATION)
        * (f64::from(clamp_mouse_sensitivity_percent(sensitivity_percent)) / 100.0);
    if fixed_step > 0 {
        let dx = (f64::from(fixed_step_delta(f64::from(vx), fixed_step)) * scale) as f32;
        let dy = (f64::from(fixed_step_delta(f64::from(-vy), fixed_step)) * scale) as f32;
        (dx != 0.0 || dy != 0.0, dx, dy)
    } else {
        let dx = (f64::from(vx) * scale) as f32;
        let dy = (f64::from(-vy) * scale) as f32;
        (dx != 0.0 || dy != 0.0, dx, dy)
    }
}

fn stick_scroll(x: i16, y: i16) -> (bool, f32, f32) {
    let (vx, vy) = mouse_stick_curve(x, y);
    let mag = (f64::from(vx).powi(2) + f64::from(vy).powi(2)).sqrt();
    if mag < 1.0 {
        return (false, 0.0, 0.0);
    }
    let comp = f64::from(MOUSE_EMULATION_TIME_COMPENSATION);
    (true, (f64::from(vy) * comp) as f32, (f64::from(vx) * comp) as f32)
}

/// Computes mouse move and high-res scroll deltas for both sticks on one 10 ms tick.
pub fn mouse_tick_step(
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
    scroll_stick: ScrollStick,
    fixed_step: i32,
    sensitivity_percent: i32,
) -> MouseTickPlan {
    match scroll_stick {
        ScrollStick::Right => {
            let (has_move1, move1_x, move1_y) = stick_move(lx, ly, fixed_step, sensitivity_percent);
            let (has_scroll, scroll_v, scroll_h) = stick_scroll(rx, ry);
            MouseTickPlan {
                has_move1,
                move1_x,
                move1_y,
                has_scroll,
                scroll_v,
                scroll_h,
                ..Default::default()
            }
        }
        ScrollStick::Left => {
            let (has_move1, move1_x, move1_y) = stick_move(rx, ry, fixed_step, sensitivity_percent);
            let (has_scroll, scroll_v, scroll_h) = stick_scroll(lx, ly);
            MouseTickPlan {
                has_move1,
                move1_x,
                move1_y,
                has_scroll,
                scroll_v,
                scroll_h,
                ..Default::default()
            }
        }
        ScrollStick::None => {
            let (has_move1, move1_x, move1_y) = stick_move(lx, ly, fixed_step, sensitivity_percent);
            let (has_move2, move2_x, move2_y) = stick_move(rx, ry, fixed_step, sensitivity_percent);
            MouseTickPlan {
                has_move1,
                move1_x,
                move1_y,
                has_move2,
                move2_x,
                move2_y,
                ..Default::default()
            }
        }
    }
}

/// Hysteresis stick direction matching `PcKeyboardController.stickDirection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StickDirection {
    Up = 1,
    Down = 2,
    Left = 3,
    Right = 4,
}

/// Computes left-stick direction with 0.6 engage / 0.35 release hysteresis (`y` grows downward).
pub fn stick_direction(x: f32, y: f32, current: Option<StickDirection>) -> Option<StickDirection> {
    let ax = x.abs();
    let ay = y.abs();
    let threshold = if current.is_none() { 0.6f32 } else { 0.35f32 };
    if ax.max(ay) < threshold {
        return None;
    }
    if ax > ay {
        Some(if x < 0.0 {
            StickDirection::Left
        } else {
            StickDirection::Right
        })
    } else {
        Some(if y < 0.0 {
            StickDirection::Up
        } else {
            StickDirection::Down
        })
    }
}

/// Per-USB-pad menu-steering, keyboard-capture, and `heldFromMenu` suppression state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UsbMenuCaptureState {
    pub steering_menu: bool,
    pub capture_stick: Option<StickDirection>,
    pub capture_generation: i32,
    pub menu_buttons: u32,
    pub held_from_menu: u32,
    pub last_buttons: u32,
    pub trigger_held: [Option<bool>; 2],
    pub guide_down: bool,
    pub combo_armed: bool,
}

/// Result of one [`UsbInputStates::capture_pad`] call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturePadOutcome {
    pub entered: bool,
    pub changed_mask: u32,
    pub new_buttons: u32,
    pub stick_changed: bool,
}

/// Collection of USB pad menu/capture states owned per `ControllerHandler` via `MutexSlab`.
#[derive(Clone, Debug, Default)]
pub struct UsbInputStates {
    pads: BTreeMap<i32, UsbMenuCaptureState>,
}

impl UsbInputStates {
    pub fn steer_menu(&mut self, id: i32, buttons: u32) -> (bool, [i32; 13], usize) {
        self.steer_menu_with_triggers(id, buttons, 0.0, 0.0)
    }

    pub fn steer_menu_with_triggers(
        &mut self,
        id: i32,
        buttons: u32,
        left_trigger: f32,
        right_trigger: f32,
    ) -> (bool, [i32; 13], usize) {
        let entry = self.pads.entry(id).or_default();
        entry.capture_stick = None;
        let mut clear_combos = false;
        if !entry.steering_menu {
            entry.steering_menu = true;
            // A Guide hold that opened the menu is still down on the first report: ignore its release.
            entry.menu_buttons = entry.last_buttons | (buttons & button::SPECIAL);
            entry.trigger_held = [None; 2];
            entry.guide_down = false;
            entry.combo_armed = false;
            clear_combos = true;
        }
        let pressed = buttons & !entry.menu_buttons;
        let released = entry.menu_buttons & !buttons;
        entry.menu_buttons = buttons;

        let mut keys = [0i32; 13];
        let mut count = 0usize;
        if (pressed & button::SPECIAL) != 0 && (buttons & !button::SPECIAL) == 0 {
            entry.guide_down = true;
        }
        let mut close_menu = false;
        if entry.guide_down && (released & button::SPECIAL) != 0 {
            entry.guide_down = false;
            close_menu = true;
        } else if (buttons & !button::SPECIAL) != 0 {
            entry.guide_down = false;
        }
        let chording = (buttons & (button::PLAY | button::BACK)) != 0;
        if (buttons & MENU_COMBO_MASK) == MENU_COMBO_MASK {
            entry.combo_armed = true;
        } else if entry.combo_armed && (buttons & MENU_COMBO_MASK) == 0 {
            entry.combo_armed = false;
            close_menu = true;
        }
        if pressed != 0 {
            for &(flag, key_code) in &STEER_MENU_MAP {
                if (flag == button::LB || flag == button::RB) && (chording || entry.combo_armed) {
                    continue;
                }
                if (pressed & flag) != 0 {
                    keys[count] = key_code;
                    count += 1;
                }
            }
        }
        for (side, value, code) in [
            (0usize, left_trigger, keycode::BUTTON_L2),
            (1usize, right_trigger, keycode::BUTTON_R2),
        ] {
            match entry.trigger_held[side] {
                None => entry.trigger_held[side] = Some(value >= 0.5),
                Some(false) if value >= 0.5 => {
                    entry.trigger_held[side] = Some(true);
                    keys[count] = code;
                    count += 1;
                }
                Some(true) if value <= 0.25 => entry.trigger_held[side] = Some(false),
                _ => {}
            }
        }
        if close_menu {
            keys[count] = keycode::BUTTON_MODE;
            count += 1;
        }
        (clear_combos, keys, count)
    }

    pub fn capture_pad(
        &mut self,
        id: i32,
        buttons: u32,
        stick_x: f32,
        stick_y: f32,
        generation: i32,
    ) -> CapturePadOutcome {
        let entry = self.pads.entry(id).or_default();
        let mut entered = false;
        if !entry.steering_menu {
            entered = true;
            entry.steering_menu = true;
            entry.menu_buttons = buttons | entry.last_buttons;
            entry.capture_stick = None;
            entry.capture_generation = generation;
        }
        if entry.capture_generation != generation {
            entry.capture_generation = generation;
            entry.capture_stick = None;
        }
        let changed_mask = buttons ^ entry.menu_buttons;
        entry.menu_buttons = buttons;

        let dir = stick_direction(stick_x, stick_y, entry.capture_stick);
        let stick_changed = dir != entry.capture_stick;
        if stick_changed {
            entry.capture_stick = dir;
        }

        CapturePadOutcome {
            entered,
            changed_mask,
            new_buttons: buttons,
            stick_changed,
        }
    }

    pub fn filter_game_buttons(&mut self, id: i32, buttons: u32) -> u32 {
        let entry = self.pads.entry(id).or_default();
        if entry.steering_menu {
            entry.steering_menu = false;
            entry.capture_stick = None;
            entry.held_from_menu = entry.menu_buttons;
        }
        entry.held_from_menu &= buttons;
        let filtered = buttons & !entry.held_from_menu;
        entry.last_buttons = filtered;
        filtered
    }

    pub fn remove_pad(&mut self, id: i32) -> (bool, u32, bool) {
        if let Some(entry) = self.pads.remove(&id) {
            (
                entry.steering_menu,
                entry.menu_buttons,
                entry.capture_stick.is_some(),
            )
        } else {
            (false, 0, false)
        }
    }
}

#[cfg(test)]
#[path = "state_machine_tests.rs"]
mod tests;
