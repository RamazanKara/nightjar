//! Game detail page profiles, neural rendering switches, and art rows.

use super::*;

/// A PC's one action says what the screen and the line say, and does it.
#[test]
fn a_pc_offers_the_one_action_its_state_calls_for() {
    let action_of = |f: &Frames| crate::ui::pcs::pc_action(&f.hosts.list[0]);
    let line_of = |f: &Frames| crate::ui::pcs::pc_line_of(&f.hosts.list[0]);
    use crate::ui::pcs::{PcAction, PcLine};
    let click_action = |f: &mut Frames| {
        let at = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).expect("the action has focus").rect.center();
        f.click_at(at, egui::PointerButton::Primary)
    };
    // Up and paired, idle: Open, a green "Online", and no made-up latency.
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), vec![game(7)]);
    assert_eq!(action_of(&f), PcAction::Open);
    assert_eq!(line_of(&f), PcLine::Online);
    assert!(shows(&f, "Open") && shows(&f, "Online") && !f.drawn.iter().any(|t| t.ends_with(" ms")), "{:?}", f.drawn);
    let id = f.hosts.list[0].id;
    assert!(click_action(&mut f).iter().any(|c| matches!(c, Command::OpenApps(h) if *h == id)));

    // Running a game of its known list: Resume starts it again, the line names the game.
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 7)), vec![game(7)]);
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![game(7)]));
    f.settle();
    assert_eq!(action_of(&f), PcAction::Resume(7));
    assert!(matches!(line_of(&f), PcLine::Running(_)));
    assert!(shows(&f, "Resume"), "{:?}", f.drawn);
    let commands = click_action(&mut f);
    assert!(commands.iter().any(|c| matches!(c, Command::Launch { app: 7, resume: true, .. })));

    // Running but the list is not known: it asks for it, and offers what still works.
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 7)), Vec::new());
    f.hosts.list[0].apps = Apps::NotLoaded;
    let commands = f.settle();
    assert_eq!(action_of(&f), PcAction::Open);
    assert!(commands.iter().any(|c| matches!(c, Command::LoadApps(_))));

    // Asleep with a saved address: Wake up, as its options say. Without one: Try again, which checks again.
    let mut f = Frames::new(HostRecord { mac: Some("00:1a:2b:3c:4d:5e".into()), ..record("Den", true, true) }, Reach::Offline);
    assert_eq!((action_of(&f), line_of(&f)), (PcAction::Wake, PcLine::Offline));
    assert!(shows(&f, "Wake up") && shows(&f, "Offline"), "{:?}", f.drawn);
    assert!(click_action(&mut f).iter().any(|c| matches!(c, Command::Wake(_))));
    let mut f = Frames::new(HostRecord { mac: None, ..record("Den", true, true) }, Reach::Offline);
    assert_eq!(action_of(&f), PcAction::TryAgain);
    assert!(click_action(&mut f).iter().any(|c| matches!(c, Command::Refresh(_))));

    // Up and not paired: Pair, and the lock says why.
    let mut f = Frames::new(record("Den", false, false), Reach::Online);
    assert_eq!((action_of(&f), line_of(&f)), (PcAction::Pair, PcLine::NotPaired));
    assert!(shows(&f, "Not paired"), "{:?}", f.drawn);
    assert!(click_action(&mut f).iter().any(|c| matches!(c, Command::Pair(_))));
}

/// An offline PC says why it did not answer in its options, and Try again shows Checking… on the
/// card until the PC answers.
#[test]
fn an_offline_pc_says_why_and_try_again_shows_checking() {
    let refused = "The PC refused the connection. Is its streaming app running?";
    let mut f = Frames::new(HostRecord { mac: None, ..record("Den", true, true) }, Reach::Offline);
    f.hosts.list[0].error = Some(refused.into());
    f.settle();
    assert!(shows(&f, "Offline"), "{:?}", f.drawn);
    f.press(Key::F2);
    assert!(f.options_open());
    assert!(shows(&f, refused), "the options name the reason: {:?}", f.drawn);
    f.press(Key::Escape);
    f.hosts.list[0].checking = true;
    f.settle();
    assert!(shows(&f, "Checking…") && !shows(&f, "Offline"), "{:?}", f.drawn);
    assert!(crate::ui::pcs::offline_reason(&f.hosts.list[0]).is_none(), "no old reason while it checks");
}

