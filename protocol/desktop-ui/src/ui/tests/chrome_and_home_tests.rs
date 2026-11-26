//! Top-bar destinations, header layout, and Home hero/shelf tests.

use super::*;

/// A PC that cannot play yet offers what would let it, in the place and the colour of Play.
#[test]
fn a_games_page_on_an_asleep_or_unpaired_pc_offers_the_way_back() {
    let mut f = home_with_games(1);
    let pc = f.hosts.list[0].id;
    f.l.go(Screen::Game { host: pc, app: 1000 });
    f.settle();
    f.hosts.list[0].reach = Reach::Offline;
    f.settle();
    assert!(shows(&f, "Desk  \u{b7}  Offline"), "{:?}", f.drawn);
    assert!(!shows(&f, "Play"), "{:?}", f.drawn);
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Refresh(h) | Command::Wake(h) if *h == pc)));
}

/// Android's game page: a game the PC lists as not installed has Play off and says why, and a
/// copy on another PC that is not installed says so under "Also on".
#[test]
fn a_game_the_pc_has_not_installed_cannot_be_played_from_its_page() {
    let mut f = home_with_games(2);
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().expect("the library")).games[0].installed = Some(false);
    // Game 01 runs: Play could not close it, so nothing says it would.
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[1].running = true;
    // A second PC with the same game, not installed there either.
    let mut hosts = Hosts::without_discovery(vec![record("Desk", true, true), record("Den", true, false)], Arc::new(|| {}));
    for host in &mut hosts.list {
        host.reach = Reach::Online;
        host.native.library = f.hosts.list[0].native.library.clone();
    }
    hosts.list[0].info = f.hosts.list[0].info.clone();
    hosts.list[0].apps = Apps::Loaded(Arc::new(vec![nightjar_policy::http::App { name: "Game 00".into(), uuid: "install-0".into(), ..game(1000) }]));
    f.hosts = hosts;
    let pc = f.hosts.list[0].id;
    f.l.go(Screen::Game { host: pc, app: 1000 });
    f.settle();
    assert!(shows(&f, "Desk  \u{b7}  Not installed"), "{:?}", f.drawn);
    assert!(shows(&f, "Den, not installed"), "Also on marks it: {:?}", f.drawn);
    let closes = "Game 01 is running. Playing this game closes it.";
    assert!(!shows(&f, closes), "{:?}", f.drawn);
    assert!(f.ctx.memory(|m| m.focused()).is_some(), "something has focus for the D-pad");
    let play = f.placed.iter().find(|(t, ..)| t == "Play").map(|p| p.1).expect("Play is shown");
    let commands = f.click_at(play.center(), egui::PointerButton::Primary);
    assert!(!commands.iter().any(|c| matches!(c, Command::Launch { .. })), "Play is off");
    // Installed, the same page plays it.
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[0].installed = Some(true);
    f.settle();
    assert!(!shows(&f, "Not installed on Desk"), "{:?}", f.drawn);
    assert!(shows(&f, closes), "{:?}", f.drawn);
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[1].running = false;
    f.settle();
    let play = f.placed.iter().find(|(t, ..)| t == "Play").map(|p| p.1).expect("Play is shown");
    let commands = f.click_at(play.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::Launch { app: 1000, .. })), "Play starts it");
}

#[test]
fn a_games_page_names_its_store_when_the_pc_sends_only_the_key() {
    use rubylight_native_api::library::Store;
    let mut f = home_with_games(1);
    let store = |source: &str, name: &str| Some(Store { source: source.into(), id: "1".into(), name: name.into() });
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().expect("the library")).games[0].store = store("microsoft", "");
    let pc = f.hosts.list[0].id;
    f.l.go(Screen::Game { host: pc, app: 1000 });
    f.settle();
    assert!(shows(&f, "Xbox"), "the key's known name: {:?}", f.drawn);
    // The PC's own name wins.
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[0].store = store("microsoft", "Microsoft Store");
    f.settle();
    assert!(shows(&f, "Microsoft Store") && !shows(&f, "Xbox"), "{:?}", f.drawn);
}

