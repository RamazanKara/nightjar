//! Controller input navigation, pairing/invite links, and PIN lock tests.

use super::*;

/// A PC's card shows the cover of the game it runs, else of the last game this device played
/// there, as Android's PC tile does; with neither, its own tile.
#[test]
fn a_pc_card_shows_the_running_or_last_games_cover() {
    let mut f = Frames::with(record("Den", true, true), Reach::Online, Some(answer(true, 0)), vec![nightjar_policy::http::App { name: "Racing Game".into(), ..game(7) }]);
    let texture = cover_texture(&f, "racing");
    Arc::make_mut(&mut f.hosts.list[0].art).insert(7, Art::Ready(texture.clone()));
    let shows_cover = |f: &Frames| meshes_drawn(f).iter().any(|(m, _)| m.texture_id == texture.id());
    f.settle();
    assert!(!shows_cover(&f), "nothing played there yet");
    f.l.recents = vec![crate::config::Recent { host: "Den".into(), install_id: String::new(), app_id: 7, title: "Racing Game".into(), at: 1 }];
    f.settle();
    assert!(shows_cover(&f), "the last game played there");
    assert_eq!(super::pcs::card_game(&f.hosts.list[0], &f.l.recents), Some(7));
    f.hosts.list[0].info = Some(answer(true, 9));
    assert_eq!(super::pcs::card_game(&f.hosts.list[0], &f.l.recents), Some(9), "the game it runs comes first");
    f.hosts.list[0].reach = Reach::Offline;
    assert_eq!(super::pcs::card_game(&f.hosts.list[0], &f.l.recents), None, "away, its screen is off");
}

#[test]
fn a_pcs_own_wake_addresses_are_checked_before_they_are_kept() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::WakeAddresses { host: id, text: "the 2.5 Gb adapter".into(), invalid: false };
    f.settle();
    let commands = click_text(&mut f, "Save");
    assert!(!commands.iter().any(|c| matches!(c, Command::WakeAddresses { .. })), "nothing is kept");
    assert!(matches!(f.l.dialog, Dialog::WakeAddresses { invalid: true, .. }), "the dialog stays open");
    f.settle();
    assert!(shows(&f, "Enter up to 8 MAC addresses, such as 00:11:22:33:44:55"), "{:?}", f.drawn);

    let Dialog::WakeAddresses { text, .. } = &mut f.l.dialog else { panic!("the wake addresses dialog") };
    *text = "0A-00-27-00-00-05\n".into();
    let commands = click_text(&mut f, "Save");
    let macs = commands.iter().find_map(|c| match c {
        Command::WakeAddresses { host, macs } if *host == id => Some(macs.clone()),
        _ => None,
    });
    assert_eq!(macs, Some(vec!["0a:00:27:00:00:05".to_owned()]));
    assert!(!f.hosts.list[0].record.can_wake());
    assert!(f.hosts.set_own_wake_macs(id, vec!["0a:00:27:00:00:05".into()]));
    assert!(f.hosts.list[0].record.can_wake(), "a PC with only the user's own address can be woken");
}

// --- Artwork and game info (SteamGridDB, IGDB) --------------------------------------------------

/// Without facts (no IGDB keys) All games has no Genre or Length chip; with some, both come, and
/// each narrows the grid with the source chips.
#[test]
fn all_games_offers_genre_and_length_only_once_games_have_facts() {
    let mut f = store_mix();
    f.l.go(Screen::AllGames);
    f.settle();
    assert!(!shows(&f, "Genre") && !shows(&f, "Length"), "{:?}", f.drawn);
    let (fortnite, diablo) = (facts_key(&f, "Fortnite"), facts_key(&f, "Diablo IV"));
    f.hosts.put_facts(&fortnite, game_facts(&["Shooter"], &[], 10));
    f.hosts.put_facts(&diablo, game_facts(&["Role-playing (RPG)"], &["Fantasy"], 60));
    f.settle();
    assert!(shows(&f, "Genre") && shows(&f, "Length"), "{:?}", f.drawn);
    f.l.genre_filter = Some("Role-playing (RPG)".into());
    f.settle();
    assert!(shows(&f, "Diablo IV") && !shows(&f, "Fortnite") && !shows(&f, "Steam 00"), "{:?}", f.drawn);
    assert!(shows(&f, "Role-playing (RPG)"), "the chip names the genre chosen");
    f.l.genre_filter = None;
    f.l.length_filter = Some(nightjar_policy::game_info::library::Length::Medium);
    f.settle();
    assert!(shows(&f, "Fortnite") && !shows(&f, "Diablo IV"), "{:?}", f.drawn);
    assert!(shows(&f, "5 to 15 hours"));
    // With the Epic chip too: Fortnite is Epic's, so it stays; Battle.net's chip leaves nothing.
    f.l.library_filter = console::LibraryFilter::Store("battlenet".into());
    f.settle();
    assert!(shows(&f, "No games match this filter."), "{:?}", f.drawn);
    // A genre no game has any more goes back to any.
    f.l.library_filter = console::LibraryFilter::All;
    f.l.length_filter = None;
    f.l.genre_filter = Some("Racing".into());
    f.settle();
    assert_eq!(f.l.genre_filter, None);
}

