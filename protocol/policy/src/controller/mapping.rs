//! Android key/scancode quirk remapping, button flag translation, emulated combos
//! (Select, Special, Touchpad), USB clickpad emulation, and SharedPreferences button/axis map
//! codecs matching `ControllerHandler`, `ControllerButtonMap`, and `ControllerAxisMap`.

use std::collections::BTreeMap;

use crate::gamepad_constants::button;

pub use super::combos::{
    apply_emulated_combos_on_down, apply_emulated_combos_on_up, apply_usb_clickpad, held_buttons,
};
pub use super::maps::{AxisMap, ButtonMap, is_remappable_source};

/// Android `KeyEvent` keycodes, flags, and extended `ControllerButtonMap` targets.
pub mod keycode {
    pub const UNKNOWN: i32 = 0;
    pub const BACK: i32 = 4;
    pub const DPAD_UP: i32 = 19;
    pub const DPAD_DOWN: i32 = 20;
    pub const DPAD_LEFT: i32 = 21;
    pub const DPAD_RIGHT: i32 = 22;
    pub const DPAD_CENTER: i32 = 23;
    pub const MENU: i32 = 82;
    pub const SEARCH: i32 = 84;
    pub const BUTTON_A: i32 = 96;
    pub const BUTTON_B: i32 = 97;
    pub const BUTTON_C: i32 = 98;
    pub const BUTTON_X: i32 = 99;
    pub const BUTTON_Y: i32 = 100;
    pub const BUTTON_Z: i32 = 101;
    pub const BUTTON_L1: i32 = 102;
    pub const BUTTON_R1: i32 = 103;
    pub const BUTTON_L2: i32 = 104;
    pub const BUTTON_R2: i32 = 105;
    pub const BUTTON_THUMBL: i32 = 106;
    pub const BUTTON_THUMBR: i32 = 107;
    pub const BUTTON_START: i32 = 108;
    pub const BUTTON_SELECT: i32 = 109;
    pub const BUTTON_MODE: i32 = 110;
    pub const MEDIA_RECORD: i32 = 130;
    pub const BUTTON_1: i32 = 188;
    pub const BUTTON_16: i32 = 203;
    pub const DPAD_UP_LEFT: i32 = 268;
    pub const DPAD_DOWN_LEFT: i32 = 269;
    pub const DPAD_UP_RIGHT: i32 = 270;
    pub const DPAD_DOWN_RIGHT: i32 = 271;

    pub const FLAG_SOFT_KEYBOARD: i32 = 0x2;
    pub const FLAG_VIRTUAL_HARD_KEY: i32 = 0x40;
    pub const FLAG_FALLBACK: i32 = 0x400;
    pub const SOURCE_CLASS_POINTER: i32 = 0x2;

    pub const DISABLED: i32 = 0;
    pub const UNMAPPED: i32 = i32::MIN;
    pub const TARGET_TOUCHPAD: i32 = 0x10001;
    pub const TARGET_MISC: i32 = 0x10002;
    pub const TARGET_PADDLE1: i32 = 0x10003;
    pub const TARGET_PADDLE2: i32 = 0x10004;
    pub const TARGET_PADDLE3: i32 = 0x10005;
    pub const TARGET_PADDLE4: i32 = 0x10006;
    pub const CONNECT_AS_ENTRY: i32 = 0x20000;
}

/// Sentinel returned by `remap_key_event` when an event should not be consumed.
pub const REMAP_IGNORE: i32 = -1;
/// Sentinel returned by `remap_key_event` when an event should be swallowed silently.
pub const REMAP_CONSUME: i32 = -2;

/// Maximum OUYA bumper-up-to-Start window (ms) for emulated Select/Special/Touchpad combos.
pub const MAXIMUM_BUMPER_UP_DELAY_MS: i64 = 100;

pub const EMULATING_SPECIAL: u32 = 0x1;
pub const EMULATING_SELECT: u32 = 0x2;
pub const EMULATING_TOUCHPAD: u32 = 0x4;

