//! JNI exports for `nightjar_policy::controller::state_machine` and mouse fixed-step curves.

use jni::JNIEnv;
use jni::objects::{JBooleanArray, JClass, JFloatArray, JIntArray, JShortArray, JString};
use jni::sys::{JNI_FALSE, jboolean, jdouble, jfloat, jint, jintArray, jlong, jshort};
use nightjar_policy::controller::deadzone::{fixed_step_delta, fixed_step_next};
use nightjar_policy::controller::state_machine::{
    CAPTURED_BUTTONS, DEFAULT_MOUSE_SENSITIVITY_PERCENT, GuideOp, MouseTickPlan, ScrollStick,
    UsbInputStates, guide_hold_step, hold_quit_combo, mouse_packet_step, mouse_silenced,
    mouse_tick_step, pick_menu_mouse_target, quit_button_flag, quit_combo_mask,
    release_quit_combo, should_arm_menu_combo, should_open_menu_on_release,
    start_long_press_toggles_mouse, tap_hold_remaining_ms, usb_guide_hold_step,
};

use super::{USB_STATE_SLAB, jbool, write_floats, write_ints};
use crate::policy::int_array;
use crate::string;

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_capturedButtonFlags(
    mut env: JNIEnv,
    _: JClass,
) -> jintArray {
    let flags: [i32; 11] = CAPTURED_BUTTONS.map(|f| f as i32);
    int_array(&mut env, &flags)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbInputStateCreate(
    _: JNIEnv,
    _: JClass,
) -> jlong {
    USB_STATE_SLAB.insert(UsbInputStates::default())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbInputStateDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    let _ = USB_STATE_SLAB.remove(handle);
}

fn usb_steer_menu_step(
    handle: jlong,
    controller_id: jint,
    buttons: jint,
    out_keys: &mut [i32; 11],
) -> (jint, usize) {
    let Some((clear_combos, keys, count)) =
        USB_STATE_SLAB.with(handle, |s| s.steer_menu(controller_id, buttons as u32))
    else {
        out_keys[0] = 0;
        return (0, 1);
    };
    out_keys[0] = i32::from(clear_combos);
    out_keys[1..=count].copy_from_slice(&keys[..count]);
    (count as jint, 1 + count)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbSteerMenuStep(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    controller_id: jint,
    buttons: jint,
    out_keys: JIntArray,
) -> jint {
    let mut buf = [0i32; 11];
    let (ret, len) = usb_steer_menu_step(handle, controller_id, buttons, &mut buf);
    write_ints(&mut env, &out_keys, &buf[..len]);
    ret
}

fn usb_capture_pad_step(
    handle: jlong,
    controller_id: jint,
    buttons: jint,
    stick_x: jfloat,
    stick_y: jfloat,
    generation: jint,
) -> [i32; 4] {
    let Some(res) = USB_STATE_SLAB.with(handle, |s| {
        s.capture_pad(controller_id, buttons as u32, stick_x, stick_y, generation)
    }) else {
        return [0, 0, 0, 0];
    };
    [
        i32::from(res.entered),
        res.changed_mask as i32,
        res.new_buttons as i32,
        i32::from(res.stick_changed),
    ]
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbCapturePadStep(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    controller_id: jint,
    buttons: jint,
    stick_x: jfloat,
    stick_y: jfloat,
    generation: jint,
    out: JIntArray,
) {
    let vals = usb_capture_pad_step(handle, controller_id, buttons, stick_x, stick_y, generation);
    write_ints(&mut env, &out, &vals);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbFilterGameButtons(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    controller_id: jint,
    buttons: jint,
) -> jint {
    USB_STATE_SLAB
        .with(handle, |s| {
            s.filter_game_buttons(controller_id, buttons as u32)
        })
        .unwrap_or(buttons as u32) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbRemovePadCapture(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    controller_id: jint,
    out: JIntArray,
) {
    let (was_steering, menu_buttons, had_stick) = USB_STATE_SLAB
        .with(handle, |s| s.remove_pad(controller_id))
        .unwrap_or((false, 0, false));
    write_ints(
        &mut env,
        &out,
        &[
            i32::from(was_steering),
            menu_buttons as i32,
            i32::from(had_stick),
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_quitComboMask(
    mut env: JNIEnv,
    _: JClass,
    combo: JString,
) -> jint {
    let combo = string(&mut env, &combo);
    quit_combo_mask(combo.as_deref().unwrap_or("")) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_quitButtonFlag(
    _: JNIEnv,
    _: JClass,
    key_code: jint,
) -> jint {
    quit_button_flag(key_code) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_holdQuitComboStep(
    mut env: JNIEnv,
    _: JClass,
    pending: jboolean,
    buttons: jint,
    combo: JString,
) -> jboolean {
    let combo = string(&mut env, &combo);
    jbool(hold_quit_combo(
        pending != JNI_FALSE,
        buttons as u32,
        combo.as_deref().unwrap_or(""),
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_releaseQuitComboStep(
    _: JNIEnv,
    _: JClass,
    pending: jboolean,
    buttons: jint,
) -> jint {
    let (new_pending, should_leave) = release_quit_combo(pending != JNI_FALSE, buttons as u32);
    i32::from(new_pending) | (i32::from(should_leave) << 1)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_shouldArmMenuCombo(
    _: JNIEnv,
    _: JClass,
    held_buttons: jint,
) -> jboolean {
    jbool(should_arm_menu_combo(held_buttons as u32))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_shouldOpenMenuOnRelease(
    _: JNIEnv,
    _: JClass,
    pending_exit: jboolean,
    input_map: jint,
    pressed_empty: jboolean,
) -> jboolean {
    jbool(should_open_menu_on_release(
        pending_exit != JNI_FALSE,
        input_map as u32,
        pressed_empty != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_tapHoldRemainingMs(
    _: JNIEnv,
    _: JClass,
    down_time_ms: jlong,
    event_time_ms: jlong,
) -> jlong {
    tap_hold_remaining_ms(down_time_ms, event_time_ms)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_startLongPressTogglesMouse(
    _: JNIEnv,
    _: JClass,
    start_down_time_ms: jlong,
    event_time_ms: jlong,
    mouse_emulation_enabled: jboolean,
) -> jboolean {
    jbool(start_long_press_toggles_mouse(
        start_down_time_ms,
        event_time_ms,
        mouse_emulation_enabled != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_guideHoldStep(
    _: JNIEnv,
    _: JClass,
    state_bits: jint,
    op: jint,
    hold_ms: jlong,
    repeat: jboolean,
) -> jint {
    let Some(op) = GuideOp::from_i32(op) else {
        return state_bits & 0x3;
    };
    let (new_bits, result) = guide_hold_step(state_bits as u8, op, hold_ms, repeat != JNI_FALSE);
    i32::from(new_bits) | (result << 8)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_usbGuideHoldStep(
    mut env: JNIEnv,
    _: JClass,
    guide_down: jboolean,
    guide_hold_bits: jint,
    guide_tapping: jboolean,
    button_flags: jint,
    hold_ms: jlong,
    can_hold: jboolean,
    out: JIntArray,
) {
    let res = usb_guide_hold_step(
        guide_down != JNI_FALSE,
        guide_hold_bits as u8,
        guide_tapping != JNI_FALSE,
        button_flags as u32,
        hold_ms,
        can_hold != JNI_FALSE,
    );
    let action_bits = i32::from(res.schedule_hold) | (i32::from(res.cancel_hold) << 1);
    write_ints(
        &mut env,
        &out,
        &[
            i32::from(res.guide_down),
            i32::from(res.guide_hold_bits),
            i32::from(res.guide_tapping),
            res.out_buttons as i32,
            action_bits,
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_fixedStepNext(
    _: JNIEnv,
    _: JClass,
    step: jint,
) -> jint {
    fixed_step_next(step)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_fixedStepDelta(
    _: JNIEnv,
    _: JClass,
    component: jdouble,
    step: jint,
) -> jint {
    fixed_step_delta(component, step)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_mouseSilenced(
    _: JNIEnv,
    _: JClass,
    send_controllers: jboolean,
    is_default_context: jboolean,
) -> jboolean {
    jbool(mouse_silenced(
        send_controllers != JNI_FALSE,
        is_default_context != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_pickMenuMouseTarget(
    env: JNIEnv,
    _: JClass,
    eligible: JBooleanArray,
    numbers: JShortArray,
    active: JBooleanArray,
    count: jint,
) -> jint {
    if count <= 0 || eligible.is_null() || numbers.is_null() || active.is_null() {
        return -2;
    }
    let (Ok(e_len), Ok(n_len), Ok(a_len)) = (
        env.get_array_length(&eligible),
        env.get_array_length(&numbers),
        env.get_array_length(&active),
    ) else {
        return -2;
    };
    if count > e_len || count > n_len || count > a_len {
        return -2;
    }
    let n = count as usize;
    let mut e_buf = vec![0u8; n];
    let mut n_buf = vec![0i16; n];
    let mut a_buf = vec![0u8; n];
    if env
        .get_boolean_array_region(&eligible, 0, &mut e_buf)
        .is_err()
        || env.get_short_array_region(&numbers, 0, &mut n_buf).is_err()
        || env
            .get_boolean_array_region(&active, 0, &mut a_buf)
            .is_err()
    {
        return -2;
    }
    if a_buf.iter().any(|&v| v != 0) {
        return -1;
    }
    let contexts: Vec<(bool, i16, bool)> = (0..n)
        .map(|i| (e_buf[i] != 0, n_buf[i], a_buf[i] != 0))
        .collect();
    match pick_menu_mouse_target(&contexts) {
        Some(idx) => idx as jint,
        None => -2,
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_mousePacketStep(
    mut env: JNIEnv,
    _: JClass,
    input_map: jint,
    last_input_map: jint,
    fixed_step_enabled: jboolean,
    current_step: jint,
    out: JIntArray,
) {
    let plan = mouse_packet_step(
        input_map as u32,
        last_input_map as u32,
        fixed_step_enabled != JNI_FALSE,
        current_step,
    );
    let flags = i32::from(plan.fixed_step_held)
        | (i32::from(plan.step_changed) << 1)
        | (i32::from(plan.scroll_up) << 2)
        | (i32::from(plan.scroll_down) << 3)
        | (i32::from(plan.hscroll_right) << 4)
        | (i32::from(plan.hscroll_left) << 5);
    write_ints(
        &mut env,
        &out,
        &[
            plan.last_input_map as i32,
            plan.step,
            flags,
            i32::from(plan.left_btn),
            i32::from(plan.right_btn),
        ],
    );
}

fn mouse_tick_flags(plan: MouseTickPlan) -> i32 {
    i32::from(plan.has_move1)
        | (i32::from(plan.has_move2) << 1)
        | (i32::from(plan.has_scroll) << 2)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_mouseTickStep(
    mut env: JNIEnv,
    _: JClass,
    lx: jshort,
    ly: jshort,
    rx: jshort,
    ry: jshort,
    scroll_stick_mode: jint,
    fixed_step: jint,
    out: JIntArray,
) {
    let plan = mouse_tick_step(
        lx,
        ly,
        rx,
        ry,
        ScrollStick::from_i32(scroll_stick_mode),
        fixed_step,
        DEFAULT_MOUSE_SENSITIVITY_PERCENT,
    );
    write_ints(
        &mut env,
        &out,
        &[
            mouse_tick_flags(plan),
            plan.move1_x.round() as i32,
            plan.move1_y.round() as i32,
            plan.move2_x.round() as i32,
            plan.move2_y.round() as i32,
            plan.scroll_v.round() as i32,
            plan.scroll_h.round() as i32,
        ],
    );
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarInputJni_mouseTickStepFloat(
    mut env: JNIEnv,
    _: JClass,
    lx: jshort,
    ly: jshort,
    rx: jshort,
    ry: jshort,
    scroll_stick_mode: jint,
    fixed_step: jint,
    sensitivity_percent: jint,
    out: JFloatArray,
) -> jint {
    let plan = mouse_tick_step(
        lx,
        ly,
        rx,
        ry,
        ScrollStick::from_i32(scroll_stick_mode),
        fixed_step,
        sensitivity_percent,
    );
    write_floats(
        &mut env,
        &out,
        &[
            plan.move1_x,
            plan.move1_y,
            plan.move2_x,
            plan.move2_y,
            plan.scroll_v,
            plan.scroll_h,
        ],
    );
    mouse_tick_flags(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_tick_step_float_handles_subpixel_moves_sensitivity_and_scroll() {
        // Sub-pixel deflection returns non-zero fractional delta instead of zeroing out.
        let slow = mouse_tick_step(2000, 0, 0, 0, ScrollStick::None, 0, 100);
        assert!(slow.has_move1);
        assert!(slow.move1_x > 0.0 && slow.move1_x < 1.0);
        assert_eq!(slow.move1_y, 0.0);
        assert_eq!(mouse_tick_flags(slow), 1);

        // Clamped sensitivity scales movement without affecting scroll.
        let clamped_low = mouse_tick_step(0x7FFE, 0, 0, 0x7FFE, ScrollStick::Right, 0, 1);
        let at_25 = mouse_tick_step(0x7FFE, 0, 0, 0x7FFE, ScrollStick::Right, 0, 25);
        assert_eq!(clamped_low, at_25);
        let at_200 = mouse_tick_step(0x7FFE, 0, 0, 0x7FFE, ScrollStick::Right, 0, 500);
        assert!((at_200.move1_x - at_25.move1_x * 8.0).abs() < 1e-4);
        assert!(at_200.has_scroll && at_25.has_scroll);
        assert_eq!(at_200.scroll_v, at_25.scroll_v);
        assert_eq!(mouse_tick_flags(at_200), 1 | 4);

        // Scroll keeps mag < 1.0 gate even when movement is sub-pixel active.
        let gated_scroll = mouse_tick_step(0, 0, 2000, 0, ScrollStick::Right, 0, 100);
        assert!(!gated_scroll.has_scroll);
        assert_eq!(gated_scroll.scroll_v, 0.0);
        assert_eq!(gated_scroll.scroll_h, 0.0);

        // Fixed step scales by 0.2 * sensitivity and tracks negative Y sign.
        let fixed = mouse_tick_step(0x7FFE, -0x7FFE, 0, 0, ScrollStick::None, 20, 150);
        assert!(fixed.has_move1);
        assert!((fixed.move1_x - 6.0).abs() < 1e-4);
        assert!((fixed.move1_y - 6.0).abs() < 1e-4);
    }

    #[test]
    fn invalid_and_destroyed_usb_state_handles_zero_outputs() {
        let mut steer_out = [99i32; 11];
        assert_eq!(usb_steer_menu_step(0, 1, 0x10, &mut steer_out), (0, 1));
        assert_eq!(steer_out[0], 0);
        assert_eq!(usb_capture_pad_step(0, 1, 0x10, 0.9, 0.0, 1), [0, 0, 0, 0]);

        let handle = USB_STATE_SLAB.insert(UsbInputStates::default());
        let mut active_steer = [0i32; 11];
        assert_eq!(usb_steer_menu_step(handle, 3, 0, &mut active_steer), (0, 1));
        assert_eq!(active_steer[0], 1);
        assert_eq!(
            usb_steer_menu_step(handle, 3, 0x1000, &mut active_steer),
            (1, 2)
        );
        assert_ne!(
            usb_capture_pad_step(handle, 3, 0x1000, 0.9, 0.0, 1),
            [0, 0, 0, 0]
        );
        assert!(USB_STATE_SLAB.remove(handle));

        let mut destroyed_steer = [77i32; 11];
        assert_eq!(
            usb_steer_menu_step(handle, 3, 0x1000, &mut destroyed_steer),
            (0, 1)
        );
        assert_eq!(destroyed_steer[0], 0);
        assert_eq!(
            usb_capture_pad_step(handle, 3, 0x1000, 0.9, 0.0, 2),
            [0, 0, 0, 0]
        );
    }
}
