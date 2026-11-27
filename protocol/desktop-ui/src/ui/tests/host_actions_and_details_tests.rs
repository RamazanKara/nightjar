//! Host menus, wake/sleep/power flows, and host details page tests.

use super::*;

#[test]
fn pair_another_device_is_listed_where_offered_and_says_what_it_needs() {
    let f = every_state().into_iter().find(|s| s.name == "paired").unwrap().frames();
    assert!(!host_options(&f.hosts.list[0]).contains(&HostOption::PairAnother), "an older host");
    let mut f = approving_frames(false);
    assert!(host_options(&f.hosts.list[0]).contains(&HostOption::PairAnother), "no permission");
    assert_eq!(f.hosts.list[0].pair_another_option(), Some(Err(crate::hosts::PAIRING_NOT_ALLOWED)));
    assert!(tr(crate::hosts::PAIRING_NOT_ALLOWED).contains("Approve new devices"));
    assert!(!f.hosts.list[0].may_approve_pairing());
    // The permission alone is enough, as Host control is.
    f.hosts.list[0].native.capabilities.as_mut().unwrap().files.pairing_approve = true;
    assert_eq!(f.hosts.list[0].pair_another_option(), Some(Ok(())));
    let mut f = approving_frames(true);
    assert_eq!(f.hosts.list[0].pair_another_option(), Some(Ok(())));
    // Requests read before Host control went away are not offered.
    f.hosts.list[0].native.pairing_requests = vec![tv_request("a1")];
    assert_eq!(f.hosts.list[0].waiting_to_pair().len(), 1);
    f.hosts.list[0].native.capabilities.as_mut().unwrap().capabilities.permissions.host_control = false;
    assert!(f.hosts.list[0].waiting_to_pair().is_empty());
}

#[test]
fn an_empty_request_list_says_what_to_do_on_the_new_device() {
    let mut f = approving_frames(true);
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::PairRequests { only: Some(id) };
    f.l.focus_dialog = true;
    f.settle();
    assert!(f.placed.iter().any(|(t, ..)| t.contains("Nightjar or Moonlight")), "{:?}", f.placed.iter().map(|p| &p.0).collect::<Vec<_>>());
    f.l.dialog = Dialog::PairRequests { only: None };
    f.settle();
    assert!(f.placed.iter().any(|(t, ..)| t.contains("select this PC in Nightjar or Moonlight")), "{:?}", f.drawn);
}

#[test]
fn the_pin_pad_is_an_even_grid_under_its_pin_line() {
    let mut f = approving_frames(true);
    f.hosts.list[0].native.pairing_requests = vec![tv_request("a1")];
    f.l.dialog = dialogs::approve_dialog(&f.hosts.list[0], &f.hosts.list[0].native.pairing_requests[0].clone());
    f.settle();
    let at = |text: &str| f.placed.iter().find(|(t, ..)| t == text).map(|p| p.1.center()).unwrap_or_else(|| panic!("{text}"));
    for (a, b) in [("1", "4"), ("4", "7"), ("2", "5"), ("5", "8"), ("8", "0"), ("3", "6"), ("6", "9")] {
        assert!((at(a).x - at(b).x).abs() < 1.0, "{a} above {b}");
    }
    let (left, right) = (at("2").x - at("1").x, at("3").x - at("2").x);
    assert!((left - right).abs() < 1.0, "even columns: {left} and {right}");
    assert!((at("2").x - f.size.x / 2.0).abs() < 1.0, "the middle column is the dialog's middle, as its slots are");
}

#[test]
fn the_pin_dialog_works_with_a_controller_and_with_the_keyboard() {
    let mut f = approving_frames(true);
    let id = f.hosts.list[0].id;
    f.hosts.list[0].native.pairing_requests = vec![tv_request("a1")];
    f.l.dialog = dialogs::approve_dialog(&f.hosts.list[0], &f.hosts.list[0].native.pairing_requests[0].clone());
    f.l.focus_dialog = true;
    f.settle();
    // The pad opens focused on 1, and Enter (a controller's A) presses it; 2 is one step right.
    f.press(Key::Enter);
    f.press(Key::ArrowRight);
    f.press(Key::Enter);
    let pin = |f: &Frames| match &f.l.dialog {
        Dialog::ApprovePairing { pin, .. } => pin.clone(),
        other => panic!("{:?}", std::mem::discriminant(other)),
    };
    assert_eq!(pin(&f), "12");
    // Approve stays off until the PIN is whole: Backspace keeps it so.
    f.press(Key::Backspace);
    assert_eq!(pin(&f), "1");
    if let Dialog::ApprovePairing { pin, .. } = &mut f.l.dialog {
        "123".clone_into(pin);
    }
    f.settle();
    // The last digit moves the focus to Approve, so Enter sends it.
    let four = f.placed.iter().find(|(t, ..)| t == "4").map(|p| p.1).expect("4");
    f.click_at(four.center(), egui::PointerButton::Primary);
    assert_eq!(pin(&f), "1234");
    f.settle();
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::ApprovePairing { host, request, pin, .. } if *host == id && request == "a1" && pin == "1234")), "Enter on Approve");
    assert!(matches!(&f.l.dialog, Dialog::ApprovePairing { busy: true, .. }));
    // While the PC answers, nothing more goes out.
    let commands = f.press(Key::Enter);
    assert!(!commands.iter().any(|c| matches!(c, Command::ApprovePairing { .. } | Command::DenyPairing { .. })));
}