/// Bitmask of device quirk flags passed to `remap_key_event`.
pub mod quirk {
    pub const IGNORE_BACK: u32 = 1 << 0;
    pub const HAS_SHARE: u32 = 1 << 1;
    pub const DS4_STANDALONE_TOUCHPAD: u32 = 1 << 2;
    pub const LINUX_STD_FACE: u32 = 1 << 3;
    pub const NON_STD_DS4: u32 = 1 << 4;
    pub const SERVAL: u32 = 1 << 5;
    pub const NON_STD_XBOX_BT: u32 = 1 << 6;
    pub const NO_HAT: u32 = 1 << 7;
    pub const BACK_IS_START: u32 = 1 << 8;
    pub const MODE_IS_SELECT: u32 = 1 << 9;
    pub const SEARCH_IS_MODE: u32 = 1 << 10;
}

/// Canonical 23-entry mapping from Android `KeyEvent` keycodes to `ControllerPacket` flags.
pub const ANDROID_TO_LI_BUTTON_MAP: [(i32, u32); 23] = [
    (keycode::BUTTON_A, button::A),
    (keycode::BUTTON_B, button::B),
    (keycode::BUTTON_X, button::X),
    (keycode::BUTTON_Y, button::Y),
    (keycode::DPAD_UP, button::UP),
    (keycode::DPAD_DOWN, button::DOWN),
    (keycode::DPAD_LEFT, button::LEFT),
    (keycode::DPAD_RIGHT, button::RIGHT),
    (keycode::DPAD_UP_LEFT, button::UP | button::LEFT),
    (keycode::DPAD_UP_RIGHT, button::UP | button::RIGHT),
    (keycode::DPAD_DOWN_LEFT, button::DOWN | button::LEFT),
    (keycode::DPAD_DOWN_RIGHT, button::DOWN | button::RIGHT),
    (keycode::BUTTON_L1, button::LB),
    (keycode::BUTTON_R1, button::RB),
    (keycode::BUTTON_THUMBL, button::LS_CLK),
    (keycode::BUTTON_THUMBR, button::RS_CLK),
    (keycode::BUTTON_START, button::PLAY),
    (keycode::MENU, button::PLAY),
    (keycode::BUTTON_SELECT, button::BACK),
    (keycode::BACK, button::BACK),
    (keycode::BUTTON_MODE, button::SPECIAL),
    (keycode::MEDIA_RECORD, button::MISC),
    (keycode::BUTTON_1, button::TOUCHPAD),
];

/// Valid user-mappable target keycodes matching `ControllerButtonMap.TARGETS`.
pub const MAPPABLE_TARGETS: [i32; 18] = [
    keycode::BUTTON_A,
    keycode::BUTTON_B,
    keycode::BUTTON_X,
    keycode::BUTTON_Y,
    keycode::BUTTON_L1,
    keycode::BUTTON_R1,
    keycode::BUTTON_THUMBL,
    keycode::BUTTON_THUMBR,
    keycode::BUTTON_START,
    keycode::BUTTON_SELECT,
    keycode::BUTTON_MODE,
    keycode::TARGET_TOUCHPAD,
    keycode::TARGET_MISC,
    keycode::TARGET_PADDLE1,
    keycode::TARGET_PADDLE2,
    keycode::TARGET_PADDLE3,
    keycode::TARGET_PADDLE4,
    keycode::DISABLED,
];

/// Returns the supported controller button mask from a boolean slice aligned with
/// `ANDROID_TO_LI_BUTTON_MAP`.
pub fn supported_buttons_from_keys(present: &[bool]) -> u32 {
    let mut flags = 0u32;
    for ((_, flag), &has_key) in ANDROID_TO_LI_BUTTON_MAP.iter().zip(present.iter()) {
        if has_key {
            flags |= *flag;
        }
    }
    flags
}