/// The rest of the card still does what selecting a PC always did.
#[test]
fn selecting_a_pcs_card_opens_it_and_a_right_click_offers_its_menu() {
    let mut f = Frames::new(record("Den", true, true), Reach::Online);
    let card = pc_card_rect(&f);
    let commands = f.click_at(card.right_top() + vec2(-20.0, 20.0), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::OpenApps(_))));
    let mut f = Frames::new(record("Den", true, true), Reach::Online);
    let card = pc_card_rect(&f);
    f.click_at(card.right_top() + vec2(-20.0, 20.0), egui::PointerButton::Secondary);
    assert!(f.options_open(), "its menu");
}

/// Tab meets each card's action, then its More. The card itself takes no focus.
#[test]
fn a_pcs_card_has_two_stops_for_focus() {
    let mut f = Frames::new(record("Den", true, true), Reach::Online);
    let (action, more) = f.card_and_dots();
    let card = pc_card_rect(&f);
    assert!(card.contains_rect(action) && card.contains_rect(more), "{action:?} {more:?} in {card:?}");
    assert!((action.left() - card.left() - 22.0).abs() < 0.5 && (card.right() - more.right() - 22.0).abs() < 0.5, "{action:?} {more:?} in {card:?}");
    assert_eq!((more.width(), more.height()), (48.0, 48.0));
    f.press(Key::Tab);
    let focused = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).unwrap();
    assert_eq!(focused.rect, more, "More comes after the action");
}

/// The halo of a focused action paints over the card beside it, so no neighbour cuts it.
#[test]
fn a_focused_actions_halo_paints_after_the_next_card() {
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut l = launcher(false, false);
    l.go(Screen::Hosts);
    let mut second = record("Attic", true, true);
    second.address = "192.0.2.77".into();
    second.unique_id = "attic".into();
    let hosts = Hosts::without_discovery(vec![record("Den", true, true), second], Arc::new(|| {}));
    let mut f = Frames::over(ctx, l, hosts);
    f.settle();
    let ring = f.shapes.iter().position(|s| !meshes_holding(std::slice::from_ref(s), design_tokens::FOCUS_RING_COLOR).is_empty()).expect("the ring");
    let neighbour = f.shapes.iter().position(|s| {
        let mut shown = Vec::new();
        collect_visible(&s.shape, &mut shown);
        shown.iter().any(|(t, _)| t == "Attic")
    });
    assert!(neighbour.is_some_and(|at| at < ring), "ring at {ring}, the next card at {neighbour:?}");
}

