//! Card hover/focus painting, app grid layout, and controller navigation tests.

use super::*;

/// Issue 55: on a game's (or PC's, or profile's) settings an unchecked row's menu was disabled,
/// so a controller's focus could only land on its Override box, and A there pinned the value it
/// inherited. The menu itself takes focus now; opening it and backing out keeps the row
/// inherited, choosing another value checks Override, and the box still unchecks it.
#[test]
fn an_inherited_row_opens_from_a_controller_and_only_a_new_value_overrides_it() {
    let mut f = Frames::new(record("PC", true, false), Reach::Online);
    f.size = vec2(1280.0, 4000.0);
    f.l.settings.fps = 120;
    let id = f.hosts.list[0].id;
    f.l.go(Screen::Apps(id));
    f.l.open_app_settings(id, &f.hosts.list[0], game(7));
    f.settle();
    assert!(focus_down_to(&mut f, "120 fps"), "Down reaches the inherited Frame rate menu");
    f.press(Key::Enter);
    assert!(shows(&f, "60 fps") && shows(&f, "240 fps"), "A opens it: {:?}", f.drawn);
    // B closes the menu only; the next B leaves the page.
    let commands = f.press(Key::Escape);
    assert!(!shows(&f, "240 fps"), "B closes the menu: {:?}", f.drawn);
    assert!(!commands.iter().any(|c| matches!(c, Command::SaveAppSettings(_))), "and stays on the page");
    assert!(f.l.edit_overrides.is_empty(), "backing out pins nothing: {:?}", f.l.edit_overrides);
    let commands = f.press(Key::Escape);
    assert!(commands.iter().any(|c| matches!(c, Command::SaveAppSettings(_))), "B with no menu open leaves the page");

    f.l.open_app_settings(id, &f.hosts.list[0], game(7));
    f.settle();
    assert!(f.l.edit_overrides.is_empty());
    let at = |f: &Frames, text: &str| f.placed.iter().find(|(t, ..)| t == text).map(|p| p.1).unwrap_or_else(|| panic!("{text} in {:?}", f.drawn));
    click_text(&mut f, "120 fps");
    click_text(&mut f, "60 fps");
    assert!(f.l.edit_overrides.has(&["fps"]), "a new value overrides the row");
    assert_eq!(f.l.edit_overrides.apply(&f.l.inherited_settings).fps, 60);
    // The Frame rate row's own box, the nearest "For this game" under its menu, gives it back.
    let menu = at(&f, "60 fps");
    let boxed = f.placed.iter().filter(|(t, r, _)| t == "For this game" && r.top() >= menu.bottom()).map(|p| p.1)
        .min_by(|a, b| (a.top() - menu.bottom()).total_cmp(&(b.top() - menu.bottom()))).expect("its box");
    f.click_at(boxed.center(), egui::PointerButton::Primary);
    assert!(f.l.edit_overrides.is_empty(), "{:?}", f.l.edit_overrides);
    assert_eq!(f.l.edit_settings.fps, 120);
    assert!(shows(&f, "120 fps"), "{:?}", f.drawn);
}

#[test]
fn an_inherited_menu_item_chosen_with_a_controller_overrides_the_row() {
    pick_inherited_frame_rate(false);
}

#[test]
fn an_inherited_menu_item_chosen_by_a_screen_reader_overrides_the_row() {
    pick_inherited_frame_rate(true);
}

/// Review of issue 55's fix: a number field steps on Left and Right and consumes them, so an
/// inherited Custom frame rate (a Deck's 40 or 45) stepped with the D-pad was drawn and dropped.
#[test]
fn stepping_an_inherited_number_field_with_a_controller_overrides_it() {
    let mut f = Frames::new(record("PC", true, false), Reach::Online);
    f.size = vec2(1280.0, 4000.0);
    f.l.settings.fps = 45;
    let id = f.hosts.list[0].id;
    f.l.go(Screen::Apps(id));
    f.l.open_app_settings(id, &f.hosts.list[0], game(7));
    f.settle();
    assert!(focus_down_to(&mut f, "45"), "Down reaches the inherited Custom frame rate field: {:?}", f.drawn);
    f.press(Key::ArrowRight);
    assert!(f.l.edit_overrides.has(&["fps"]), "the step overrides the row: {:?}", f.l.edit_overrides);
    assert_eq!(f.l.edit_overrides.apply(&f.l.inherited_settings).fps, 46);
    let commands = f.press(Key::Escape);
    assert!(commands.iter().any(|c| matches!(c, Command::SaveAppSettings(_))), "B leaves and saves");
    assert_eq!(f.l.edit_overrides.apply(&f.l.inherited_settings).fps, 46, "what the save writes");
}