#[test]
fn approve_needs_all_four_digits_and_deny_needs_none() {
    let mut f = approving_frames(true);
    let id = f.hosts.list[0].id;
    f.hosts.list[0].native.pairing_requests = vec![tv_request("a1")];
    f.l.dialog = dialogs::approve_dialog(&f.hosts.list[0], &f.hosts.list[0].native.pairing_requests[0].clone());
    f.settle();
    let approve = f.placed.iter().find(|(t, ..)| t == "Approve").map(|p| p.1).expect("Approve");
    let commands = f.click_at(approve.center(), egui::PointerButton::Primary);
    assert!(!commands.iter().any(|c| matches!(c, Command::ApprovePairing { .. })), "no PIN yet");
    let deny = f.placed.iter().find(|(t, ..)| t == "Deny").map(|p| p.1).expect("Deny");
    let commands = f.click_at(deny.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::DenyPairing { host, request, .. } if *host == id && request == "a1")));
}

#[test]
fn a_refusal_shows_the_hosts_words_and_a_success_a_toast() {
    let mut f = approving_frames(true);
    let id = f.hosts.list[0].id;
    f.hosts.list[0].native.pairing_requests = vec![tv_request("a1")];
    f.l.dialog = dialogs::approve_dialog(&f.hosts.list[0], &f.hosts.list[0].native.pairing_requests[0].clone());
    f.l.pairing_answered(id, "a1", "TV", false, Err("That is not the PIN TV shows.".into()));
    assert!(matches!(&f.l.dialog, Dialog::Message { body, .. } if body == "That is not the PIN TV shows."));
    f.l.dialog = dialogs::approve_dialog(&f.hosts.list[0], &f.hosts.list[0].native.pairing_requests[0].clone());
    f.l.pairing_answered(id, "a1", "TV", true, Ok(()));
    assert!(f.no_dialog(), "a denial says nothing more");
}

#[test]
fn home_shows_who_waits_and_opens_their_pin() {
    let mut f = approving_frames(true);
    f.l.screen = Screen::Home;
    f.hosts.list[0].native.pairing_requests = vec![tv_request("a1")];
    f.settle();
    let banner = f.placed.iter().find(|(t, ..)| t == "TV wants to pair").map(|p| p.1).expect("the banner");
    f.click_at(banner.center(), egui::PointerButton::Primary);
    assert!(matches!(&f.l.dialog, Dialog::ApprovePairing { request, .. } if request == "a1"));
    f.l.dialog = Dialog::None;
    f.hosts.list[0].native.pairing_requests.push(tv_request("a2"));
    f.settle();
    let banner = f.placed.iter().find(|(t, ..)| t == "2 devices want to pair").map(|p| p.1).expect("the count");
    f.click_at(banner.center(), egui::PointerButton::Primary);
    assert!(matches!(&f.l.dialog, Dialog::PairRequests { only: None }));
}

#[test]
fn update_the_pcs_host_is_offered_with_host_control_and_confirmed_with_cancel_focused() {
    let offered = |name| {
        let f = every_state().into_iter().find(|s| s.name == name).unwrap().frames();
        host_options(&f.hosts.list[0]).contains(&HostOption::UpdateHost)
    };
    assert!(offered("power"));
    assert!(!offered("power not allowed"), "not without Host control");
    // Not again while it installs or is queued as an Install now, as after a tap: no second
    // update.started. An automatic update waiting for an idle PC, as while streaming, still
    // takes Install now.
    for phase in ["waiting", "downloading", "ready", "installing"] {
        for install_now in [false, true] {
            let mut f = every_state().into_iter().find(|s| s.name == "power").unwrap().frames();
            let update = f.hosts.list[0].native.update.as_mut().unwrap();
            (update.phase, update.install_now) = (phase.into(), install_now);
            let shown = host_options(&f.hosts.list[0]).contains(&HostOption::UpdateHost);
            assert_eq!(shown, phase != "installing" && !install_now, "{phase} {install_now}");
        }
    }
    assert!(!offered("paired"), "not where the PC does not list it");
    // Not allowed: one row says so for both.
    let locked = every_state().into_iter().find(|s| s.name == "power not allowed").unwrap().frames();
    let options = host_options(&locked.hosts.list[0]);
    assert!(options.contains(&HostOption::RestartPc) && !options.contains(&HostOption::ShutDownPc), "{options:?}");
    let mut f = every_state().into_iter().find(|s| s.name == "power").unwrap().frames();
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::ConfirmUpdate { host: id, name: "PC".into(), version: "3.4.0".into() };
    f.l.focus_dialog = true;
    f.settle();
    let commands = f.press(Key::Enter);
    assert!(!commands.iter().any(|c| matches!(c, Command::InstallUpdate { .. })) && f.no_dialog(), "Enter on the focused Cancel");
    f.l.dialog = Dialog::ConfirmUpdate { host: id, name: "PC".into(), version: "3.4.0".into() };
    f.settle();
    let update = f.placed.iter().find(|(t, ..)| t == "Update").map(|p| p.1).expect("Update");
    let commands = f.click_at(update.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::InstallUpdate { host } if *host == id)));
    // Done is a notice, not a dialog to dismiss.
    f.l.update_answered(id, "PC", Ok(UpdateStep::Back("3.4.0".into())));
    assert!(f.no_dialog());
    assert!(f.l.toast.as_ref().is_some_and(|(text, _)| text.contains("3.4.0")));
}