/// Ctrl+K still opens the search from PCs, where the field gave way to Add PC.
#[test]
fn ctrl_k_searches_from_pcs_and_add_pc_opens_its_dialog() {
    let mut f = Frames::new(record("Den", true, true), Reach::Online);
    let add = f.placed.iter().find(|(t, ..)| t == "Add PC").map(|p| p.1).expect("Add PC");
    assert!(add.top() < 68.0, "in the bar: {add:?}");
    f.click_at(add.center(), egui::PointerButton::Primary);
    assert!(matches!(f.l.dialog, Dialog::AddHost { .. }));
    f.press(Key::Escape);
    f.frame(vec![egui::Event::Key { key: Key::K, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    f.settle();
    assert_eq!(f.l.screen, Screen::AllGames);
    assert!(f.l.search_open);
}

/// A PC's page asked for every cover of its list each pass, on screen or not and without a
/// version, while Home and All games asked for the same covers at the library's: each page had the
/// other's cover fetched again.
#[test]
fn a_pcs_page_asks_for_the_covers_on_screen_at_the_version_every_page_asks_with() {
    use rubylight_native_api::library::{Artwork, Game, Library};
    let apps: Vec<_> = (1..=150).map(|id| nightjar_policy::http::App { name: format!("App {id}"), art_version: id.to_string(), ..game(id) }).collect();
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), apps);
    f.size = vec2(1280.0, 800.0);
    let id = f.hosts.list[0].id;
    let games = (1..=3u32)
        .map(|n| Game {
            install_id: format!("install-{n}"),
            app_id: n,
            title: format!("App {n}"),
            game_id: Some(format!("steam:{n}")),
            store: None,
            installed: Some(true),
            running: false,
            artwork: Artwork { cover: Some(format!("/cover/{n}")), version: format!("cover-{n}") },
            last_played: None,
            playtime_minutes: None,
            collections: Vec::new(),
        })
        .collect();
    f.hosts.list[0].native.library = Some(Arc::new(Library { revision: "r1".into(), games }));
    let covers = |commands: Vec<Command>| -> std::collections::BTreeMap<i32, String> {
        let asked = commands.into_iter().filter_map(|c| match c {
            Command::RequestArt { host, app, version } if host == id => Some((app, version)),
            _ => None,
        });
        asked.collect()
    };
    let asked = |f: &mut Frames, screen: Screen| {
        f.l.go(screen);
        f.settle();
        covers(f.frame(Vec::new()))
    };

    let mut page = asked(&mut f, Screen::Apps(id));
    assert_eq!(page[&1], "cover-1", "the library's version of a game it lists");
    assert_eq!(page[&4], "4", "the app list's of an app it leaves out");
    assert!(page.len() < 100 && !page.contains_key(&150), "only the covers on screen: {:?}", page.keys());
    // The running game lights the page from its cover, wherever its card is: the last one here,
    // with its neighbour in that row still off screen.
    f.hosts.list[0].info = Some(answer(true, 150));
    let lit = covers(f.frame(Vec::new()));
    assert!(lit.get(&150).is_some_and(|v| v == "150") && !lit.contains_key(&149), "{:?}", lit.keys());
    page.extend(lit);

    let elsewhere = [asked(&mut f, Screen::AllGames), asked(&mut f, Screen::Home), asked(&mut f, Screen::Hosts)];
    assert!(elsewhere[0].contains_key(&1) && elsewhere[2].contains_key(&150), "{elsewhere:?}");
    for (app, version) in elsewhere.iter().flatten() {
        assert_eq!(page.get(app), Some(version), "app {app}");
    }

    // The covers the PC's page loaded are asked for again by no page.
    let pc = &mut f.hosts.list[0];
    for (app, version) in &page {
        assert_eq!(pc.begin_art(*app, version).as_ref(), Some(version));
    }
    for screen in [Screen::Apps(id), Screen::AllGames, Screen::Home, Screen::Hosts] {
        let again = asked(&mut f, screen);
        assert!(again.keys().all(|app| !page.contains_key(app)), "{again:?}");
    }
}

/// Nothing of Back is clipped to its own square: that once showed as a dark box around the disc.
#[test]
fn backs_shadow_and_halo_leave_its_square() {
    let mut f = store_mix();
    f.l.go(Screen::AllGames);
    f.settle();
    let back = f.ctx.read_response(Id::unique("back")).expect("Back").rect;
    assert!(f.shapes.iter().all(|s| s.clip_rect != back), "clipped to {back:?}");
}

#[test]
fn an_app_grid_has_seven_fixed_columns_and_no_labels_on_art() {
    let mut f = app_grid(15, 0);
    let id = f.hosts.list[0].id;
    for app in 2..=15 {
        let art = demo_art(&f.ctx, app as usize);
        Arc::make_mut(&mut f.hosts.get_mut(id).unwrap().art).insert(app, art);
    }
    Arc::make_mut(&mut f.hosts.get_mut(id).unwrap().art).insert(1, Art::Missing);
    f.settle();
    let (_, width) = crate::ui::cards::grid(1184.0);
    assert_eq!(width, 155.0, "seven columns at the design's width");
    // Art says its own title; no line is drawn under it, but a tile still holds the name.
    assert!(!shows(&f, "App 2"), "{:?}", f.drawn);
    assert!(f.drawn.iter().any(|t| t.contains("App 1")), "a tile holds it: {:?}", f.drawn);
    // A screen reader still hears them all: the slot is a labelled button.
    let first = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).expect("the first cover has focus").rect;
    assert_eq!(first.size(), vec2(155.0, 232.0), "the slot is the cover");
    assert_eq!(first.left(), 48.0, "at the margin");
    f.press(Key::ArrowRight);
    let second = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).unwrap().rect;
    assert_eq!(second.left() - first.right(), 16.0, "the columns' gap");
    assert_eq!(second.top(), first.top());
    for _ in 0..5 {
        f.press(Key::ArrowRight);
    }
    let last = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).unwrap().rect;
    assert_eq!((last.top(), last.left()), (first.top(), first.left() + 6.0 * (155.0 + 16.0)), "seven to a row");
    f.press(Key::ArrowDown);
    for _ in 0..6 {
        f.press(Key::ArrowLeft);
    }
    let below = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).unwrap().rect;
    assert_eq!(below.left(), first.left(), "the next row starts under it");
    assert_eq!(below.top() - first.bottom(), 18.0, "the rows' gap");
}