/// Issue 55: a frame rate a PC or profile overrides looked like the main settings' from a game,
/// whose "Reset all to inherited" cannot clear it. Unchecked rows and the game page name it.
#[test]
fn a_value_from_a_pc_or_profile_says_so_on_the_game_settings_and_page() {
    let mut f = home_with_games(1);
    f.size = vec2(1280.0, 4000.0);
    let pc = f.hosts.list[0].id;
    f.l.settings.fps = 60;
    let travel = f.l.profiles.create("Travel", serde_json::from_str(r#"{"bitrate_kbps":20000}"#).unwrap()).unwrap();
    f.l.profiles.active = Some(travel);
    f.hosts.list[0].record.settings = Some(serde_json::from_str(r#"{"fps":120}"#).unwrap());
    f.hosts.list[0].info = Some(answer(true, 1000));
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![nightjar_policy::http::App { name: "Game 00".into(), uuid: "install-0".into(), ..game(1000) }]));
    f.l.go(Screen::Game { host: pc, app: 1000 });
    f.settle();
    let line = f.drawn.iter().find(|t| t.contains("Travel")).unwrap_or_else(|| panic!("{:?}", f.drawn)).clone();
    assert!(line.contains("Frame rate set for this PC") && line.contains("Bitrate from profile Travel") && !line.contains("Resolution from"), "{line}");

    f.l.open_app_settings(pc, &f.hosts.list[0], nightjar_policy::http::App { name: "Game 00".into(), uuid: "install-0".into(), ..game(1000) });
    f.settle();
    assert!(shows(&f, "Set for this PC") && shows(&f, "From profile Travel"), "{:?}", f.drawn);
    // Clearing the game's own settings keeps showing where the inherited rate comes from.
    f.l.edit_overrides.capture(&Settings { hdr: true, ..Settings::default() }, &["hdr"]);
    f.settle();
    click_text(&mut f, "Clear all for this game");
    click_text(&mut f, "Clear all");
    assert!(f.l.edit_overrides.is_empty(), "{:?}", f.l.edit_overrides);
    assert!(shows(&f, "120 fps") && shows(&f, "Set for this PC"), "{:?}", f.drawn);
    // A PC's own page names the profile only.
    f.l.open_host_settings(pc, &f.hosts.list[0]);
    f.settle();
    assert!(!shows(&f, "Set for this PC") && shows(&f, "From profile Travel"), "{:?}", f.drawn);
    assert_eq!(summary_sources(None, None, None), Vec::<String>::new());
}

/// Collection joins Group by only while the PCs name collections; its rows are the collections by
/// name, a game in several in each, then the games in none.
#[test]
fn home_groups_by_collection_only_when_the_pc_names_any() {
    let mut f = store_mix();
    f.size = vec2(1280.0, 2400.0);
    in_collections(&mut f, &[("install-0", &["VR", "Couch"]), ("install-12", &["Couch"])]);
    let view = f.placed.iter().find(|(t, ..)| t == "View").map(|p| p.1).expect("View in the top row");
    f.click_at(view.center(), egui::PointerButton::Primary);
    let row = |f: &Frames, label: &str| f.placed.iter().find(|(t, ..)| t == label).map(|p| p.1).expect(label);
    for (next, says) in [(HomeGroup::Type, "Type"), (HomeGroup::Collection, "Collection")] {
        let at = row(&f, "Group by");
        f.click_at(at.center(), egui::PointerButton::Primary);
        assert_eq!(f.l.settings.home_group, next);
        assert!(shows(&f, says), "{says}: {:?}", f.drawn);
    }
    let shelf = |f: &Frames, name: &str| f.placed.iter().find(|(t, r, _)| t == name && r.center().y > 80.0).map(|p| p.1.top()).expect(name);
    assert!(shelf(&f, "Couch") < shelf(&f, "VR") && shelf(&f, "VR") < shelf(&f, "No collection"), "{:?}", f.drawn);
    let copies = f.drawn.iter().filter(|t| *t == "Steam 00").count();
    assert!(copies >= 2, "in both of its collections: {:?}", f.drawn);

    // The PC stops naming any: the rows go back to stores, and the choice stays for when it does again.
    in_collections(&mut f, &[]);
    assert_eq!(f.l.settings.home_group, HomeGroup::Collection);
    assert!(shows(&f, "Steam") && !shows(&f, "No collection"), "{:?}", f.drawn);
    assert!(shows(&f, "Store"), "the menu says what shows: {:?}", f.drawn);
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::Type);
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::Installation, "no Collection to step through");
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::None);
}

