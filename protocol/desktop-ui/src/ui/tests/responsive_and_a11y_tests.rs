//! Responsive chrome, screen-reader labels, and destructive menu ordering tests.

use super::*;

/// From a game's ambient to the stage the fade starts faint, runs forward to its end and leaves
/// the stage alone behind the page.
#[test]
fn from_art_to_the_stage_the_fade_runs_to_its_end() {
    for (window, zoom, page) in [(vec2(1280.0, 800.0), 100, Screen::Hosts), (vec2(1280.0, 800.0), 150, Screen::Profiles), (vec2(1280.0, 800.0), 150, Screen::Hosts)] {
        let mut f = home_with_games(2);
        let pc = f.hosts.list[0].id;
        f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![nightjar_policy::http::App { name: "Game 00".into(), uuid: "install-0".into(), ..game(1000) }]));
        Arc::make_mut(&mut f.hosts.list[0].art).insert(1000, Art::Missing);
        f.l.settings.ui_zoom = zoom;
        f.size = window;
        f.l.go(Screen::Game { host: pc, app: 1000 });
        f.settle();
        f.settle();
        let what = format!("{page:?} at {window:?}, {zoom} %");
        assert!(f.l.ambient.shows_art(), "{what}: the game's tile shows");
        let art = glass::backdrop_of(&f.ctx).top.expect("the game's tile").texture;
        f.l.go(page);
        let frames = follow_the_fade(&mut f, &what);
        let first = &frames[0];
        assert!(first.fading && first.along < 0.35, "{what}: the stage fades in from the art: {first:?}");
        let behind = glass::backdrop_of(&f.ctx);
        assert!(!f.l.ambient.shows_art() && behind.base.is_none(), "{what}: the stage alone: {behind:?}");
        assert!(behind.top.is_some_and(|stage| stage.texture != art && stage.alpha == 1.0), "{what}: {behind:?}");
        assert_eq!(backdrop_shapes(&f).len(), 3, "{what}: the canvas, the stage and the page colour");
    }
}

/// From one page without art to the next the stage holds still: it is the same on every one of
/// them, so nothing fades, nothing moves and no frame is asked for behind the page.
#[test]
fn from_page_to_page_the_stage_holds_still() {
    let pairs = [(Screen::Settings, Screen::Help), (Screen::Profiles, Screen::Hosts), (Screen::Hosts, Screen::Profiles), (Screen::Profiles, Screen::Controllers), (Screen::Hosts, Screen::Settings), (Screen::StreamHotkeys, Screen::Profiles)];
    let windows = [(vec2(1280.0, 800.0), 150), (vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100)];
    for (window, zoom) in windows {
        for (from, to) in pairs {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.go(from);
            f.settle();
            f.settle();
            let before = glass::backdrop_of(&f.ctx);
            assert!(before.base.is_none() && before.top.is_some(), "{from:?}: the stage: {before:?}");
            f.l.go(to);
            let what = format!("{from:?} to {to:?} at {window:?}, {zoom} %");
            for n in 0..6 {
                f.frame(Vec::new());
                assert_eq!(glass::backdrop_of(&f.ctx), before, "{what}, frame {n}: the same stage in the same place");
                assert_eq!(backdrop::painted(&f.ctx), backdrop::Painted { along: 1.0, fading: false }, "{what}, frame {n}");
            }
        }
    }
}