/// Settings is a top-level page like Home and PCs: no Back in its header. Its sub-pages keep theirs.
#[test]
fn settings_has_no_back_but_its_sub_pages_do() {
    let mut f = Frames::new(record("Desk", true, true), Reach::Online);
    f.l.go(Screen::Settings);
    f.settle();
    let below_bar = |f: &Frames, name: &str| f.placed.iter().any(|(t, r, _)| t == name && r.center().y > 60.0 && r.center().y < 140.0);
    assert!(shows(&f, "Settings") && !below_bar(&f, "Settings") && back_rect(&f).is_none(), "{:?}", f.drawn);
    f.l.open(Screen::Profiles);
    f.settle();
    assert!(back_rect(&f).is_some(), "Back is there on a sub-page");
}

/// However far a long page scrolled, Back is one click away: it rises with the page, then holds in
/// the bar where the brand gave way to it. It takes the click over the cover passing beneath.
#[test]
fn back_holds_in_the_bar_when_the_page_scrolled_far() {
    let mut f = home_with_games(70);
    f.l.open(Screen::AllGames);
    f.settle();
    let rest = back_rect(&f).expect("Back on All games");
    assert_eq!(rest.top(), chrome::PAGE_TOP, "at rest it heads the page");
    assert!(shows(&f, "Nightjar"), "the brand is there at rest: {:?}", f.drawn);
    f.scroll_to_end();
    let held = back_rect(&f).expect("Back after scrolling");
    assert!((held.top() - 22.0).abs() < 0.5 && held.left() == rest.left(), "held in the bar: {held:?}");
    assert!(!shows(&f, "Nightjar"), "the brand gave way: {:?}", f.drawn);
    f.click_at(held.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.screen, Screen::Home, "the click left the page and opened no game");
    assert_eq!(chrome::brand_alpha(Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0)), None), 1.0, "Home keeps its brand");
}

/// The brand fades over the same stretch of scrolling each time, so Back never rides over its words.
#[test]
fn the_brand_is_gone_before_back_reaches_it() {
    let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
    let alpha_at = |scrolled: f32| chrome::brand_alpha(screen, Some(chrome::PAGE_TOP - scrolled));
    assert_eq!(alpha_at(0.0), 1.0);
    assert_eq!(alpha_at(16.0), 1.0);
    assert!(alpha_at(26.0) > 0.0 && alpha_at(26.0) < 1.0);
    assert_eq!(alpha_at(36.0), 0.0);
    // Back's top passes the mark's foot (59) at 29 scrolled: the brand is mostly gone by then.
    assert!(alpha_at(29.0) < 0.4);
    assert_eq!(chrome::back_top(screen, chrome::PAGE_TOP - 500.0), 22.0);
    assert_eq!(chrome::back_top(screen, chrome::PAGE_TOP), chrome::PAGE_TOP);
}

/// Under a short list of PCs: the search goes on, and How to connect opens the pairing guide.
#[test]
fn a_short_pc_list_says_the_search_goes_on_and_links_how_to_connect() {
    let mut f = Frames::new(record("Desk", true, true), Reach::Online);
    f.hosts.discovering = true;
    f.l.go(Screen::Hosts);
    f.settle();
    assert!(shows(&f, "Looking for other PCs on this network"), "{:?}", f.drawn);
    assert!(!shows(&f, "Search again"), "{:?}", f.drawn);
    let link = f.placed.iter().find(|(t, ..)| t == "How to connect").map(|p| p.1).expect("the link");
    f.click_at(link.center(), egui::PointerButton::Primary);
    let commands = f.settle();
    let opened = |c: &Command| matches!(c, Command::OpenGuide("pairing.md"));
    let mut commands = commands;
    if !commands.iter().any(opened) {
        commands = f.click_at(link.center(), egui::PointerButton::Primary);
    }
    assert!(commands.iter().any(opened), "a click opens the pairing guide");
}