/// Installation joins Group by while some game is not installed: the installed games, then the
/// rest. Sort by and Order step through their choices and Home follows at once.
#[test]
fn home_groups_by_installation_and_sorts_from_the_view_menu() {
    let mut f = store_mix();
    f.size = vec2(1280.0, 2400.0);
    f.l.settings.home_group = HomeGroup::Installation;
    // A play time from the PC: Sort by offers Play time only then.
    f.hosts.list[0].native.playtime_minutes = Arc::new([("install-0".to_owned(), 90)].into());
    f.settle();
    let shelf = |f: &Frames, name: &str| f.placed.iter().find(|(t, r, _)| t == name && r.center().y > 80.0).map(|p| p.1.top()).expect(name);
    assert!(shelf(&f, "Installed") < shelf(&f, "Not installed"), "{:?}", f.drawn);
    let view = f.placed.iter().find(|(t, ..)| t == "View").map(|p| p.1).expect("View in the top row");
    f.click_at(view.center(), egui::PointerButton::Primary);
    let row = |f: &Frames, label: &str| f.placed.iter().find(|(t, ..)| t == label).map(|p| p.1).expect(label);
    assert!(shows(&f, "Title") && shows(&f, "Ascending"), "{:?}", f.drawn);
    for (next, says) in [(HomeSort::LastPlayed, "Last played"), (HomeSort::Playtime, "Play time"), (HomeSort::Title, "Title")] {
        let at = row(&f, "Sort by");
        f.click_at(at.center(), egui::PointerButton::Primary);
        assert_eq!(f.l.settings.home_sort, next);
        assert!(shows(&f, says), "{says}: {:?}", f.drawn);
    }
    let at = row(&f, "Order");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_order, HomeOrder::Descending);
    assert!(shows(&f, "Descending"), "{:?}", f.drawn);
}

/// A game in several rows is a card of its own in each: focus comes back to the one it left, and
/// moving between them counts as moving, so the new one scrolls into view.
#[test]
fn a_game_in_two_rows_has_a_focus_key_in_each() {
    let mut f = store_mix();
    f.size = vec2(1280.0, 2400.0);
    in_collections(&mut f, &[("install-0", &["VR", "Couch"])]);
    f.l.settings.home_group = HomeGroup::Collection;
    f.settle();
    let game = "game:steam:0";
    let of_game: Vec<String> = f.l.cards.iter().filter(|(_, g)| g == game).map(|(key, _)| key.clone()).collect();
    assert_eq!(of_game, [format!("row:collection:couch:game:{game}"), format!("row:collection:vr:game:{game}")]);
    f.l.restore_key = Some(of_game[1].clone());
    f.settle();
    assert_eq!(f.l.focused_key.as_ref(), Some(&of_game[1]), "the second card, not the first");
    assert!(f.l.restore_key.is_none());
    f.press(Key::ArrowUp);
    assert_eq!(f.l.focused_key.as_ref(), Some(&of_game[0]), "up to the same game in the row above");
}

/// Hide moves the focus to the next card of another game, as Unstar does, past the game's other
/// cards; the last one's goes to the one before it.
#[test]
fn hiding_a_game_from_its_menu_focuses_the_next_card() {
    let mut f = home_with_stores(&[("Alpha", "steam", true), ("Beta", "steam", true)]);
    f.l.restore_key = Some("row:store:steam:game:game:steam:0".to_owned());
    f.settle();
    f.press(Key::F2);
    assert!(shows(&f, "Hide game"), "{:?}", f.drawn);
    // Down past Game details and the star to Hide game, as a controller does.
    f.press(Key::ArrowDown);
    f.press(Key::ArrowDown);
    let commands = f.frame(tap_key(Key::Enter).to_vec());
    let Some(Command::SetHidden { games, hidden: true }) = commands.into_iter().find(|c| matches!(c, Command::SetHidden { .. })) else {
        panic!("no SetHidden");
    };
    assert!(crate::config::set_hidden(&mut f.l.hidden_games, games, true));
    f.settle();
    assert!(f.no_dialog());
    assert_eq!(f.l.focused_key.as_deref(), Some("row:store:steam:game:game:steam:1"), "Beta has the focus");
    assert!(f.ctx.memory(|m| m.focused()).is_some(), "something holds the focus");

    let cards = |keys: &[(&str, &str)]| -> Vec<(String, String)> { keys.iter().map(|(k, g)| ((*k).to_owned(), (*g).to_owned())).collect() };
    let shown = cards(&[("couch:a", "a"), ("couch:b", "b"), ("vr:a", "a"), ("vr:c", "c")]);
    assert_eq!(library::card_neighbour(&shown, "couch:a").as_deref(), Some("couch:b"));
    assert_eq!(library::card_neighbour(&shown, "couch:b").as_deref(), Some("vr:a"));
    assert_eq!(library::card_neighbour(&shown, "vr:a").as_deref(), Some("vr:c"));
    assert_eq!(library::card_neighbour(&shown, "vr:c").as_deref(), Some("vr:a"), "the last: the one before");
    assert_eq!(library::card_neighbour(&cards(&[("x:a", "a"), ("y:a", "a")]), "y:a"), None, "nothing else");
    assert_eq!(library::card_neighbour(&shown, "gone"), None);
}