/// Maps an Android keycode (or paddle evdev scancode) to its wire `ControllerPacket` button flag.
pub fn key_code_to_controller_flag(key_code: i32, scan_code: i32, has_paddles: bool) -> u32 {
    match key_code {
        keycode::DPAD_CENTER => button::A,
        keycode::TARGET_TOUCHPAD => button::TOUCHPAD,
        keycode::TARGET_MISC => button::MISC,
        keycode::TARGET_PADDLE1 => button::PADDLE1,
        keycode::TARGET_PADDLE2 => button::PADDLE2,
        keycode::TARGET_PADDLE3 => button::PADDLE3,
        keycode::TARGET_PADDLE4 => button::PADDLE4,
        keycode::UNKNOWN if has_paddles => match scan_code {
            0x2c4 => button::PADDLE1,
            0x2c5 => button::PADDLE2,
            0x2c6 => button::PADDLE3,
            0x2c7 => button::PADDLE4,
            _ => 0,
        },
        _ => ANDROID_TO_LI_BUTTON_MAP
            .iter()
            .find_map(|&(kc, flag)| (kc == key_code).then_some(flag))
            .unwrap_or(0),
    }
}

/// Whether `key_code` (or paddle scancode) is handled as a gamepad button up/down event.
pub fn is_handled_gamepad_key(key_code: i32, scan_code: i32, has_paddles: bool) -> bool {
    matches!(
        key_code,
        keycode::BUTTON_MODE
            | keycode::BUTTON_START
            | keycode::MENU
            | keycode::BACK
            | keycode::BUTTON_SELECT
            | keycode::DPAD_LEFT
            | keycode::DPAD_RIGHT
            | keycode::DPAD_UP
            | keycode::DPAD_DOWN
            | keycode::DPAD_UP_LEFT
            | keycode::DPAD_UP_RIGHT
            | keycode::DPAD_DOWN_LEFT
            | keycode::DPAD_DOWN_RIGHT
            | keycode::BUTTON_B
            | keycode::DPAD_CENTER
            | keycode::BUTTON_A
            | keycode::BUTTON_X
            | keycode::BUTTON_Y
            | keycode::BUTTON_L1
            | keycode::BUTTON_R1
            | keycode::BUTTON_THUMBL
            | keycode::BUTTON_THUMBR
            | keycode::MEDIA_RECORD
            | keycode::BUTTON_1
            | keycode::BUTTON_L2
            | keycode::BUTTON_R2
            | keycode::TARGET_TOUCHPAD
            | keycode::TARGET_MISC
            | keycode::TARGET_PADDLE1
            | keycode::TARGET_PADDLE2
            | keycode::TARGET_PADDLE3
            | keycode::TARGET_PADDLE4
    ) || (key_code == keycode::UNKNOWN && has_paddles && matches!(scan_code, 0x2c4..=0x2c7))
}

/// Swaps Nintendo/Xbox face buttons (`A<->B`, `X<->Y`).
pub fn flip_face_buttons(key_code: i32) -> i32 {
    match key_code {
        keycode::BUTTON_A => keycode::BUTTON_B,
        keycode::BUTTON_B => keycode::BUTTON_A,
        keycode::BUTTON_X => keycode::BUTTON_Y,
        keycode::BUTTON_Y => keycode::BUTTON_X,
        _ => key_code,
    }
}

/// Result of `remap_key_event`, including dynamic `back_is_start`/`mode_is_select` clears.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemapOutput {
    pub key_code: i32,
    pub clear_back_is_start: bool,
    pub clear_mode_is_select: bool,
}

