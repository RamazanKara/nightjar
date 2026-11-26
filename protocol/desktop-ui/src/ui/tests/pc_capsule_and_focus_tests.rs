//! Top-bar PC switcher sheet and focus retention tests.

use super::*;

/// Ctrl K opens the search on All games; the words narrow it; Escape takes both away and gives the
/// focus back to what had it.
#[test]
fn the_search_filters_all_games_and_escape_goes_back() {
    let mut f = store_mix();
    let before = f.ctx.memory(|m| m.focused()).expect("focus on Home");
    f.frame(vec![egui::Event::Key { key: Key::K, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::COMMAND }]);
    f.settle();
    assert_eq!(f.l.screen, Screen::AllGames);
    assert!(f.ctx.text_edit_focused(), "the field has the keys");
    f.l.library_query = "fort".into();
    f.settle();
    assert!(shows(&f, "Fortnite") && !shows(&f, "Forza Horizon 5") && !shows(&f, "Diablo IV"), "{:?}", f.drawn);
    f.press(Key::Escape);
    assert_eq!(f.l.screen, Screen::Home, "a page opened for the search goes with it");
    assert!(f.l.library_query.is_empty());
    assert_eq!(f.ctx.memory(|m| m.focused()), Some(before), "the focus is back");
}

/// Into a game and back: Back lands on the cover it left.
#[test]
fn back_from_a_game_restores_the_focus_on_home() {
    let mut f = store_mix();
    let shelf = (0..6).find_map(|_| {
        f.press(Key::ArrowDown);
        let id = f.ctx.memory(|m| m.focused())?;
        let rect = f.ctx.read_response(id)?.rect;
        (rect.height() > 200.0 && rect.top() > 300.0).then_some(id)
    });
    let shelf = shelf.expect("a shelf's cover");
    f.press(Key::Enter);
    assert!(matches!(f.l.screen, Screen::Game { .. }), "{:?}", f.l.screen);
    f.press(Key::Escape);
    assert_eq!(f.l.screen, Screen::Home);
    assert_eq!(f.ctx.memory(|m| m.focused()), Some(shelf));
}

/// Covers arriving change nothing's place: a cover's slot is the same with art, without, and while
/// it is on its way.
#[test]
fn covers_arriving_move_nothing() {
    let mut f = home_with_games(4);
    let place = |f: &Frames| f.placed.iter().find(|(t, ..)| t == "PCs").map(|p| p.1.top());
    let heading = |f: &Frames| f.placed.iter().filter(|(t, ..)| t == "Desk").map(|p| p.1.top()).fold(0.0, f32::max);
    let loading = (place(&f), heading(&f));
    let texture = f.ctx.load_texture("art", egui::ColorImage::new([3, 4], vec![Color32::GRAY; 12]), egui::TextureOptions::LINEAR);
    let pc = &mut f.hosts.list[0];
    for g in &mut Arc::make_mut(pc.native.library.as_mut().expect("a library")).games {
        Arc::make_mut(&mut pc.art).insert(g.app_id as i32, Art::Ready(texture.clone()));
    }
    f.settle();
    assert_eq!((place(&f), heading(&f)), loading, "art");
    let pc = &mut f.hosts.list[0];
    for g in &mut Arc::make_mut(pc.native.library.as_mut().expect("a library")).games {
        Arc::make_mut(&mut pc.art).insert(g.app_id as i32, Art::Missing);
    }
    f.settle();
    assert_eq!((place(&f), heading(&f)), loading, "tiles");
}

