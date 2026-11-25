//! Unit tests for `controller::mapping`, `controller::combos`, and `controller::maps`.

use super::*;
use crate::controller::controller_type;

#[test]
fn android_map_covers_all_23_buttons() {
    let all = supported_buttons_from_keys(&[true; 23]);
    assert_ne!(all & button::A, 0);
    assert_ne!(all & button::TOUCHPAD, 0);
    assert_ne!(all & button::MISC, 0);
    assert_ne!(all & button::SPECIAL, 0);
    assert_eq!(
        key_code_to_controller_flag(keycode::UNKNOWN, 0x2c4, true),
        button::PADDLE1
    );
    assert_eq!(
        key_code_to_controller_flag(keycode::UNKNOWN, 0x2c4, false),
        0
    );
}

#[test]
fn remap_handles_all_device_quirks() {
    let r = remap_key_event(
        true,
        keycode::BACK,
        0,
        0,
        true,
        0,
        0,
        34,
        quirk::IGNORE_BACK,
    );
    assert_eq!(r.key_code, keycode::BUTTON_MODE);

    let r = remap_key_event(
        false,
        keycode::BUTTON_SELECT,
        317,
        0,
        true,
        0x054c,
        0x09cc,
        34,
        0,
    );
    assert_eq!(r.key_code, keycode::BUTTON_1);

    // 8BitDo BTN_C (scan code 306) must stay remappable instead of forcing Guide.
    let r = remap_key_event(false, keycode::UNKNOWN, 306, 0, true, 0x2dc8, 0, 34, 0);
    assert_eq!(r.key_code, keycode::UNKNOWN);
    let r_btn_c = remap_key_event(false, keycode::BUTTON_C, 306, 0, true, 0x2dc8, 0, 34, 0);
    assert_eq!(r_btn_c.key_code, keycode::BUTTON_C);

    let r = remap_key_event(
        false,
        keycode::UNKNOWN,
        0x131,
        0,
        true,
        0x057e,
        0x2009,
        28,
        0,
    );
    assert_eq!(r.key_code, keycode::BUTTON_B);
    let r_hori = remap_key_event(
        false,
        keycode::UNKNOWN,
        0x13d,
        0,
        true,
        0x0f0d,
        0x00c1,
        34,
        0,
    );
    assert_eq!(r_hori.key_code, keycode::BUTTON_MODE);

    let r = remap_key_event(false, keycode::UNKNOWN, 265, 0, true, 0x0b05, 0x7902, 34, 0);
    assert_eq!(r.key_code, keycode::BUTTON_SELECT);

    let r = remap_key_event(
        false,
        keycode::UNKNOWN,
        704,
        0,
        true,
        0,
        0,
        34,
        quirk::NO_HAT,
    );
    assert_eq!(r.key_code, keycode::DPAD_LEFT);

    let r = remap_key_event(
        false,
        keycode::BACK,
        0,
        keycode::FLAG_SOFT_KEYBOARD,
        false,
        0,
        0,
        34,
        quirk::BACK_IS_START,
    );
    assert_eq!(r.key_code, keycode::BUTTON_B);

    let r = remap_key_event(
        false,
        keycode::BUTTON_START,
        0,
        0,
        true,
        0,
        0,
        34,
        quirk::BACK_IS_START,
    );
    assert_eq!(r.key_code, keycode::BUTTON_START);
    assert!(r.clear_back_is_start);

    let r = remap_key_event(
        false,
        keycode::UNKNOWN,
        999,
        0,
        true,
        0x054c,
        0,
        34,
        quirk::NON_STD_DS4,
    );
    assert_eq!(r.key_code, REMAP_CONSUME);
}

