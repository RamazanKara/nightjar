//! Unit tests for `nightjar_policy::controller::state_machine`.

use super::*;

#[test]
fn quit_combo_mask_excludes_off_empty_unknown_and_desktop_default() {
    assert_eq!(quit_combo_mask("off"), 0);
    assert_eq!(quit_combo_mask(""), 0);
    assert_eq!(quit_combo_mask("start_select_lb_rb"), 0);
    assert_eq!(quit_combo_mask("unknown"), 0);

    let sh = button::LB | button::RB;
    assert_eq!(
        quit_combo_mask("select_lb_rb_y"),
        sh | button::BACK | button::Y
    );
    assert_eq!(
        quit_combo_mask("start_lb_rb_a"),
        sh | button::PLAY | button::A
    );
    assert_eq!(
        quit_combo_mask("start_lb_rb_b"),
        sh | button::PLAY | button::B
    );
    assert_eq!(quit_combo_mask("lb_rb_x_y"), sh | button::X | button::Y);
    assert_eq!(quit_combo_mask("lb_rb_a_b"), sh | button::A | button::B);
}

#[test]
fn quit_button_and_hold_release_state_transitions() {
    assert_eq!(quit_button_flag(keycode::BUTTON_START), button::PLAY);
    assert_eq!(quit_button_flag(keycode::MENU), button::PLAY);
    assert_eq!(quit_button_flag(keycode::BUTTON_SELECT), button::BACK);
    assert_eq!(quit_button_flag(keycode::BACK), button::BACK);
    assert_eq!(quit_button_flag(keycode::BUTTON_MODE), button::SPECIAL);
    assert_eq!(quit_button_flag(keycode::DPAD_UP), button::UP);
    assert_eq!(quit_button_flag(keycode::UNKNOWN), 0);

    let combo = "lb_rb_a_b";
    let mask = quit_combo_mask(combo);
    assert!(!hold_quit_combo(false, mask, "off"));
    assert!(!hold_quit_combo(false, button::LB | button::RB, combo));
    assert!(hold_quit_combo(false, mask, combo));
    assert_eq!(release_quit_combo(true, button::A), (true, false));
    assert_eq!(release_quit_combo(true, 0), (false, true));
    assert_eq!(release_quit_combo(false, 0), (false, false));
}

#[test]
fn menu_combo_tap_hold_and_start_long_press_boundaries() {
    assert!(should_arm_menu_combo(MENU_COMBO_MASK));
    assert!(!should_arm_menu_combo(MENU_COMBO_MASK | button::A));
    assert!(!should_arm_menu_combo(button::LB | button::RB));

    assert!(!should_open_menu_on_release(false, 0, true));
    assert!(!should_open_menu_on_release(true, button::A, true));
    assert!(!should_open_menu_on_release(true, 0, false));
    assert!(should_open_menu_on_release(true, 0, true));

    assert_eq!(tap_hold_remaining_ms(1000, 1000), 25);
    assert_eq!(tap_hold_remaining_ms(1000, 1024), 1);
    assert_eq!(tap_hold_remaining_ms(1000, 1025), 0);
    assert_eq!(tap_hold_remaining_ms(1000, 1050), -25);
    assert_eq!(tap_hold_remaining_ms(i64::MIN, i64::MAX), 25 - i64::MAX);

    assert!(!start_long_press_toggles_mouse(1000, 1750, true));
    assert!(start_long_press_toggles_mouse(1000, 1751, true));
    assert!(!start_long_press_toggles_mouse(1000, 2000, false));
}