#[test]
fn sleep_pc_waits_for_the_offer_and_spends_it_only_when_confirmed() {
    let mut f = every_state().into_iter().find(|s| s.name == "can sleep").unwrap().frames();
    let id = f.hosts.list[0].id;
    let at = Handed::now();
    let offer = || Ok(SleepStep::Offer { token: "tok".into(), at });
    let sleeps = |commands: &[Command]| commands.iter().any(|c| matches!(c, Command::Sleep { .. }));
    let waiting = || Dialog::ConfirmSleep { host: id, ask: 1, name: "PC".into(), offer: None };

    // Cancelled while the PC is asked: its offer, or its failure, says nothing afterwards.
    f.l.dialog = waiting();
    f.l.focus_dialog = true;
    f.settle();
    assert!(!sleeps(&f.press(Key::Enter)), "Cancel has focus");
    assert!(f.no_dialog());
    f.l.sleep_answered(id, 1, "Gaming PC", false, offer());
    f.l.sleep_answered(id, 1, "Gaming PC", false, Err("The PC did not answer.".into()));
    assert!(f.no_dialog(), "the user walked away from it");

    // The offer fills the confirmation; one for another PC does not.
    f.l.dialog = waiting();
    f.l.focus_dialog = true;
    f.settle();
    f.l.sleep_answered(id + 1, 1, "Gaming PC", false, offer());
    assert!(matches!(f.l.dialog, Dialog::ConfirmSleep { offer: None, .. }));
    f.l.sleep_answered(id, 1, "Gaming PC", false, offer());
    assert!(matches!(&f.l.dialog, Dialog::ConfirmSleep { offer: Some((token, _)), .. } if token == "tok"));
    f.settle();
    assert!(!sleeps(&f.press(Key::Enter)), "Cancel still has focus once the offer is in");
    f.l.dialog = waiting();
    f.l.sleep_answered(id, 1, "Gaming PC", false, offer());
    f.settle();
    f.press(Key::ArrowRight);
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Sleep { host, ask: 1, token, at: when } if *host == id && token == "tok" && *when == at)));
    assert!(f.no_dialog());

    // The answers to the confirmed request are shown, the host's refusal word for word.
    // A request the PC took is a notice; nothing waits for OK.
    f.l.sleep_answered(id, 1, "Gaming PC", true, Ok(SleepStep::Accepted));
    assert!(f.no_dialog());
    assert!(matches!(&f.l.toast, Some((text, _)) if text == "Gaming PC is going to sleep"));
    let refusal = "End all applications and sessions before requesting sleep";
    f.l.sleep_answered(id, 1, "Gaming PC", true, Err(refusal.into()));
    assert!(matches!(&f.l.dialog, Dialog::Message { title, body } if title == "Could not put Gaming PC to sleep" && body == refusal));
    // A failed offer the user still waits for is shown too.
    f.l.dialog = waiting();
    f.l.sleep_answered(id, 1, "Gaming PC", false, Err(crate::hosts::SLEEP_BUSY.into()));
    assert!(matches!(&f.l.dialog, Dialog::Message { body, .. } if body == crate::hosts::SLEEP_BUSY));
}

