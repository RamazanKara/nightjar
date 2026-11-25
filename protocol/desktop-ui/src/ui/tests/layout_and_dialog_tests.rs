//! Window bounds, scrim fades, localized widths, and open-play dialog tests.

use super::*;

#[test]
fn input_only_through_a_small_stream_needs_the_setting_and_leaves_a_running_game_alone() {
    use nightjar_policy::http::{App, AppControl};
    use nightjar_policy::launch::InputOnlyBlocker;
    let states = every_state();
    let by = |name: &str| states.iter().find(|s| s.name == name).unwrap().frames().hosts.list.remove(0);
    let busy = by("running a game");
    assert_eq!(small_stream_offer(&busy, &game(7), true, false), None, "the setting is off");
    assert_eq!(small_stream_offer(&busy, &game(7), true, true), Some(Some(InputOnlyBlocker::ViewPermission)),
        "permission 1 allows neither viewing nor launching");
    assert_eq!(small_stream_offer(&busy, &game(8), false, true), None, "another game runs: never replace it");
    let mut allowed = by("running a game");
    allowed.info.as_mut().unwrap().permission = None;
    assert_eq!(small_stream_offer(&allowed, &game(7), true, true), Some(None), "joins the running game");
    assert_eq!(small_stream_offer(&allowed, &App { control: AppControl::Resume, ..game(7) }, true, true), None, "only ordinary games");
    allowed.apps = Apps::Loaded(vec![game(7), App { control: AppControl::Input, ..game(9) }]);
    assert_eq!(small_stream_offer(&allowed, &game(7), true, true), None, "the host's own Input-only tile is used instead");
    let mut idle = by("paired");
    idle.info.as_mut().unwrap().permission = None;
    assert_eq!(small_stream_offer(&idle, &game(8), false, true), Some(None), "starts the game on an idle PC");
}

#[test]
fn f2_opens_the_options_for_the_focused_pc() {
    for State { name: state, .. } in every_state() {
        let mut f = every_state().into_iter().find(|s| s.name == state).unwrap().frames();
        assert!(!f.options_open(), "{state}");
        f.press(Key::F2);
        assert!(f.options_open(), "{state}: F2, which the controller's West button sends, opens the options");
        f.press(Key::Escape);
        assert!(f.no_dialog(), "{state}: Escape closes them");
        f.press_with(Key::F10, egui::Modifiers::SHIFT);
        assert!(f.options_open(), "{state}: Shift+F10");
    }
}

#[test]
fn right_click_and_the_three_dots_open_the_options() {
    let mut f = Frames::new(record("Paired", true, false), Reach::Online);
    let (card, dots) = f.card_and_dots();
    let body = card.left_bottom() + vec2(20.0, -10.0);
    f.click_at(body, egui::PointerButton::Secondary);
    assert!(f.options_open(), "right-click");
    f.press(Key::Escape);
    f.click_at(dots.center(), egui::PointerButton::Primary);
    assert!(f.options_open(), "the visible dots");
    f.press(Key::Escape);
    let commands = f.click_at(body, egui::PointerButton::Primary);
    assert!(f.no_dialog(), "a plain click still takes the card's own action");
    assert!(commands.iter().any(|c| matches!(c, Command::OpenApps(_))));
}