/// Hide on a card's menu takes the game off Home on this device; All games has it under Hidden,
/// where the same menu brings it back.
#[test]
fn a_game_hides_from_home_and_shows_again_from_all_games() {
    let mut f = home_with_stores(&[("Alpha", "steam", true), ("Beta", "steam", true)]);
    let card = f.placed.iter().find(|(t, r, _)| t == "Beta" && r.center().y > 80.0).map(|p| p.1).expect("Beta's card");
    f.click_at(card.center(), egui::PointerButton::Secondary);
    let hide = f.placed.iter().find(|(t, ..)| t == "Hide game").map(|p| p.1).expect("Hide game in the menu");
    let commands = f.click_at(hide.center(), egui::PointerButton::Primary);
    let Some(Command::SetHidden { games, hidden: true }) = commands.into_iter().find(|c| matches!(c, Command::SetHidden { .. })) else {
        panic!("no SetHidden");
    };
    assert_eq!(games.len(), 1);
    assert_eq!((games[0].host.as_str(), games[0].install_id.as_str()), ("Desk", "install-1"));
    assert!(crate::config::set_hidden(&mut f.l.hidden_games, games, true));
    f.settle();
    assert!(f.no_dialog());
    assert!(!shows(&f, "Beta") && shows(&f, "Alpha"), "{:?}", f.drawn);

    f.l.go(Screen::AllGames);
    f.settle();
    assert!(shows(&f, "Hidden 1") || shows(&f, "Hidden"), "{:?}", f.drawn);
    assert!(!shows(&f, "Beta"), "only under Hidden: {:?}", f.drawn);
    f.l.library_filter = crate::console::LibraryFilter::Hidden;
    f.settle();
    assert!(shows(&f, "Beta") && !shows(&f, "Alpha"), "{:?}", f.drawn);
    let card = f.placed.iter().find(|(t, r, _)| t == "Beta" && r.center().y > 80.0).map(|p| p.1).expect("Beta's card");
    f.click_at(card.center(), egui::PointerButton::Secondary);
    let show = f.placed.iter().find(|(t, ..)| t == "Show game").map(|p| p.1).expect("Show game in the menu");
    let commands = f.click_at(show.center(), egui::PointerButton::Primary);
    let Some(Command::SetHidden { games, hidden: false }) = commands.into_iter().find(|c| matches!(c, Command::SetHidden { .. })) else {
        panic!("no SetHidden");
    };
    assert!(crate::config::set_hidden(&mut f.l.hidden_games, games, false));
    f.settle();
    assert!(f.l.hidden_games.is_empty());
    assert_eq!(f.l.library_filter, crate::console::LibraryFilter::All, "the empty Hidden chip gives way");
    assert!(shows(&f, "Beta"), "{:?}", f.drawn);
}

/// Home has a row per store, largest first, each as wide as the window and ending in an All tile
/// when more games follow; the apps of no store come last as Other apps.
#[test]
fn home_shows_a_row_per_store_that_ends_in_an_all_tile() {
    let mut f = store_mix();
    f.size = vec2(1280.0, 2400.0);
    f.settle();
    let shelf = |f: &Frames, name: &str| f.placed.iter().find(|(t, r, _)| t == name && r.center().y > 80.0).map(|p| p.1.top()).expect(name);
    // Largest first, ties by key; Epic, Battle.net and Xbox have their rows as Steam does.
    let order = ["Steam", "Battle.net", "Epic Games", "Xbox", "Other apps"].map(|name| shelf(&f, name));
    assert!(order.windows(2).all(|w| w[0] < w[1]), "rows in order: {order:?}");
    f.size = vec2(1280.0, 720.0);
    f.settle();
    // Twelve Steam games do not fit one row at 1280: the row ends in "All 12", which opens All
    // games on Steam.
    assert!(shows(&f, "All 12"), "{:?}", f.drawn);
    assert!(!shows(&f, "Steam 11"), "the rest wait behind the tile");
    let tile = f.placed.iter().find(|(t, ..)| t == "All 12").map(|p| p.1).unwrap();
    f.click_at(tile.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.screen, Screen::AllGames);
    assert_eq!(f.l.library_filter, console::LibraryFilter::Store("steam".into()));
    assert!(shows(&f, "Steam 11") && !shows(&f, "Fortnite"), "only Steam's games: {:?}", f.drawn);
    // Back returns to Home.
    f.press(Key::Escape);
    assert_eq!(f.l.screen, Screen::Home);
}