#[test]
fn a_sleep_pc_answer_waits_for_the_dialog_the_user_is_in() {
    let mut f = every_state().into_iter().find(|s| s.name == "can sleep").unwrap().frames();
    let id = f.hosts.list[0].id;
    let other = id + 1;
    f.l.dialog = Dialog::Rename { host: other, text: "Living r".into() };
    f.settle();
    f.l.sleep_answered(id, 1, "Gaming PC", true, Ok(SleepStep::Accepted));
    f.l.sleep_answered(id, 2, "Gaming PC", true, Err("The PC did not answer.".into()));
    f.settle();
    assert!(matches!(&f.l.dialog, Dialog::Rename { text, .. } if text == "Living r"), "the rename keeps its text");
    assert!(matches!(&f.l.toast, Some((text, _)) if text == "Gaming PC is going to sleep"), "a taken request is a notice over it");
    f.l.dialog = Dialog::None;
    f.settle();
    assert!(matches!(&f.l.dialog, Dialog::Message { title, .. } if title == "Could not put Gaming PC to sleep"), "the failure waits for it");
    f.l.dialog = Dialog::None;
    f.settle();
    assert!(f.no_dialog(), "each once");
}

#[test]
fn a_late_offer_from_a_cancelled_sleep_pc_is_not_taken_for_the_new_one() {
    let mut f = every_state().into_iter().find(|s| s.name == "can sleep").unwrap().frames();
    let id = f.hosts.list[0].id;
    let at = Handed::now();
    let offer = |token: &str| Ok(SleepStep::Offer { token: token.into(), at });
    let ask = |f: &mut Frames| {
        let mut commands = Vec::new();
        f.l.dialog = f.l.choose_host_option(&f.hosts.list[0], HostOption::Sleep, &mut commands).unwrap();
        match commands[..] {
            [Command::SleepOffer { host, ask }] if host == id => ask,
            _ => panic!("Sleep PC asks the PC for a token"),
        }
    };
    let first = ask(&mut f);
    f.l.dialog = Dialog::None;
    let second = ask(&mut f);
    assert_ne!(first, second);

    // The first request's token comes back after the second request replaced it on the PC.
    f.l.sleep_answered(id, first, "Gaming PC", false, offer("old"));
    assert!(matches!(f.l.dialog, Dialog::ConfirmSleep { offer: None, .. }), "the old token is dropped");
    f.l.sleep_answered(id, second, "Gaming PC", false, offer("new"));
    assert!(matches!(&f.l.dialog, Dialog::ConfirmSleep { offer: Some((token, _)), .. } if token == "new"));
    f.l.focus_dialog = true;
    f.settle();
    f.press(Key::ArrowRight);
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Sleep { token, ask, .. } if token == "new" && *ask == second)));
    // A failure of the cancelled request is not shown over the new one either.
    f.l.dialog = Dialog::ConfirmSleep { host: id, ask: second, name: "PC".into(), offer: None };
    f.l.sleep_answered(id, first, "Gaming PC", false, Err("The PC did not answer.".into()));
    assert!(matches!(f.l.dialog, Dialog::ConfirmSleep { offer: None, .. }));
}

#[test]
fn sleep_pc_not_allowed_is_listed_with_the_reason_and_does_nothing() {
    let mut f = every_state().into_iter().find(|s| s.name == "sleep not allowed").unwrap().frames();
    f.ctx.enable_accesskit();
    assert!(host_options(&f.hosts.list[0]).contains(&HostOption::Sleep));
    f.press(Key::F2);
    assert!(f.drawn.iter().any(|t| t == crate::hosts::SLEEP_NOT_ALLOWED), "the reason is shown");
    // And heard: the row's name carries the reason, not only the painted line.
    let sleep = HostOption::Sleep.label();
    let said = format!("{}, {}", tr(sleep), tr(crate::hosts::SLEEP_NOT_ALLOWED));
    let labels: Vec<&str> = f.access.as_ref().unwrap().nodes.iter().filter_map(|(_, n)| n.label()).collect();
    assert!(labels.contains(&said.as_str()), "{said} in {labels:?}");
    let mut commands = Vec::new();
    let next = f.l.choose_host_option(&f.hosts.list[0], HostOption::Sleep, &mut commands);
    assert!(next.is_none() && commands.is_empty(), "nothing is asked of the PC");
    // A stock host never lists it.
    let stock = every_state().into_iter().find(|s| s.name == "paired").unwrap().frames();
    assert!(!host_options(&stock.hosts.list[0]).contains(&HostOption::Sleep));
}

#[test]
fn the_replace_prompt_starts_on_resume_else_cancel_never_on_the_button_that_quits() {
    for (resume, name) in [(Some(7), "with something to resume"), (None, "with nothing to resume")] {
        let mut f = Frames::new(record("Paired", true, false), Reach::Online);
        f.l.dialog = Dialog::ConfirmQuit {
            host: f.hosts.list[0].id,
            start: Some((9, "Zelda".into())),
            running: "Portal".into(),
            resume,
            named: Some(7),
        };
        f.l.focus_dialog = true;
        f.settle();
        let commands = f.press(Key::Enter);
        assert!(!commands.iter().any(|c| matches!(c, Command::QuitAndLaunch { .. })), "{name}: Enter must not quit");
        assert_eq!(
            commands.iter().any(|c| matches!(c, Command::Launch { app: 7, resume: true, .. })),
            resume.is_some(),
            "{name}: Enter resumes, or cancels when there is nothing to resume"
        );
        assert!(f.no_dialog(), "{name}");
    }
}