#[test]
fn guide_hold_matches_java_state_machine_and_step_codec() {
    let mut hold = GuideHoldState::default();
    assert!(!hold.down(0, false));
    assert_eq!(hold.up(), GuideRelease::Pass);

    assert!(hold.down(1000, false));
    assert!(hold.active());
    assert!(hold.down(1000, true));
    assert_eq!(hold.up(), GuideRelease::Tap);
    assert!(!hold.expire());

    assert!(hold.down(1000, false));
    assert!(hold.expire());
    assert!(hold.active());
    assert_eq!(hold.up(), GuideRelease::Swallow);
    assert!(!hold.active());

    assert!(hold.down(1000, false));
    assert!(hold.expire());
    assert!(hold.down(1000, false));
    assert_eq!(hold.up(), GuideRelease::Tap);

    assert!(hold.down(1000, false));
    assert!(hold.cancel_pending());
    assert!(!hold.expire());
    assert!(hold.active());
    assert_eq!(hold.up(), GuideRelease::Swallow);

    assert!(hold.down(1000, false));
    assert!(hold.chord());
    assert!(!hold.chord());
    assert!(!hold.active());
    assert!(!hold.expire());
    assert_eq!(hold.up(), GuideRelease::Pass);

    let (bits, res) = guide_hold_step(0, GuideOp::Down, 500, false);
    assert_eq!((bits, res), (1, 1));
    let (bits, res) = guide_hold_step(bits, GuideOp::Up, 500, false);
    assert_eq!((bits, res), (0, GuideRelease::Tap as i32));
}

#[test]
fn usb_guide_hold_step_handles_tap_hold_and_chords() {
    // Press Guide with hold enabled -> held back, schedule timer.
    let s1 = usb_guide_hold_step(false, 0, false, button::SPECIAL, 1000, true);
    assert!(s1.guide_down);
    assert!(s1.schedule_hold);
    assert!(!s1.cancel_hold);
    assert_eq!(s1.out_buttons, 0);

    // Release before expire -> Guide tap.
    let s2 = usb_guide_hold_step(s1.guide_down, s1.guide_hold_bits, false, 0, 1000, true);
    assert!(!s2.guide_down);
    assert!(s2.guide_tapping);
    assert!(s2.cancel_hold);
    assert_eq!(s2.out_buttons, button::SPECIAL);

    // Guide + RB pressed in the same report -> chord immediately, no hold timer.
    let s3 = usb_guide_hold_step(false, 0, false, button::SPECIAL | button::RB, 1000, true);
    assert!(!s3.schedule_hold);
    assert!(s3.cancel_hold);
    assert_eq!(s3.out_buttons, button::SPECIAL | button::RB);

    // Hold disabled (hold_ms = 0) -> cancel timer and let Guide pass.
    let s4 = usb_guide_hold_step(false, 0, false, button::SPECIAL, 0, true);
    assert!(!s4.schedule_hold);
    assert!(s4.cancel_hold);
    assert_eq!(s4.out_buttons, button::SPECIAL);
}