/// A window resized or zoomed keeps the stage whole: the frame it changes in, the one layer covers
/// the new window past its edges, and nothing fades for it.
#[test]
fn a_resized_or_zoomed_window_keeps_the_stage_whole() {
    for page in [Screen::Hosts, Screen::Profiles, Screen::Controllers, Screen::Settings] {
        let mut f = four_pcs();
        f.size = vec2(1280.0, 800.0);
        f.l.go(page);
        f.settle();
        f.settle();
        let stage = glass::backdrop_of(&f.ctx).top.expect("the stage").texture;
        // A window drag changes the size; a zoom changes the points the next frame.
        let steps = [(vec2(1100.0, 650.0), 100), (vec2(1100.0, 650.0), 150), (vec2(1280.0, 800.0), 150), (vec2(1920.0, 1080.0), 150), (vec2(1920.0, 1080.0), 100), (vec2(720.0, 490.0), 100), (vec2(1280.0, 800.0), 100), (vec2(1280.0, 800.0), 125)];
        for (window, zoom) in steps {
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            for n in 0..3 {
                f.frame(Vec::new());
                let what = format!("{page:?}, frame {n} at {window:?}, {zoom} %");
                let behind = glass::backdrop_of(&f.ctx);
                let layer = behind.top.unwrap_or_else(|| panic!("{what}: the stage"));
                let screen = f.ctx.content_rect();
                assert!(behind.base.is_none() && layer.texture == stage && layer.alpha == 1.0, "{what}: {behind:?}");
                assert!(layer.area.contains_rect(screen) && layer.area.width() > screen.width(), "{what}: {:?} covers {screen:?}", layer.area);
            }
        }
    }
}

/// The PIN lock's own page stands on the stage as the launcher's pages do.
#[test]
fn the_pin_page_stands_on_the_stage() {
    let dir = std::env::temp_dir().join(format!("nightjar-pin-stage-{}", std::process::id()));
    for (window, zoom) in [(vec2(1280.0, 800.0), 150), (vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100), (vec2(1920.0, 1080.0), 100)] {
        for mode in [None, Some(crate::pin_lock::Mode::App)] {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.pin = Some(crate::pin_ui::PinLock::demo_page(&dir, mode));
            f.settle();
            f.settle();
            let what = format!("the PIN page ({mode:?}) at {window:?}, {zoom} %");
            assert_eq!(backdrop_shapes(&f).len(), 3, "{what}: the canvas, the stage and the page colour");
            assert!(!f.l.ambient.shows_art(), "{what}");
        }
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Back to the art a fade is leaving, the fade turns where it stands: what shows never jumps.
#[test]
fn a_fade_turned_back_turns_where_it_stands() {
    for frames_on in [0, 1, 2] {
        let mut f = home_with_games(2);
        let pc = f.hosts.list[0].id;
        f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![nightjar_policy::http::App { name: "Game 00".into(), uuid: "install-0".into(), ..game(1000) }]));
        Arc::make_mut(&mut f.hosts.list[0].art).insert(1000, Art::Missing);
        f.l.go(Screen::Game { host: pc, app: 1000 });
        f.settle();
        f.settle();
        let art = glass::backdrop_of(&f.ctx).top.expect("the game's tile shows").texture;
        // How much of the art shows.
        let shows = |f: &Frames| {
            let b = glass::backdrop_of(&f.ctx);
            match (b.base, b.top) {
                (_, Some(top)) if top.texture == art => top.alpha,
                (Some(base), Some(top)) if base.texture == art => 1.0 - top.alpha,
                (Some(base), None) if base.texture == art => 1.0,
                _ => 0.0,
            }
        };
        let mut seen = vec![shows(&f)];
        f.l.go(Screen::Hosts);
        for _ in 0..frames_on {
            f.frame(Vec::new());
            seen.push(shows(&f));
        }
        f.l.go(Screen::Game { host: pc, app: 1000 });
        for _ in 0..8 {
            f.frame(Vec::new());
            seen.push(shows(&f));
        }
        for pair in seen.windows(2) {
            assert!((pair[1] - pair[0]).abs() <= 0.35, "turned back after {frames_on} frames, the art jumps: {seen:?}");
        }
        assert!((seen.last().copied().unwrap_or_default() - 1.0).abs() < 1e-3, "and ends on the art: {seen:?}");
    }
}