/// The pairing PIN is what the dialog is for: large, bold digits in the amber accent.
#[test]
fn the_pairing_pin_is_large_and_bold_in_its_slots() {
    fn digits(shape: &egui::Shape, out: &mut Vec<(String, egui::FontId, Color32)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| digits(s, out)),
            egui::Shape::Text(t) => {
                if let Some(section) = t.galley.job.sections.first() {
                    out.push((t.galley.text().to_owned(), section.format.font_id.clone(), t.fallback_color));
                }
            }
            _ => {}
        }
    }
    let mut f = Frames::new(record("Studio", false, false), Reach::Online);
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::Pairing { host: id, pin: "4071".into() };
    // Past the dialog's fade-in.
    for _ in 0..4 {
        f.settle();
    }
    let mut texts = Vec::new();
    f.shapes.iter().for_each(|c| digits(&c.shape, &mut texts));
    let pin: Vec<_> = texts.iter().filter(|(t, ..)| t.len() == 1 && "4071".contains(t.as_str())).collect();
    assert_eq!(pin.len(), 4, "{texts:?}");
    // Centred in the panel's column, as its title is.
    let title = f.placed.iter().find(|(t, ..)| t == "Pair with Studio").map(|p| p.1.center().x).expect("the title");
    let digit = |d: &str| f.placed.iter().find(|(t, ..)| t == d).map(|p| p.1.center().x).expect("the digit");
    let middle = (digit("4") + digit("1")) / 2.0;
    assert!((middle - title).abs() < 2.0, "the PIN is centred under the title: {middle}, {title}");
    assert!(pin.iter().all(|(_, font, color)| font.size >= 36.0 && *font == semibold(font.size) && *color == Color32::from(ON_SURFACE)), "{pin:?}");
}

/// Home's card for a PC to pair says only that, whole: no "Games not loaded yet" cut short after it.
#[test]
fn the_home_card_of_a_pc_to_pair_fits_its_state() {
    use rubylight_native_api::library::{Artwork, Game, Library};
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut l = launcher(true, false);
    l.identity_ready = true;
    let hosts = Hosts::without_discovery(vec![record("Desk", true, true), HostRecord { address: "192.0.2.12".into(), ..record("Studio", false, false) }], Arc::new(|| {}));
    let mut f = Frames::over(ctx, l, hosts);
    f.hosts.list[0].reach = Reach::Online;
    f.hosts.list[0].info = Some(answer(true, 0));
    let game = Game {
        install_id: "install-0".into(),
        app_id: 1000,
        title: "Game 00".into(),
        game_id: None,
        store: None,
        installed: Some(true),
        running: false,
        artwork: Artwork { cover: None, version: String::new() },
        last_played: None,
        playtime_minutes: None,
        collections: Vec::new(),
    };
    f.hosts.list[0].native.library = Some(Arc::new(Library { revision: "r1".into(), games: vec![game] }));
    f.hosts.list[1].reach = Reach::Online;
    f.hosts.list[1].info = Some(answer(false, 0));
    f.l.go(Screen::Home);
    f.settle();
    assert!(shows(&f, "Not paired"), "as the PCs page says: {:?}", f.drawn);
    assert!(!f.drawn.iter().any(|t| t.contains("Games not") || t.ends_with('…')), "{:?}", f.drawn);
}

/// The first run is the stage, what is happening in one line, one action and the way to help; Home
/// sends it there.
#[test]
fn the_first_run_is_one_line_and_add_pc_from_home_too() {
    for screen in [Screen::Home, Screen::Hosts] {
        let mut f = first_run_frames(Vec::new());
        f.l.go(screen);
        f.settle();
        assert_eq!(f.l.screen, Screen::Hosts, "{:?}", f.drawn);
        assert!(shows(&f, "Searching for PCs\u{2026}"), "{:?}", f.drawn);
        let adds = f.placed.iter().filter(|(t, ..)| t == "Add PC").count();
        assert_eq!(adds, 1, "the page's, not the bar's as well: {:?}", f.drawn);
        assert!(shows(&f, "How to connect") && !f.drawn.iter().any(|t| t.to_lowercase().contains("host")), "{:?}", f.drawn);
        let add = f.placed.iter().find(|(t, ..)| t == "Add PC").map(|p| p.1).expect("Add PC");
        let focused = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).expect("Add PC has focus").rect;
        assert!(focused.contains(add.center()), "{focused:?}");
        f.press(Key::Enter);
        assert!(!matches!(f.l.dialog, Dialog::None), "Enter opens Add PC");
    }
}

