//! Responsive chrome, screen-reader labels, and destructive menu ordering tests.

use super::*;

/// From a game's ambient to the sky the moon fades in with it; nothing pops in at full, the fade
/// runs to its end and the moon stands in one place all along.
#[test]
fn from_art_to_the_sky_the_moon_fades_in_with_it() {
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
        assert!(f.l.ambient.shows_art() && backdrop::painted(&f.ctx).moons.is_empty(), "{what}: the game's tile shows");
        f.l.go(page);
        let frames = follow_the_fade(&mut f, &what, false);
        let first = &frames[0];
        assert!(first.fading && first.along < 0.35, "{what}: the sky fades in from the art: {first:?}");
        let discs: std::collections::HashSet<_> = frames.iter().flat_map(|p| p.moons.iter().map(|(d, _)| (d.0.x.to_bits(), d.0.y.to_bits(), d.1.to_bits()))).collect();
        assert!(discs.len() <= 1, "{what}: one moon in one place: {frames:?}");
        assert_moon_clear(&f, &what);
    }
}

/// From one page's sky to the next the old moon fades out where it stood as the new one fades in
/// where it stands: none pops in or out at full, and the sky's fade is never cut short.
#[test]
fn from_sky_to_sky_the_moons_cross_fade() {
    let pairs = [(Screen::Settings, Screen::Help), (Screen::Profiles, Screen::Hosts), (Screen::Hosts, Screen::Profiles), (Screen::Profiles, Screen::Controllers), (Screen::Hosts, Screen::Settings), (Screen::StreamHotkeys, Screen::Profiles)];
    let windows = [(vec2(1280.0, 800.0), 150), (vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100)];
    let mut moved = 0;
    for (window, zoom) in windows {
        for (from, to) in pairs {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.go(from);
            f.settle();
            let before = backdrop::disc_of(&f.ctx);
            f.l.go(to);
            let what = format!("{from:?} to {to:?} at {window:?}, {zoom} %");
            follow_the_fade(&mut f, &what, false);
            let after = backdrop::disc_of(&f.ctx);
            if before.is_some() && after.is_some() && before != after {
                moved += 1;
            }
            assert_moon_clear(&f, &what);
        }
    }
    assert!(moved > 0, "some page change moves the moon");
}

/// Pointing at or focusing what stands nearest the moon moves nothing: a card's lift shadow and a
/// focus halo are no cards, and the moon neither moves nor fades.
#[test]
fn hover_and_focus_by_the_moon_leave_it_where_it_stands() {
    let pages = [Screen::Hosts, Screen::Profiles, Screen::Settings, Screen::Controllers];
    // The PCs at 1200x560 and 1020x600: the moon stands its gap below a card, within its lift shadow.
    let windows = [(vec2(1200.0, 560.0), 100), (vec2(1020.0, 600.0), 100), (vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100), (vec2(1280.0, 800.0), 150), (vec2(1600.0, 900.0), 125)];
    let mut reached = 0;
    for (window, zoom) in windows {
        for page in pages {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.go(page);
            f.settle();
            let Some((center, r)) = backdrop::disc_of(&f.ctx) else { continue };
            let screen = Rect::from_min_size(Pos2::ZERO, f.size);
            // What can be pointed at near the moon, but the chrome and the scroll bar.
            let mut near: Vec<egui::WidgetRect> = f.ctx.viewport(|v| {
                v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter().copied()).filter(|w| w.sense.senses_click() && w.rect.top() >= chrome::PAGE_TOP && w.rect.height() < screen.height() * 0.8 && w.rect.distance_to_pos(center) < r + 80.0).collect()
            });
            near.sort_by(|a, b| a.rect.distance_to_pos(center).total_cmp(&b.rect.distance_to_pos(center)));
            near.truncate(4);
            for widget in near {
                let what = format!("{page:?} at {window:?}, {zoom} %, by {:?}", widget.rect);
                let soft = glass::soft_textures(&f.ctx);
                let reaches = |f: &Frames| meshes_drawn(f).iter().any(|(m, _)| soft.contains(&m.texture_id) && m.calc_bounds().distance_to_pos(center) < r);
                let calm = |f: &Frames, how: &str| {
                    assert_eq!(backdrop::disc_of(&f.ctx), Some((center, r)), "{what}: {how} moves the moon");
                    let painted = backdrop::painted(&f.ctx);
                    assert!(!painted.fading && painted.moons == vec![((center, r), 1.0)], "{what}: {how} fades the moon: {painted:?}");
                };
                f.frame(vec![egui::Event::PointerMoved(widget.rect.center())]);
                for _ in 0..3 {
                    f.settle();
                    reached += usize::from(reaches(&f));
                    calm(&f, "pointing");
                }
                f.frame(vec![egui::Event::PointerGone]);
                f.settle();
                if widget.sense.is_focusable() {
                    f.ctx.memory_mut(|m| m.request_focus(widget.id));
                    for _ in 0..3 {
                        f.settle();
                        reached += usize::from(reaches(&f));
                        if backdrop::disc_of(&f.ctx) == Some((center, r)) {
                            calm(&f, "the focus");
                        } else {
                            panic!("{what}: the focus moves the moon to {:?}", backdrop::disc_of(&f.ctx));
                        }
                    }
                    f.ctx.memory_mut(|m| m.surrender_focus(widget.id));
                    f.settle();
                }
            }
        }
    }
    assert!(reached > 0, "a shadow or halo reached the moon somewhere, and it stayed");
}