/// A page on the stage paints nothing more at rest.
#[test]
fn a_page_on_the_stage_at_rest_asks_for_no_frames() {
    for page in [Screen::Hosts, Screen::Profiles, Screen::Settings, Screen::Help] {
        for (window, zoom) in [(vec2(1280.0, 800.0), 100), (vec2(1920.0, 1080.0), 100)] {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.go(page);
            for _ in 0..12 {
                f.settle();
            }
            assert!(!f.ctx.has_requested_repaint(), "{page:?} at {window:?}, {zoom} % at rest asks for no frame");
        }
    }
}

/// Home, All games, the game page and the Hide dialog each put the merged library together on
/// every pass they drew. It is kept for as long as what it is made from stays the same.
#[test]
fn the_pages_merge_the_library_once_and_again_when_it_changes() {
    let mut f = home_with_games(3);
    let merges = f.l.library.merges();
    assert!(merges > 0, "Home drew it");
    f.settle();
    f.l.go(Screen::AllGames);
    f.settle();
    f.l.go(Screen::Home);
    f.settle();
    assert_eq!(f.l.library.merges(), merges, "the same library on every one of these frames");

    // What a PC says next shows on the next frame.
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().expect("a library")).games[1].title = "Renamed".into();
    f.frame(Vec::new());
    assert_eq!(f.l.library.merges(), merges + 1);
    assert!(shows(&f, "Renamed") && !shows(&f, "Game 01"), "{:?}", f.drawn);
    f.frame(Vec::new());
    assert_eq!(f.l.library.merges(), merges + 1, "and is kept again");
    // So does a PC that went away, whose games can no longer be started.
    f.hosts.list[0].reach = Reach::Offline;
    f.frame(Vec::new());
    assert_eq!(f.l.library.merges(), merges + 2);
}

/// Only the covers a frame notes are safe from being given up for room: the ones it draws, and
/// the one a page lights itself from wherever that game's card is.
#[test]
fn a_frame_notes_the_covers_it_draws_or_needs_as_in_use() {
    let apps: Vec<_> = (1..=150).map(|id| nightjar_policy::http::App { name: format!("App {id}"), ..game(id) }).collect();
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), apps);
    f.size = vec2(1280.0, 800.0);
    let id = f.hosts.list[0].id;
    let (first, last) = (cover_texture(&f, "first"), cover_texture(&f, "last"));
    Arc::make_mut(&mut f.hosts.list[0].art).insert(1, Art::Ready(first.clone()));
    Arc::make_mut(&mut f.hosts.list[0].art).insert(150, Art::Ready(last.clone()));
    f.l.go(Screen::Apps(id));
    f.settle();
    crate::hosts::covers_noted(&f.ctx);
    f.frame(Vec::new());
    let noted = crate::hosts::covers_noted(&f.ctx);
    assert!(noted.contains(&first.id()), "the card on screen");
    assert!(!noted.contains(&last.id()), "not the one far below it");

    // The running game lights the page from its cover, with its card still off screen.
    f.hosts.list[0].info = Some(answer(true, 150));
    f.frame(Vec::new());
    assert!(crate::hosts::covers_noted(&f.ctx).contains(&last.id()));

    // The PCs page draws it on the PC's tile, and no card of the list.
    f.l.go(Screen::Hosts);
    f.settle();
    crate::hosts::covers_noted(&f.ctx);
    f.frame(Vec::new());
    let noted = crate::hosts::covers_noted(&f.ctx);
    assert!(noted.contains(&last.id()) && !noted.contains(&first.id()));
}