/// The game page writes the genres, themes and time to beat IGDB has for the game, and nothing
/// without them.
#[test]
fn the_game_page_shows_the_facts_of_its_game() {
    let mut f = store_mix();
    let pc = f.hosts.list[0].id;
    f.l.go(Screen::Game { host: pc, app: 1012 });
    f.settle();
    assert!(!f.drawn.iter().any(|t| t.starts_with("Genres") || t.starts_with("Time to beat")), "{:?}", f.drawn);
    let key = facts_key(&f, "Fortnite");
    f.hosts.put_facts(&key, game_facts(&["Shooter", "Battle royale"], &["Action"], 12));
    f.settle();
    assert!(shows(&f, "Genres: Shooter · Battle royale"), "{:?}", f.drawn);
    assert!(shows(&f, "Themes: Action"), "{:?}", f.drawn);
    assert!(shows(&f, "Time to beat: Story 9 h · Story + extras 12 h · Everything 24 h"), "{:?}", f.drawn);
}

/// No keys: no lookups are asked for. With IGDB's keys, opening All games asks for each game's
/// facts once.
#[test]
fn facts_are_asked_for_only_with_igdb_keys_and_once() {
    struct OfferedFacts;
    impl crate::hosts::FactsSource for OfferedFacts {
        fn offers_facts(&self) -> bool { true }
        fn cached(&self, _: &str) -> Option<nightjar_policy::game_info::cache::Facts> { None }
    }
    let mut f = store_mix();
    f.l.go(Screen::AllGames);
    let commands = f.frame(Vec::new());
    assert!(!commands.iter().any(|c| matches!(c, Command::WantFacts(_))));
    let dir = std::env::temp_dir().join(format!("nightjar-ui-facts-{}", std::process::id()));
    f.hosts.set_facts_source(Some(Arc::new(OfferedFacts)));
    let commands = f.frame(Vec::new());
    let asked: Vec<_> = commands.iter().filter_map(|c| if let Command::WantFacts(games) = c { Some(games.len()) } else { None }).collect();
    assert_eq!(asked, [19], "every game once");
    for command in commands {
        if let Command::WantFacts(games) = command {
            f.hosts.want_facts(games);
        }
    }
    let commands = f.frame(Vec::new());
    assert!(!commands.iter().any(|c| matches!(c, Command::WantFacts(_))), "not again");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The pages that show SteamGridDB's and IGDB's part: All games with the Genre and Length chips,
/// a game page with its facts, and Settings → Advanced with the keys' rows.
#[test]
#[ignore = "writes PNGs; set NIGHTJAR_SHOTS"]
fn game_info_screenshots() {
    let Some(dir) = std::env::var_os("NIGHTJAR_SHOTS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).expect("the folder for the screenshots");
    let mut f = store_mix();
    f.size = vec2(1280.0, 800.0);
    f.native_ppp = 1.5;
    for (title, genres, themes, hours) in [
        ("Fortnite", &["Shooter"][..], &["Action"][..], 10),
        ("Diablo IV", &["Role-playing (RPG)", "Hack and slash/Beat 'em up"][..], &["Fantasy"][..], 30),
        ("Halo Infinite", &["Shooter"][..], &["Science fiction"][..], 12),
        ("Forza Horizon 5", &["Racing", "Sport"][..], &["Open world"][..], 60),
        ("Steam 00", &["Puzzle"][..], &[][..], 4),
    ] {
        let key = facts_key(&f, title);
        f.hosts.put_facts(&key, game_facts(genres, themes, hours));
    }
    f.l.go(Screen::AllGames);
    f.settle();
    f.shoot(&dir, "all-games-facts-desktop");
    f.l.genre_filter = Some("Shooter".into());
    f.l.length_filter = Some(nightjar_policy::game_info::library::Length::Medium);
    f.settle();
    f.shoot(&dir, "all-games-filtered-desktop");
    let pc = f.hosts.list[0].id;
    f.l.go(Screen::Game { host: pc, app: 1014 });
    f.settle();
    f.shoot(&dir, "game-facts-desktop");
    f.l.go(Screen::Settings);
    f.l.settings_view.section = "Advanced";
    f.l.game_info_keys.steamgriddb = "0123456789abcdef".into();
    f.settle();
    f.shoot(&dir, "settings-advanced-game-info-desktop");
}

/// A pairing key that cannot be read never arrives: the first-run page says why instead of the
/// creating line, and offers no search that cannot start.
#[test]
fn the_first_run_says_why_a_pairing_key_cannot_be_read() {
    let reason = format!("{} (identity.json: is a directory)", crate::strings::PAIRING_KEY_UNREADABLE);
    let mut f = first_run_frames(Vec::new());
    f.l.identity_ready = false;
    f.l.identity_error = Some(reason.clone());
    f.l.go(Screen::Hosts);
    f.settle();
    assert!(shows(&f, &reason), "{:?}", f.drawn);
    assert!(!f.drawn.iter().any(|t| t.starts_with("Creating this device")), "{:?}", f.drawn);
    assert!(!shows(&f, "Search again"), "no search can start without a key: {:?}", f.drawn);
}
