//! Home's hero stage: its width and place, the one amber control, the focus order, the narrow
//! window, the states without art and without games, and its screenshots:
//! `NIGHTJAR_SHOTS=<dir> cargo test -p nightjar-desktop-ui --lib hero_stage_screenshots -- --ignored`.

use super::*;
use crate::config::Theme;
use crate::theme::Look;

/// The demo's hero (its running game) and its app id.
const HERO_APP: i32 = 1001;

fn focused(f: &Frames) -> Option<Id> {
    f.ctx.memory(|m| m.focused())
}

fn rect_of(f: &Frames, id: Id) -> Rect {
    f.ctx.read_response(id).unwrap_or_else(|| panic!("{id:?} was laid out")).rect
}

fn stage(f: &Frames) -> hero_stage::Placed {
    hero_stage::placed(&f.ctx).expect("Home drew its hero stage")
}

/// The meshes filled with Play's amber: each is one amber control.
fn amber_controls(f: &Frames) -> usize {
    let mut found = meshes_holding(&f.shapes, design_tokens::PLAY_FILL_TOP);
    for rect in meshes_holding(&f.shapes, design_tokens::PLAY_FILL_BOTTOM) {
        if !found.contains(&rect) {
            found.push(rect);
        }
    }
    found.len()
}

/// The demo with its hero's cover gone: the PC has no art for it.
fn demo_without_art(size: Vec2) -> Frames {
    let mut f = demo_frames(size);
    let pc = &mut f.hosts.list[0];
    Arc::make_mut(&mut pc.art).insert(HERO_APP, Art::Missing);
    Arc::make_mut(&mut pc.ambient).remove(&HERO_APP);
    Arc::make_mut(&mut pc.ambient_glow).remove(&HERO_APP);
    f.settle();
    f
}

fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (hero_stage::luminance(a), hero_stage::luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn on_an_ultrawide_the_stage_keeps_its_width_and_stands_in_the_middle() {
    let f = demo_frames(vec2(2560.0, 1080.0));
    let at = stage(&f);
    assert_eq!(at.stage.width(), design_tokens::LAYOUT_STAGE_MAX_WIDTH, "{at:?}");
    assert!((at.stage.center().x - 1280.0).abs() < 1.0, "centred: {at:?}");
    let cover = rect_of(&f, Id::unique("hero-cover"));
    assert!((cover.left() - at.stage.left()).abs() < 1.0, "the cover stands at the stage's start: {cover:?}");
    let rail = at.rail.expect("Continue");
    assert!(at.beside && (rail.right() - at.stage.right()).abs() < 1.0, "the rail ends with the stage: {rail:?}");
    // A usual window: the stage is the page.
    let window = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
    let page = Rect::from_min_max(pos2(48.0, 88.0), pos2(1232.0, 88.0));
    assert_eq!(hero_stage::place(page, window, 3).stage.width(), 1184.0);
    // The cover keeps its 2:3 shape in every window.
    for (w, h) in [(960.0, 600.0), (1280.0, 800.0), (1920.0, 1080.0), (2560.0, 1080.0), (720.0, 480.0)] {
        let size = hero_stage::cover_size(h, w - 96.0);
        assert!((size.y / size.x - 1.5).abs() < 1e-3, "{w}x{h}: {size:?}");
    }
}

#[test]
fn home_has_one_amber_control_whatever_the_heros_state() {
    // Running, with a Continue rail: Resume and Quit game.
    let mut f = demo_frames(vec2(1280.0, 800.0));
    assert!(shows(&f, "Resume") && shows(&f, "Quit game"), "{:?}", f.drawn);
    assert_eq!(amber_controls(&f), 1, "running");
    f.l.settings.theme = Theme::Light;
    f.settle();
    assert_eq!(amber_controls(&f), 1, "by day");
    // Idle: Play and the star.
    let f = home_with_games(3);
    assert!(shows(&f, "Play") && f.ctx.read_response(Id::unique("hero-star")).is_some(), "{:?}", f.drawn);
    assert_eq!(amber_controls(&f), 1, "idle");
    // Something stops Play: Try again is the one quiet action, never a second amber one.
    let mut f = home_with_games(1);
    f.hosts.list[0].apps = Apps::Failed("The PC did not answer".into());
    f.l.refocus();
    f.settle();
    assert!(shows(&f, "Try again") && f.ctx.read_response(Id::unique("hero-star")).is_none(), "{:?}", f.drawn);
    assert!(amber_controls(&f) <= 1, "with a problem");
}

#[test]
fn the_second_action_is_try_again_then_quit_game_then_the_star() {
    use hero_stage::{second_action, Second};
    assert_eq!(second_action(false, true, true, true), Second::Retry);
    assert_eq!(second_action(true, true, false, true), Second::Star, "away, Play is the way back");
    assert_eq!(second_action(false, false, true, true), Second::End);
    assert_eq!(second_action(false, false, false, true), Second::Star);
    assert_eq!(second_action(false, false, false, false), Second::None);
}

#[test]
fn play_has_the_focus_first_then_tab_walks_the_second_action_the_cover_and_continue() {
    let mut f = demo_frames(vec2(1280.0, 800.0));
    f.l.input_source = crate::input_source::InputSource::Keyboard;
    f.settle();
    assert_eq!(focused(&f), Some(Id::unique("hero-play")), "Play first");
    f.press(Key::Tab);
    assert_eq!(focused(&f), Some(Id::unique("hero-end")), "then the one quiet action");
    f.press(Key::Tab);
    assert_eq!(focused(&f), Some(Id::unique("hero-cover")), "then the cover");
    f.press(Key::Tab);
    let rail = stage(&f).rail.expect("Continue");
    let at = focused(&f).and_then(|id| f.ctx.read_response(id)).expect("focus").rect;
    assert!(rail.contains(at.center()), "then Continue: {at:?} in {rail:?}");
}

#[test]
fn a_narrow_window_puts_continue_under_the_hero() {
    let wide = demo_frames(vec2(1280.0, 800.0));
    let at = stage(&wide);
    let rail = at.rail.expect("Continue");
    assert!(at.beside && rail.left() >= at.words.max && rail.bottom() <= at.cover.bottom() + 1.0, "beside: {at:?}");
    let narrow = demo_frames(vec2(960.0, 600.0));
    let at = stage(&narrow);
    let rail = at.rail.expect("Continue");
    assert!(!at.beside && rail.top() >= at.cover.bottom(), "under: {at:?}");
    assert!(at.columns > 1, "its lines side by side: {at:?}");
    for (text, rect, _) in &narrow.placed {
        if ["Open World", "Ocean Explorer", "Space Sim"].contains(&text.as_str()) && rect.top() < 600.0 && rail.contains(rect.center()) {
            assert!(rect.right() <= 960.0, "{text}: {rect:?}");
        }
    }
}

#[test]
fn nothing_on_the_stage_is_cut_off_at_960x600() {
    for language in [crate::config::Language::English, crate::config::Language::German] {
        let mut f = demo_frames(vec2(960.0, 600.0));
        f.l.settings.language = language;
        f.settle();
        f.settle();
        let what = format!("Home at 960x600 in {language:?}");
        let window = Rect::from_min_size(Pos2::ZERO, f.size);
        let at = stage(&f);
        let foot = at.rail.map_or(at.stage.bottom(), |rail| rail.bottom().max(at.stage.bottom()));
        let hero = Rect::from_min_max(pos2(0.0, at.stage.top()), pos2(window.right(), foot));
        let mut seen = 0;
        for (text, rect, clip) in f.placed.iter().filter(|(t, r, _)| !t.trim().is_empty() && hero.contains(r.center())) {
            seen += 1;
            assert!(rect.left() >= clip.left() - 1.0 && rect.right() <= clip.right() + 1.0, "{what}: {text:?} runs past its clip: {rect:?} {clip:?}");
            assert!(rect.left() >= at.stage.left() - 1.0 && rect.right() <= at.stage.right() + 1.0, "{what}: {text:?} leaves the stage: {rect:?} {at:?}");
            assert!(rect.bottom() <= window.bottom(), "{what}: {text:?} under the window's foot: {rect:?}");
        }
        assert!(seen >= 6, "{what}: the title, the facts, the buttons and Continue: {:?}", f.placed);
        for id in ["hero-play", "hero-end", "hero-cover"] {
            let rect = rect_of(&f, Id::unique(id));
            assert!(window.contains_rect(rect), "{what}: {id} {rect:?}");
        }
        let play = rect_of(&f, Id::unique("hero-play"));
        let end = rect_of(&f, Id::unique("hero-end"));
        assert!(play.right() < end.left() && end.right() <= at.words.max + 1.0, "{what}: {play:?} {end:?} {at:?}");
        assert!(at.cover.top() >= chrome::PAGE_TOP - 1.0, "{what}: under the chrome");
    }
}

#[test]
fn a_hero_without_art_lays_its_tiles_gradient_where_the_art_would_be() {
    let f = demo_frames(vec2(1280.0, 800.0));
    assert_eq!(hero_stage::art_drawn(&f.ctx), Some(false), "the cover");
    let f = demo_without_art(vec2(1280.0, 800.0));
    assert_eq!(hero_stage::art_drawn(&f.ctx), Some(true), "the tile's gradient");
    assert!(f.l.ambient.shows_art(), "and the tile behind the page");
    assert!(shows(&f, "Racing Game"), "its title: {:?}", f.drawn);
}

#[test]
fn with_no_games_home_is_the_dark_stage_and_one_line() {
    let line = "No games to show yet. They appear here once a PC that shares its game library is online.";
    let f = home_with_stores(&[]);
    assert!(hero_stage::placed(&f.ctx).is_none(), "no hero");
    assert!(shows(&f, line), "{:?}", f.drawn);
    assert!(!f.l.ambient.shows_art(), "the stage, no art");
    assert_eq!(amber_controls(&f), 0, "nothing to play");
    let words = f.placed.iter().find(|(t, ..)| t == line).map(|p| p.1).expect("the line");
    let way = f.placed.iter().find(|(t, ..)| t == "Go to PCs").map(|p| p.1).expect("the way to games");
    let middle = f.size.x / 2.0;
    assert!((words.center().x - middle).abs() < 2.0 && (way.center().x - middle).abs() < 2.0, "centred: {words:?} {way:?}");
    assert!(words.bottom() + 12.0 <= way.top(), "the way under the line, apart: {words:?} {way:?}");
    // The stage keeps the hero's height: the PCs' shelf starts under it, not under the button.
    let pcs = f.placed.iter().filter(|(t, ..)| t == "PCs").map(|p| p.1).find(|r| r.top() > way.bottom()).expect("the PCs' shelf");
    assert!(pcs.top() >= chrome::PAGE_TOP + 220.0, "under the stage: {pcs:?}, the way {way:?}");
    assert_eq!(f.drawn.iter().filter(|t| *t == line).count(), 1, "said once: {:?}", f.drawn);
}

#[test]
fn the_heros_words_read_over_the_brightest_art_by_night_and_the_darkest_by_day() {
    // By night the art and the ambient are held to the text zone's luminance: a white cover is
    // the worst ground.
    let night = Color32::from_gray((hero_stage::night_hold() * 255.0).round() as u8);
    for ink in [design_tokens::COLOR_LABEL, design_tokens::COLOR_LABEL_SECONDARY] {
        assert!(contrast(ink, night) >= 4.5, "{ink:?} over {night:?}: {}", contrast(ink, night));
    }
    // By day a black cover is washed toward the canvas, and the ambient (black at worst) lies
    // under the backdrop's day wash (`backdrop::DAY_WASH`, 0.65).
    let canvas = design_tokens::DAY_COLOR_CANVAS;
    let washed = |by: f32| Color32::from_rgb((f32::from(canvas.r()) * by) as u8, (f32::from(canvas.g()) * by) as u8, (f32::from(canvas.b()) * by) as u8);
    for ground in [washed(hero_stage::day_wash()), washed(0.65)] {
        assert!(contrast(design_tokens::DAY_COLOR_LABEL, ground) >= 4.5, "{ground:?}");
    }
    let tile = hero_stage::held(design_tokens::TILE_PAIR6_TOP, true);
    assert!(contrast(design_tokens::DAY_COLOR_LABEL, tile) >= 4.5, "a tile by day: {tile:?}");
    crate::theme::own_look(Look::Day);
    assert_eq!(hero_stage::facts_ink(), *ON_SURFACE, "by day the facts take the full label colour");
    crate::theme::own_look(Look::Night);
    assert_eq!(hero_stage::facts_ink(), *ON_SURFACE_VARIANT);
    crate::theme::own_palette();
}

/// Home's hero in each look and window, with art, without it, and with no games at all.
#[test]
#[ignore = "writes PNGs; set NIGHTJAR_SHOTS"]
fn hero_stage_screenshots() {
    const WINDOWS: [(&str, [f32; 2]); 4] = [("1280x800", [1280.0, 800.0]), ("1920x1080", [1920.0, 1080.0]), ("2560x1080", [2560.0, 1080.0]), ("960x600", [960.0, 600.0])];
    const LOOKS: [(&str, Theme, bool); 3] = [("night", Theme::Dark, false), ("day", Theme::Light, false), ("high-contrast", Theme::Dark, true)];
    let Some(dir) = std::env::var_os("NIGHTJAR_SHOTS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).expect("the folder for the screenshots");
    for (look_name, theme, high) in LOOKS {
        for (window_name, size) in WINDOWS {
            let size = Vec2::from(size);
            let mut empty = home_with_stores(&[]);
            empty.size = size;
            for (state, mut f) in [("art", demo_frames(size)), ("no-art", demo_without_art(size)), ("no-games", empty)] {
                f.l.settings.theme = theme;
                f.l.settings.high_contrast = high;
                // Past every fade and spring: what the page looks like at rest.
                for _ in 0..6 {
                    f.settle();
                }
                f.shoot(&dir, &format!("hero-{state}-{look_name}-{window_name}"));
            }
        }
    }
}