/// A cover that was given up for room loads from its file, so it is asked for whether its PC is
/// up or away. One that was never loaded waits for its PC, as it always did.
#[test]
fn a_cover_given_up_for_room_is_asked_for_again_whether_its_pc_is_up_or_away() {
    let mut f = app_grid(3, 0);
    let id = f.hosts.list[0].id;
    let here = cover_texture(&f, "here");
    // Both were loaded, at the version their PC lists them with.
    assert!(f.hosts.list[0].begin_art(1, "").is_some() && f.hosts.list[0].begin_art(2, "").is_some());
    Arc::make_mut(&mut f.hosts.list[0].art).insert(1, Art::Ready(here.clone()));
    Arc::make_mut(&mut f.hosts.list[0].art).insert(2, Art::Dropped);
    let asked = |f: &Frames| -> Vec<i32> {
        let view = View { hosts: &f.hosts, pads: &f.pads, connecting: None, waking: None };
        let mut commands = Vec::new();
        let mut output = f.ctx.run_ui(egui::RawInput::default(), |ui| {
            for app in 1..=3 {
                super::cards::request_cover_unseen(ui, &view, id, app, &mut commands);
            }
        });
        output.textures_delta.clear();
        let apps = commands.into_iter().filter_map(|c| match c {
            Command::RequestArt { app, .. } => Some(app),
            _ => None,
        });
        apps.collect()
    };
    crate::hosts::covers_noted(&f.ctx);
    assert_eq!(asked(&f), [2, 3], "up: the one given up, and the one never loaded");
    assert!(crate::hosts::covers_noted(&f.ctx).contains(&here.id()));
    f.hosts.list[0].reach = Reach::Offline;
    assert_eq!(asked(&f), [2], "away: the one its file brings back");
    assert!(crate::hosts::covers_noted(&f.ctx).contains(&here.id()), "a cover that is here is in use all the same");
    // On its way, it is not asked for again.
    assert!(f.hosts.list[0].begin_art(2, "").is_some());
    assert_eq!(asked(&f), [0i32; 0]);
}

/// Giving a cover up changes nothing on a page that only stands on it: the backdrop was baked
/// from it and is kept, and its card waits as one whose cover is on its way.
#[test]
fn a_page_keeps_its_backdrop_when_its_cover_is_given_up_for_room() {
    let mut f = app_grid(1, 0);
    let id = f.hosts.list[0].id;
    let ambient = cover_texture(&f, "ambient");
    let pc = &mut f.hosts.list[0];
    Arc::make_mut(&mut pc.art).insert(1, Art::Dropped);
    Arc::make_mut(&mut pc.ambient).insert(1, ambient.clone());
    Arc::make_mut(&mut pc.ambient_glow).insert(1, Color32::RED);
    let view = View { hosts: &f.hosts, pads: &f.pads, connecting: None, waking: None };
    match super::home::ambient_for(&view, id, 1, "App 1") {
        super::backdrop::Want::Art { texture, glow, .. } => assert_eq!((texture.id(), glow), (ambient.id(), Color32::RED)),
        _ => panic!("the ambient of the cover that was given up"),
    }
    assert!(super::cards::poster_caption(Some(&Art::Dropped)), "named underneath, as a cover on its way is");
    assert_eq!(super::cards::poster_caption(Some(&Art::Dropped)), super::cards::poster_caption(Some(&Art::Loading)));
}

/// Nothing draws a cover larger than the game page does, which is the size covers are kept for
/// (`hosts::COVER_POINTS`): a larger place would show one soft.
#[test]
fn no_page_draws_a_cover_larger_than_covers_are_kept_for() {
    let [w, h] = crate::hosts::COVER_POINTS;
    assert_eq!(super::game::COVER_MAX_W, w, "the game page's cover, the largest there is");
    assert_eq!(h, w * 1.5, "two by three, as that cover is");
    assert!(super::launch_scene::COVER_MOST <= h, "the launch scene's cover");
    // The largest card, at the largest size the settings have and grown under the focus.
    let card = super::cards::POSTER_MAX_W * crate::config::CardSize::Large.scale() * design_tokens::FOCUS_SCALE_COVER_TV.max(design_tokens::FOCUS_SCALE_COVER);
    assert!(card <= w, "{card}");
}