/// A column so narrow that one cover fills a line still shows each long row's All tile, alone,
/// instead of a header over nothing; the tile still opens the row's games. (The top bar keeps the
/// window's page wider than that, so Home is drawn into a narrow column of its own here.)
#[test]
fn at_one_column_a_long_row_is_its_all_tile() {
    let mut f = store_mix();
    let column = vec2(210.0, 2400.0);
    assert_eq!(grid(column.x - 2.0 * chrome::page_margin(column.x)).0, 1, "one column");
    let mut drawn = Vec::new();
    let mut tile = None;
    let mut clicked_at = None;
    for n in 0..6 {
        // The last passes click where the tile was drawn.
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        let events = match (n, tile.or(clicked_at)) {
            (3, Some(at)) => {
                clicked_at = Some(at);
                vec![egui::Event::PointerMoved(at)]
            }
            (4, Some(at)) => vec![button(at, true)],
            (5, Some(at)) => vec![button(at, false)],
            _ => Vec::new(),
        };
        f.time += 0.05;
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, column)), time: Some(f.time), events, focused: true, ..Default::default() };
        let mut commands = Vec::new();
        let mut output = f.ctx.run_ui(input, |ui| {
            let view = View { hosts: &f.hosts, pads: &f.pads, connecting: None, waking: None };
            draw_home(&mut f.l, ui, &view, &mut commands);
        });
        // Debug builds of epaint refuse to drop font textures nobody uploaded.
        output.textures_delta.clear();
        drawn.clear();
        let mut placed = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut drawn);
            collect_placed(&clipped.shape, clipped.clip_rect, &mut placed);
        }
        tile = placed.iter().find(|(t, ..)| t == "All 12").map(|p| p.1.center());
    }
    assert!(drawn.iter().any(|t| t == "All 2"), "every long row ends in its tile: {drawn:?}");
    assert!(!drawn.iter().any(|t| t == "Steam 00"), "no cover fits beside the tile: {drawn:?}");
    assert!(tile.is_some(), "Steam's row is its All tile: {drawn:?}");
    assert_eq!(f.l.screen, Screen::AllGames, "the tile opens All games");
    assert_eq!(f.l.library_filter, console::LibraryFilter::Store("steam".into()));
}

/// A screen reader hears whose games an All tile opens, which destination is the current one,
/// and a subpage's Back apart from the destination of the same name.
#[test]
fn screen_readers_hear_the_all_tiles_store_the_current_destination_and_where_back_leads() {
    fn node<'a>(f: &'a Frames, label: &str) -> Option<&'a egui::accesskit::Node> {
        f.access.as_ref()?.nodes.iter().map(|(_, n)| n).find(|n| n.label() == Some(label))
    }
    use egui::accesskit::Toggled;
    let mut f = store_mix();
    f.ctx.enable_accesskit();
    f.settle();
    assert!(node(&f, "All Steam games, 12").is_some(), "the tile names its store");
    assert_eq!(node(&f, "Home").and_then(|n| n.toggled()), Some(Toggled::True), "Home is the current destination");
    assert_eq!(node(&f, "PCs").and_then(|n| n.toggled()), Some(Toggled::False));
    // A PC's library from the PCs list: Back leads to PCs and says so.
    let id = f.hosts.list[0].id;
    f.l.go(Screen::Hosts);
    f.l.open(Screen::Apps(id));
    f.settle();
    assert!(node(&f, "Back to PCs").is_some(), "Back is not a second \"PCs\"");
    assert_eq!(node(&f, "PCs").and_then(|n| n.toggled()), Some(Toggled::True), "the PCs pill, marked");
}

/// All games: chips for everything, what is installed, each store and the apps of no store; a chip
/// shows only its games, and a not-installed game says so on its cover.
#[test]
fn all_games_filters_by_store_and_by_what_is_installed() {
    let mut f = store_mix();
    f.l.open(Screen::AllGames);
    f.settle();
    let chip = |f: &Frames, name: &str| f.placed.iter().find(|(t, r, _)| t == name && r.center().y < 200.0).map(|p| p.1);
    for name in ["All", "Installed", "Steam", "Epic Games", "Battle.net", "Xbox", "Other apps"] {
        assert!(chip(&f, name).is_some(), "a {name} chip: {:?}", f.drawn);
    }
    assert!(shows(&f, "Alan Wake 2") && shows(&f, "Not installed"), "All shows what is not installed, marked: {:?}", f.drawn);
    for (name, shown, hidden) in [
        ("Xbox", "Halo Infinite", "Fortnite"),
        ("Battle.net", "Diablo IV", "Steam 00"),
        ("Epic Games", "Fortnite", "Halo Infinite"),
        ("Other apps", "Desktop", "Diablo IV"),
        ("Installed", "Fortnite", "Alan Wake 2"),
    ] {
        let at = chip(&f, name).unwrap();
        f.click_at(at.center(), egui::PointerButton::Primary);
        assert!(shows(&f, shown) && !shows(&f, hidden), "{name}: shows {shown}, not {hidden}: {:?}", f.drawn);
    }
    // The chosen chip is the filled one.
    let installed = chip(&f, "Installed").unwrap();
    assert!(meshes_holding(&f.shapes, design_tokens::TINT_SELECTED_FILL).iter().any(|r| r.contains_rect(installed)), "the chosen chip is marked");
    assert_eq!(f.l.library_filter, console::LibraryFilter::Installed);
}

