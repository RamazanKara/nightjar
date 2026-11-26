//! What Java reads of the curtain: the state it shows, when its watchdog fires and how the
//! curtain lifted, and a string for every word the shared rules name.

use std::time::Duration;

use nightjar_policy::launch_curtain::{ALL, LIMIT, QUIET};
use rubylight_protocol::launch_curtain::{Phase, PromptKind, flags};

use super::*;

fn state(sequence: u32, phase: Phase) -> CurtainState {
    CurtainState {
        phase,
        sequence,
        ..CurtainState::default()
    }
}

fn json_at(slot: &Option<Curtain>, now: Instant) -> Value {
    state_json(slot, now).expect("a curtain")
}

#[test]
fn a_stream_whose_launch_did_not_ask_has_no_curtain_and_tells_java_nothing() {
    let t0 = Instant::now();
    let mut slot: Option<Curtain> = None;
    assert!(state_json(&slot, t0).is_none());
    assert!(!offered_in(&mut slot, &[]));
    assert!(!heard_in(&mut slot, t0, state(1, Phase::Ready)));
    assert!(!picture_in(&mut slot, t0));
}

#[test]
fn a_describe_without_the_message_lifts_the_curtain_at_once() {
    let t0 = Instant::now();
    let mut slot = Some(Curtain::new(t0));
    assert!(!offered_in(
        &mut slot,
        &[0x5500, LAUNCH_CURTAIN_STATE_MESSAGE_TYPE]
    ));
    assert_eq!(json_at(&slot, t0)["down"], true);
    assert!(offered_in(&mut slot, &[0x5500]));
    let lifted = json_at(&slot, t0);
    assert_eq!(lifted["down"], false);
    assert_eq!(
        lifted["lift"],
        json!({"how": "not_offered", "notice": null, "reason": null})
    );
    assert_eq!(lifted["due_ms"], Value::Null);
    assert!(
        !offered_in(&mut slot, &[]),
        "a lifted curtain does not lift again"
    );
}

#[test]
fn the_watchdog_is_due_at_the_limit_until_the_picture_runs_then_after_the_quiet() {
    let t0 = Instant::now();
    let mut slot = Some(Curtain::new(t0));
    let down = json_at(&slot, t0);
    assert_eq!(down["down"], true);
    assert_eq!(
        down["stage"],
        Value::Null,
        "the app's own words until the host speaks"
    );
    assert_eq!(down["dismissible"], false);
    assert_eq!(down["due_ms"], LIMIT.as_millis() as u64);

    let first = t0 + Duration::from_secs(2);
    assert!(picture_in(&mut slot, first), "the first picture tells Java");
    assert!(
        !picture_in(&mut slot, first + Duration::from_secs(1)),
        "later ones do not"
    );
    let running = json_at(&slot, first);
    assert_eq!(running["dismissible"], true);
    assert_eq!(running["due_ms"], QUIET.as_millis() as u64);
    // Rounded up, so a timer that fires on the millisecond finds the curtain due.
    let between = first + Duration::from_micros(500);
    assert_eq!(json_at(&slot, between)["due_ms"], QUIET.as_millis() as u64);

    let late = first + QUIET;
    assert!(slot.as_mut().unwrap().tick(late).is_some());
    let lifted = json_at(&slot, late);
    assert_eq!(lifted["down"], false);
    assert_eq!(
        lifted["lift"],
        json!({"how": "quiet", "notice": "still_starting", "reason": null})
    );
}

#[test]
fn the_host_s_states_move_the_word_and_its_lifting_phases_lift() {
    let t0 = Instant::now();
    let mut slot = Some(Curtain::new(t0));
    assert!(heard_in(&mut slot, t0, state(1, Phase::WaitingWindow)));
    assert_eq!(json_at(&slot, t0)["stage"], "opening");
    assert!(
        heard_in(&mut slot, t0, state(1, Phase::FocusingWindow)),
        "an old sequence still counts as a word from the host"
    );
    assert_eq!(json_at(&slot, t0)["down"], true);
    assert_eq!(json_at(&slot, t0)["stage"], "opening");
    // A lift lifts whatever its sequence.
    assert!(heard_in(&mut slot, t0, state(1, Phase::Ready)));
    let lifted = json_at(&slot, t0);
    assert_eq!(lifted["lift"]["how"], "ready");
    assert_eq!(lifted["lift"]["notice"], Value::Null);
    assert!(
        !heard_in(&mut slot, t0, state(3, Phase::Preparing)),
        "nothing brings a lifted curtain back"
    );
}

#[test]
fn an_attention_card_names_the_launcher_and_the_window() {
    let t0 = Instant::now();
    let mut slot = Some(Curtain::new(t0));
    let sign_in = CurtainState {
        prompt: PromptKind::SignIn,
        launcher: "steam".into(),
        title: "Steam Guard".into(),
        flags: flags::DISMISSIBLE,
        ..state(1, Phase::AttentionNeeded)
    };
    assert!(heard_in(&mut slot, t0, sign_in));
    let card = json_at(&slot, t0);
    assert_eq!(card["stage"], "starting");
    assert_eq!(
        card["attention"],
        json!({"key": "sign_in", "launcher": "Steam", "title": "Steam Guard"})
    );
    assert_eq!(card["host_dismissible"], true);

    let unknown = CurtainState {
        prompt: PromptKind::Update,
        launcher: "newstore".into(),
        ..state(2, Phase::AttentionNeeded)
    };
    heard_in(&mut slot, t0, unknown);
    let card = json_at(&slot, t0);
    assert_eq!(
        card["attention"],
        json!({"key": "updating_any", "launcher": null, "title": ""})
    );
    assert_eq!(card["host_dismissible"], false);
}

#[test]
fn show_pc_screen_lifts_without_a_notice() {
    let t0 = Instant::now();
    let mut slot = Some(Curtain::new(t0));
    slot.as_mut().unwrap().dismiss();
    let lifted = json_at(&slot, t0);
    assert_eq!(lifted["down"], false);
    assert_eq!(
        lifted["lift"],
        json!({"how": "dismissed", "notice": null, "reason": null})
    );
}

#[test]
fn a_failed_start_carries_the_host_s_reason_and_a_timeout_only_its_notice() {
    let t0 = Instant::now();
    let mut failed = Some(Curtain::new(t0));
    let failure = CurtainState {
        reason: "The game closed right after it started.".into(),
        ..state(1, Phase::Failed)
    };
    heard_in(&mut failed, t0, failure);
    assert_eq!(
        json_at(&failed, t0)["lift"],
        json!({
            "how": "failed",
            "notice": "failed",
            "reason": "The game closed right after it started.",
        })
    );

    let mut timed_out = Some(Curtain::new(t0));
    let timeout = CurtainState {
        reason: "No window after 45 s.".into(),
        ..state(1, Phase::TimedOut)
    };
    heard_in(&mut timed_out, t0, timeout);
    assert_eq!(
        json_at(&timed_out, t0)["lift"],
        json!({"how": "timed_out", "notice": "still_starting", "reason": null})
    );
}

#[test]
fn every_word_of_the_shared_rules_has_an_android_string() {
    let strings = include_str!("../../../../app/src/main/res/values/launch_curtain.xml");
    for words in ALL {
        let name = format!("name=\"launch_curtain_{}\"", words.key);
        assert!(
            strings.contains(&name),
            "values/launch_curtain.xml lacks {name}"
        );
        let filled = words.en.contains("{}");
        let line = strings
            .lines()
            .find(|line| line.contains(&name))
            .expect("found above");
        assert_eq!(line.contains("%1$s"), filled, "{line}");
    }
}