#[test]
fn a_cover_without_its_art_yet_holds_its_name() {
    let f = app_grid(3, 0);
    assert!(!f.hosts.list[0].art.contains_key(&2));
    assert!(shows(&f, "App 2"), "the placeholder names it: {:?}", f.drawn);
}

#[test]
fn a_running_app_offers_resume_and_quit_game_beside_its_name() {
    let mut f = app_grid(5, 3);
    let name = "App 3";
    assert_eq!(f.drawn.iter().filter(|t| **t == name).count(), 2, "its chip and its cover: {:?}", f.drawn);
    assert!(shows(&f, "Resume") && shows(&f, "Quit game"), "{:?}", f.drawn);
    for gone in ["Refresh", "PC settings"] {
        assert!(!shows(&f, gone), "{gone} lives in the PC's menu now: {:?}", f.drawn);
    }
    let at = |f: &Frames, word: &str| f.placed.iter().find(|(t, ..)| t == word).map(|p| p.1).unwrap();
    let (resume, quit) = (at(&f, "Resume"), at(&f, "Quit game"));
    assert!(resume.right() < quit.left(), "{resume:?} {quit:?}");
    assert!(1280.0 - quit.right() < 140.0, "at the right edge, by the round Refresh: {quit:?}");
    // The row does not grow for them: what runs does not move the grid.
    let top = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).expect("the running cover has focus").rect.top();
    let idle = app_grid(5, 0);
    let idle_top = idle.ctx.memory(|m| m.focused()).and_then(|i| idle.ctx.read_response(i)).unwrap().rect.top();
    assert_eq!(top, idle_top);
    // Resume starts it again; Quit game asks first.
    let commands = f.click_at(resume.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::Launch { app: 3, resume: true, .. })));
    let quit = at(&f, "Quit game");
    f.click_at(quit.center(), egui::PointerButton::Primary);
    assert!(matches!(f.l.dialog, Dialog::ConfirmQuit { start: None, .. }));
}

#[test]
fn a_narrow_window_lets_the_running_name_go_before_the_title() {
    let mut f = app_grid(5, 3);
    f.size = vec2(560.0, 700.0);
    f.settle();
    f.settle();
    let name = "App 3";
    assert!(shows(&f, "Den"), "the title stays: {:?}", f.drawn);
    assert!(shows(&f, "Resume") && shows(&f, "Quit game"), "{:?}", f.drawn);
    let chips = visible(&f).iter().filter(|(t, r)| *t == name && r.top() < 150.0).count();
    assert_eq!(chips, 0, "no chip in the row: {:?}", visible(&f));
}