/// With no search running it says so in the page's own ink, not as an error: Add PC is the way on,
/// and Search again the fix.
#[test]
fn a_first_run_that_cannot_search_says_so() {
    let mut f = first_run_frames(Vec::new());
    f.hosts.discovering = false;
    f.l.go(Screen::Hosts);
    f.settle();
    let words = "Can\u{2019}t search this network for PCs";
    assert!(shows(&f, words) && !shows(&f, "Searching for PCs\u{2026}"), "{:?}", f.drawn);
    assert!(shows(&f, "Add PC"), "{:?}", f.drawn);
    let mut inks = Vec::new();
    for clipped in &f.shapes {
        galley_inks(&clipped.shape, words, &mut inks);
    }
    assert!(!inks.is_empty() && inks.iter().all(|ink| *ink != *ERROR), "{inks:?}");
    let again = f.placed.iter().find(|(t, ..)| t == "Search again").map(|p| p.1).expect("Search again");
    let commands = f.click_at(again.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::SearchAgain)));
    // A search that runs has nothing to try again.
    f.hosts.discovering = true;
    f.settle();
    assert!(!shows(&f, "Search again"), "{:?}", f.drawn);
}

/// A short PC list with no search running says so and offers it again, as the first run does.
#[test]
fn a_short_pc_list_with_no_search_offers_search_again() {
    let mut f = Frames::new(record("Desk", true, true), Reach::Online);
    f.hosts.discovering = false;
    f.l.go(Screen::Hosts);
    f.settle();
    assert!(!shows(&f, "Looking for other PCs on this network"), "{:?}", f.drawn);
    assert!(shows(&f, "Can’t search this network for PCs"), "{:?}", f.drawn);
    let again = f.placed.iter().find(|(t, ..)| t == "Search again").map(|p| p.1).expect("Search again");
    let commands = f.click_at(again.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::SearchAgain)));
}

/// A found PC arrives as a card with Pair, the page's one amber action, beside its name; Pair takes
/// the focus, Add PC steps down, and more PCs than a first run has make the page a list.
#[test]
fn a_found_pc_rises_in_with_pair_and_add_pc_steps_down() {
    let mut f = first_run_frames(Vec::new());
    f.l.go(Screen::Hosts);
    f.settle();
    let before = f.placed.iter().find(|(t, ..)| t == "Add PC").map(|p| p.1).expect("Add PC");
    let mut found = Hosts::without_discovery(vec![record("Game Room", false, false)], Arc::new(|| {}));
    found.discovering = true;
    found.list[0].reach = Reach::Online;
    f.hosts = found;
    let name = |f: &Frames| f.placed.iter().find(|(t, r, _)| t == "Game Room" && r.top() > chrome::PAGE_TOP).map(|p| p.1).unwrap_or_else(|| panic!("{:?}", f.drawn));
    f.frame(Vec::new());
    f.frame(Vec::new());
    let rising = name(&f);
    f.settle();
    let settled = name(&f);
    assert!(rising.top() > settled.top(), "it rises: {rising:?} to {settled:?}");
    let at = |f: &Frames, text: &str| f.placed.iter().find(|(t, ..)| t == text).map(|p| p.1).unwrap_or_else(|| panic!("{text} in {:?}", f.drawn));
    let (pair, line, add) = (at(&f, "Pair"), at(&f, "Searching for PCs\u{2026}"), at(&f, "Add PC"));
    assert!(settled.right() <= pair.left() && pair.center().y > settled.top(), "Pair in the card's row, after the name: {settled:?} {pair:?}");
    assert!(pair.bottom() <= line.top() && line.bottom() <= add.top(), "{pair:?} {line:?} {add:?}");
    assert!(add.top() != before.top(), "Add PC stepped down");
    assert_eq!(f.placed.iter().filter(|(t, ..)| t == "PCs").count(), 1, "the tab's alone, no page title: {:?}", f.drawn);
    let focused = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).expect("Pair has focus").rect;
    assert!(focused.contains(pair.center()), "{focused:?}");
    let commands = f.press(Key::Enter);
    let id = f.hosts.list[0].id;
    assert!(commands.iter().any(|c| matches!(c, Command::Pair(host) if *host == id)));
    // Paired, it is the ordinary list.
    let mut paired = Hosts::without_discovery(vec![record("Game Room", true, false)], Arc::new(|| {}));
    paired.discovering = true;
    paired.list[0].reach = Reach::Online;
    f.hosts = paired;
    f.settle();
    assert!(!shows(&f, "Searching for PCs\u{2026}"), "{:?}", f.drawn);
}