/// A favourite's card is the same game as its library card, which goes with it when it is hidden:
/// the focus moves past that one too, or the next D-pad press finds nothing and starts at the top.
#[test]
fn hiding_a_favourite_focuses_the_next_game_not_its_own_library_card() {
    let mut f = home_with_stores(&[("Alpha", "steam", true), ("Beta", "steam", true)]);
    let id = f.hosts.list[0].id;
    f.l.favourites = vec![crate::config::Favourite { host: "Desk".into(), install_id: "install-0".into(), app_id: 1000, title: "Alpha".into() }];
    f.l.restore_key = Some(format!("favourite:{id}:1000"));
    f.settle();
    f.press(Key::F2);
    assert!(shows(&f, "Hide game"), "{:?}", f.drawn);
    f.press(Key::ArrowDown);
    f.press(Key::ArrowDown);
    let commands = f.frame(tap_key(Key::Enter).to_vec());
    let Some(Command::SetHidden { games, hidden: true }) = commands.into_iter().find(|c| matches!(c, Command::SetHidden { .. })) else {
        panic!("no SetHidden");
    };
    assert!(crate::config::set_hidden(&mut f.l.hidden_games, games, true));
    f.settle();
    assert_eq!(f.l.focused_key.as_deref(), Some("row:store:steam:game:game:steam:1"), "Beta, not Alpha's own card");
    assert!(f.ctx.memory(|m| m.focused()).is_some(), "something holds the focus");
}

/// A PC's page showing hidden games keeps the card through Hide and Show: the focus stays on it.
/// With them not shown the card goes, and the focus with it.
#[test]
fn hiding_on_a_pcs_page_moves_the_focus_only_when_the_card_goes() {
    let mut f = home_with_games(3);
    let id = f.hosts.list[0].id;
    f.hosts.list[0].apps = Apps::Loaded(Arc::new(vec![game(7), game(9)]));
    f.l.go(Screen::Apps(id));
    for (shows_hidden, focus) in [(true, "app:7"), (false, "app:9")] {
        f.l.show_hidden_apps = shows_hidden;
        f.l.restore_key = Some("app:7".to_owned());
        f.settle();
        assert_eq!(f.l.focused_key.as_deref(), Some("app:7"));
        f.press(Key::F2);
        let hide = f.placed.iter().find(|(t, ..)| t == "Hide game").map(|p| p.1).expect("Hide game in the menu");
        let commands = f.click_at(hide.center(), egui::PointerButton::Primary);
        assert!(commands.iter().any(|c| matches!(c, Command::SetHidden { hidden: true, .. })));
        assert_eq!(f.l.focused_key.as_deref(), Some(focus));
    }
}

/// A running game stays under Continue playing when hidden, off the library's rows, and its card's
/// menu shows it again: hiding must not lose the way back to it.
#[test]
fn a_running_hidden_game_stays_under_continue_playing_and_can_be_shown_again() {
    let mut f = home_with_stores(&[("Alpha", "steam", true), ("Beta", "steam", true)]);
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[0].running = true;
    let id = f.hosts.list[0].id;
    let keys = {
        let sources: Vec<crate::console::Source> = f.hosts.list.iter().map(crate::console::Source::of).collect();
        crate::console::hide_keys(&sources, id, 1000)
    };
    assert_eq!(keys.len(), 1);
    f.l.hidden_games = keys;
    f.settle();
    // Running, it is Home's hero: hiding must not lose the way back to it.
    assert!(shows(&f, "Alpha") && shows(&f, "Resume"), "{:?}", f.drawn);
    let card = f.ctx.read_response(egui::Id::unique("hero-cover")).expect("the hero's cover").rect;
    f.click_at(card.center(), egui::PointerButton::Secondary);
    let show = f.placed.iter().find(|(t, ..)| t == "Show game").map(|p| p.1).expect("Show game in the menu");
    let commands = f.click_at(show.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::SetHidden { hidden: false, .. })));
    assert!(f.l.focused_key.as_deref().is_some_and(|key| key.starts_with("hero-")), "the hero keeps the focus: {:?}", f.l.focused_key);
    assert_eq!(id, f.hosts.list[0].id);
    assert!(!shows(&f, "Close game"), "one word, Quit game: {:?}", f.drawn);
}