/// Runs Android `ControllerHandler.handleRemapping` quirk rules in exact precedence order.
#[allow(clippy::too_many_arguments)]
pub fn remap_key_event(
    back_as_guide: bool,
    key_code: i32,
    scan_code: i32,
    flags: i32,
    has_no_modifiers: bool,
    vendor_id: i32,
    product_id: i32,
    sdk_int: i32,
    quirks: u32,
) -> RemapOutput {
    let mut out = RemapOutput {
        key_code,
        clear_back_is_start: false,
        clear_mode_is_select: false,
    };
    let back_is_start = (quirks & quirk::BACK_IS_START) != 0;
    if crate::controller::arrival::is_pad_back_as_guide(
        back_as_guide,
        key_code,
        flags,
        back_is_start,
    ) {
        out.key_code = keycode::BUTTON_MODE;
        return out;
    }
    if (quirks & quirk::IGNORE_BACK) != 0 && key_code == keycode::BACK {
        out.key_code = REMAP_IGNORE;
        return out;
    }
    if (quirks & quirk::HAS_SHARE) != 0 && key_code == keycode::UNKNOWN && scan_code == 167 {
        out.key_code = keycode::MEDIA_RECORD;
        return out;
    }
    if vendor_id == 0x054c
        && key_code == keycode::BUTTON_SELECT
        && (scan_code == 317 || (quirks & quirk::DS4_STANDALONE_TOUCHPAD) != 0)
    {
        out.key_code = keycode::BUTTON_1;
        return out;
    }
    if ((vendor_id == 0x057e && product_id == 0x2009 && sdk_int < 29)
        || (vendor_id == 0x0f0d && product_id == 0x00c1))
        && let Some(mapped) = match scan_code {
            0x130 => Some(keycode::BUTTON_A),
            0x131 => Some(keycode::BUTTON_B),
            0x132 => Some(keycode::BUTTON_X),
            0x133 => Some(keycode::BUTTON_Y),
            0x134 => Some(keycode::BUTTON_L1),
            0x135 => Some(keycode::BUTTON_R1),
            0x136 => Some(keycode::BUTTON_L2),
            0x137 => Some(keycode::BUTTON_R2),
            0x138 => Some(keycode::BUTTON_SELECT),
            0x139 => Some(keycode::BUTTON_START),
            0x13A => Some(keycode::BUTTON_THUMBL),
            0x13B => Some(keycode::BUTTON_THUMBR),
            0x13D => Some(keycode::BUTTON_MODE),
            _ => None,
        }
    {
        out.key_code = mapped;
        return out;
    }
    if (quirks & quirk::LINUX_STD_FACE) != 0
        && let Some(mapped) = match scan_code {
            304 => Some(keycode::BUTTON_A),
            305 => Some(keycode::BUTTON_B),
            307 => Some(keycode::BUTTON_Y),
            308 => Some(keycode::BUTTON_X),
            _ => None,
        }
    {
        out.key_code = mapped;
        return out;
    }
    if (quirks & quirk::NON_STD_DS4) != 0 {
        out.key_code = match scan_code {
            304 => keycode::BUTTON_X,
            305 => keycode::BUTTON_A,
            306 => keycode::BUTTON_B,
            307 => keycode::BUTTON_Y,
            308 => keycode::BUTTON_L1,
            309 => keycode::BUTTON_R1,
            312 => keycode::BUTTON_SELECT,
            313 => keycode::BUTTON_START,
            314 => keycode::BUTTON_THUMBL,
            315 => keycode::BUTTON_THUMBR,
            316 => keycode::BUTTON_MODE,
            _ => REMAP_CONSUME,
        };
        return out;
    } else if (quirks & quirk::SERVAL) != 0 && key_code == keycode::UNKNOWN {
        if scan_code == 314 {
            out.key_code = keycode::BUTTON_SELECT;
            return out;
        } else if scan_code == 315 {
            out.key_code = keycode::BUTTON_START;
            return out;
        }
    } else if (quirks & quirk::NON_STD_XBOX_BT) != 0 {
        if let Some(mapped) = match scan_code {
            306 => Some(keycode::BUTTON_X),
            307 => Some(keycode::BUTTON_Y),
            308 => Some(keycode::BUTTON_L1),
            309 => Some(keycode::BUTTON_R1),
            310 => Some(keycode::BUTTON_SELECT),
            311 => Some(keycode::BUTTON_START),
            312 => Some(keycode::BUTTON_THUMBL),
            313 => Some(keycode::BUTTON_THUMBR),
            139 => Some(keycode::BUTTON_MODE),
            _ => None,
        } {
            out.key_code = mapped;
            return out;
        }
        if key_code == keycode::MENU {
            out.key_code = keycode::BUTTON_MODE;
            return out;
        }
    } else if vendor_id == 0x0b05 && (product_id == 0x7900 || product_id == 0x7902) {
        match scan_code {
            264 | 266 => {
                out.key_code = keycode::BUTTON_START;
                return out;
            }
            265 | 267 => {
                out.key_code = keycode::BUTTON_SELECT;
                return out;
            }
            _ => {}
        }
    }

    if (quirks & quirk::NO_HAT) != 0
        && key_code == keycode::UNKNOWN
        && let Some(mapped) = match scan_code {
            704 => Some(keycode::DPAD_LEFT),
            705 => Some(keycode::DPAD_RIGHT),
            706 => Some(keycode::DPAD_UP),
            707 => Some(keycode::DPAD_DOWN),
            _ => None,
        }
    {
        out.key_code = mapped;
        return out;
    }

    let mut kc = key_code;
    if kc == keycode::BACK && !has_no_modifiers && (flags & keycode::FLAG_SOFT_KEYBOARD) != 0 {
        kc = keycode::BUTTON_B;
    }
    if kc == keycode::BUTTON_START || kc == keycode::MENU {
        out.clear_back_is_start = true;
    } else if kc == keycode::BUTTON_SELECT {
        out.clear_mode_is_select = true;
    } else if back_is_start && kc == keycode::BACK {
        out.key_code = keycode::BUTTON_START;
        return out;
    } else if (quirks & quirk::MODE_IS_SELECT) != 0 && kc == keycode::BUTTON_MODE {
        out.key_code = keycode::BUTTON_SELECT;
        return out;
    } else if (quirks & quirk::SEARCH_IS_MODE) != 0 && kc == keycode::SEARCH {
        out.key_code = keycode::BUTTON_MODE;
        return out;
    }
    out.key_code = kc;
    out
}