/// A window resized or zoomed shows no moon from before: the frame it changes in, the moon already
/// stands clear and whole in the new window, or has stepped back.
#[test]
fn a_resized_or_zoomed_window_shows_no_stale_moon() {
    for page in [Screen::Hosts, Screen::Profiles, Screen::Controllers, Screen::Settings] {
        let mut f = four_pcs();
        f.size = vec2(1280.0, 800.0);
        f.l.go(page);
        f.settle();
        // A window drag changes the size; a zoom changes the points the next frame.
        let steps = [(vec2(1100.0, 650.0), 100), (vec2(1100.0, 650.0), 150), (vec2(1280.0, 800.0), 150), (vec2(1920.0, 1080.0), 150), (vec2(1920.0, 1080.0), 100), (vec2(720.0, 490.0), 100), (vec2(1280.0, 800.0), 100), (vec2(1280.0, 800.0), 125)];
        for (window, zoom) in steps {
            if zoom != f.l.settings.ui_zoom {
                f.l.settings.ui_zoom = zoom;
                f.frame(Vec::new());
            }
            let (before, last) = (backdrop::disc_of(&f.ctx), backdrop::painted(&f.ctx));
            f.size = window;
            f.frame(Vec::new());
            let what = format!("{page:?} the frame it went to {window:?}, {zoom} %");
            assert_moon_clear(&f, &what);
            let disc = backdrop::disc_of(&f.ctx);
            if let (Some(_), Some(now)) = (before, disc) {
                assert_eq!(backdrop::painted(&f.ctx).moons, vec![(now, 1.0)], "{what}: one moon that moves, at full");
            } else {
                // It steps back or comes back: it fades.
                fade_step(&last, &backdrop::painted(&f.ctx), &what, 0, false);
            }
            // As the page settles in the new window it may move on, never fade out and in.
            follow_the_fade(&mut f, &what, true);
            f.settle();
            assert_moon_clear(&f, &what);
        }
    }
}

/// A page first seen scrolled (here: the window changed size while it was) could not show what
/// scrolled away; its moon keeps out of that, so it stays put when the page goes back to its top.
#[test]
fn a_page_first_seen_scrolled_keeps_its_moon_at_the_top() {
    let (mut placed, mut back) = (0, 0);
    // Ten PCs: rows scroll away above, and the last row leaves sky on the right.
    for (page, more) in [(Screen::Profiles, 0), (Screen::Hosts, 0), (Screen::Hosts, 6), (Screen::Settings, 0), (Screen::Help, 0), (Screen::Controllers, 0)] {
        for (first, then) in [(vec2(1280.0, 800.0), vec2(1100.0, 650.0)), (vec2(1100.0, 650.0), vec2(1280.0, 800.0)), (vec2(1280.0, 800.0), vec2(1920.0, 1080.0)), (vec2(853.0, 533.0), vec2(1000.0, 800.0))] {
            let mut f = pcs_more(more);
            f.size = first;
            f.l.go(page);
            f.settle();
            f.scroll_to_end();
            f.size = then;
            f.settle();
            let what = format!("{page:?} scrolled at {first:?}, then {then:?}");
            let disc = backdrop::disc_of(&f.ctx);
            placed += usize::from(disc.is_some());
            f.frame(vec![egui::Event::PointerMoved((f.size / 2.0).to_pos2())]);
            f.frame(vec![egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(0.0, 1000.0), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers: egui::Modifiers::NONE }]);
            for _ in 0..10 {
                f.settle();
            }
            if disc.is_some() {
                assert_eq!(backdrop::disc_of(&f.ctx), disc, "{what}: back at the top the moon stays");
            } else {
                // One that stepped back for what it could not see comes back as on the page first seen at its top.
                let mut top = pcs_more(more);
                top.size = then;
                top.l.go(page);
                top.settle();
                assert_eq!(backdrop::disc_of(&f.ctx), backdrop::disc_of(&top.ctx), "{what}: back at the top the moon is where it stands");
                back += usize::from(backdrop::disc_of(&f.ctx).is_some());
            }
            assert_moon_clear(&f, &what);
        }
    }
    assert!(placed > 0, "some page first seen scrolled has a moon");
    assert!(back > 0, "some page first seen scrolled gets its moon back at the top");
}

/// The PIN lock's settings page stands its moon clear of its rows too: its sky goes under it once
/// it is drawn, as the launcher's does (it sat under the rows at 1280x800, 150 %).
#[test]
fn the_pin_page_moon_stands_clear_of_its_rows() {
    let dir = std::env::temp_dir().join(format!("nightjar-pin-moon-{}", std::process::id()));
    for (window, zoom) in [(vec2(1280.0, 800.0), 150), (vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100), (vec2(1920.0, 1080.0), 100)] {
        for mode in [None, Some(crate::pin_lock::Mode::App)] {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.pin = Some(crate::pin_ui::PinLock::demo_page(&dir, mode));
            f.settle();
            f.settle();
            let what = format!("the PIN page ({mode:?}) at {window:?}, {zoom} %");
            assert_moon_clear(&f, &what);
        }
    }
    std::fs::remove_dir_all(dir).ok();
}