/// What an automatic start goes through (`open_play`): the PC's own report asks, whatever else is known.
#[test]
fn starting_over_a_game_the_pc_reports_asks_first() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?><root status_code="200"><hostname>PC</hostname><uniqueid>uid</uniqueid><HttpsPort>47984</HttpsPort><ExternalPort>47989</ExternalPort><PairStatus>1</PairStatus><currentgame>7</currentgame><state>SUNSHINE_SERVER_BUSY</state><Permission>1</Permission></root>"#;
    let info = nightjar_policy::http::ServerInfo::parse(xml, 47989).expect("the answer parses");
    let mut f = Frames::with(record("Paired", true, false), Reach::Online, Some(info), vec![game(9)]);
    assert!(f.hosts.list[0].native.session.is_none(), "no session was read");
    let mut commands = Vec::new();
    let asked = f.l.start_app(&f.hosts.list[0], &game(9), &mut commands);
    assert!(matches!(asked, Some(Dialog::ConfirmQuit { .. })), "the running game is not in the list either");
    assert!(commands.is_empty());
}

#[test]
fn a_pc_that_confirms_replacement_starts_without_the_client_prompt() {
    let info = |extension: &str| {
        let xml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><root status_code="200"><hostname>PC</hostname><uniqueid>uid</uniqueid><HttpsPort>47984</HttpsPort><ExternalPort>47989</ExternalPort><PairStatus>1</PairStatus><currentgame>7</currentgame><state>SUNSHINE_SERVER_BUSY</state><Permission>1</Permission>{extension}</root>"#
        );
        nightjar_policy::http::ServerInfo::parse(&xml, 47989).expect("the answer parses")
    };
    for (extension, confirms) in [("", false), ("<RustHostVersion>3.0.0</RustHostVersion>", true)] {
        let mut f = Frames::with(record("Paired", true, false), Reach::Online, Some(info(extension)), vec![game(7), game(9)]);
        assert_eq!(f.hosts.list[0].confirms_replacement(), confirms);
        let id = f.hosts.list[0].id;
        f.l.go(Screen::Apps(id));
        f.settle();
        // The running game has focus first; the next poster is the other game.
        f.press(Key::ArrowRight);
        let commands = f.press(Key::Enter);
        let launched = commands.iter().any(|c| matches!(c, Command::Launch { app: 9, resume: false, .. }));
        assert_eq!(launched, confirms, "{extension}");
        assert_eq!(matches!(f.l.dialog, Dialog::ConfirmQuit { .. }), !confirms, "{extension}");
    }
}

/// With a controller and no keyboard from the system, Add PC and Rename get keys on screen that
/// type into the field, the focus on them; with a mouse they don't.
#[test]
fn a_controller_alone_can_type_in_add_pc_and_rename() {
    let mut f = Frames::new(record("Den", true, false), Reach::Online);
    let host = f.hosts.list[0].id;
    f.l.screen_keyboard = false;
    f.l.input_source = crate::input_source::InputSource::Pad(crate::glyphs::Style::Xbox);
    f.l.ask(Dialog::AddHost { text: String::new(), error: None });
    f.settle();
    assert!(shows(&f, "q") && shows(&f, "Space"), "{:?}", f.drawn);
    // A on the focused first key types it.
    f.press(Key::Enter);
    for key in [".", "2"] {
        let at = f.placed.iter().find(|(t, ..)| t == key).map(|p| p.1.center()).unwrap_or_else(|| panic!("{key}: {:?}", f.drawn));
        f.click_at(at, egui::PointerButton::Primary);
    }
    assert!(matches!(&f.l.dialog, Dialog::AddHost { text, .. } if text == "1.2"), "typed");
    f.l.ask(Dialog::Rename { host, text: String::new() });
    f.settle();
    let at = f.placed.iter().find(|(t, ..)| t == "Aa").map(|p| p.1.center()).expect("Shift");
    f.click_at(at, egui::PointerButton::Primary);
    let at = f.placed.iter().find(|(t, ..)| t == "D").map(|p| p.1.center()).expect("a capital once Shift is down");
    f.click_at(at, egui::PointerButton::Primary);
    let at = f.placed.iter().find(|(t, ..)| t == "e").map(|p| p.1.center()).expect("Shift holds for one letter");
    f.click_at(at, egui::PointerButton::Primary);
    assert!(matches!(&f.l.dialog, Dialog::Rename { text, .. } if text == "De"));
    // A mouse in the hand: the field alone.
    f.l.input_source = crate::input_source::InputSource::None;
    f.l.ask(Dialog::AddHost { text: String::new(), error: None });
    f.settle();
    assert!(!shows(&f, "Space"), "{:?}", f.drawn);
}