/// Screenshot review: the focused first cover rises into the top of the scroll, and its ring was
/// cut there (in high contrast most). The grid leaves the room, on All games and a PC's library.
#[test]
fn a_risen_covers_ring_is_not_cut_at_the_top_of_the_grid() {
    for high in [false, true] {
        let mut store = store_mix();
        store.l.open(Screen::AllGames);
        let mut classic = Frames::with(record("Desk", true, true), Reach::Online, Some(answer(true, 0)), vec![game(7), game(9)]);
        let id = classic.hosts.list[0].id;
        classic.l.open(Screen::Apps(id));
        for (page, f) in [("All games", &mut store), ("library", &mut classic)] {
            f.l.settings.high_contrast = high;
            for _ in 0..5 {
                f.settle();
            }
            let cover = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).expect("the first cover has focus").rect;
            let color = if high { design_tokens::HC_FOCUS_RING_COLOR } else { design_tokens::FOCUS_RING_COLOR };
            let mut rings = Vec::new();
            for clipped in &f.shapes {
                if let egui::Shape::Mesh(mesh) = &clipped.shape
                    && mesh.vertices.iter().any(|v| v.color == color) {
                        rings.push((mesh.calc_bounds(), clipped.clip_rect));
                    }
            }
            let (ring, clip) = *rings.first().unwrap_or_else(|| panic!("{page}, {high}: a ring"));
            assert!(ring.top() < cover.top() - design_tokens::FOCUS_GAP - 1.0, "{page}, {high}: the cover rose: {ring:?} for {cover:?}");
            assert!(clip.contains_rect(ring), "{page}, {high}: the ring {ring:?} is cut by {clip:?}");
        }
    }
}

/// With nothing installed the Installed chip is omitted and Not installed is offered, an active
/// filter with zero matches says so, and an empty library has no chips at all.
#[test]
fn an_empty_filter_and_an_empty_library_say_so() {
    let mut f = home_with_stores(&[("Halo Infinite", "microsoft", false), ("Diablo IV", "battlenet", false)]);
    f.l.open(Screen::AllGames);
    f.settle();
    assert!(!f.placed.iter().any(|(t, r, _)| t == "Installed" && r.center().y < 200.0), "{:?}", f.drawn);
    assert!(f.placed.iter().any(|(t, r, _)| t == "Not installed" && r.center().y < 200.0), "{:?}", f.drawn);
    f.l.library_filter = console::LibraryFilter::NotInstalled;
    f.l.library_query = "nomatch".into();
    f.settle();
    assert!(shows(&f, "No games match this filter.") && !shows(&f, "Halo Infinite"), "{:?}", f.drawn);
    let mut f = home_with_stores(&[]);
    f.l.open(Screen::AllGames);
    f.settle();
    assert!(shows(&f, "No games to show yet. They appear here once a PC that shares its game library is online."), "{:?}", f.drawn);
    assert!(!shows(&f, "All") && !shows(&f, "No games match this filter."), "no chips: {:?}", f.drawn);
}

/// An empty library offers the way to games: wake the paired PC that sleeps, else go to the PCs.
#[test]
fn an_empty_library_offers_a_way_to_games() {
    let mut f = home_with_stores(&[]);
    f.l.open(Screen::AllGames);
    f.settle();
    let go = f.placed.iter().find(|(t, ..)| t == "Go to PCs").map(|p| p.1).unwrap_or_else(|| panic!("{:?}", f.drawn));
    f.click_at(go.center(), egui::PointerButton::Primary);
    assert!(f.l.screen == Screen::Hosts);
    let mut f = home_with_stores(&[]);
    f.hosts.list[0].reach = Reach::Offline;
    f.l.open(Screen::AllGames);
    f.settle();
    let wake = f.placed.iter().find(|(t, ..)| t == "Wake Desk").map(|p| p.1).unwrap_or_else(|| panic!("{:?}", f.drawn));
    let commands = f.click_at(wake.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::Wake(_))));
}