/// Where the moon stepped back for a long Settings section, a short one where it stands when opened
/// first brings it back, clear; and back on the long one it steps back again.
#[test]
fn the_moon_comes_back_where_the_page_makes_room_again() {
    let sections = ["Recommended", "Video", "Display & image", "Controls", "Audio", "Streaming", "App", "Advanced", "Debug"];
    let mut back = 0;
    for (window, zoom) in [(vec2(1280.0, 800.0), 150), (vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100), (vec2(1920.0, 1080.0), 100)] {
        let open = |section: &'static str| {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            // Every row, so some sections are long.
            f.l.settings_view.more = [true; 9];
            f.l.settings_view.section = section;
            f.l.go(Screen::Settings);
            f.settle();
            f.settle();
            f
        };
        let firsts: Vec<_> = sections.iter().map(|s| backdrop::disc_of(&open(s).ctx)).collect();
        let Some(long) = firsts.iter().position(Option::is_none) else { continue };
        for (short, first) in firsts.iter().enumerate().filter(|(_, d)| d.is_some()) {
            let mut f = open(sections[long]);
            let what = format!("Settings from {} to {} at {window:?}, {zoom} %", sections[long], sections[short]);
            f.l.settings_view.section = sections[short];
            follow_the_fade(&mut f, &what, false);
            f.settle();
            assert!(first.is_some() && backdrop::disc_of(&f.ctx).is_some(), "{what}: the moon comes back");
            assert_moon_clear(&f, &what);
            back += 1;
            f.l.settings_view.section = sections[long];
            follow_the_fade(&mut f, &what, false);
            f.settle();
            assert_eq!(backdrop::disc_of(&f.ctx), None, "{what}: and steps back again");
        }
    }
    assert!(back > 0, "some section brings the moon back");
}

/// Back to the sky a fade is leaving, the fade turns where it stands: what shows never jumps.
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

/// A cover raised by the focus (drawn after the page, over its neighbours) counts as what the page
/// holds: the moon kept from the sky that came before stands clear of it, also where it is placed
/// anew (the window changed) while the cover is up.
#[test]
fn a_raised_cover_by_the_moon_moves_it_clear() {
    let mut checked = 0;
    for (window, zoom) in [(vec2(1280.0, 800.0), 100), (vec2(1100.0, 650.0), 100), (vec2(1280.0, 800.0), 150), (vec2(1920.0, 1080.0), 100), (vec2(1600.0, 900.0), 125)] {
        for count in [3, 5, 8] {
            let mut f = home_with_games(count);
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.go(Screen::Hosts);
            f.settle();
            f.l.go(Screen::AllGames);
            f.settle();
            f.settle();
            let Some((center, r)) = backdrop::disc_of(&f.ctx) else { continue };
            let mut near: Vec<egui::WidgetRect> = f.ctx.viewport(|v| v.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter().copied()).filter(|w| w.sense.is_focusable() && w.rect.top() >= chrome::PAGE_TOP && w.rect.distance_to_pos(center) < r + 60.0).collect());
            near.sort_by(|a, b| a.rect.distance_to_pos(center).total_cmp(&b.rect.distance_to_pos(center)));
            near.truncate(3);
            for widget in near {
                let what = format!("All games, {count} games at {window:?}, {zoom} %, focus on {:?}", widget.rect);
                f.ctx.memory_mut(|m| m.request_focus(widget.id));
                for _ in 0..4 {
                    f.frame(Vec::new());
                    assert_moon_clear(&f, &what);
                }
                for grow in [vec2(24.0, 0.0), vec2(0.0, 24.0), vec2(-48.0, -24.0)] {
                    f.size += grow;
                    f.frame(Vec::new());
                    assert_moon_clear(&f, &format!("{what}, the window by {grow:?}"));
                    f.settle();
                    assert_moon_clear(&f, &format!("{what}, the window by {grow:?}"));
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "some cover by the moon was raised");
}

/// A sky page with its moon placed paints nothing more at rest.
#[test]
fn a_placed_moon_at_rest_asks_for_no_frames() {
    let mut placed = 0;
    for page in [Screen::Hosts, Screen::Profiles, Screen::Settings, Screen::Help] {
        for (window, zoom) in [(vec2(1280.0, 800.0), 100), (vec2(1920.0, 1080.0), 100)] {
            let mut f = four_pcs();
            f.l.settings.ui_zoom = zoom;
            f.size = window;
            f.l.go(page);
            for _ in 0..12 {
                f.settle();
            }
            placed += usize::from(backdrop::disc_of(&f.ctx).is_some());
            assert!(!f.ctx.has_requested_repaint(), "{page:?} at {window:?}, {zoom} % at rest asks for no frame");
        }
    }
    assert!(placed > 0, "some page placed its moon");
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