/// Normalizes a mapped button target (`MENU -> BUTTON_START`, `BACK -> BUTTON_SELECT`),
/// returning `None` when disabled or not a valid target.
pub fn normalize_mapped_target(target: i32) -> Option<i32> {
    let normalized = match target {
        keycode::MENU => keycode::BUTTON_START,
        keycode::BACK => keycode::BUTTON_SELECT,
        other => other,
    };
    if normalized == keycode::DISABLED || !MAPPABLE_TARGETS.contains(&normalized) {
        None
    } else {
        Some(normalized)
    }
}

/// Rust-native equivalent of `ControllerHandler.updateMappedButton`.
pub fn update_mapped_button_map(
    pressed: &mut BTreeMap<i32, i32>,
    physical_key: i32,
    target: i32,
    down: bool,
) -> bool {
    let Some(target) = normalize_mapped_target(target) else {
        return true;
    };
    if down {
        let already_pressed = pressed.values().any(|&v| v == target);
        pressed.insert(physical_key, target);
        !already_pressed
    } else {
        pressed.remove(&physical_key);
        !pressed.values().any(|&v| v == target)
    }
}

/// Outcome of `button_down_step` or `button_up_step`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepStatus {
    Ok = 0,
    SuppressedDuplicate = 1,
    Unhandled = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonDownOutput {
    pub status: StepStatus,
    pub input_map: u32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub set_has_select: bool,
    pub set_has_mode: bool,
    pub is_start_or_menu: bool,
}