/// All games is reachable over Home's first shelf even when every row fits and no All tile shows.
/// It opens on All, whatever chip was chosen last.
#[test]
fn home_opens_all_games_from_its_top_row_when_every_row_fits() {
    let mut f = home_with_stores(&[("Portal", "steam", true), ("Celeste", "steam", true), ("Fortnite", "epic", true)]);
    f.l.library_filter = console::LibraryFilter::Store("epic".into());
    f.settle();
    assert!(!f.drawn.iter().any(|t| t.strip_prefix("All ").is_some_and(|n| n.parse::<usize>().is_ok())), "every row fits, no All tile: {:?}", f.drawn);
    let action = f.placed.iter().find(|(t, ..)| t == "All games").map(|p| p.1).expect("All games over the first shelf");
    let view = f.placed.iter().find(|(t, ..)| t == "View").map(|p| p.1).expect("View beside it");
    assert!(action.right() <= view.left() && 1280.0 - view.right() < 80.0, "All games, then View, at the right edge: {action:?} {view:?}");
    f.click_at(action.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.screen, Screen::AllGames);
    assert_eq!(f.l.library_filter, console::LibraryFilter::All);
    assert!(shows(&f, "Portal") && shows(&f, "Fortnite"), "{:?}", f.drawn);
    // An empty library offers no way to an empty page, and nothing to view.
    let mut f = home_with_stores(&[]);
    f.settle();
    assert!(!f.placed.iter().any(|(t, ..)| t == "All games" || t == "View"), "{:?}", f.drawn);
}

/// Home's View: a menu under its button whose rows step through the layout, grouping and card
/// size. Home changes behind it at once, the menu stays for the next try, and each choice is
/// saved; Escape closes it and gives focus back to the button.
#[test]
fn home_view_changes_layout_grouping_and_card_size_in_place() {
    let mut f = store_mix();
    let view = f.placed.iter().find(|(t, ..)| t == "View").map(|p| p.1).expect("View in the top row");
    f.click_at(view.center(), egui::PointerButton::Primary);
    assert!(matches!(f.l.dialog, Dialog::HomeView), "the menu opens");
    for row in ["Layout", "Group by", "Card size", "Rows", "Store", "Medium"] {
        assert!(shows(&f, row), "{row}: {:?}", f.drawn);
    }
    // Rows by store: Steam's twelve end in their All tile.
    assert!(shows(&f, "All 12") && !shows(&f, "Steam 11"), "{:?}", f.drawn);
    let row = |f: &Frames, label: &str| f.placed.iter().find(|(t, ..)| t == label).map(|p| p.1).expect(label);
    let at = row(&f, "Layout");
    let commands = f.click_at(at.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::SaveSettings)), "the choice is saved");
    assert_eq!(f.l.settings.home_layout, HomeLayout::Grid);
    assert!(matches!(f.l.dialog, Dialog::HomeView), "the menu stays open");
    assert!(shows(&f, "Grid"), "the row says the layout now: {:?}", f.drawn);
    // The grid shows every game of each row: no All tile.
    f.size = vec2(1280.0, 2400.0);
    f.settle();
    assert!(shows(&f, "Steam 11") && !shows(&f, "All 12"), "{:?}", f.drawn);
    assert!(shows(&f, "Battle.net"), "still a part per store: {:?}", f.drawn);

    // Group by: type. The games of every store in one part, the apps of no store in their own.
    let shelves = |f: &Frames| f.placed.iter().filter(|(t, r, _)| r.center().y > 80.0 && ["Steam", "Battle.net", "Epic Games", "Xbox", "Other apps", "Library", "Games"].contains(&t.as_str())).map(|p| p.0.clone()).collect::<Vec<_>>();
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::Type);
    assert!(shows(&f, "Type"), "the row says the grouping now: {:?}", f.drawn);
    assert_eq!(shelves(&f), ["Games", "Other apps"], "{:?}", f.drawn);

    // Group by: installation, offered since a game is not installed. Then none.
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::Installation);

    // Group by: none. One Library part, no store's.
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::None);
    assert_eq!(shelves(&f), ["Library"], "{:?}", f.drawn);
    assert!(shows(&f, "Desktop") && shows(&f, "Fortnite"), "every game in it: {:?}", f.drawn);

    // Back to a part per store, where Steam's first line holds Steam's covers alone.
    let at = row(&f, "Group by");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_group, HomeGroup::Store);
    // Card size: Medium, then Large, then Small, fewer covers a line the larger they are.
    let covers_in_line = |f: &Frames| {
        let first = f.placed.iter().find(|(t, ..)| t == "Steam 00").map(|p| p.1).expect("Steam 00");
        f.placed.iter().filter(|(t, r, _)| t.starts_with("Steam ") && (r.top() - first.top()).abs() < 1.0).count()
    };
    let medium = covers_in_line(&f);
    let at = row(&f, "Card size");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_card_size, CardSize::Large);
    let large = covers_in_line(&f);
    let at = row(&f, "Card size");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_card_size, CardSize::Small);
    let small = covers_in_line(&f);
    assert!(large < medium && medium < small, "large {large}, medium {medium}, small {small}");

    // List: every game a line of its own, its store and when it was last played under the title.
    let at = row(&f, "Layout");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_layout, HomeLayout::List);
    assert!(shows(&f, "List"), "{:?}", f.drawn);
    assert!(!shows(&f, "Card size"), "a list's lines have one size: {:?}", f.drawn);
    assert!(shows(&f, "Steam 11") && !shows(&f, "All 12"), "{:?}", f.drawn);
    assert!(!f.drawn.iter().any(|t| t.to_lowercase().contains("played")), "no PC says when: nothing about playing {:?}", f.drawn);
    // What the PC says: Steam 00 played just now, Steam 01 never.
    let install = |f: &Frames, title: &str| {
        let library = f.hosts.list[0].native.library.as_ref().expect("a library");
        library.games.iter().find(|g| g.title == title).map(|g| g.install_id.clone()).expect(title)
    };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let (played, never) = (install(&f, "Steam 00"), install(&f, "Steam 01"));
    f.hosts.list[0].native.last_played = Arc::new(std::collections::BTreeMap::from([(played, now), (never, 0)]));
    f.settle();
    assert!(shows(&f, "Steam · Played today") && shows(&f, "Steam · Not played yet"), "{:?}", f.drawn);
    // The title beside the cover (the last of the two the line draws), each line under the one before.
    let title = |f: &Frames, name: &str| f.placed.iter().rev().find(|(t, ..)| t == name).map(|p| p.1).expect(name);
    let (first, second) = (title(&f, "Steam 00"), title(&f, "Steam 01"));
    assert!(second.top() > first.bottom() && (second.left() - first.left()).abs() < 1.0, "one under the other: {first:?} {second:?}");
    // And round to Rows.
    let at = row(&f, "Layout");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_layout, HomeLayout::Rows);
    assert!(shows(&f, "Card size"), "back with the covers: {:?}", f.drawn);
    let at = row(&f, "Layout");
    f.click_at(at.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.settings.home_layout, HomeLayout::Grid);

    f.press(Key::Escape);
    assert!(matches!(f.l.dialog, Dialog::None));
    let focused = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).map(|r| r.rect).expect("focus");
    assert!(focused.contains_rect(view), "back on View: {focused:?}");
}