/// Quitting a game loses unsaved progress, so its questions start on Cancel: A twice (the
/// action, then A again) leaves the game running.
#[test]
fn quit_game_questions_open_with_focus_on_cancel() {
    let mut f = Frames::new(record("PC", true, false), Reach::Online);
    let host = f.hosts.list[0].id;
    for dialog in [
        Dialog::ConfirmClose { host, name: "Racing Game".into() },
        Dialog::ConfirmQuit { host, start: None, running: "Racing Game".into(), resume: None, named: None },
        Dialog::ConfirmQuit { host, start: Some((8, "Hades".into())), running: "Racing Game".into(), resume: None, named: None },
    ] {
        f.l.ask(dialog);
        f.settle();
        assert!(shows(&f, "Quit game") || shows(&f, "Quit and start"), "{:?}", f.drawn);
        assert!(!shows(&f, "Quit app"), "one name for the action: {:?}", f.drawn);
        let commands = f.press(Key::Enter);
        assert!(!commands.iter().any(|c| matches!(c, Command::CloseGame(_) | Command::QuitApp { .. } | Command::QuitAndLaunch { .. })), "Cancel has focus");
        assert!(f.no_dialog(), "Enter on the focused Cancel");
    }
}

#[test]
fn removing_a_pc_asks_first() {
    let mut f = Frames::new(record("Paired", true, false), Reach::Online);
    let last = host_options(&f.hosts.list[0]).len() - 1;
    let open_remove = |f: &mut Frames, round: &str| {
        f.press(Key::F2);
        for _ in 0..last {
            f.press(Key::ArrowDown);
        }
        let commands = f.press(Key::Enter);
        assert!(matches!(f.l.dialog, Dialog::ConfirmRemove { .. }), "{round}: the confirmation opens");
        assert!(!removes(&commands), "{round}: choosing Remove does not remove");
    };
    // Cancel has focus: Enter and Escape both leave the PC alone.
    open_remove(&mut f, "enter");
    let commands = f.press(Key::Enter);
    assert!(f.no_dialog());
    assert!(!removes(&commands));
    assert!(f.l.focused_key.is_some(), "Cancel keeps the card's focus");
    assert!(f.ctx.memory(|m| m.focused()).is_some(), "and the card has it again");
    open_remove(&mut f, "escape");
    let commands = f.press(Key::Escape);
    assert!(f.no_dialog());
    assert!(!removes(&commands));
    assert!(f.l.focused_key.is_some());
    // Only the Remove button does it.
    open_remove(&mut f, "confirm");
    f.press(Key::ArrowRight);
    let commands = f.frame(tap_key(Key::Enter).to_vec());
    assert!(removes(&commands));
    assert!(f.l.focused_key.is_none(), "the card goes with the PC");
}

#[test]
fn rename_keeps_the_name_on_this_device() {
    let mut f = Frames::new(record("Gaming PC", true, false), Reach::Online);
    f.press(Key::F2);
    f.press(Key::ArrowDown);
    f.press(Key::ArrowDown);
    f.press(Key::Enter);
    let Dialog::Rename { text, .. } = &mut f.l.dialog else { panic!("the rename dialog") };
    assert_eq!(text, "Gaming PC", "prefilled with the name shown");
    *text = "  Living room  ".into();
    let commands = f.press(Key::Enter);
    let alias = commands.iter().find_map(|c| match c {
        Command::Rename { alias, .. } => Some(alias.clone()),
        _ => None,
    });
    assert_eq!(alias.as_deref(), Some("  Living room  "));
    let id = f.hosts.list[0].id;
    assert!(f.hosts.rename(id, "  Living room  "));
    assert_eq!(f.hosts.list[0].display_name(), "Living room");
    assert!(f.hosts.rename(id, ""));
    assert_eq!(f.hosts.list[0].display_name(), "Gaming PC", "empty brings the PC's own name back");
    assert!(f.hosts.rename(id, "Gaming PC"));
    assert_eq!(f.hosts.list[0].record.alias, None, "the PC's own name is not stored as an alias");
}

#[test]
fn a_pc_that_never_reported_a_name_starts_the_rename_empty() {
    let mut unnamed = record("x", true, false);
    unnamed.name = String::new();
    let mut f = Frames::new(unnamed, Reach::Online);
    let id = f.hosts.list[0].id;
    assert_eq!(f.hosts.list[0].display_name(), "192.0.2.7", "the card falls back to the address");
    f.press(Key::F2);
    f.press(Key::ArrowDown);
    f.press(Key::ArrowDown);
    f.press(Key::Enter);
    let Dialog::Rename { text, .. } = &f.l.dialog else { panic!("the rename dialog") };
    assert_eq!(text, "", "the address is not offered as a name");
    // Saving the address back, or nothing, leaves no alias.
    assert!(f.hosts.rename(id, "192.0.2.7"));
    assert_eq!(f.hosts.list[0].record.alias, None);
    assert!(f.hosts.rename(id, ""));
    assert_eq!(f.hosts.list[0].record.alias, None);
}