#[test]
fn every_option_is_reachable_with_the_keyboard() {
    let mut seen: Vec<HostOption> = Vec::new();
    for scenario in every_state() {
        let state = scenario.name;
        let frames = scenario.frames();
        let host = &frames.hosts.list[0];
        let options = host_options(host);
        seen.extend(&options);
        // Sleep PC listed for a device the PC does not allow cannot take focus.
        let reachable = options.iter().filter(|o| match o {
            HostOption::Sleep => host.sleep_option() == Some(Ok(())),
            HostOption::RestartPc => host.pc_power_option(PcPower::Restart) == Some(Ok(())),
            HostOption::ShutDownPc => host.pc_power_option(PcPower::ShutDown) == Some(Ok(())),
            HostOption::PairAnother => host.pair_another_option() == Some(Ok(())),
            _ => true,
        });
        for (n, option) in reachable.enumerate() {
            let mut f = scenario.frames();
            f.press(Key::F2);
            assert!(f.options_open(), "{state}");
            for _ in 0..n {
                f.press(Key::ArrowDown);
            }
            let commands = f.press(Key::Enter);
            let said = |what: fn(&Command) -> bool| commands.iter().any(what);
            let ok = match option {
                HostOption::OpenLibrary => said(|c| matches!(c, Command::OpenApps(_))),
                HostOption::Resume => said(|c| matches!(c, Command::Launch { app: 7, resume: true, .. })),
                HostOption::PairingHelp => matches!(f.l.dialog, Dialog::PairingHelp),
                HostOption::Pair => said(|c| matches!(c, Command::Pair(_))),
                HostOption::Wake => said(|c| matches!(c, Command::Wake(_))),
                HostOption::Sleep => {
                    said(|c| matches!(c, Command::SleepOffer { .. })) && matches!(f.l.dialog, Dialog::ConfirmSleep { offer: None, .. })
                }
                HostOption::RestartPc => {
                    said(|c| matches!(c, Command::PcPowerOffer { action: PcPower::Restart, .. }))
                        && matches!(f.l.dialog, Dialog::ConfirmPcPower { action: PcPower::Restart, offer: None, .. })
                }
                HostOption::ShutDownPc => {
                    said(|c| matches!(c, Command::PcPowerOffer { action: PcPower::ShutDown, .. }))
                        && matches!(f.l.dialog, Dialog::ConfirmPcPower { action: PcPower::ShutDown, offer: None, .. })
                }
                HostOption::UpdateHost => matches!(&f.l.dialog, Dialog::ConfirmUpdate { version, .. } if version == "3.4.0"),
                HostOption::PairAnother => matches!(&f.l.dialog, Dialog::PairRequests { only: Some(_) }),
                HostOption::Files => said(|c| matches!(c, Command::OpenFiles(_))) && f.no_dialog(),
                HostOption::Settings => said(|c| matches!(c, Command::HostSettings(_))),
                HostOption::Details => matches!(f.l.screen, Screen::HostDetails(_)) && f.no_dialog(),
                HostOption::CheckAgain => said(|c| matches!(c, Command::Refresh(_))),
                HostOption::Rename => matches!(f.l.dialog, Dialog::Rename { .. }),
                HostOption::Unpair => matches!(f.l.dialog, Dialog::ConfirmUnpair { .. }),
                HostOption::Remove => matches!(f.l.dialog, Dialog::ConfirmRemove { .. }),
                HostOption::PairOtp => matches!(f.l.dialog, Dialog::Otp { .. }),
                HostOption::QuitApp => matches!(f.l.dialog, Dialog::ConfirmQuit { .. }),
                HostOption::MicOnly => said(|c| matches!(c, Command::MicOnly { .. })) && f.no_dialog(),
            };
            assert!(ok, "{state}: {} by keys (entry {n})", option.label());
        }
    }
    use HostOption::*;
    for option in [
        OpenLibrary, Resume, QuitApp, Pair, PairOtp, Wake, Sleep, RestartPc, ShutDownPc, UpdateHost, PairAnother, Files, Settings, Details, Rename, Unpair,
        PairingHelp, CheckAgain, Remove, MicOnly,
    ] {
        assert!(seen.contains(&option), "no state reaches {}", option.label());
    }
}

#[test]
fn microphone_only_is_offered_only_where_the_pc_lets_this_device_start_it() {
    let offered = |reach, info| {
        let frames = Frames::with(record("Mic", true, false), reach, Some(info), Vec::new());
        host_options(&frames.hosts.list[0]).contains(&HostOption::MicOnly)
    };
    assert!(offered(Reach::Online, mic_only_answer(true)));
    // Without Launch apps, offline, or before the native read (and on stock hosts) it stays hidden.
    assert!(!offered(Reach::Online, mic_only_answer(false)));
    assert!(!offered(Reach::Offline, mic_only_answer(true)));
    assert!(!offered(Reach::Online, files::tests::native_answer()));
    assert!(!offered(Reach::Online, sleep_answer(true)));
}

// ---- A PC's details page. ----