/// List: the pad's Down walks from line to line, and A on a line opens its game.
#[test]
fn home_list_lines_walk_with_the_pad_and_open_their_game() {
    let mut f = store_mix();
    f.l.settings.home_layout = HomeLayout::List;
    f.settle();
    let focused = |f: &Frames| {
        let rect = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).map(|r| r.rect)?;
        f.placed.iter().rev().find(|(t, at, _)| t.starts_with("Steam ") && !t.contains('·') && rect.contains(at.center())).map(|p| p.0.clone())
    };
    for _ in 0..8 {
        if focused(&f).as_deref() == Some("Steam 00") { break; }
        pad_press(&mut f, Key::ArrowDown);
    }
    assert_eq!(focused(&f).as_deref(), Some("Steam 00"), "{:?}", f.drawn);
    pad_press(&mut f, Key::ArrowDown);
    assert_eq!(focused(&f).as_deref(), Some("Steam 01"));
    pad_press(&mut f, Key::Enter);
    assert!(matches!(f.l.screen, Screen::Game { .. }), "{:?}", f.l.screen);
}

/// Rows, not by store: one Library line of what fits, its All tile opening All games.
#[test]
fn home_rows_of_one_library_end_in_its_all_tile() {
    let mut f = store_mix();
    f.l.settings.home_group = HomeGroup::None;
    f.settle();
    assert!(shows(&f, "Library") && shows(&f, "All 19"), "{:?}", f.drawn);
    let tile = f.placed.iter().find(|(t, ..)| t == "All 19").map(|p| p.1).unwrap();
    f.click_at(tile.center(), egui::PointerButton::Primary);
    assert_eq!(f.l.screen, Screen::AllGames);
    assert_eq!(f.l.library_filter, console::LibraryFilter::All);
}