/// How to connect stays on the first run, searching and found: under Add PC, centred on it, by
/// pointer and by keyboard; Tab scrolls it into a short window.
#[test]
fn the_first_run_links_how_to_connect_in_both_states() {
    let link = |f: &Frames| f.placed.iter().find(|(t, ..)| t == "How to connect").map(|p| p.1);
    let add = |f: &Frames| f.placed.iter().find(|(t, ..)| t == "Add PC").map(|p| p.1).expect("Add PC");
    let opens = |commands: &[Command]| commands.iter().any(|c| matches!(c, Command::OpenGuide("pairing.md")));
    for found in [false, true] {
        for size in [vec2(1280.0, 800.0), vec2(853.0, 480.0)] {
            let mut f = first_run_frames(if found { vec![record("Game Room", false, false)] } else { Vec::new() });
            if found {
                f.hosts.list[0].reach = Reach::Online;
            }
            f.size = size;
            f.l.go(Screen::Hosts);
            f.settle();
            assert_eq!(shows(&f, "Pair"), found, "{:?}", f.drawn);
            let mut tabbed = false;
            for _ in 0..8 {
                f.press(Key::Tab);
                let focus = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i));
                let on_link = link(&f).filter(|at| focus.is_some_and(|r| r.rect.contains(at.center())));
                if let Some(at) = on_link {
                    let under = add(&f);
                    assert!(at.top() >= under.bottom(), "{at:?} under {under:?}");
                    assert!((at.center().x - under.center().x).abs() < 16.0, "{at:?} over {under:?}");
                    assert!(at.bottom() <= size.y, "{at:?} in {size:?}");
                    assert!(opens(&f.press(Key::Enter)), "Enter on the link");
                    tabbed = true;
                    break;
                }
            }
            assert!(tabbed, "Tab reaches it, found {found}");
        }
    }
}

/// A saved PC is on Home from the first frame, before it has answered: the first run never flashes on a start.
#[test]
fn a_saved_pc_that_has_not_answered_yet_is_not_the_first_run() {
    let mut f = first_run_frames(vec![record("Desk", true, false)]);
    f.l.go(Screen::Home);
    f.settle();
    assert_eq!(f.l.screen, Screen::Home);
    assert!(!shows(&f, "Searching for PCs\u{2026}"), "{:?}", f.drawn);
}

/// A saved PC that answers as offline with no library saved is still its card on Home, with Wake in its options.
#[test]
fn an_offline_pc_with_nothing_saved_is_a_card_not_the_first_run() {
    let mut f = Frames::new(record("Den", true, true), Reach::Offline);
    f.l.go(Screen::Home);
    f.settle();
    assert_eq!(f.l.screen, Screen::Home);
    assert!(shows(&f, "PCs") && shows(&f, "Den"), "{:?}", f.drawn);
    assert!(host_options(&f.hosts.list[0]).contains(&HostOption::Wake));
}