/// The hero's cover opens the same menu as a shelf's, with F2 from its buttons too, and the focus
/// comes back to what asked.
#[test]
fn the_hero_opens_its_games_menu_and_gives_focus_back() {
    let mut f = home_with_stores(&[("Alpha", "steam", true), ("Beta", "steam", true)]);
    let cover = f.ctx.read_response(egui::Id::unique("hero-cover")).expect("the hero's cover").rect;
    f.click_at(cover.center(), egui::PointerButton::Secondary);
    assert!(shows(&f, "Hide game"), "{:?}", f.drawn);
    f.press(Key::Escape);
    let focused = f.ctx.memory(|m| m.focused()).expect("focus");
    assert_eq!(focused, egui::Id::unique("hero-cover"));
    let star = egui::Id::unique("hero-star");
    f.ctx.memory_mut(|m| m.request_focus(star));
    f.settle();
    f.press(Key::F2);
    assert!(matches!(f.l.dialog, Dialog::GameMenu { .. }), "F2 opens the game's menu");
}

/// A title's word is never cut in two on a tile: the size gives way.
#[test]
fn a_long_word_on_a_tile_stays_whole() {
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut out = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        cover_shapes(ui, &mut out, Rect::from_min_size(Pos2::ZERO, vec2(155.0, 232.0)), 155.0, 16.0, Id::unique(("cover-in", "test")), "Brightwater", None, 1.0);
    });
    output.textures_delta.clear();
    let rows: Vec<usize> = out.iter().filter_map(|shape| match shape {
        egui::Shape::Text(text) => Some(text.galley.rows.len()),
        _ => None,
    }).collect();
    assert_eq!(rows, [1], "one line");
}

/// At 760 wide the Help tab ran under the search button. The tabs give way to what is on their right:
/// first their padding, then their words, and each keeps its name for a screen reader and the pointer.
#[test]
fn the_tabs_never_run_under_the_search_button_in_a_narrow_window() {
    for (width, worded) in [(1280.0, true), (900.0, true), (760.0, true), (506.0, false)] {
        let mut f = demo_frames(vec2(width, 700.0));
        f.settle();
        let in_bar = |rect: &&Rect| rect.top() < 60.0;
        let pc = f.placed.iter().filter(|(text, ..)| text.contains("Living Room PC")).map(|(_, rect, _)| rect).rfind(in_bar).copied().expect("the PC capsule");
        let help = f.placed.iter().filter(|(text, ..)| text == "Help").map(|(_, rect, _)| rect).find(in_bar).copied();
        assert_eq!(help.is_some(), worded, "{width}: the tabs' words");
        if let Some(help) = help {
            // The search button, its gaps and the PC capsule's own padding sit between them.
            assert!(help.right() + 40.0 < pc.left(), "{width}: Help ends at {}, the PC's name starts at {}", help.right(), pc.left());
        }
    }
    assert_eq!(super::chrome::nav_fit(&[50.0, 30.0, 60.0, 35.0], 1000.0), (18.0, true));
    let (pad, worded) = super::chrome::nav_fit(&[50.0, 30.0, 60.0, 35.0], 400.0);
    assert!(worded && (10.0..18.0).contains(&pad), "{pad}");
    assert_eq!(super::chrome::nav_fit(&[50.0, 30.0, 60.0, 35.0], 250.0), (10.0, false));
}

/// Every page is one area the size of the window: what it holds moves up beneath the chrome and is
/// cut only by the window's own edges, never at a line of its own below the chrome or above the
/// bottom. The scrim that keeps the chrome legible comes with it, and it leaves again at the top.
#[test]
fn every_page_scrolls_whole_beneath_the_chrome() {
    use super::demo::Page;
    let size = vec2(1000.0, 300.0);
    let mut f = demo_frames(size);
    for (name, page) in Page::ALL.into_iter().filter(|(_, page)| !page.covers_the_page()) {
        demo_page(&mut f, page);
        let before = f.placed.clone();
        let opened = f.l.page_scrolled;
        wheel(&mut f, Pos2::new(990.0, 280.0), -160.0);
        // Files lists on a thread of its own, late on a busy machine; until then the page is short.
        for _ in 0..200 {
            if f.l.page_scrolled > opened + 50.0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
            wheel(&mut f, Pos2::new(990.0, 280.0), -160.0);
        }
        assert!(f.l.page_scrolled > opened + 50.0, "{name}: the wheel scrolls it from anywhere on the window");
        let moved: Vec<_> = f.placed.iter().filter(|(text, rect, _)| before.iter().any(|(was, at, _)| was == text && at.top() > rect.top() + 50.0)).collect();
        assert!(!moved.is_empty(), "{name}: the page's text moved");
        // A card may clip its own words; the page's are cut by the window alone.
        let window = |clip: &Rect| clip.top() <= 0.5 && clip.bottom() >= size.y - 0.5;
        assert!(moved.iter().any(|(_, _, clip)| window(clip)), "{name}: a clip of its own: {:?}", moved.iter().map(|m| m.2).collect::<Vec<_>>());
        let screen = Rect::from_min_size(Pos2::ZERO, size);
        assert!(!f.l.ambient.veil(&f.ctx, screen, chrome::brand_rect(screen), f.l.page_scrolled).is_empty(), "{name}: the veil comes with scrolling");
        wheel(&mut f, Pos2::new(990.0, 280.0), 4000.0);
        assert!(f.l.page_scrolled < 0.5, "{name} comes back to its top");
    }
}