/// A focused cover's title that is cut short runs sideways after a pause, and only then.
#[test]
fn a_focused_cover_title_too_long_runs_sideways_after_a_pause() {
    assert_eq!(caption_offset(200.0, 150.0, false, true, 5.0), None, "not focused");
    assert_eq!(caption_offset(200.0, 150.0, true, false, 5.0), None, "reduced motion");
    assert_eq!(caption_offset(120.0, 150.0, true, true, 5.0), None, "it fits");
    let (start, span) = caption_offset(200.0, 150.0, true, true, 0.5).unwrap();
    assert_eq!(start, 0.0, "the start stays a moment");
    assert!(span > 200.0, "the next copy follows after a gap");
    let (later, _) = caption_offset(200.0, 150.0, true, true, 3.0).unwrap();
    assert!(later > 0.0 && later <= span);
    // Two passes after the focus came, the title rests cut short: nothing runs on.
    let pass = 1.2 + span / 36.0;
    assert!(caption_offset(200.0, 150.0, true, true, f64::from(pass) + 0.5).is_some(), "the second pass");
    assert_eq!(caption_offset(200.0, 150.0, true, true, f64::from(2.0 * pass) + 0.01), None, "done");
    assert_eq!(caption_offset(200.0, 150.0, true, true, 1000.0), None);
}

/// While a PC's library is on its way, All games says so as Home does instead of calling the
/// library empty: the covers' places in glass, under the words.
#[test]
fn all_games_shows_the_library_loading_as_home_does() {
    let mut f = Frames::with(record("Desk", true, true), Reach::Online, Some(files::tests::native_answer()), Vec::new());
    f.l.go(Screen::Home);
    f.settle();
    assert!(shows(&f, "Loading the games…"), "Home: {:?}", f.drawn);
    assert!(!meshes_holding(&f.shapes, design_tokens::COVER_PLACEHOLDER).is_empty(), "Home holds the covers' places");
    f.l.open(Screen::AllGames);
    f.settle();
    assert!(shows(&f, "Loading the games…"), "All games: {:?}", f.drawn);
    assert!(meshes_holding(&f.shapes, design_tokens::COVER_PLACEHOLDER).len() >= 2, "two lines of places");
    assert!(!shows(&f, "No games to show yet. They appear here once a PC that shares its game library is online."), "{:?}", f.drawn);
}

/// The chips come before the games for Tab and the arrow keys, and the first game has focus on
/// arrival, so A starts browsing straight away.
#[test]
fn all_games_focuses_the_first_game_and_reaches_the_chips_above_it() {
    let mut f = store_mix();
    f.l.open(Screen::AllGames);
    f.settle();
    let focused = |f: &Frames| f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).map(|r| r.rect).expect("focus");
    let first = focused(&f);
    assert!(first.top() > 120.0, "a game has focus, not a chip: {first:?}");
    f.press(Key::ArrowUp);
    let chip = focused(&f);
    assert!(chip.bottom() < first.top() && chip.height() < 40.0, "Up reaches the chips: {chip:?}");
}

/// A chip keeps the focus when the Installed chip before it comes or goes, and a store Nightjar
/// does not know is named with a capital, not by its bare key.
#[test]
fn a_filter_chip_keeps_the_focus_when_the_installed_chip_comes_or_goes() {
    let mut f = home_with_stores(&[("Portal", "steam", true), ("Celeste", "steam", false), ("Fortnite", "epic", true), ("Lone", "itchio", true)]);
    f.l.open(Screen::AllGames);
    f.settle();
    assert!(shows(&f, "Itchio") && !shows(&f, "itchio"), "{:?}", f.drawn);
    let chip = |f: &Frames, name: &str| f.placed.iter().find(|(t, r, _)| t == name && r.center().y < 200.0).map(|p| p.1).expect(name);
    let focused = |f: &Frames| f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).map(|r| r.rect).expect("focus");
    // Up from the first game reaches the chips; Right walks them to Epic Games.
    f.press(Key::ArrowUp);
    for _ in 0..8 {
        if focused(&f).contains_rect(chip(&f, "Epic Games")) {
            break;
        }
        f.press(Key::ArrowRight);
    }
    assert!(focused(&f).contains_rect(chip(&f, "Epic Games")), "the Epic Games chip has focus");
    // Celeste gets installed: the Installed chip goes, the chips after it move left.
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[1].installed = Some(true);
    f.settle();
    assert!(!f.placed.iter().any(|(t, r, _)| t == "Installed" && r.center().y < 200.0), "{:?}", f.drawn);
    assert!(focused(&f).contains_rect(chip(&f, "Epic Games")), "the focus stayed on Epic Games");
    // And it comes back.
    Arc::make_mut(f.hosts.list[0].native.library.as_mut().unwrap()).games[1].installed = Some(false);
    f.settle();
    assert!(focused(&f).contains_rect(chip(&f, "Epic Games")), "the focus stayed on Epic Games");
}