#[test]
fn a_running_app_the_list_does_not_know_can_be_resumed_too() {
    let mut f = app_grid(5, 99);
    f.settle();
    assert!(shows(&f, "Resume") && shows(&f, "Quit game") && shows(&f, "The running app"), "{:?}", f.drawn);
    let resume = f.placed.iter().find(|(t, ..)| t == "Resume").map(|p| p.1).expect("Resume");
    let commands = f.click_at(resume.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::Launch { app: 99, resume: true, .. })), "the game itself, not a control tile");
}

#[test]
fn the_pc_capsule_speaks_of_the_pc_whose_page_it_is() {
    let mut other = record("Attic", true, true);
    other.address = "192.0.2.77".into();
    other.unique_id = "attic".into();
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut l = launcher(false, false);
    l.go(Screen::Hosts);
    let mut hosts = Hosts::without_discovery(vec![record("Den", true, true), other], Arc::new(|| {}));
    for host in &mut hosts.list {
        host.reach = Reach::Online;
        host.apps = Apps::Loaded(Arc::new(vec![game(1)]));
    }
    let attic = hosts.list[1].id;
    let mut f = Frames::over(ctx, l, hosts);
    f.l.go(Screen::Apps(attic));
    f.settle();
    let in_bar = |f: &Frames, word: &str| f.placed.iter().any(|(t, r, _)| t == word && r.top() < 68.0);
    assert!(in_bar(&f, "Attic") && !in_bar(&f, "Den"), "{:?}", f.placed);
}

#[test]
fn the_capsule_opens_the_pcs_with_two_or_more() {
    let (mut f, [den, ..]) = three_pcs();
    f.click_at(capsule_at(&f, "Den"), egui::PointerButton::Primary);
    assert!(matches!(f.l.dialog, Dialog::PcSheet { current } if current == den));
    for word in ["PCs", "Den", "Online", "Attic", "Not paired", "Cellar", "Offline"] {
        assert!(shows(&f, word), "{word}: {:?}", f.drawn);
    }
    assert_eq!(focused_row(&f), None, "opened by the pointer, no row wears the ring");
    f.press(Key::ArrowDown);
    assert_eq!(focused_row(&f).as_deref(), Some("Den"), "the first key starts on the first row");
}

#[test]
fn the_capsule_of_the_only_pc_opens_its_options() {
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), Vec::new());
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![game(1)]));
    let id = f.hosts.list[0].id;
    f.l.go(Screen::Apps(id));
    f.settle();
    f.click_at(capsule_at(&f, "Den"), egui::PointerButton::Primary);
    assert!(f.options_open());
}

#[test]
fn the_capsules_options_stay_on_their_keys_and_the_right_button() {
    let (mut f, [den, ..]) = three_pcs();
    focus_capsule(&mut f);
    for key in [Key::F, Key::F2] {
        f.press(key);
        assert!(matches!(f.l.dialog, Dialog::HostOptions { host } if host == den), "{key:?}");
        f.press(Key::Escape);
        assert!(f.no_dialog());
        f.settle();
    }
    f.click_at(capsule_at(&f, "Den"), egui::PointerButton::Secondary);
    assert!(matches!(f.l.dialog, Dialog::HostOptions { host } if host == den));
}