/// The arrow keys walk a page past the window's edges and back: what they reach rests clear of the
/// chrome, on any page, and coming back up ends at the top.
#[test]
fn the_arrow_keys_keep_the_focus_clear_of_the_chrome_on_every_long_page() {
    use super::demo::Page;
    let size = vec2(1280.0, 480.0);
    for (name, page) in [("all-games", Page::AllGames), ("apps", Page::Apps), ("settings", Page::Settings), ("pc-settings", Page::PcSettings)] {
        let mut f = demo_frames(size);
        // Recommended, where Settings open, fits this window: Video is long enough to walk.
        (f.l.settings_view.rail_more, f.l.settings_view.section) = (true, "Video");
        demo_page(&mut f, page);
        let focused = |f: &Frames| {
            let id = f.ctx.memory(|m| m.focused()).expect("something is focused");
            (id, f.ctx.read_response(id).expect("the focused widget was drawn").rect)
        };
        // Down the rows of a settings page: the list beside them takes Right first.
        if matches!(page, Page::Settings | Page::PcSettings) {
            f.press(Key::ArrowRight);
        }
        let start = f.l.page_scrolled;
        let mut walked = 0;
        let mut last = focused(&f).0;
        for _ in 0..14 {
            f.press(Key::ArrowDown);
            f.settle();
            f.settle();
            let (id, rect) = focused(&f);
            if id == last {
                continue;
            }
            last = id;
            walked += 1;
            assert!(rect.top() >= chrome::PAGE_TOP - 8.0, "{name}: {rect:?} rests beneath the chrome");
            assert!(rect.bottom() <= size.y + 0.5, "{name}: {rect:?} runs off the window");
        }
        assert!(walked >= 3, "{name}: the focus walked: {walked}");
        assert!(f.l.page_scrolled > start + 50.0, "{name}: the page followed it: {}", f.l.page_scrolled);
        for _ in 0..20 {
            f.press(Key::ArrowUp);
            f.settle();
            f.settle();
            let (_, rect) = focused(&f);
            assert!(rect.bottom() >= 0.0 && rect.top() <= size.y, "{name}: the focus stays on screen going up: {rect:?}");
        }
    }
}

/// On a wide window the sections stay put below the chrome while the rows go by, and a settings page
/// with a header first lets them rise to that place. The list is not cut by the chrome.
#[test]
fn the_side_list_stays_below_the_chrome_while_the_rows_scroll() {
    use super::demo::Page;
    // Tall enough for the whole list, More settings open: a shorter window's list goes with the page.
    let mut f = demo_frames(vec2(1600.0, 860.0));
    // Every row: a section long enough to scroll.
    f.l.settings_view.more = [true; crate::ui::settings::SECTION_COUNT];
    let top_of = |f: &Frames, text: &str| f.placed.iter().find(|(t, ..)| t == text).map(|p| p.1.top()).unwrap_or_else(|| panic!("{text} is drawn"));
    demo_page(&mut f, Page::Settings);
    f.l.settings_view.section = "Video";
    f.settle();
    let (entry, row) = (top_of(&f, "Audio"), top_of(&f, "Video codec"));
    wheel(&mut f, Pos2::new(1500.0, 600.0), -200.0);
    assert!(top_of(&f, "Video codec") < row - 50.0, "the rows move");
    assert!((top_of(&f, "Audio") - entry).abs() < 0.5, "the sections do not");
    wheel(&mut f, Pos2::new(120.0, 680.0), 200.0);
    assert!((top_of(&f, "Video codec") - row).abs() < 0.5, "the wheel over the list scrolls the page too");

    demo_page(&mut f, Page::PcSettings);
    let entry = top_of(&f, "Audio");
    wheel(&mut f, Pos2::new(1500.0, 600.0), -600.0);
    let risen = top_of(&f, "Audio");
    assert!(risen < entry, "under a header the list rises first: {entry} to {risen}");
    let search_top = f.placed.iter().filter(|(t, ..)| t == "Audio").map(|p| p.1.top()).fold(f32::MAX, f32::min);
    assert!(search_top >= chrome::PAGE_TOP, "and stops below the chrome: {search_top}");
    // The window too short for the list: it goes with the page, or its end could never be seen.
    let mut f = demo_frames(vec2(1600.0, 300.0));
    demo_page(&mut f, Page::Settings);
    f.settle();
    let entry = top_of(&f, "Audio");
    wheel(&mut f, Pos2::new(1500.0, 280.0), -100.0);
    assert!(top_of(&f, "Audio") < entry - 50.0, "the short window's list scrolls");
}