/// A settings row stays a readable width on a wide window instead of stretching across it.
#[test]
fn rows_keep_a_readable_width_on_a_wide_window() {
    let ctx = egui::Context::default();
    let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))), ..Default::default() };
    let (label, description) = ("Label", "Description");
    let mut span = 0.0;
    let mut output = ctx.run_ui(input, |ui| {
        let left = ui.max_rect().left();
        span = row(ui, label, description, |ui| ui.button("x")).rect.right() - left;
    });
    output.textures_delta.clear();
    assert!(span <= SETTINGS_W && span > SETTINGS_W - 60.0, "the row's control ends at {span}");
}

/// The game page's quit on leave switch, now a row like every setting, still sends the choice.
#[test]
fn the_quit_on_leave_switch_sends_the_choice() {
    let mut f = home_with_games(1);
    f.ctx.enable_accesskit();
    let pc = f.hosts.list[0].id;
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![nightjar_policy::http::App { name: "Game 00".into(), uuid: "install-0".into(), ..game(1000) }]));
    f.l.go(Screen::Game { host: pc, app: 1000 });
    f.settle();
    let bounds = f.access.as_ref().expect("AccessKit is on").nodes.iter()
        .find(|(_, n)| n.label().is_some_and(|l| l.starts_with("Quit game when I leave")))
        .and_then(|(_, n)| n.bounds())
        .expect("the switch has the row's name");
    let at = Pos2::new(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32);
    let commands = f.click_at(at, egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::SetQuitOnLeave { enabled: true, .. })), "a click turns it on");
}

#[test]
fn a_pairing_that_was_cancelled_never_shows_an_error() {
    let mut f = Frames::new(record("PC", false, false), Reach::Online);
    let id = f.hosts.list[0].id;
    let waiting = |f: &mut Frames| {
        f.l.dialog = Dialog::Pairing { host: id, pin: "1234".into() };
        f.l.focus_dialog = true;
        f.settle();
    };
    waiting(&mut f);
    let commands = f.press(Key::Escape);
    assert!(commands.iter().any(|c| matches!(c, Command::CancelPair)), "Escape cancels the request");
    assert!(!f.l.pairing_finished(id, Err(crate::hosts::PAIR_NETWORK.into())), "the late failure");
    assert!(f.no_dialog(), "is not shown");
    assert!(!f.l.pairing_finished(id, Ok(())), "and a late success does not open the library");
    assert!(f.no_dialog());
    // A pairing the user still waits for does report both.
    waiting(&mut f);
    assert!(!f.l.pairing_finished(id, Err(crate::hosts::PAIR_PIN_WRONG.into())));
    assert!(matches!(f.l.dialog, Dialog::PairError { one_time: false, .. }));
    waiting(&mut f);
    assert!(f.l.pairing_finished(id, Ok(())));
    assert!(f.no_dialog());
    // Another PC's answer is not this dialog's.
    waiting(&mut f);
    assert!(!f.l.pairing_finished(id + 1, Err(crate::hosts::PAIR_NETWORK.into())));
    assert!(matches!(f.l.dialog, Dialog::Pairing { .. }));
}

#[test]
fn a_failed_pairing_offers_to_try_again() {
    let mut f = Frames::new(record("PC", false, false), Reach::Online);
    let id = f.hosts.list[0].id;
    f.l.pair_failed(id, crate::hosts::PAIR_PIN_WRONG, false);
    f.settle();
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Pair(host) if *host == id)), "Try again pairs again");
    f.l.pair_failed(id, crate::hosts::PAIR_OTP_PIN_WRONG, true);
    f.settle();
    f.press(Key::Enter);
    assert!(matches!(f.l.dialog, Dialog::Otp { .. }), "a one-time PIN failure goes back to the PIN form");
}