#[test]
fn keys_walk_the_pcs_from_the_first_row_and_back_to_the_capsule() {
    let (mut f, [den, ..]) = three_pcs();
    f.ctx.enable_accesskit();
    focus_capsule(&mut f);
    f.press(Key::Enter);
    assert!(matches!(f.l.dialog, Dialog::PcSheet { current } if current == den));
    assert_eq!(focused_row(&f).as_deref(), Some("Den"), "by keys the ring starts on the first row");
    f.press(Key::ArrowDown);
    assert_eq!(focused_row(&f).as_deref(), Some("Attic"));
    let tree = f.access.as_ref().expect("AccessKit is on");
    // The capsule above says the same of the PC ("Den, Online"); the sheet's row is the one with a mark.
    let marked = |label: &str| {
        let rows: Vec<_> = tree.nodes.iter().filter(|(_, n)| n.label() == Some(label)).collect();
        assert!(!rows.is_empty(), "{label}");
        rows.iter().find_map(|(_, n)| n.toggled())
    };
    assert_eq!(marked("Den, Online"), Some(egui::accesskit::Toggled::True), "the mark is read out too");
    assert_eq!(marked("Attic, Not paired"), Some(egui::accesskit::Toggled::False));
    f.press(Key::Escape);
    f.settle();
    assert!(f.no_dialog());
    assert_eq!(f.ctx.memory(|m| m.focused()), Some(Id::unique("top-pc")), "the focus goes back to the capsule");
}

/// A focused row's ring was cut thin at the list's top and its glow ended in a line across the panel.
#[test]
fn a_menu_rows_ring_and_glow_end_before_the_list_cuts_them() {
    let (mut f, _) = three_pcs();
    for key in [Key::Enter, Key::F2] {
        focus_capsule(&mut f);
        f.press(key);
        f.settle();
        assert!(!f.no_dialog(), "{key:?} opens a list");
        let row = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).expect("the first row has the focus").rect;
        let (glow, clip) = meshes_drawn(&f)
            .iter()
            .find(|(mesh, _)| mesh.vertices.first().is_some_and(|v| v.color == design_tokens::FOCUS_HALO_COLOR))
            .map(|(mesh, clip)| (mesh.calc_bounds(), *clip))
            .expect("the focused row glows");
        assert!(glow.contains_rect(row.expand(design_tokens::FOCUS_GAP + design_tokens::FOCUS_RING_WIDTH)), "{glow:?} goes around the ring of {row:?}");
        assert!(clip.expand(0.6).contains_rect(glow), "{glow:?} is cut by {clip:?}");
        f.press(Key::Escape);
        f.settle();
    }
}

/// Enter on the card, on the PCs page: the card's own action.
#[test]
fn picking_a_pc_does_what_enter_does_on_its_card() {
    for (name, want) in [("Den", 0), ("Attic", 1), ("Cellar", 2)] {
        let (mut f, ids) = three_pcs();
        f.click_at(capsule_at(&f, "Den"), egui::PointerButton::Primary);
        // The sheet draws last: the page behind it may name the PC too.
        let row = f.placed.iter().rev().find(|(t, ..)| t == name).expect("its row").1.center();
        let commands = f.click_at(row, egui::PointerButton::Primary);
        assert!(f.no_dialog(), "{name}: the sheet closes");
        let id = ids[want];
        let did = commands.iter().any(|c| match want {
            0 => matches!(c, Command::OpenApps(h) if *h == id),
            1 => matches!(c, Command::Pair(h) if *h == id),
            _ => matches!(c, Command::Wake(h) if *h == id),
        });
        assert!(did, "{name} did something else");
    }
}

/// A PC with no apps has nothing to start: no Desktop tile that could only fail, one line and Refresh.
#[test]
fn a_pc_with_no_apps_shows_one_line_and_refresh() {
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), Vec::new());
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(Vec::new()));
    let id = f.hosts.list[0].id;
    f.l.go(Screen::Apps(id));
    f.settle();
    assert!(shows(&f, "This PC lists no apps.") && shows(&f, "Refresh"), "{:?}", f.drawn);
    assert!(!shows(&f, "Desktop"), "no tile to fail: {:?}", f.drawn);
    let focused = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).expect("Refresh has focus").rect;
    let at = |f: &Frames, text: &str| f.placed.iter().find(|(t, ..)| t == text).unwrap().1;
    assert!(focused.contains(at(&f, "Refresh").center()), "{focused:?}");
    assert!(at(&f, "This PC lists no apps.").bottom() < focused.top());
    let commands = f.frame(tap_key(Key::Enter).to_vec());
    assert!(commands.iter().any(|c| matches!(c, Command::Refresh(host) if *host == id)));
    assert!(!commands.iter().any(|c| matches!(c, Command::Launch { .. })));
    // While the answer is on its way the page holds still: no covers' places for a list that had none.
    let line = at(&f, "This PC lists no apps.");
    f.hosts.list[0].apps = Apps::Loading;
    f.settle();
    assert_eq!(at(&f, "This PC lists no apps."), line);
    assert!(!shows(&f, "Loading the library\u{2026}"), "{:?}", f.drawn);
    // A list with a game in it lands as any.
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![game(7)]));
    f.settle();
    assert!(!shows(&f, "This PC lists no apps.") && !shows(&f, "Refresh"), "{:?}", f.drawn);
    // Failed, it says so out loud with the way to try again.
    f.hosts.list[0].apps = Apps::Failed("The PC did not answer".into());
    f.settle();
    assert!(shows(&f, "The library could not be loaded.") && shows(&f, "The PC did not answer") && shows(&f, "Try again"), "{:?}", f.drawn);
}