/// Help's section chips land the section below the chrome, not beneath it.
#[test]
fn a_help_section_jump_lands_below_the_chrome() {
    use super::demo::Page;
    let mut f = demo_frames(vec2(1280.0, 480.0));
    demo_page(&mut f, Page::Help);
    let chip = f.placed.iter().find(|(t, ..)| t == "Sharing").map(|p| p.1.center()).expect("the Sharing chip");
    f.click_at(chip, egui::PointerButton::Primary);
    f.settle();
    f.settle();
    let tops: Vec<f32> = f.placed.iter().filter(|(t, ..)| t == "Sharing").map(|p| p.1.top()).collect();
    assert!(f.l.page_scrolled > 50.0, "the page moved to it");
    let heading = tops.iter().copied().fold(f32::MIN, f32::max);
    // The chips stay under the chrome, and the heading rests just below them, clear of the fade.
    let chip = f.placed.iter().filter(|(t, r, _)| t == "Controllers" && r.top() > 0.0).map(|p| p.1).fold(None, |a: Option<Rect>, r| Some(a.map_or(r, |a| if r.top() < a.top() { r } else { a }))).expect("the chips");
    assert!((chrome::PAGE_TOP - 1.0..chrome::PAGE_TOP + 24.0).contains(&chip.top()), "the chips are held: {chip:?}");
    assert!((chip.bottom()..chip.bottom() + 40.0).contains(&heading), "the heading rests just below the chips: {tops:?} {chip:?}");
}

/// Home at rest draws nothing more: springs and fades ask for frames only while they move.
#[test]
fn home_at_rest_asks_for_no_more_frames() {
    let mut f = store_mix();
    for art in f.hosts.list[0].native.library.as_ref().expect("a library").games.iter().map(|g| g.app_id as i32).collect::<Vec<_>>() {
        Arc::make_mut(&mut f.hosts.list[0].art).insert(art, Art::Missing);
    }
    for _ in 0..12 {
        f.settle();
    }
    assert!(!f.ctx.has_requested_repaint(), "Home at rest asks for no frame");
    f.press(Key::ArrowDown);
    for _ in 0..12 {
        f.settle();
    }
    assert!(!f.ctx.has_requested_repaint(), "nor after the focus has landed");
}

/// The launcher wakes for workers and timers with nothing new to show: the frame comes out the same,
/// so nothing is presented. Whatever does change it, a hover here, is a different frame.
#[test]
fn a_launcher_at_rest_paints_the_same_frame_whenever_it_wakes() {
    use crate::live_glass::{frame_differs, frame_print};
    let mut f = store_mix();
    for art in f.hosts.list[0].native.library.as_ref().expect("a library").games.iter().map(|g| g.app_id as i32).collect::<Vec<_>>() {
        Arc::make_mut(&mut f.hosts.list[0].art).insert(art, Art::Missing);
    }
    for _ in 0..12 {
        f.settle();
    }
    let print = |f: &mut Frames| {
        f.frame(Vec::new());
        let shapes = std::mem::take(&mut f.shapes);
        let primitives = f.ctx.tessellate(shapes, 1.0);
        frame_print(&primitives, 1.0, (1280, 800))
    };
    let shown = print(&mut f);
    for _ in 0..8 {
        let again = print(&mut f);
        assert!(!frame_differs(Some(shown), again, false, false), "an idle wake draws nothing");
    }
    assert!(frame_differs(None, shown, false, false), "the first frame after a stream's is drawn");
    assert!(frame_differs(Some(shown), shown, true, false), "so is one the window asked for");
    assert!(frame_differs(Some(shown), shown, false, true), "and one with a new texture");
    f.frame(vec![egui::Event::PointerMoved(Pos2::new(600.0, 44.0))]);
    for _ in 0..6 {
        f.settle();
    }
    assert!(frame_differs(Some(shown), print(&mut f), false, false), "a hover is a new frame");
}