#[test]
fn tab_walks_the_header_left_to_right_and_the_actions_sit_at_the_right_edge() {
    // PCs: Home stays though no library is in yet, so the bar does not change under the pointer;
    // the destinations sit together beside the brand, and Add PC at the right edge, where the
    // search and the PC capsule are elsewhere.
    let mut f = Frames::new(record("Paired", true, false), Reach::Online);
    let stops = header_stops(&mut f);
    assert_eq!(stops.len(), 5, "Home, PCs, Settings, Help and Add PC: {stops:?}");
    assert!(stops.windows(2).all(|w| w[0].right() <= w[1].left()), "in reading order: {stops:?}");
    assert!(stops[3].right() < 640.0, "the destinations sit at the left: {stops:?}");
    assert!(1280.0 - stops[4].right() < 80.0, "Add PC is at the right edge: {:?}", stops[4]);
    assert!(shows(&f, "Add PC") && !shows(&f, "Ctrl K"), "{:?}", f.drawn);
    // Hit targets a finger or a pointer finds: no destination is shorter than 36 points.
    assert!(stops.iter().all(|r| r.height() >= 36.0), "{stops:?}");

    // Home shows all four destinations in order, one after the other.
    let mut f = Frames::new(record("Paired", true, false), Reach::Online);
    f.l.go(Screen::Home);
    f.settle();
    let stops = header_stops(&mut f);
    assert_eq!(stops.len(), 6, "Home, PCs, Settings, Help, the search and the PC: {stops:?}");
    assert!(stops[..4].windows(2).all(|w| w[0].right() <= w[1].left() && w[1].left() - w[0].right() < 12.0), "in reading order, together: {stops:?}");
}

/// The page shown marks its destination in the top bar, below the top level too: a PC's library
/// marks PCs, a Settings page Settings.
#[test]
fn the_top_bar_marks_the_destination_of_the_page_shown() {
    assert_eq!(Screen::Apps(1).destination(), Screen::Hosts);
    assert_eq!(Screen::HostDetails(1).destination(), Screen::Hosts);
    assert_eq!(Screen::AppSettings(1).destination(), Screen::Hosts);
    assert_eq!(Screen::Game { host: 1, app: 2 }.destination(), Screen::Home);
    assert_eq!(Screen::AllGames.destination(), Screen::Home);
    assert_eq!(Screen::Profiles.destination(), Screen::Settings);
    for (_, screen) in DESTINATIONS {
        assert_eq!(screen.destination(), screen, "a destination marks itself");
    }
    // Drawn: the marked tab is filled, the others are not.
    let mut f = Frames::new(record("Desk", true, true), Reach::Online);
    let id = f.hosts.list[0].id;
    f.l.open(Screen::HostDetails(id));
    f.settle();
    let tab = |f: &Frames, name: &str| f.placed.iter().find(|(t, r, _)| t == name && r.center().y < 60.0).map(|p| p.1).expect(name);
    let pcs = tab(&f, "PCs");
    let settings = tab(&f, "Settings");
    let filled = |f: &Frames, at: Rect| meshes_holding(&f.shapes, design_tokens::TINT_SELECTED_FILL).iter().any(|r| r.contains_rect(at));
    assert!(filled(&f, pcs), "PCs is marked on a PC's page");
    assert!(!filled(&f, settings), "Settings is not");
}

/// The first pairing on an install offers Home's setup card for that PC; a later pairing, or an
/// install that offered it before, does not. A PC whose last answer predates the pairing counts.
#[test]
fn the_first_pairing_offers_the_setup_card_once() {
    let mut l = launcher(true, true);
    assert!(l.offer_setup_card(5, 2), "a second paired PC is no first pairing, but a pairing seen, to be saved");
    assert!(l.settings.setup_card_seen && l.settings.setup_card_host.is_none());
    assert!(!l.offer_setup_card(6, 1), "and no later pairing offers it");
    assert_eq!(l.settings.setup_card_host, None);
    // An install from before the card starts with its PC paired: pairing it again offers nothing.
    let mut l = launcher(true, true);
    assert!(l.paired_before_setup_card(), "to be saved");
    assert!(!l.paired_before_setup_card());
    assert!(!l.offer_setup_card(5, 1) && l.settings.setup_card_host.is_none());
    let mut l = launcher(true, true);
    assert!(l.offer_setup_card(5, 1), "the first pairing offers it, to be saved");
    assert_eq!((l.settings.setup_card_host, l.settings.setup_card_seen), (Some(5), true));
    l.settings.setup_card_host = None;
    assert!(!l.offer_setup_card(6, 1), "offered once on an install, even after every PC is unpaired");
    assert_eq!(l.settings.setup_card_host, None);
    // The PC's last answer still says unpaired when the pairing lands: it counts all the same.
    let f = Frames::with(record("Desk", true, true), Reach::Online, Some(answer(false, 0)), Vec::new());
    let id = f.hosts.list[0].id;
    assert!(!f.hosts.list[0].paired());
    assert_eq!(f.hosts.paired_with(id), 1);
}