#[test]
fn add_pc_takes_a_pasted_pairing_link_or_one_from_the_clipboard() {
    let mut f = Frames::new(record("PC", false, false), Reach::Online);
    // A link that reads: Add PC keeps one that does not.
    let link = format!("rubylight://pair?v=1&host=192.168.4.2&port=47989&otp=4821&fp={}", "00".repeat(32));
    f.l.dialog = Dialog::AddHost { text: link.clone(), error: None };
    f.l.focus_dialog = true;
    f.settle();
    let add = f.placed.iter().rev().find(|(t, ..)| t == tr("Add")).map(|(_, rect, _)| rect.center()).expect("Add is drawn");
    let commands = f.click_at(add, egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::AddHost(text) if *text == link)), "the link goes to Add PC as typed");
    assert!(f.no_dialog());

    f.l.dialog = Dialog::AddHost { text: String::new(), error: None };
    f.settle();
    let paste = f.placed.iter().rev().find(|(t, ..)| t == tr("Paste link")).map(|(_, rect, _)| rect.center()).expect("Paste link is drawn");
    let commands = f.click_at(paste, egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::AddFromClipboard)), "Paste link reads the clipboard");
    assert!(f.no_dialog());
}

#[test]
fn add_pc_keeps_an_address_that_does_not_read_with_its_reason() {
    let mut f = Frames::new(record("PC", false, false), Reach::Online);
    for (typed, why) in [("den:notaport", crate::hosts::ADDRESS_INVALID), ("rubylight://pair?v=1", crate::hosts::LINK_INVALID)] {
        f.l.dialog = Dialog::AddHost { text: typed.into(), error: None };
        f.l.focus_dialog = true;
        f.settle();
        let add = f.placed.iter().rev().find(|(t, ..)| t == tr("Add")).map(|(_, rect, _)| rect.center()).expect("Add is drawn");
        let commands = f.click_at(add, egui::PointerButton::Primary);
        assert!(!commands.iter().any(|c| matches!(c, Command::AddHost(_))), "{typed}: nothing is added");
        f.settle();
        assert!(matches!(&f.l.dialog, Dialog::AddHost { text, error: Some(e) } if text == typed && e == tr(why)), "{typed}: the dialog stays with the words and the reason");
        assert!(shows(&f, tr(why)), "{typed}: the reason is drawn: {:?}", f.drawn);
    }
}

/// Screenshots of the main pages in both palettes, painted on the CPU from made-up PCs (no
/// network): `NIGHTJAR_SHOTS=<dir> cargo test -p nightjar-desktop page_screenshots -- --ignored`.
#[test]
fn a_pc_that_is_up_reads_green_and_amber_never_stands_for_a_state() {
    crate::theme::own_palette();
    let records = vec![record("Up", true, true), record("Playing", true, true), record("Down", true, true), record("New", false, false), record("Unsure", true, true)];
    let mut hosts = Hosts::without_discovery(records, Arc::new(|| {}));
    hosts.list[0].reach = Reach::Online;
    hosts.list[0].info = Some(answer(true, 0));
    hosts.list[1].reach = Reach::Online;
    hosts.list[1].info = Some(answer(true, 7));
    hosts.list[2].reach = Reach::Offline;
    hosts.list[3].reach = Reach::Online;
    hosts.list[3].info = Some(answer(false, 0));
    hosts.list[4].reach = Reach::Unknown;
    let looks: Vec<_> = hosts.list.iter().map(host_status).collect();
    for (host, (words, color, ring)) in hosts.list.iter().zip(&looks) {
        assert!(!words.is_empty(), "{}: a state is always a word", host.record.name);
        assert_ne!(**color, *MOON, "{}: amber is the action, not a state", host.record.name);
        assert_ne!(**ring, *MOON, "{}: amber is the action, not a state", host.record.name);
    }
    assert_eq!([*looks[0].1, *looks[1].1], [*OK, *OK], "up and paired, playing or not, is green");
    assert_eq!(*looks[2].1, *OUTLINE, "offline is quiet");
    assert_eq!(*looks[3].1, *TERTIARY, "not paired is information");
    assert_ne!(*looks[4].2, *OK, "a PC still being checked is not shown as up");
}