/// A PC that lists a Desktop entry has an ordinary tile for it, which starts it.
#[test]
fn a_desktop_entry_the_pc_lists_is_a_normal_tile() {
    let desktop = nightjar_policy::http::App { name: "Desktop".into(), ..game(3) };
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), vec![desktop]);
    let id = f.hosts.list[0].id;
    f.l.go(Screen::Apps(id));
    f.settle();
    assert!(!shows(&f, "Refresh") && !shows(&f, "This PC lists no apps."), "{:?}", f.drawn);
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Launch { host, app: 3, resume: false } if *host == id)));
}

/// A cover's ⋮ takes the bottom right, clear of a logotype at the top or the bottom left, and the bottom
/// left on a running cover, whose dot has the other corner.
#[test]
fn a_covers_dots_sit_clear_of_its_logotype_and_its_running_dot() {
    let cover = Rect::from_min_size(pos2(100.0, 100.0), vec2(160.0, 240.0));
    for badge in [Badge::None, Badge::NotInstalled] {
        let dots = crate::ui::cards::dots_rect(cover, badge);
        assert!(cover.contains_rect(dots), "{dots:?}");
        assert!(dots.top() > cover.center().y && dots.left() > cover.center().x, "{dots:?}");
        assert!(cover.right() - dots.right() < 12.0, "in the corner: {dots:?}");
    }
    let running = crate::ui::cards::dots_rect(cover, Badge::Running);
    let status = cover.right_bottom() - Vec2::splat(18.0);
    assert!(cover.contains_rect(running) && running.right() < status.x - 10.0, "away from the running dot: {running:?}");
    assert!(running.left() - cover.left() < 12.0, "{running:?}");
}

/// Pointing at a cover shows its ⋮ in that corner, and pressing it opens the menu, on Home's shelves too.
#[test]
fn a_covers_dots_open_its_menu_on_homes_shelves() {
    let mut f = home_with_games(9);
    let host = f.hosts.list[0].record.unique_id.clone();
    f.l.favourites = vec![crate::config::Favourite { host, install_id: "install-3".into(), app_id: 1003, title: "Game 03".into() }];
    f.settle();
    // The arrows reach the favourite's cover, first of the page's "Game 03"s; its rect holds its caption.
    let mut cover = None;
    for _ in 0..8 {
        f.press(Key::ArrowDown);
        f.settle();
        let caption = f.placed.iter().find(|(t, ..)| t == "Game 03").map(|p| p.1).unwrap_or_else(|| panic!("{:?}", f.drawn));
        let rect = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).map(|r| r.rect);
        cover = rect.filter(|r| r.contains(caption.center()));
        if cover.is_some() {
            break;
        }
    }
    let cover = cover.expect("the arrows reach the favourite");
    let foot = cover.top() + cover_height(cover.width());
    let at = pos2(cover.right() - 8.0 - 15.0, foot - 8.0 - 15.0);
    f.frame(vec![egui::Event::PointerMoved(at)]);
    f.settle();
    f.click_at(at, egui::PointerButton::Primary);
    assert!(matches!(f.l.dialog, Dialog::GameMenu { .. }), "the menu, not the game's page: {:?}, cover {cover:?}", f.l.screen);
}