/// A cover's menu opens with F2 (X) beside it, with a right-click at the pointer, and from its ⋮
/// under the dots; it lists the game's actions with Enter on what selecting the cover does, and
/// Escape gives focus back to the cover.
#[test]
fn a_cover_menu_opens_by_the_cover_and_gives_focus_back() {
    let mut f = store_mix();
    let cover = f.ctx.memory(|m| m.focused()).expect("a cover has focus");
    let rect = f.ctx.read_response(cover).unwrap().rect;
    f.press(Key::F2);
    assert!(matches!(f.l.dialog, Dialog::GameMenu { game: GameTarget::Library { .. }, .. }), "F2 opens the game's menu");
    for row in ["Game details", "Add to favourites", "Open this when Nightjar starts", "Open library"] {
        assert!(shows(&f, row), "{row}: {:?}", f.drawn);
    }
    assert!(shows(&f, "Enter"), "the row selecting does says Enter");
    let details = f.placed.iter().find(|(t, ..)| t == "Game details").map(|p| p.1).unwrap();
    assert!(details.left() > rect.right() || details.right() < rect.left(), "beside the cover, not over it: {details:?} by {rect:?}");
    f.press(Key::Escape);
    assert!(f.no_dialog());
    assert_eq!(f.ctx.memory(|m| m.focused()), Some(cover), "focus is back on the cover");
    // A right-click opens it at the pointer.
    let at = rect.center();
    f.click_at(at, egui::PointerButton::Secondary);
    assert!(matches!(f.l.dialog, Dialog::GameMenu { .. }));
    let details = f.placed.iter().find(|(t, ..)| t == "Game details").map(|p| p.1).unwrap();
    assert!((details.left() - at.x).abs() < 80.0 && (details.top() - at.y).abs() < 120.0, "at the pointer: {details:?} for {at:?}");
    // A press outside closes it.
    f.click_at(Pos2::new(1200.0, 700.0), egui::PointerButton::Primary);
    assert!(f.no_dialog(), "a press outside closes the menu");
    // Enter on Game details opens the game's page.
    f.l.focused_key = None;
    f.l.restore_key = None;
    f.ctx.memory_mut(|m| m.request_focus(cover));
    f.settle();
    f.press(Key::F2);
    f.press(Key::Enter);
    assert!(matches!(f.l.screen, Screen::Game { .. }), "Game details opens the page");
}

/// The page under a menu by its card stays as it is: not faded, and the pointer cannot reach it.
#[test]
fn a_menu_by_its_card_leaves_the_page_undimmed_and_out_of_reach() {
    let mut f = Frames::new(record("Desk", true, true), Reach::Online);
    let (card, dots) = f.card_and_dots();
    f.click_at(dots.center(), egui::PointerButton::Primary);
    assert!(f.options_open());
    assert!(rects_filled(&f, Color32::from_black_alpha(150)).is_empty(), "no backdrop over the page");
    let rename = f.placed.iter().find(|(t, ..)| t == "Rename").map(|p| p.1).unwrap();
    assert!(rename.left() < dots.right(), "by the dots: {rename:?} by {dots:?}");
    // A click on the card under the page closes the menu without starting anything.
    let commands = f.click_at(card.left_center() + vec2(20.0, 0.0), egui::PointerButton::Primary);
    assert!(f.no_dialog(), "a press outside closes the menu");
    assert!(!commands.iter().any(|c| matches!(c, Command::OpenApps(_))), "and does not reach the card");
}

/// Remove PC and Unpair come last, under a divider, in the error colour.
#[test]
fn destructive_options_come_last_in_red() {
    let mut f = Frames::new(record("Desk", true, true), Reach::Online);
    let options = host_options(&f.hosts.list[0]);
    assert_eq!(&options[options.len() - 2..], [HostOption::Unpair, HostOption::Remove]);
    assert!(options[..options.len() - 2].iter().all(|o| !o.destructive()));
    f.press(Key::F2);
    let mut colours = Vec::new();
    for clipped in &f.shapes {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Color32)>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(t) => out.push((t.galley.text().to_owned(), t.fallback_color)),
                _ => {}
            }
        }
        walk(&clipped.shape, &mut colours);
    }
    for (name, red) in [("Remove PC", true), ("Unpair", true), ("Rename", false)] {
        let colour = colours.iter().find(|(t, _)| t == name).map(|c| c.1).expect(name);
        assert_eq!(colour == *ERROR, red, "{name}");
    }
}
