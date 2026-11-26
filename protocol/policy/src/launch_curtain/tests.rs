//! The client's launch curtain: it follows the host, never trusts an old frame, and always
//! lifts, whether the host speaks, goes quiet or never offered it.
use super::*;
use rubylight_protocol::launch_curtain::flags;

fn state(sequence: u32, phase: Phase) -> CurtainState {
    CurtainState {
        phase,
        sequence,
        ..CurtainState::default()
    }
}

fn s(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

#[test]
fn a_stream_without_the_curtain_lifts_it_at_once() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    assert_eq!(curtain.offered(true), None);
    assert!(curtain.down());
    assert_eq!(curtain.offered(false), Some(Lift::NotOffered));
    assert!(!curtain.down());
    assert_eq!(curtain.lifted(), Some(&Lift::NotOffered));
    assert_eq!(Lift::NotOffered.notice(), None);
    assert_eq!(curtain.due(), None);
}

#[test]
fn the_stage_word_follows_the_host_and_its_lifting_phases_lift() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    assert_eq!(curtain.view().stage, None, "the app's own words show first");
    for (sequence, phase, words) in [
        (1, Phase::Preparing, PREPARING),
        (2, Phase::StartingApp, STARTING),
        (3, Phase::WaitingWindow, OPENING),
        (4, Phase::FocusingWindow, FOCUSING),
    ] {
        assert_eq!(curtain.heard(t0, state(sequence, phase)), None);
        assert_eq!(curtain.view().stage, Some(words));
        assert_eq!(curtain.view().attention, None);
    }
    for (phase, lift) in [
        (Phase::Ready, Lift::Ready),
        (Phase::Dismissed, Lift::Dismissed),
        (Phase::TimedOut, Lift::TimedOut("took long".into())),
        (Phase::Failed, Lift::Failed("took long".into())),
    ] {
        let mut curtain = Curtain::new(t0);
        let lifted = CurtainState {
            reason: "took long".into(),
            ..state(9, phase)
        };
        assert_eq!(curtain.heard(t0, lifted), Some(lift.clone()));
        assert!(!curtain.down());
        assert_eq!(curtain.lifted(), Some(&lift));
    }
}

#[test]
fn a_phase_from_a_newer_host_lifts_the_curtain() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    let mut frame = state(1, Phase::StartingApp).encode().unwrap();
    frame[1] = 42;
    let newer = CurtainState::decode(&frame).unwrap();
    assert_eq!(curtain.heard(t0, newer), Some(Lift::Ready));
}

#[test]
fn an_old_or_repeated_state_changes_nothing_and_nothing_comes_after_the_lift() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    curtain.heard(t0, state(5, Phase::WaitingWindow));
    assert_eq!(curtain.heard(t0, state(4, Phase::Ready)), None);
    assert_eq!(curtain.heard(t0, state(5, Phase::Failed)), None);
    assert!(curtain.down());
    assert_eq!(curtain.view().stage, Some(OPENING));
    assert_eq!(curtain.dismiss(), Some(Lift::Dismissed));
    assert_eq!(curtain.dismiss(), None);
    assert_eq!(curtain.heard(t0, state(6, Phase::Failed)), None);
    assert_eq!(curtain.lifted(), Some(&Lift::Dismissed));
}

#[test]
fn a_quiet_host_lifts_the_curtain_ten_seconds_into_the_picture() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    curtain.heard(t0, state(1, Phase::StartingApp));
    // No picture yet: only the limit applies.
    assert_eq!(curtain.due(), Some(t0 + LIMIT));
    assert_eq!(curtain.tick(t0 + s(30)), None);
    curtain.picture(t0 + s(31));
    curtain.picture(t0 + s(35));
    assert_eq!(curtain.due(), Some(t0 + s(41)));
    // A heartbeat, even an unchanged one, is a word from the host.
    curtain.heard(t0 + s(39), state(1, Phase::StartingApp));
    assert_eq!(curtain.tick(t0 + s(45)), None);
    assert_eq!(curtain.due(), Some(t0 + s(49)));
    assert_eq!(curtain.tick(t0 + s(49)), Some(Lift::Quiet));
    assert_eq!(curtain.tick(t0 + s(50)), None);
    assert_eq!(Lift::Quiet.notice(), Some(STILL_STARTING));
}

#[test]
fn the_curtain_lifts_at_the_limit_even_while_the_host_keeps_talking() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    curtain.picture(t0 + s(1));
    for second in (2..60).step_by(2) {
        curtain.heard(t0 + s(second), state(second as u32, Phase::WaitingWindow));
        assert_eq!(curtain.tick(t0 + s(second)), None);
    }
    assert_eq!(curtain.tick(t0 + LIMIT), Some(Lift::Quiet));
}

#[test]
fn show_pc_screen_is_offered_once_the_picture_runs() {
    let t0 = Instant::now();
    let mut curtain = Curtain::new(t0);
    curtain.heard(
        t0,
        CurtainState {
            flags: flags::DISMISSIBLE,
            ..state(1, Phase::WaitingWindow)
        },
    );
    assert!(!curtain.view().dismissible);
    assert!(curtain.host_dismissible());
    curtain.picture(t0);
    assert!(curtain.view().dismissible);
}

#[test]
fn the_card_names_what_wants_the_player() {
    let t0 = Instant::now();
    let card = |launcher: &str, prompt| {
        let mut curtain = Curtain::new(t0);
        curtain.heard(
            t0,
            CurtainState {
                prompt,
                launcher: launcher.into(),
                title: "Steam Guard".into(),
                ..state(1, Phase::AttentionNeeded)
            },
        );
        assert_eq!(curtain.view().stage, Some(STARTING));
        curtain.view().attention.unwrap()
    };
    let steam = card("steam", PromptKind::SignIn);
    assert_eq!(
        (steam.words, steam.launcher, steam.title.as_str()),
        (SIGN_IN, Some("Steam"), "Steam Guard")
    );
    assert_eq!(card("epic", PromptKind::Update).words, UPDATING);
    assert_eq!(
        card("epic", PromptKind::Update).launcher,
        Some("Epic Games")
    );
    assert_eq!(card("", PromptKind::Update).words, UPDATING_ANY);
    assert_eq!(card("newstore", PromptKind::SignIn).words, SIGN_IN_ANY);
    assert_eq!(card("", PromptKind::Prompt).words, WAITING_ANY);
    assert_eq!(card("ubisoft", PromptKind::Prompt).words, WAITING);
    let uac = card("windows", PromptKind::Uac);
    assert_eq!((uac.words, uac.launcher), (UAC, None));
}

#[test]
fn a_failure_carries_the_hosts_reason() {
    assert_eq!(Lift::Failed("No window".into()).reason(), Some("No window"));
    assert_eq!(Lift::Failed(String::new()).reason(), None);
    assert_eq!(Lift::Failed(String::new()).notice(), Some(FAILED));
    assert_eq!(Lift::TimedOut("slow".into()).reason(), Some("slow"));
    assert_eq!(Lift::Ready.reason(), None);
}

#[test]
fn every_line_has_its_own_key_and_only_a_named_line_takes_a_name() {
    let mut keys = std::collections::BTreeSet::new();
    for words in ALL {
        assert!(keys.insert(words.key), "{}", words.key);
        assert!(!words.en.is_empty());
        assert_eq!(
            words.en.contains("{}"),
            [UPDATING, SIGN_IN, WAITING].contains(&words),
            "{:?}",
            words.en
        );
    }
}