/// From Play until the stream takes over, the scene covers the launcher: the game, its PC, the
/// stage and Cancel, which has the focus, and nothing of the page under it.
#[test]
fn a_start_under_way_covers_the_launcher_with_its_scene() {
    let (mut f, _) = launching();
    for words in ["Racing Game", "Living Room PC", "Checking network profile", "Cancel"] {
        assert!(shows(&f, words), "{words}: {:?}", f.drawn);
    }
    assert!(!shows(&f, "PCs"), "the top bar is not drawn under it");
    assert_eq!(f.ctx.memory(|m| m.focused()), Some(Id::unique("launch-cancel")));
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::CancelConnect)), "A or Enter cancels");
    let commands = f.press(Key::Escape);
    assert!(commands.iter().any(|c| matches!(c, Command::CancelConnect)), "so does B");
}

/// A failed start keeps its scene and turns it into a card: Try again (focused), the stream's
/// settings in a line, a network test and Back, never a bare OK.
#[test]
fn a_failed_start_offers_try_again_its_settings_and_a_network_test() {
    let (mut f, launch) = launching();
    f.connecting = None;
    f.l.start_failed(launch, "The PC did not answer.", Some(settings_line((1920, 1080, 60, 20_000))));
    f.settle();
    for words in ["The stream could not start", "Racing Game, Living Room PC", "The PC did not answer.", "Try again", "Stream settings", "1920 × 1080 at 60 fps, 20 Mbps", "Test the network", "Back"] {
        assert!(shows(&f, words), "{words}: {:?}", f.drawn);
    }
    assert!(!shows(&f, "OK"));
    assert_eq!(f.ctx.memory(|m| m.focused()), Some(Id::unique("launch-retry")));
    assert_eq!(f.l.message_shown().as_deref(), Some("The stream could not start: The PC did not answer."), "what --host reports");
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::RetryLaunch)));
    assert!(f.no_dialog(), "the scene goes on with the new start");
}

#[test]
fn the_failed_starts_card_opens_settings_or_runs_the_test_or_goes_back() {
    let (mut f, launch) = launching();
    f.connecting = None;
    let open = |f: &mut Frames| {
        f.l.start_failed(launch.clone(), "The PC did not answer.", Some(settings_line((1920, 1080, 60, 20_000))));
        f.settle();
    };
    open(&mut f);
    let row = f.ctx.read_response(Id::unique("launch-test")).expect("the test row").rect;
    let commands = f.click_at(row.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::TestConnection)));
    assert!(matches!(f.l.dialog, Dialog::ConnectFailed { .. }), "the card stays for the answer");
    let row = f.ctx.read_response(Id::unique("launch-settings")).expect("the settings row").rect;
    f.click_at(row.center(), egui::PointerButton::Primary);
    assert!(f.no_dialog());
    assert_eq!(f.l.screen, Screen::Settings);
    f.l.go(Screen::Hosts);
    open(&mut f);
    f.press(Key::Escape);
    assert!(f.no_dialog(), "B closes the card");
    assert_eq!(f.l.screen, Screen::Hosts, "and the page it was started from is there again");
}

/// The card says what the test found, in the row that ran it.
#[test]
fn the_network_test_answers_in_its_row() {
    let (mut f, launch) = launching();
    f.connecting = None;
    f.l.start_failed(launch, "The PC did not answer.", None);
    let (tx, rx) = std::sync::mpsc::channel();
    if let Dialog::ConnectFailed { test, .. } = &mut f.l.dialog {
        *test = super::launch_scene::CardTest::Running(rx);
    }
    f.settle();
    assert!(!shows(&f, "Stream settings"), "no settings known, no row");
    tx.send(crate::conn_test::Outcome::Blocked).unwrap();
    f.settle();
    assert!(f.drawn.iter().any(|t| t.starts_with("Internet streaming may be blocked.")), "{:?}", f.drawn);
}