/// The driver reaches every page of the demo library, on what `fill` puts there and with no dialog
/// in the way: README, store and trailer captures depend on it.
#[test]
fn the_demo_library_reaches_every_page() {
    use super::demo::{self, Page};
    crate::theme::own_palette();
    let ctx = egui::Context::default();
    crate::theme::apply(&ctx);
    let mut l = launcher(true, false);
    let mut hosts = Hosts::without_discovery(demo::records(), Arc::new(|| {}));
    demo::fill(&mut hosts, &mut l, &ctx);
    let mut f = Frames::over(ctx, l, hosts);
    f.settle();
    assert_eq!(f.l.screen, Screen::Home);
    assert_eq!(f.hosts.list[0].native.library.as_ref().expect("the library").games.len(), 36);
    assert!(!f.hosts.has_identity, "no identity: nothing can ask a PC");
    let pc = f.hosts.list[0].id;
    for (name, page) in Page::ALL {
        let waker: crate::hosts::Waker = Arc::new(|| {});
        let command = { let Frames { l, hosts, .. } = &mut f; l.demo_page(page, hosts, waker) };
        if let Some(Command::ProfileSettings(id)) = command {
            f.l.go(Screen::ProfileSettings(id));
        }
        for _ in 0..4 {
            f.settle();
        }
        let expected = match page {
            Page::Home => Screen::Home,
            Page::Toast => Screen::Home,
            Page::AllGames | Page::Search => Screen::AllGames,
            Page::Game => Screen::Game { host: pc, app: 1002 },
            Page::Pcs | Page::FirstRun | Page::FirstRunFound | Page::Dialog(_) | Page::PinStream | Page::PinChoose | Page::PinLock(_) | Page::StreamMenu(_) => Screen::Hosts,
            Page::Apps => Screen::Apps(pc),
            Page::AppsEmpty => Screen::Apps(f.hosts.list[1].id),
            Page::Pc => Screen::HostDetails(pc),
            Page::Files => Screen::Files(pc),
            Page::Settings => Screen::Settings,
            Page::PcSettings => Screen::HostSettings(pc),
            Page::GameSettings => Screen::AppSettings(pc),
            Page::Profiles => Screen::Profiles,
            Page::ProfileSettings => Screen::ProfileSettings(1),
            Page::Controllers => Screen::Controllers,
            Page::Shortcuts => Screen::Shortcuts,
            Page::Hotkeys => Screen::StreamHotkeys,
            Page::Help => Screen::Help,
        };
        assert_eq!(f.l.screen, expected, "{name}");
        assert!(page.covers_the_page() || f.no_dialog(), "{name} opens with nothing over it");
    }
}

/// The hero is what runs, else what ran last, else the library's first installed game.
#[test]
fn the_hero_is_the_running_game_then_the_last_played_then_the_first_installed() {
    let mut f = home_with_stores(&[("Alpha", "steam", false), ("Beta", "steam", true), ("Gamma", "steam", true)]);
    let hero = |f: &Frames| f.l.hero.as_ref().map(|h| h.title.clone());
    assert_eq!(hero(&f).as_deref(), Some("Beta"), "the first installed");
    f.l.recents.push(crate::config::Recent { host: "Desk".into(), install_id: "install-2".into(), app_id: 1002, title: "Gamma".into(), at: 1000 });
    f.settle();
    assert_eq!(hero(&f).as_deref(), Some("Gamma"), "the last played");
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().expect("a library")).games[0].running = true;
    f.settle();
    assert_eq!(hero(&f).as_deref(), Some("Alpha"), "what runs");
}

#[test]
fn identical_game_titles_on_two_pcs_do_not_share_cover_animation() {
    let art_a = egui::ColorImage::filled([4, 4], egui::Color32::from_rgb(120, 40, 40));
    let art_b = egui::ColorImage::filled([4, 4], egui::Color32::from_rgb(40, 40, 120));
    let f = demo_frames(vec2(1000.0, 700.0));
    // First PC has "Desktop" cover loaded; second PC switches in while both share the same title.
    let id_a = Id::unique(("cover-in", 1_u64, 100_u32));
    let id_b = Id::unique(("cover-in", 2_u64, 100_u32));
    assert_ne!(id_a, id_b);
    let art_a = crate::hosts_model::Art::Ready(f.ctx.load_texture("a", art_a, egui::TextureOptions::LINEAR));
    let tex_b = f.ctx.load_texture("b", art_b, egui::TextureOptions::LINEAR);
    let mut out_a = Vec::new();
    let mut out_b = Vec::new();
    let mut out = f.ctx.run_ui(egui::RawInput::default(), |ui| {
        cover_shapes(ui, &mut out_a, Rect::from_min_size(Pos2::ZERO, vec2(155.0, 232.0)), 155.0, 16.0, id_a, "Desktop", Some(&art_a), 1.0);
        cover_shapes(ui, &mut out_b, Rect::from_min_size(Pos2::new(200.0, 0.0), vec2(155.0, 232.0)), 155.0, 16.0, id_b, "Desktop", None, 1.0);
    });
    out.textures_delta.clear();
    assert!(f.ctx.animate_bool_with_time(id_a, true, 0.09) > 0.0);
    assert_eq!(f.ctx.animate_bool_with_time(id_b, false, 0.09), 0.0);
    let _ = tex_b;
}