#[test]
fn mouse_packet_and_tick_steps_match_java_semantics() {
    let p1 = mouse_packet_step(button::A | button::UP | button::X | button::Y, 0, true, 1);
    assert!(p1.fixed_step_held);
    assert!(p1.step_changed);
    assert_eq!(p1.step, 2);
    assert_eq!(p1.left_btn, 1);
    assert_eq!(p1.right_btn, 0);
    assert!(p1.scroll_up);
    assert!(!p1.scroll_down);

    let p2 = mouse_packet_step(
        button::B | button::DOWN | button::LEFT,
        p1.last_input_map,
        true,
        p1.step,
    );
    assert!(!p2.fixed_step_held);
    assert!(!p2.step_changed);
    assert_eq!(p2.step, 2);
    assert_eq!(p2.left_btn, -1);
    assert_eq!(p2.right_btn, 1);
    assert!(p2.scroll_down);
    assert!(p2.hscroll_left);

    assert_eq!(MOUSE_EMULATION_REPORT_PERIOD_MS, 10);
    assert!((MOUSE_EMULATION_TIME_COMPENSATION - 0.2).abs() < 1e-6);
    assert_eq!(clamp_mouse_sensitivity_percent(5), MIN_MOUSE_SENSITIVITY_PERCENT);
    assert_eq!(clamp_mouse_sensitivity_percent(500), MAX_MOUSE_SENSITIVITY_PERCENT);
    assert_eq!(clamp_mouse_sensitivity_percent(135), 135);

    let t_right = mouse_tick_step(0x7FFE, 0, 0, -0x7FFE, ScrollStick::Right, 0, 100);
    assert!(t_right.has_move1 && !t_right.has_move2 && t_right.has_scroll);
    assert!((t_right.move1_x - 12.8).abs() < 1e-4);
    assert_eq!(t_right.move1_y, 0.0);
    assert!((t_right.scroll_v + 12.8).abs() < 1e-4);
    assert_eq!(t_right.scroll_h, 0.0);

    // Sub-pixel pointer motion keeps fractional delta (< 1 px), while scroll preserves the 1 px gate.
    let t_subpixel = mouse_tick_step(0x1800, 0, 0, 0x1800, ScrollStick::Right, 0, 100);
    assert!(t_subpixel.has_move1);
    assert!(t_subpixel.move1_x > 0.0 && t_subpixel.move1_x < 1.0);
    assert!(!t_subpixel.has_scroll);

    // Sensitivity scales pointer motion (including fixed-step) and leaves scroll untouched.
    let t_scaled = mouse_tick_step(0x7FFE, 0, 0, -0x7FFE, ScrollStick::Right, 0, 50);
    assert!((t_scaled.move1_x - 6.4).abs() < 1e-4);
    assert!((t_scaled.scroll_v + 12.8).abs() < 1e-4);

    let t_fixed = mouse_tick_step(0x7FFE, 0x7FFE, 0, 0, ScrollStick::None, 16, 150);
    assert!(t_fixed.has_move1);
    assert!((t_fixed.move1_x - 4.8).abs() < 1e-4);
    assert!((t_fixed.move1_y + 4.8).abs() < 1e-4);

    assert!(mouse_silenced(false, false));
    assert!(!mouse_silenced(false, true));
    assert!(!mouse_silenced(true, false));

    // Active mouse -> None; else lowest controller_number among eligible.
    assert_eq!(
        pick_menu_mouse_target(&[(false, 0, true), (true, 0, false)]),
        None
    );
    assert_eq!(
        pick_menu_mouse_target(&[(false, 0, false), (true, 2, false), (true, 0, false)]),
        Some(2)
    );
}

#[test]
fn usb_steer_capture_and_held_from_menu_lifecycle() {
    let mut states = UsbInputStates::default();

    // Game presses A before opening menu.
    assert_eq!(states.filter_game_buttons(1, button::A), button::A);

    // Entering steer_menu seeds menu_buttons from last_buttons (A), so A doesn't re-fire,
    // while newly pressed DOWN steers immediately.
    let (clear, keys, count) = states.steer_menu(1, button::A | button::DOWN);
    assert!(clear);
    assert_eq!(count, 1);
    assert_eq!(keys[0], keycode::DPAD_DOWN);

    // Returning to game suppresses buttons still held from the menu until released.
    assert_eq!(states.filter_game_buttons(1, button::A | button::DOWN), 0);
    assert_eq!(
        states.filter_game_buttons(1, button::DOWN | button::B),
        button::B
    );
    assert_eq!(states.filter_game_buttons(1, button::A), button::A);

    // Entering capture_pad seeds menu_buttons = buttons | last_buttons so held A isn't reported
    // as a newly pressed capture button.
    let cap1 = states.capture_pad(1, button::A, 0.7, 0.0, 1);
    assert!(cap1.entered);
    assert_eq!(cap1.changed_mask, 0);
    assert!(cap1.stick_changed);

    // Generation bump clears capture_stick so held stick reports again.
    let cap2 = states.capture_pad(1, button::A, 0.7, 0.0, 2);
    assert!(!cap2.entered);
    assert!(cap2.stick_changed);

    let (was_steering, menu_buttons, had_stick) = states.remove_pad(1);
    assert!(was_steering);
    assert_eq!(menu_buttons, button::A);
    assert!(had_stick);
}