/// Settings' Connection test while it runs: its own title and stage (the calibration keeps
/// "Network test"), and Cancel, or B, stops it.
#[test]
fn the_connection_test_runs_under_its_own_name_until_cancelled() {
    let mut f = Frames::new(record("Den", true, false), Reach::Online);
    f.l.ask(Dialog::NetworkTest { stage: "Measuring ping, jitter and loss".into(), calibrate: false });
    f.settle();
    assert!(shows(&f, "Connection test") && shows(&f, "Measuring ping, jitter and loss") && !shows(&f, "Network test"), "{:?}", f.drawn);
    let commands = f.press(Key::Escape);
    assert!(commands.iter().any(|c| matches!(c, Command::CancelNetworkTest)));
    assert!(f.no_dialog());
    f.l.ask(Dialog::NetworkTest { stage: "Preparing frame-shaped UDP probes".into(), calibrate: true });
    f.settle();
    assert!(shows(&f, "Network test") && !shows(&f, "Connection test"), "{:?}", f.drawn);
    let commands = click_in_dialog(&mut f, "Cancel");
    assert!(commands.iter().any(|c| matches!(c, Command::CancelNetworkTest)));
    assert!(f.no_dialog());
}

/// Its result, as Android's: the delay line, then the speed or why it is missing; OK (focused)
/// closes it and Try again runs the test once more on the same PC.
#[test]
fn the_connection_test_result_shows_its_lines_and_runs_again() {
    use nightjar_policy::connection_test::Speed;
    let mut f = Frames::new(record("Den", true, false), Reach::Online);
    let host = f.hosts.list[0].id;
    let result = |speed| crate::network_test::Connection { latency_ms: Some(2.5), jitter_ms: Some(1.0), loss_percent: Some(25.0), speed };
    f.l.ask(Dialog::ConnectionTest { host, result: result(Some(Speed::Measured { mbps: 312.4, link_mbps: 1000 })) });
    f.settle();
    // The verdict and its next step first; the numbers wait under Details.
    assert!(shows(&f, "Streaming will stutter") && shows(&f, "Details"), "{:?}", f.drawn);
    assert!(!shows(&f, "Ping 2.5 ms · Jitter 1.0 ms · Loss 25.0 %"), "{:?}", f.drawn);
    let commands = f.press(Key::Enter);
    assert!(f.no_dialog() && !commands.iter().any(|c| matches!(c, Command::NetworkTest { .. })), "OK has the focus and only closes it");
    f.l.ask(Dialog::ConnectionTest { host, result: result(Some(Speed::Measured { mbps: 312.4, link_mbps: 1000 })) });
    f.settle();
    click_in_dialog(&mut f, "Details");
    for line in ["Connection test", "Ping 2.5 ms · Jitter 1.0 ms · Loss 25.0 %", "Speed 312.4 Mbps · PC link 1000 Mbps", "OK", "Try again"] {
        assert!(shows(&f, line), "{line}: {:?}", f.drawn);
    }
    f.l.dialog = Dialog::None;

    f.l.ask(Dialog::ConnectionTest { host, result: result(None) });
    f.settle();
    assert!(shows(&f, "Ping 2.5 ms · Jitter 1.0 ms · Loss 25.0 %") && !f.drawn.iter().any(|t| t.starts_with("Speed")), "a PC without a speed test: the delay alone: {:?}", f.drawn);
    let commands = click_in_dialog(&mut f, "Try again");
    assert!(commands.iter().any(|c| matches!(c, Command::NetworkTest { host: again, calibrate: false } if *again == host)), "{} commands", commands.len());
    assert!(f.no_dialog(), "the running test's dialog follows");

    f.l.ask(Dialog::ConnectionTest { host, result: result(Some(Speed::NoAnswer)) });
    f.settle();
    assert!(shows(&f, "Speed not measured: the PC didn’t answer"), "{:?}", f.drawn);
}