#[test]
fn a_running_cover_wears_a_green_dot_and_no_amber() {
    crate::theme::own_palette();
    let mut out = Vec::new();
    running_dot(&mut out, Rect::from_min_size(Pos2::ZERO, vec2(160.0, 240.0)));
    let fills: Vec<Color32> = out.iter().filter_map(|shape| match shape {
        egui::Shape::Circle(c) => Some(c.fill),
        _ => None,
    }).collect();
    assert!(fills.contains(&*OK), "a green dot: {fills:?}");
    assert!(!fills.contains(&*MOON), "and no amber: {fills:?}");
}

#[test]
fn primary_and_quiet_buttons_take_the_control_height() {
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut heights = [0.0; 2];
    for n in 0..2 {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))), time: Some(f64::from(n)), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            heights = [primary_button(ui, "Play").rect.height(), quiet_button(ui, "Cancel").rect.height()];
        });
        output.textures_delta.clear();
    }
    assert_eq!(heights, [CONTROL_HEIGHT; 2]);
}

#[test]
fn a_switched_off_primary_button_is_grey_not_faded_amber() {
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut fills = Vec::new();
    for enabled in [true, false] {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0))), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            ui.add_enabled_ui(enabled, |ui| primary_button(ui, "Play"));
        });
        output.textures_delta.clear();
        // Amber whatever the fade: red well above blue (the surfaces are blue-black).
        let is_amber = |c: Color32| f32::from(c.r()) > 1.6 * f32::from(c.b()) + 10.0;
        let painted: Vec<Color32> = output
            .shapes
            .iter()
            .flat_map(|clipped| match &clipped.shape {
                egui::Shape::Mesh(mesh) => mesh.vertices.iter().map(|v| v.color).filter(|c| c.a() > 0).collect(),
                _ => Vec::new(),
            })
            .collect();
        let amber = painted.iter().any(|&c| is_amber(c));
        fills.push((enabled, amber, painted));
    }
    assert!(fills[0].1, "an enabled Play is amber: {:?}", fills[0].2);
    assert!(!fills[1].1 && !fills[1].2.is_empty(), "a switched-off Play is drawn, but not in amber: {:?}", fills[1].2);
}

#[test]
fn a_long_pc_name_ends_in_an_ellipsis_inside_the_card_and_the_tooltip_shows_it_whole() {
    for zoomed in [false, true] {
        let full = "The Very Long Name Of The Living Room Gaming PC Under The Television";
        let mut f = Frames::new(record(full, true, true), Reach::Online);
        if zoomed {
            fit_150(&mut f);
            f.size = vec2(720.0 / 1.5, 480.0 / 1.5);
        }
        f.settle();
        let card = pc_card_rect(&f);
        assert!(card.right() <= f.size.x, "{zoomed}: the card {card:?} fits the window");
        let (line, rect) = visible(&f).into_iter().find(|(t, _)| t.starts_with("The Very")).expect("the name is drawn");
        assert!(line.ends_with('\u{2026}'), "{zoomed}: the name is cut: {line}");
        assert!(rect.right() <= card.right() + 0.5, "{zoomed}: {line} ends at {} in a card ending at {}", rect.right(), card.right());
        // Up to the card's top, away from its action and More.
        let over = pos2(card.left() + 8.0, card.top() + 8.0);
        f.frame(vec![egui::Event::PointerMoved(over)]);
        for _ in 0..30 {
            f.frame(Vec::new());
        }
        assert!(visible(&f).iter().any(|(t, _)| t.starts_with(full)), "{zoomed}: the tooltip shows it whole: {:?}", visible(&f));
    }
}

#[test]
fn a_short_pc_name_is_drawn_whole_with_no_tooltip() {
    let mut f = Frames::new(record("Den", true, true), Reach::Online);
    let card = pc_card_rect(&f);
    hover_card(&mut f, card);
    let shown = visible(&f);
    assert!(shown.iter().any(|(t, _)| t == "Den"), "{shown:?}");
    assert!(!shown.iter().any(|(t, _)| t.starts_with("Den, ")), "no tooltip: {shown:?}");
}