/// The details page says what the remote test tells the user, not how it works, and has no
/// jargon about ports.
#[test]
fn the_details_page_says_what_the_remote_test_gives() {
    let f = details(Reach::Online, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    assert!(shows(&f, "Checks whether this PC answers from the network you are on now, for example away from home. It does not start a stream."), "{:?}", f.drawn);
    assert!(!f.drawn.iter().any(|t| t.contains("HTTPS requests") || t.contains("web UI port")), "{:?}", f.drawn);
}

#[test]
fn the_details_page_shows_what_the_pc_offers_not_its_brand() {
    let f = details(Reach::Online, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    // The address sits inside the web UI card, under its description.
    let in_card = f.drawn.iter().any(|t| t.starts_with("Manage the PC’s apps") && t.ends_with("
https://192.0.2.7:47990"));
    assert!(shows(&f, "Rubylight 3.0.0") && shows(&f, "Open in browser") && in_card, "{:?}", f.drawn);
    assert!(shows(&f, "Test this PC"));
    let f = details(Reach::Online, "SUNSHINE_SERVER_FREE", "<Permission>1</Permission>");
    assert!(shows(&f, "Apollo-compatible streaming app") && shows(&f, "Open in browser"), "a known host has a web UI: {:?}", f.drawn);
    // A stock host: the generic details and the check, no web UI.
    let f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    assert!(shows(&f, "NVIDIA GameStream") && shows(&f, "Test this PC"), "{:?}", f.drawn);
    assert!(!shows(&f, "Open in browser"));
    let f = details(Reach::Online, "SUNSHINE_SERVER_FREE", "");
    assert!(shows(&f, "Moonlight-compatible streaming app") && !shows(&f, "Open in browser"), "an unknown host's web UI is not guessed");
    let f = details(Reach::Offline, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    assert!(!shows(&f, "Open in browser"), "an offline PC's web UI cannot be reached");
    // A saved address no browser opens: no button, and the page says why.
    let mut record = record("uid", true, false);
    record.address = "fe80::1%12".into();
    let mut f = Frames::with(record, Reach::Online, Some(host_answer("SUNSHINE_SERVER_FREE", RUBYLIGHT)), Vec::new());
    f.l.open(Screen::HostDetails(f.hosts.list[0].id));
    f.settle();
    assert!(!shows(&f, "Open in browser") && f.drawn.iter().any(|t| t.contains("zone ID (fe80::1%12)")), "{:?}", f.drawn);
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::RemoteCheck(_))), "the check is the first item then");
}

#[test]
fn settings_a_pc_kept_before_the_update_are_listed_once_and_reset_only_on_request() {
    // As 4.1.0 could leave it: Frame rate pinned for the whole PC, and one game's own HDR.
    let mut pinned = record("Gaming PC", true, false);
    pinned.settings = Some(serde_json::from_str(r#"{"fps":120}"#).unwrap());
    pinned.save_app_settings("a", 1, serde_json::from_str(r#"{"hdr":true}"#).unwrap());
    let mut f = Frames::new(pinned, Reach::Online);
    let line = "Gaming PC: PC settings, 1 game";

    assert!(f.l.show_kept_settings_once(&f.hosts.list), "the look is saved as done");
    assert!(matches!(&f.l.dialog, Dialog::KeptSettings { pcs } if pcs == &[KeptPc { name: "Gaming PC".into(), own: true, games: 1 }]));
    f.settle();
    assert!(f.drawn.iter().any(|t| t == line), "{:?}", f.drawn);
    // Keep has focus, so A alone drops nothing.
    let commands = f.press(Key::Enter);
    assert!(!commands.iter().any(|c| matches!(c, Command::ResetKeptSettings)));
    assert!(f.no_dialog());
    assert!(!f.l.show_kept_settings_once(&f.hosts.list), "once");
    assert!(f.no_dialog());

    f.l.settings.kept_settings_checked = false;
    f.l.show_kept_settings_once(&f.hosts.list);
    f.settle();
    assert!(matches!(click_text(&mut f, "Reset all")[..], [Command::ResetKeptSettings]));
    assert!(f.no_dialog());

    // German chosen: the look runs at start, before a frame applies the language, and the line
    // still reads in German once drawn.
    f.l.settings.kept_settings_checked = false;
    f.l.settings.language = crate::config::Language::German;
    f.l.show_kept_settings_once(&f.hosts.list);
    f.settle();
    assert!(f.drawn.iter().any(|t| t == "Gaming PC: PC-Einstellungen, 1 Spiel"), "{:?}", f.drawn);
    f.l.close_dialog();
    f.l.settings.language = crate::config::Language::default();
    f.settle();

    // Another dialog open at start: the look waits for the next start.
    f.l.settings.kept_settings_checked = false;
    f.l.dialog = Dialog::PairingHelp;
    assert!(!f.l.show_kept_settings_once(&f.hosts.list));
    assert!(!f.l.settings.kept_settings_checked);
    // Nothing kept: no dialog, and the look is done all the same.
    f.l.dialog = Dialog::None;
    f.hosts.list[0].record.clear_own_settings();
    assert!(f.l.show_kept_settings_once(&f.hosts.list));
    assert!(f.no_dialog());
}

#[test]
fn the_details_page_shows_shared_folders_and_co_op_once_the_native_read_offers_them() {
    use crate::hosts::tests::{info_lands, native_caps, native_info, native_lands, stock_info, NATIVE_EXTRAS};
    let rows = |f: &Frames| [shows(f, "Shared folders") && shows(f, "Browse"), shows(f, "Co-op") && shows(f, "Show sessions")];
    let mut f = Frames::with(record("uid", true, false), Reach::Online, Some(native_info()), Vec::new());
    f.size = vec2(1280.0, 12000.0);
    let id = f.hosts.list[0].id;
    f.l.open(Screen::HostDetails(id));
    f.settle();
    assert_eq!(rows(&f), [false; 2], "nothing is offered before the native read: {:?}", f.drawn);
    native_lands(&mut f.hosts, id, native_caps(NATIVE_EXTRAS, true));
    f.settle();
    assert_eq!(rows(&f), [true; 2], "{:?}", f.drawn);
    info_lands(&mut f.hosts, id, native_info());
    f.settle();
    assert_eq!(rows(&f), [true; 2], "the next poll keeps them");
    // A host without the native API: no rows, even with a native read from before.
    let mut f = Frames::with(record("uid", true, false), Reach::Online, Some(stock_info()), Vec::new());
    f.size = vec2(1280.0, 12000.0);
    let id = f.hosts.list[0].id;
    f.l.open(Screen::HostDetails(id));
    native_lands(&mut f.hosts, id, native_caps(NATIVE_EXTRAS, true));
    f.settle();
    assert!(shows(&f, "Test this PC"), "the page is drawn: {:?}", f.drawn);
    assert_eq!(rows(&f), [false; 2], "{:?}", f.drawn);
}

#[test]
fn the_details_page_lists_co_op_sessions_only_where_the_pc_offers_them_and_joins_a_free_slot() {
    use crate::coop_page::tests::{api, waker, Fake};
    use rubylight_native_api::error::ErrorCode;
    let native = "<NativeApiVersion>1</NativeApiVersion>";
    let mut f = details(Reach::Online, "SUNSHINE_SERVER_FREE", native);
    f.size = vec2(1280.0, 12000.0);
    f.settle();
    assert!(!shows(&f, "Co-op"), "a PC that has not said it offers co-op shows none: {:?}", f.drawn);
    let offer = |f: &mut Frames, coop_offered: bool| {
        let info = f.hosts.list[0].info.as_mut().unwrap();
        let evidence = info.capabilities.evidence().unwrap().clone();
        info.capabilities = nightjar_policy::http::HostCapabilities::from_evidence(nightjar_policy::http::Evidence { native_capabilities: true, coop_offered, ..evidence });
        f.settle();
    };
    offer(&mut f, false);
    assert!(!shows(&f, "Co-op"), "a PC without Nucleus Co-op shows none");
    offer(&mut f, true);
    assert!(shows(&f, "Co-op") && shows(&f, "Show sessions"), "{:?}", f.drawn);
    let id = f.hosts.list[0].id;
    let commands = click_text(&mut f, "Show sessions");
    assert!(commands.iter().any(|c| matches!(c, Command::Coop(h) if *h == id)), "Show sessions opens the list");
    // The list as main opens it: the sessions, the free slot, then a join that launches.
    let fake = Arc::new(Fake { app_id: 1234567, ..Fake::default() });
    f.l.coop.open(id, fake.clone(), waker());
    // Frames until the answers are in, as the waker would bring them; what they asked for.
    let listed = |f: &mut Frames| {
        let mut commands = Vec::new();
        for _ in 0..500 {
            commands.extend(f.frame(Vec::new()));
            if f.l.coop.state_for(id).is_some_and(|open| !open.loading && open.joining.is_none()) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        commands.extend(f.settle());
        commands
    };
    let mut commands = listed(&mut f);
    assert!(shows(&f, "Portal 2") && shows(&f, "Free slots: 1") && shows(&f, "Player 1: Fold7") && shows(&f, "Player 2: Free"), "{:?}", f.drawn);
    assert!(shows(&f, "Portal 2 (Co-op)") && shows(&f, "Up to 2 players"));
    commands.extend(click_text(&mut f, "Join"));
    commands.extend(listed(&mut f));
    let launches: Vec<_> = commands.iter().filter_map(|c| match c {
        Command::Launch { host, app, resume } => Some((*host, *app, *resume)),
        _ => None,
    }).collect();
    assert_eq!(launches, [(id, 1234567, false)], "the join launches the session's entry once");
    assert!(shows(&f, "Player 2: This device") && shows(&f, "Return to my slot") && shows(&f, "Joined Portal 2 as player 2. Starting the stream…"), "{:?}", f.drawn);
    // A refusal is drawn with the PC's own words, and launches nothing.
    let fake = Arc::new(Fake { refuse: Some(api(403, ErrorCode::PermissionRequired, "Turn on “Can join co-op games” for this device in the Rubylight console.")), ..Fake::default() });
    f.l.coop.open(id, fake, waker());
    let mut commands = listed(&mut f);
    commands.extend(click_text(&mut f, "Join"));
    commands.extend(listed(&mut f));
    assert!(!commands.iter().any(|c| matches!(c, Command::Launch { .. })), "a refused join launches nothing");
    assert!(shows(&f, "This device may not join co-op sessions on this PC.\nTurn on “Can join co-op games” for this device in the Rubylight console."), "{:?}", f.drawn);
}

#[test]
fn focus_on_the_web_ui_moves_to_the_check_when_the_pc_goes_away() {
    let mut f = details(Reach::Online, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    let id = f.hosts.list[0].id;
    assert!(focus_shows(&f, "Open in browser"), "the web UI is the first item");
    f.hosts.list[0].reach = Reach::Offline;
    f.settle();
    assert!(!shows(&f, "Open in browser"));
    assert!(focus_shows(&f, "Test this PC"), "the check has focus, not nothing");
    assert!(f.press(Key::Enter).iter().any(|c| matches!(c, Command::RemoteCheck(h) if *h == id)));

    // Start, then Back: the page comes back without the item it remembers.
    let mut f = details(Reach::Online, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    f.l.open_settings();
    f.settle();
    assert_eq!(f.l.screen, Screen::Settings);
    f.hosts.list[0].reach = Reach::Unknown;
    f.frame(tap_key(Key::Escape).to_vec());
    assert_eq!(f.l.screen, Screen::HostDetails(id));
    assert!(focus_shows(&f, "Test this PC"), "the check takes focus on the frame Back lands, not one later");
    f.settle();
    assert!(focus_shows(&f, "Test this PC"));
}

#[test]
fn view_details_opens_the_page_and_back_returns_to_the_card() {
    let mut f = Frames::with(record("uid", true, false), Reach::Online, Some(host_answer("SUNSHINE_SERVER_FREE", RUBYLIGHT)), Vec::new());
    let card = f.ctx.memory(|m| m.focused());
    f.press(Key::F2);
    let at = host_options(&f.hosts.list[0]).iter().position(|o| *o == HostOption::Details).unwrap();
    for _ in 0..at {
        f.press(Key::ArrowDown);
    }
    f.press(Key::Enter);
    assert_eq!(f.l.screen, Screen::HostDetails(f.hosts.list[0].id));
    // The web UI is the first item; Enter opens it.
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::OpenWebUi(url) if url == "https://192.0.2.7:47990")));
    f.press(Key::Escape);
    assert_eq!(f.l.screen, Screen::Hosts);
    assert_eq!(f.ctx.memory(|m| m.focused()), card, "the card that opened it has focus again");
}

#[test]
fn the_remote_check_runs_shows_its_measurements_and_errors_verbatim() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let id = f.hosts.list[0].id;
    // Without a web UI the check is the first item.
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::RemoteCheck(h) if *h == id)), "Test asks the app to start it");
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    f.settle();
    assert!(shows(&f, "Checking your paired PC…") && shows(&f, "Cancel"), "{:?}", f.drawn);
    probe.release();
    finish_check(&mut f);
    assert!(f.drawn.iter().any(|t| t.starts_with("The paired PC responded on this network.")), "{:?}", f.drawn);
    for text in ["Relay (DERP)", "12.0 ms", "2.0 ms", "20 %", "Test this PC"] {
        assert!(shows(&f, text), "{text}: {:?}", f.drawn);
    }

    // A failure says what the client said, word for word.
    let refused = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "No connection could be made (os error 10061)");
    let probe = FakeProbe::answering(Ok(nightjar_policy::http::RemoteCheck {
        outcome: nightjar_policy::http::RemoteOutcome::Unreachable(nightjar_policy::http::Error::Io(refused)),
        stats: Default::default(),
    }));
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    probe.release();
    finish_check(&mut f);
    assert!(shows(&f, "No connection could be made (os error 10061)"), "{:?}", f.drawn);
    assert!(f.drawn.iter().any(|t| t.starts_with("The paired PC could not be verified.")));
    assert!(!shows(&f, "Relay (DERP)"), "the last check's route is gone");
}

/// With two PCs' pages on the trail (PC 1's details, PC 2's games, PC 2's details), Settings
/// over the newest keeps PC 2's check and Files page, not PC 1's.
#[test]
fn settings_over_the_newest_pc_page_keeps_that_pcs_check_and_files() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let first = f.hosts.list[0].id;
    let second = first + 1;
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(second, probe.clone(), "uid".into(), Arc::new(|| {}));
    let downloads = files::tests::Folder::new("newest-pc");
    f.l.files.page = Some(files::Page::new(second, files::tests::fake(true, true), files::tests::native_answer().capabilities,
        Ok(downloads.0.clone()), Arc::new(|| {})));
    f.l.trail.push((Screen::HostDetails(first), None));
    f.l.trail.push((Screen::Files(first), None));
    f.l.trail.push((Screen::Apps(second), None));
    f.l.trail.push((Screen::HostDetails(second), None));
    f.l.trail.push((Screen::Files(second), None));
    f.l.go(Screen::Settings);
    f.settle();
    assert_eq!(probe.cancels.load(Ordering::SeqCst), 0, "PC 2's check was cancelled");
    assert!(f.l.remote.running(), "PC 2's check is gone");
    assert!(f.l.files.page.as_ref().is_some_and(|page| page.host == second), "PC 2's Files page is gone");
}

