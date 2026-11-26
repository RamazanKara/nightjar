//! SharedPreferences serialization codecs matching Android's `ControllerButtonMap`
//! (`"k:v,..."` + `0x20000` connect-as) and `ControllerAxisMap` (`"1,axes[6],dpad[4]"`).

use std::collections::BTreeMap;

use crate::controller::controller_type;
use crate::controller::mapping::{
    MAPPABLE_TARGETS, flip_face_buttons, key_code_to_controller_flag, keycode,
};
use crate::gamepad_constants::button;

/// USB button-flag sources remapped by `ControllerButtonMap.FLAG_SOURCES`.
const FLAG_SOURCES: [i32; 12] = [
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
    keycode::MEDIA_RECORD,
];

/// Rust codec matching `ControllerButtonMap` `SharedPreferences` serialization.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ButtonMap {
    pub mappings: BTreeMap<i32, i32>,
    pub connect_as: u8,
}

impl ButtonMap {
    pub fn target_button_flags(&self) -> u32 {
        let mut flags = 0u32;
        for &target in self.mappings.values() {
            if (keycode::TARGET_TOUCHPAD..=keycode::TARGET_PADDLE4).contains(&target) {
                flags |= key_code_to_controller_flag(target, 0, false);
            }
        }
        flags
    }

    pub fn map_button_flags(&self, flags: u32, flip_face: bool) -> u32 {
        let mut result = flags;
        for &src in &FLAG_SOURCES {
            result &= !key_code_to_controller_flag(src, 0, false);
        }
        for &src in &FLAG_SOURCES {
            if (flags & key_code_to_controller_flag(src, 0, false)) == 0 {
                continue;
            }
            let target = match self.mappings.get(&src).copied() {
                Some(t) => t,
                None => {
                    if flip_face {
                        flip_face_buttons(src)
                    } else {
                        src
                    }
                }
            };
            result |= key_code_to_controller_flag(target, 0, false);
        }
        result
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        for (k, v) in &self.mappings {
            if !out.is_empty() {
                out.push(',');
            }
            out.push_str(&format!("{k}:{v}"));
        }
        if self.connect_as != controller_type::UNKNOWN {
            if !out.is_empty() {
                out.push(',');
            }
            out.push_str(&format!(
                "{}:{}",
                keycode::CONNECT_AS_ENTRY,
                self.connect_as
            ));
        }
        out
    }

    pub fn deserialize(value: Option<&str>) -> Self {
        let mut map = Self::default();
        let Some(value) = value.filter(|v| !v.is_empty()) else {
            return map;
        };
        for entry in value.split(',') {
            let mut parts = entry.split(':');
            let (Some(k), Some(v), None) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let (Ok(src), Ok(dst)) = (k.trim().parse::<i32>(), v.trim().parse::<i32>()) else {
                continue;
            };
            if src == keycode::CONNECT_AS_ENTRY {
                if let Ok(ca) = u8::try_from(dst)
                    && matches!(
                        ca,
                        controller_type::UNKNOWN
                            | controller_type::XBOX
                            | controller_type::PS
                            | controller_type::NINTENDO
                    )
                {
                    map.connect_as = ca;
                }
            } else if is_remappable_source(src) && MAPPABLE_TARGETS.contains(&dst) {
                if src == dst {
                    map.mappings.remove(&src);
                } else {
                    map.mappings.insert(src, dst);
                }
            }
        }
        map
    }
}

pub fn is_remappable_source(key_code: i32) -> bool {
    ((keycode::BUTTON_A..=keycode::BUTTON_MODE).contains(&key_code)
        && key_code != keycode::BUTTON_L2
        && key_code != keycode::BUTTON_R2)
        || (keycode::BUTTON_1..=keycode::BUTTON_16).contains(&key_code)
        || key_code == keycode::MEDIA_RECORD
        || (0x30120..=0x3013f).contains(&key_code)
        || (0x302c0..=0x302cf).contains(&key_code)
}

/// Rust codec matching `ControllerAxisMap` `SharedPreferences` serialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AxisMap {
    pub axes: [i32; 6],
    pub dpad: [u32; 4],
}

impl Default for AxisMap {
    fn default() -> Self {
        Self {
            axes: [1, 2, 3, 4, 5, 6],
            dpad: [button::UP, button::DOWN, button::LEFT, button::RIGHT],
        }
    }
}

impl AxisMap {
    pub fn serialize(&self) -> String {
        let mut out = String::from("1");
        for v in self.axes {
            out.push_str(&format!(",{v}"));
        }
        for v in self.dpad {
            out.push_str(&format!(",{v}"));
        }
        out
    }

    pub fn deserialize(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        let parts: Vec<&str> = value.split(',').collect();
        if parts.len() != 11 || parts[0] != "1" {
            return Self::default();
        }
        let mut out = Self::default();
        for i in 0..6 {
            let Ok(entry) = parts[i + 1].parse::<i32>() else {
                return Self::default();
            };
            let Some(abs_entry) = entry.checked_abs() else {
                return Self::default();
            };
            let src = abs_entry - 1;
            let inv = entry < 0;
            if !(-1..6).contains(&src) || (src >= 0 && (i < 4) != (src < 4)) || (i >= 4 && inv) {
                return Self::default();
            }
            out.axes[i] = if src < 0 {
                0
            } else {
                (src + 1) * if inv { -1 } else { 1 }
            };
        }
        for i in 0..4 {
            let Ok(target) = parts[i + 7].parse::<u32>() else {
                return Self::default();
            };
            if !matches!(
                target,
                0 | button::UP
                    | button::DOWN
                    | button::LEFT
                    | button::RIGHT
                    | button::A
                    | button::B
                    | button::X
                    | button::Y
                    | button::LB
                    | button::RB
                    | button::LS_CLK
                    | button::RS_CLK
                    | button::PLAY
                    | button::BACK
                    | button::SPECIAL
            ) {
                return Self::default();
            }
            out.dpad[i] = target;
        }
        out
    }
}