#[test]
fn the_list_lands_where_its_places_were_held() {
    let mut f = app_grid(9, 0);
    let id = f.hosts.list[0].id;
    let landed = f.ctx.memory(|m| m.focused()).and_then(|i| f.ctx.read_response(i)).unwrap().rect;
    f.hosts.get_mut(id).unwrap().apps = Apps::Loading;
    f.settle();
    assert!(shows(&f, "Loading the library\u{2026}"), "{:?}", f.drawn);
    let held = meshes_holding(&f.shapes, design_tokens::COVER_PLACEHOLDER);
    assert!(held.iter().any(|r| (r.center() - landed.center()).length() < 0.5 && (r.width() - landed.width()).abs() <= 1.0), "{landed:?} among {held:?}");
    assert!(held.len() >= 14, "two rows of seven: {held:?}");
}

/// While words narrow the games each chip counts what it would show, and a chip that would show
/// nothing goes: All stays, and so does the chip chosen.
#[test]
fn chip_counts_follow_the_search_and_empty_chips_go() {
    let mut f = store_mix();
    f.l.go(Screen::AllGames);
    f.settle();
    let chips = |f: &Frames| -> Vec<String> { f.drawn.iter().filter(|t| ["All", "Steam", "Epic Games"].contains(&t.as_str())).cloned().collect() };
    assert_eq!(chips(&f), ["All", "Steam", "Epic Games"]);
    let total = f.hosts.list[0].native.library.as_ref().unwrap().games.len();
    assert!(shows(&f, &total.to_string()), "{:?}", f.drawn);
    f.l.search_open = true;
    f.l.library_query = "fort".into();
    f.settle();
    assert_eq!(chips(&f), ["All", "Epic Games"], "Steam holds no Fortnite");
    assert!(!shows(&f, &total.to_string()) && f.drawn.iter().any(|t| t == "1"), "only Fortnite counts: {:?}", f.drawn);
    assert!(!shows(&f, "0"), "no chip counts nothing: {:?}", f.drawn);
    // The chosen chip stays though it holds nothing, so the focus has a place to stay.
    f.l.library_filter = console::LibraryFilter::Store("steam".into());
    f.settle();
    assert_eq!(chips(&f), ["All", "Steam", "Epic Games"]);
}

/// The grid of all games names its covers as Home's shelves do, and a focused cover's ring stays
/// clear of the chips above it.
#[test]
fn all_games_names_its_covers_and_keeps_the_ring_off_the_chips() {
    let mut f = store_mix();
    f.l.go(Screen::AllGames);
    f.settle();
    let focused = f.ctx.memory(|m| m.focused()).expect("the first cover has focus");
    let slot = f.ctx.read_response(focused).expect("the cover").rect;
    assert_eq!(slot.height(), super::cards::cover_height(slot.width()) + super::cards::POSTER_CAPTION, "its name has its room under it");
    let shown: Rect = f.ctx.data(|d| d.get_temp(focused.with("shown"))).expect("the risen cover");
    let all = f.ctx.read_response(Id::unique(("all-games", &console::LibraryFilter::All.key()))).expect("the All chip").rect;
    let ring = design_tokens::HC_FOCUS_RING_WIDTH + 3.0;
    assert!(shown.top() - ring >= all.bottom() + 4.0, "ring {} vs chips {}", shown.top() - ring, all.bottom());
}

/// The light on a placeholder comes in from above it and leaves below it whole.
#[test]
fn the_placeholders_light_runs_from_above_to_below() {
    assert!(super::cards::shimmer_middle(0.0) < -0.3);
    assert!(super::cards::shimmer_middle(1.0) > 1.3);
    assert!(super::cards::shimmer_middle(0.5) > 0.4 && super::cards::shimmer_middle(0.5) < 0.6);
}