#[test]
fn mapped_buttons_and_emulated_combos_and_usb_clickpad() {
    let mut pressed = BTreeMap::new();
    assert!(update_mapped_button_map(
        &mut pressed,
        304,
        keycode::BUTTON_B,
        true
    ));
    assert!(!update_mapped_button_map(
        &mut pressed,
        305,
        keycode::BUTTON_B,
        true
    ));
    assert!(!update_mapped_button_map(
        &mut pressed,
        304,
        keycode::BUTTON_B,
        false
    ));
    assert!(update_mapped_button_map(
        &mut pressed,
        305,
        keycode::BUTTON_B,
        false
    ));
    assert!(pressed.is_empty());

    let (map, emu) =
        apply_emulated_combos_on_down(button::BACK | button::LB, 0, true, true, true, 200, 0, 0);
    assert_eq!(map, button::TOUCHPAD);
    assert_eq!(emu, EMULATING_TOUCHPAD);
    assert_eq!(
        held_buttons(map, emu, false, true, true),
        button::BACK | button::LB
    );

    let (map_up, emu_up) = apply_emulated_combos_on_up(map, emu, false, false, true);
    assert_eq!(map_up, 0);
    assert_eq!(emu_up, 0);

    let (out, swallowed) = apply_usb_clickpad(
        button::BACK | button::LB,
        0,
        controller_type::XBOX,
        controller_type::PS,
        false,
        true,
    );
    assert_eq!(out, button::TOUCHPAD);
    assert_eq!(swallowed, button::BACK | button::LB);
    let (out2, swallowed2) = apply_usb_clickpad(
        button::LB,
        swallowed,
        controller_type::XBOX,
        controller_type::PS,
        false,
        true,
    );
    assert_eq!(out2, 0);
    assert_eq!(swallowed2, button::LB);
}

#[test]
fn shared_preferences_codecs_round_trip() {
    let mut bm = ButtonMap::default();
    bm.mappings.insert(keycode::BUTTON_A, keycode::BUTTON_B);
    bm.mappings
        .insert(keycode::BUTTON_C, keycode::TARGET_PADDLE1);
    bm.mappings.insert(0x30132, keycode::BUTTON_MODE);
    bm.mappings.insert(0x30135, keycode::TARGET_PADDLE2);
    bm.mappings.insert(0x302c0, keycode::TARGET_PADDLE3);
    bm.mappings.insert(0x302c1, keycode::TARGET_PADDLE4);
    bm.connect_as = controller_type::PS;
    let s = bm.serialize();
    assert_eq!(ButtonMap::deserialize(Some(&s)), bm);
    assert!(is_remappable_source(0x30120));
    assert!(is_remappable_source(0x3013f));
    assert!(is_remappable_source(0x302c0));
    assert!(is_remappable_source(0x302cf));
    assert!(!is_remappable_source(0x3011f));
    assert!(!is_remappable_source(0x302d0));

    let mut am = AxisMap::default();
    am.axes[0] = -2;
    am.dpad[0] = button::A;
    let sa = am.serialize();
    assert_eq!(AxisMap::deserialize(Some(&sa)), am);
}

#[test]
fn axis_map_deserialize_handles_i32_min_and_disabled_zero() {
    assert_eq!(
        AxisMap::deserialize(Some("1,-2147483648,2,3,4,5,6,1,2,4,8")),
        AxisMap::default()
    );

    let disabled_x = AxisMap::deserialize(Some("1,0,2,3,4,5,6,1,2,4,8"));
    assert_eq!(disabled_x.axes[0], 0);
}

#[test]
fn emulated_combos_wrapping_timestamp_delta_does_not_panic() {
    let (map_sel, emu_sel) =
        apply_emulated_combos_on_down(button::PLAY, 0, false, true, false, i64::MAX, -10, 0);
    assert_eq!(map_sel, button::BACK);
    assert_eq!(emu_sel, EMULATING_SELECT);

    let (map_tp, emu_tp) =
        apply_emulated_combos_on_down(button::BACK, 0, true, true, true, i64::MAX, -10, 0);
    assert_eq!(map_tp, button::TOUCHPAD);
    assert_eq!(emu_tp, EMULATING_TOUCHPAD);

    let (map_sp, emu_sp) =
        apply_emulated_combos_on_down(button::PLAY, 0, false, false, false, i64::MAX, 0, -10);
    assert_eq!(map_sp, button::SPECIAL);
    assert_eq!(emu_sp, EMULATING_SPECIAL);
}