#[test]
fn multiple_copies_on_same_or_other_pc_get_distinct_click_targets_and_labels() {
    use rubylight_native_api::library::{Artwork, Game, Library, Store};
    let lib_game = |app_id: u32, title: &str, store: (&str, &str)| Game {
        install_id: format!("install-{app_id}"),
        app_id,
        title: title.into(),
        game_id: Some("steam:same-game".into()),
        store: Some(Store { source: store.0.into(), id: "same".into(), name: store.1.into() }),
        installed: Some(true),
        running: false,
        artwork: Artwork { cover: None, version: String::new() },
        last_played: None,
        playtime_minutes: None,
        collections: Vec::new(),
    };
    let mut f = home_with_games(1);
    let mut hosts = Hosts::without_discovery(vec![record("Desk", true, false), record("Den", true, false)], Arc::new(|| {}));
    for host in &mut hosts.list {
        host.reach = Reach::Online;
        host.info = f.hosts.list[0].info.clone();
    }
    hosts.list[0].native.library = Some(std::sync::Arc::new(Library {
        revision: "r1".into(),
        games: vec![
            lib_game(100, "Game A", ("steam", "Steam")),
            lib_game(101, "Game A (Epic)", ("epic", "Epic Games")),
        ],
    }));
    hosts.list[1].native.library = Some(std::sync::Arc::new(Library {
        revision: "r2".into(),
        games: vec![
            lib_game(201, "Game A", ("steam", "Steam")),
            lib_game(202, "Game A (GOG)", ("gog", "GOG")),
        ],
    }));
    f.hosts = hosts;
    let pc0 = f.hosts.list[0].id;
    let pc1 = f.hosts.list[1].id;
    f.l.go(Screen::Game { host: pc0, app: 100 });
    f.settle();
    assert!(shows(&f, "Another copy"), "{:?}", f.drawn);
    assert!(shows(&f, "Epic Games"), "{:?}", f.drawn);
    assert!(!shows(&f, "Also on Desk"), "{:?}", f.drawn);

    // Clicking the first copy on Den and then the second copy on Den must open their own app ids cleanly.
    let targets: Vec<_> = f.placed.iter().filter(|(t, ..)| t == "Den").map(|p| p.1.center()).collect();
    assert_eq!(targets.len(), 2, "two separate Den copy rows: {:?}", f.drawn);
    assert_ne!(Id::unique(("game-row", format!("copy:{pc1}:201"))), Id::unique(("game-row", format!("copy:{pc1}:202"))));
    f.click_at(targets[0], egui::PointerButton::Primary);
    assert_eq!(f.l.screen, Screen::Game { host: pc1, app: 201 });
    f.l.go(Screen::Game { host: pc0, app: 100 });
    f.settle();
    let targets: Vec<_> = f.placed.iter().filter(|(t, ..)| t == "Den").map(|p| p.1.center()).collect();
    f.click_at(targets[1], egui::PointerButton::Primary);
    assert_eq!(f.l.screen, Screen::Game { host: pc1, app: 202 });
}

/// The controller editor is reachable from a game's page: its Controller row opens Settings at the Controller section.
#[test]
fn a_games_page_opens_the_controller_editor() {
    use rubylight_native_api::library::{Artwork, Game, Library};
    let mut f = home_with_games(1);
    let pc = f.hosts.list[0].id;
    f.hosts.list[0].native.library = Some(std::sync::Arc::new(Library {
        revision: "r1".into(),
        games: vec![Game {
            install_id: "install-100".into(),
            app_id: 100,
            title: "Game A".into(),
            game_id: None,
            store: None,
            installed: Some(true),
            running: false,
            artwork: Artwork { cover: None, version: String::new() },
            last_played: None,
            playtime_minutes: None,
            collections: Vec::new(),
        }],
    }));
    f.l.go(Screen::Game { host: pc, app: 100 });
    f.settle();
    assert!(shows(&f, "Sticks, triggers, motion, vibration and buttons"), "{:?}", f.drawn);
    click_text(&mut f, "Controller");
    f.settle();
    assert_eq!(f.l.screen, Screen::Settings);
    assert_eq!(f.l.settings_view.section, "Controller");
}