/// Applies one key-down button update (matching `ControllerHandler.handleButtonDown` switch).
#[allow(clippy::too_many_arguments)]
pub fn button_down_step(
    mut input_map: u32,
    mut left_trigger: u8,
    mut right_trigger: u8,
    has_paddles: bool,
    hat_x_used: bool,
    hat_y_used: bool,
    lt_used: bool,
    rt_used: bool,
    key_code: i32,
    scan_code: i32,
) -> ButtonDownOutput {
    let mut set_has_select = false;
    let mut set_has_mode = false;
    let mut is_start_or_menu = false;
    let status = match key_code {
        keycode::BUTTON_MODE => {
            set_has_mode = true;
            input_map |= button::SPECIAL;
            StepStatus::Ok
        }
        keycode::BUTTON_START | keycode::MENU => {
            is_start_or_menu = true;
            input_map |= button::PLAY;
            StepStatus::Ok
        }
        keycode::BACK | keycode::BUTTON_SELECT => {
            set_has_select = true;
            input_map |= button::BACK;
            StepStatus::Ok
        }
        keycode::DPAD_LEFT if hat_x_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_RIGHT if hat_x_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_UP if hat_y_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_DOWN if hat_y_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_UP_LEFT
        | keycode::DPAD_UP_RIGHT
        | keycode::DPAD_DOWN_LEFT
        | keycode::DPAD_DOWN_RIGHT
            if hat_x_used && hat_y_used =>
        {
            StepStatus::SuppressedDuplicate
        }
        keycode::BUTTON_L2 => {
            if lt_used {
                StepStatus::SuppressedDuplicate
            } else {
                left_trigger = 0xFF;
                StepStatus::Ok
            }
        }
        keycode::BUTTON_R2 => {
            if rt_used {
                StepStatus::SuppressedDuplicate
            } else {
                right_trigger = 0xFF;
                StepStatus::Ok
            }
        }
        _ => {
            let flag = key_code_to_controller_flag(key_code, scan_code, has_paddles);
            if flag == 0 {
                StepStatus::Unhandled
            } else {
                input_map |= flag;
                StepStatus::Ok
            }
        }
    };
    ButtonDownOutput {
        status,
        input_map,
        left_trigger,
        right_trigger,
        set_has_select,
        set_has_mode,
        is_start_or_menu,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonUpOutput {
    pub status: StepStatus,
    pub input_map: u32,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub was_start_held: bool,
    pub is_l1: bool,
    pub is_r1: bool,
}

/// Applies one key-up button update (matching `ControllerHandler.finishButtonUp` switch).
#[allow(clippy::too_many_arguments)]
pub fn button_up_step(
    mut input_map: u32,
    mut left_trigger: u8,
    mut right_trigger: u8,
    has_paddles: bool,
    hat_x_used: bool,
    hat_y_used: bool,
    lt_used: bool,
    rt_used: bool,
    key_code: i32,
    scan_code: i32,
) -> ButtonUpOutput {
    let mut was_start_held = false;
    let mut is_l1 = false;
    let mut is_r1 = false;
    let status = match key_code {
        keycode::BUTTON_START | keycode::MENU => {
            was_start_held = (input_map & button::PLAY) != 0;
            input_map &= !button::PLAY;
            StepStatus::Ok
        }
        keycode::DPAD_LEFT if hat_x_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_RIGHT if hat_x_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_UP if hat_y_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_DOWN if hat_y_used => StepStatus::SuppressedDuplicate,
        keycode::DPAD_UP_LEFT
        | keycode::DPAD_UP_RIGHT
        | keycode::DPAD_DOWN_LEFT
        | keycode::DPAD_DOWN_RIGHT
            if hat_x_used && hat_y_used =>
        {
            StepStatus::SuppressedDuplicate
        }
        keycode::BUTTON_L1 => {
            is_l1 = true;
            input_map &= !button::LB;
            StepStatus::Ok
        }
        keycode::BUTTON_R1 => {
            is_r1 = true;
            input_map &= !button::RB;
            StepStatus::Ok
        }
        keycode::BUTTON_L2 => {
            if lt_used {
                StepStatus::SuppressedDuplicate
            } else {
                left_trigger = 0;
                StepStatus::Ok
            }
        }
        keycode::BUTTON_R2 => {
            if rt_used {
                StepStatus::SuppressedDuplicate
            } else {
                right_trigger = 0;
                StepStatus::Ok
            }
        }
        _ => {
            let flag = key_code_to_controller_flag(key_code, scan_code, has_paddles);
            if flag == 0 {
                StepStatus::Unhandled
            } else {
                input_map &= !flag;
                StepStatus::Ok
            }
        }
    };
    ButtonUpOutput {
        status,
        input_map,
        left_trigger,
        right_trigger,
        was_start_held,
        is_l1,
        is_r1,
    }
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
