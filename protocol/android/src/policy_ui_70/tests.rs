//! The bridge's pure half: each wrapper answers what `nightjar-policy` decides, by key.

use super::*;

fn device() -> Device {
    Device {
        lens: true,
        live: true,
        ..Device::default()
    }
}

#[test]
fn tiers_land_and_step_by_key() {
    assert_eq!(tier_land(device(), None), "lens");
    assert_eq!(
        tier_land(
            Device {
                lens: false,
                ..device()
            },
            None
        ),
        "live"
    );
    assert_eq!(
        tier_land(device(), Some("LIVE")),
        "live",
        "Android's stored enum name reads"
    );
    assert_eq!(
        tier_land(device(), Some("glass")),
        "lens",
        "an unknown stored name is no verdict"
    );
    assert_eq!(
        tier_land(
            Device {
                crash_guard_tripped: true,
                ..device()
            },
            None
        ),
        "baked"
    );
    assert_eq!(
        tier_land(
            Device {
                high_contrast: true,
                ..device()
            },
            Some("baked")
        ),
        "solid"
    );
    assert_eq!(tier_step_down("lens"), "live");
    assert_eq!(tier_step_down("LIVE"), "baked");
    assert_eq!(tier_step_down("solid"), "solid");
    assert_eq!(tier_step_down(""), "baked");
}

#[test]
fn the_probe_answers_with_the_policys_numbers() {
    assert_eq!(probe_constants(), [60, 2000, 8_333_333]);
    assert_eq!(probe_fractions(), [0.70, 0.25, 120.0]);
    // 120 Hz: 6.5 ms of work is over its share, slow only when it also lands late.
    assert!(!probe_slow(120.0, 6_500_000, 8_400_000));
    assert!(probe_slow(120.0, 6_500_000, 11_000_000));
    assert!(
        probe_slow(240.0, 6_500_000, 11_000_000),
        "a 240 Hz screen keeps a 120 Hz budget"
    );
    assert!(
        probe_slow(120.0, 6_500_000, -1),
        "no landing time: the work decides"
    );
    assert!(!probe_slow(60.0, -5, 50_000_000));
}

#[test]
fn the_guard_commits_and_trips_for_its_own_build() {
    assert!(guard_in_flight(true, "v1"));
    assert!(!guard_in_flight(false, "v1"));
    assert!(guard_tripped(true, "v1", "v1"));
    assert!(!guard_tripped(true, "v1", "v2"));
    assert!(!guard_tripped(false, "v1", "v1"));
}

#[test]
fn the_viewport_answers_nine_numbers() {
    let tv = viewport(1920.0, 1080.0, 0.0, "tv", 100, false);
    assert!(
        (tv[0] - 2.0).abs() < 1e-6,
        "a 4K-reporting TV is one canvas"
    );
    assert_eq!(tv[5], 7.0);
    assert_eq!(tv[6], 48.0);
    let phone = viewport(393.0, 852.0, 0.0, "phone", 100, false);
    assert_eq!((phone[0], phone[1]), (1.0, 1.0));
    let small = viewport(393.0, 852.0, 0.0, "phone", 85, false);
    assert!(
        small[0] * small[6] >= 48.0 - 0.01,
        "a target stays 48 system dp"
    );
    assert_eq!(
        viewport(393.0, 852.0, 0.0, "watch", 100, false),
        phone,
        "an unknown form is a phone"
    );
}

#[test]
fn resolution_steps_flatten_largest_first() {
    assert_eq!(
        resolution_choices(1920, 1080),
        [
            100, 1920, 1080, 85, 1632, 918, 75, 1440, 810, 67, 1280, 720, 50, 960, 540
        ]
    );
    assert_eq!(resolution_scaled(3840, 2160, 67), [2560, 1440]);
    assert_eq!(resolution_step_of(1920, 1080, 1280, 720), 67);
    assert_eq!(resolution_step_of(1920, 1080, 1600, 900), 0);
}

#[test]
fn hints_name_each_devices_keys() {
    let go =
        |source: &str, vendor: u16, swap: bool| glyph(source, vendor, "go", swap).map(|g| g[1]);
    assert_eq!(go("pad", 0x045E, false), Some("A"));
    assert_eq!(go("pad", 0x054C, false), Some("×"));
    assert_eq!(go("pad", 0x057E, false), Some("B"));
    assert_eq!(go("pad", 0x057E, true), Some("A"));
    assert_eq!(glyph("pad", 0x057E, "more", false), Some(["button", "X"]));
    assert_eq!(glyph("keyboard", 0, "back", false), Some(["escape", "Esc"]));
    assert_eq!(
        glyph("remote", 0, "more", true),
        Some(["remote_hold_ok", "Hold OK"])
    );
    assert_eq!(glyph("touch", 0, "go", false), None);
    assert_eq!(glyph("mouse", 0, "go", false), None);
    assert_eq!(glyph("pad", 0, "jump", false), None);
    assert_eq!(style_key(Style::of_vendor(0x057E)), "nintendo");
}

#[test]
fn feedback_is_silent_in_a_stream_and_limited_outside() {
    let open = Gate {
        stream_active: false,
        haptics: true,
        sounds: false,
    };
    assert_eq!(feedback("confirm", open), [Some("press"), None]);
    assert_eq!(
        feedback(
            "confirm",
            Gate {
                stream_active: true,
                ..open
            }
        ),
        [None, None]
    );
    assert_eq!(feedback("volume", open), [None, None]);
    let mut limiter = Limiter::default();
    assert_eq!(
        feedback_limited(&mut limiter, "focus_tick", open, 1000),
        [Some("tick"), None]
    );
    assert_eq!(
        feedback_limited(&mut limiter, "focus_tick", open, 1020),
        [None, None]
    );
    assert_eq!(
        feedback_limited(&mut limiter, "focus_tick", open, 1045),
        [Some("tick"), None]
    );
    for event in Event::ALL {
        let gate = Gate {
            sounds: true,
            ..open
        };
        let [haptic, sound] = feedback(event.key(), gate);
        assert!(haptic.is_some() && sound.is_some(), "{event:?}");
    }
}

#[test]
fn the_control_center_call_answers_through_the_policy() {
    let answer: serde_json::Value = serde_json::from_str(&control_center::call(
        r#"{"op":"placement","form":"phone","width_dp":400,"height_dp":800}"#,
    ))
    .unwrap();
    assert_eq!(answer["ok"], "sheet");
}