#[test]
fn cancel_and_leaving_the_page_stop_the_check() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let id = f.hosts.list[0].id;
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    f.settle();
    // The button with focus is Cancel now.
    let commands = f.press(Key::Enter);
    assert!(!commands.iter().any(|c| matches!(c, Command::RemoteCheck(_))));
    assert_eq!(probe.cancels.load(Ordering::SeqCst), 1, "the requests in flight are cancelled");
    assert!(shows(&f, "Test cancelled. No PC or network settings were changed."), "{:?}", f.drawn);
    assert!(!f.l.remote.running());

    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    f.settle();
    f.press(Key::Escape);
    assert_eq!(f.l.screen, Screen::Hosts);
    assert_eq!(probe.cancels.load(Ordering::SeqCst), 1, "leaving the page cancels it");
    assert!(!f.l.remote.running() && f.l.remote.state.is_none());
    f.l.open(Screen::HostDetails(id));
    f.settle();
    assert!(!shows(&f, "Test cancelled. No PC or network settings were changed."), "the page opens fresh");
}

#[test]
fn the_pin_screen_does_not_stop_checks_from_being_polled() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let id = f.hosts.list[0].id;
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    f.settle();
    // The app locks with the check in flight, and the page under the lock screen is left.
    let dir = std::env::temp_dir().join(format!("nightjar-ui-pin-poll-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    let store = crate::pin_ui::open_store(&dir);
    store.set_pin(b"1234").unwrap();
    store.set_mode(crate::pin_lock::Mode::App);
    store.flush();
    f.l.pin = Some(crate::pin_ui::PinLock::with_store(store));
    f.l.screen = Screen::Hosts;
    f.l.trail.clear();
    f.settle();
    assert!(f.l.pin.as_ref().unwrap().app_locked());
    assert_eq!(probe.cancels.load(Ordering::SeqCst), 1, "leaving the page cancels its check under the lock too");
    assert!(!f.l.remote.running());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn settings_opened_over_the_page_keep_its_check() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let id = f.hosts.list[0].id;
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    probe.release();
    finish_check(&mut f);
    assert!(shows(&f, "Relay (DERP)"), "{:?}", f.drawn);
    // Start, then Back: the result is still there.
    f.l.open_settings();
    f.settle();
    assert_eq!(f.l.screen, Screen::Settings);
    f.press(Key::Escape);
    assert_eq!(f.l.screen, Screen::HostDetails(id));
    assert!(shows(&f, "Relay (DERP)"), "the result survives Settings: {:?}", f.drawn);

    // A running check goes on meanwhile and reports on the page.
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    f.settle();
    f.l.open_settings();
    f.settle();
    assert!(f.l.remote.running() && probe.cancels.load(Ordering::SeqCst) == 0, "Settings do not cancel it");
    f.press(Key::Escape);
    assert!(shows(&f, "Checking your paired PC…"), "{:?}", f.drawn);
    probe.release();
    finish_check(&mut f);
    assert!(shows(&f, "Relay (DERP)"), "{:?}", f.drawn);

    // Settings that replace the page (PCs from there) leave it: the check is cancelled.
    let probe = FakeProbe::answering(relay_check());
    f.l.remote.start(id, probe.clone(), "uid".into(), Arc::new(|| {}));
    f.l.open_settings();
    f.settle();
    f.l.switch(Screen::Hosts);
    f.settle();
    assert_eq!(probe.cancels.load(Ordering::SeqCst), 1);
    assert!(!f.l.remote.running() && f.l.remote.state.is_none());
}

#[test]
fn at_150_percent_in_german_the_details_page_fits_the_width() {
    let mut f = details(Reach::Online, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    fit_150(&mut f);
    let id = f.hosts.list[0].id;
    let mut failed = crate::host_page::report(Err(nightjar_policy::http::Error::NotPaired));
    failed.error = Some("A rather long error the host gave, word for word, about why it did not answer".into());
    for state in [
        None,
        Some(CheckState::Running),
        Some(CheckState::Cancelled),
        Some(CheckState::Done(crate::host_page::report(relay_check()))),
        Some(CheckState::Done(failed)),
    ] {
        let name = format!("{state:?}");
        f.l.remote.state = state.map(|s| (id, s));
        f.settle();
        assert!(shows(&f, "Im Browser öffnen"), "German is in use: {:?}", f.drawn);
        assert_fits(&f, &format!("details page, {name}"), false);
    }
}

/// When the PC's sleep offer arrives, nothing moves: Cancel keeps its place and the focus, so
/// a click or Enter meant for Cancel never puts the PC to sleep.
#[test]
fn the_sleep_offer_arriving_moves_nothing_under_cancel() {
    let mut f = every_state().into_iter().find(|s| s.name == "can sleep").unwrap().frames();
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::ConfirmSleep { host: id, ask: 1, name: "PC".into(), offer: None };
    f.l.focus_dialog = true;
    f.settle();
    let state = |f: &Frames| {
        let cancel = f.placed.iter().find(|(t, ..)| t == "Cancel").map(|p| p.1).expect("Cancel");
        let sleep = f.placed.iter().find(|(t, ..)| t == "Sleep PC").map(|p| p.1).expect("Sleep PC is there from the start");
        let focus = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).map(|r| r.rect).expect("a focus");
        (cancel, sleep, focus)
    };
    let (cancel, sleep, focus) = state(&f);
    assert!(focus.contains(cancel.center()), "Cancel has the focus: {focus:?} {cancel:?}");
    f.l.sleep_answered(id, 1, "PC", false, Ok(SleepStep::Offer { token: "tok".into(), at: Handed::now() }));
    f.settle();
    assert_eq!(state(&f), (cancel, sleep, focus), "Cancel, Sleep PC and the focus stay put");
    f.frame(vec![egui::Event::PointerMoved(cancel.center())]);
    let commands = f.click_at(cancel.center(), egui::PointerButton::Primary);
    assert!(!commands.iter().any(|c| matches!(c, Command::Sleep { .. })) && f.no_dialog(), "the click cancels");
}

/// Restart PC and Shut down PC confirm as Sleep PC does: Cancel has the focus and keeps its
/// place when the offer arrives; a shutdown warns when the PC may not wake from off; only the
/// confirm button spends the token.
#[test]
fn restart_and_shut_down_wait_for_the_offer_keep_cancel_focused_and_warn_before_an_unwakeable_shutdown() {
    const NO_WAKE: &str = "Once off, this PC may only start with its power button.";
    for (action, label, wake_from_off) in [(PcPower::Restart, "Restart", false), (PcPower::ShutDown, "Shut down", true), (PcPower::ShutDown, "Shut down", false)] {
        let mut f = every_state().into_iter().find(|s| s.name == "power").unwrap().frames();
        let id = f.hosts.list[0].id;
        f.l.dialog = Dialog::ConfirmPcPower { host: id, ask: 1, name: "PC".into(), action, offer: None };
        f.l.focus_dialog = true;
        f.settle();
        let place = |f: &Frames, text: &str| f.placed.iter().find(|(t, ..)| t == text).map(|p| p.1).unwrap_or_else(|| panic!("{text}"));
        let cancel = place(&f, "Cancel");
        let go = place(&f, label);
        let focus = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).map(|r| r.rect).expect("a focus");
        assert!(focus.contains(cancel.center()), "{label}: Cancel has the focus");
        // Waiting: the button is off and a click on it does nothing.
        let commands = f.click_at(go.center(), egui::PointerButton::Primary);
        assert!(!commands.iter().any(|c| matches!(c, Command::PcPower { .. })), "{label}: nothing to spend yet");

        f.l.pc_power_answered(id, 1, "PC", action, false, Ok(PcPowerStep::Offer { token: "tok".into(), at: Handed::now(), wake_from_off }));
        f.settle();
        assert_eq!((place(&f, "Cancel"), place(&f, label)), (cancel, go), "{label}: nothing moves");
        let warned = f.drawn.iter().any(|t| t == tr(NO_WAKE));
        assert_eq!(warned, action == PcPower::ShutDown && !wake_from_off, "{label}, wake from off {wake_from_off}");
        let commands = f.click_at(go.center(), egui::PointerButton::Primary);
        assert!(
            commands.iter().any(|c| matches!(c, Command::PcPower { action: a, token, .. } if *a == action && token == "tok")) && f.no_dialog(),
            "{label}: confirmed spends the token"
        );
    }
    // A refusal arriving for the dialog shows the host's words.
    let mut f = every_state().into_iter().find(|s| s.name == "power").unwrap().frames();
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::ConfirmPcPower { host: id, ask: 1, name: "PC".into(), action: PcPower::Restart, offer: None };
    f.l.pc_power_answered(id, 1, "PC", PcPower::Restart, false, Err("TV is streaming from this PC.".into()));
    assert!(matches!(&f.l.dialog, Dialog::Message { body, .. } if body == "TV is streaming from this PC."));
    // After the 202: Windows' refusal is a failure, a timeout only that the PC may still go.
    let late = |action, result| {
        let mut f = every_state().into_iter().find(|s| s.name == "power").unwrap().frames();
        let id = f.hosts.list[0].id;
        f.l.pc_power_answered(id, 1, "PC", action, true, result);
        match std::mem::replace(&mut f.l.dialog, Dialog::None) {
            Dialog::Message { title, body, .. } => (title, body),
            _ => panic!("a message"),
        }
    };
    let unanswered = "Windows did not confirm the request in time; the PC may still restart: process timed out";
    assert_eq!(late(PcPower::Restart, Ok(PcPowerStep::Unconfirmed(unanswered.into()))), ("PC may still restart".into(), unanswered.into()));
    assert_eq!(late(PcPower::ShutDown, Ok(PcPowerStep::Unconfirmed(unanswered.into()))).0, "PC may still shut down");
    assert_eq!(late(PcPower::Restart, Err("Windows refused to restart the PC: no (exit code 1191)".into())).0, "Could not restart PC");
}

#[test]
fn connection_test_judges_configured_stream_mode_bitrate() {
    let mut f = Frames::new(record("Desk", true, false), Reach::Online);
    f.l.settings = Settings { width: 3840, height: 2160, fps: 120, bitrate_kbps: 0, ..Settings::default() };
    let expected = crate::stream_types::stream_mode(&f.l.settings, f.l.display).3;
    let baseline_1080p60 = crate::stream_types::stream_mode(
        &Settings { width: 1920, height: 1080, fps: 60, bitrate_kbps: 0, ..Settings::default() },
        f.l.display,
    ).3;
    assert_eq!(f.l.stream_bitrate_kbps(&f.hosts, None), expected);
    assert!(expected > baseline_1080p60, "4K120 bitrate ({expected}) must exceed 1080p60 ({baseline_1080p60})");
}
