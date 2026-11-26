//! The first run's scan and what it finds: the pulse alone asks for frames while the page rests,
//! at most thirty a second and only while the network is searched with nothing found, and a found
//! PC is a hardware card with the page's one amber action.

use super::*;
use std::sync::Mutex;
use std::time::Duration;

/// The first run on the PCs page, at rest.
fn scanning() -> Frames {
    let mut f = first_run_frames(Vec::new());
    f.l.go(Screen::Hosts);
    rest(&mut f);
    f
}

/// Past every spring and fade.
fn rest(f: &mut Frames) {
    for _ in 0..4 {
        f.settle();
    }
}

/// `count` PCs found on the network being searched, none of them paired yet.
fn found(count: usize) -> Hosts {
    let records = (0..count).map(|n| HostRecord { address: format!("192.0.2.{}", 20 + n), ..record(&format!("PC {n}"), false, false) }).collect();
    let mut hosts = Hosts::without_discovery(records, Arc::new(|| {}));
    hosts.discovering = true;
    for host in &mut hosts.list {
        host.reach = Reach::Online;
    }
    hosts
}

/// How soon each frame asked for from here on is wanted, as it is asked.
fn asked(f: &Frames) -> Arc<Mutex<Vec<Duration>>> {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let sink = asked.clone();
    f.ctx.set_repaint_observer(move |info| sink.lock().unwrap().push(info.delay));
    asked
}

/// The pulse's rings in the last frame: circles in line around the window's middle, as (centre's
/// height, radius, line width, colour).
fn rings(f: &Frames) -> Vec<(f32, f32, f32, Color32)> {
    fn walk(shape: &egui::Shape, middle: f32, out: &mut Vec<(f32, f32, f32, Color32)>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, middle, out)),
            egui::Shape::Circle(c) if c.fill == Color32::TRANSPARENT && (c.center.x - middle).abs() < 0.5 && c.center.y > chrome::PAGE_TOP => {
                out.push((c.center.y, c.radius, c.stroke.width, c.stroke.color));
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &f.shapes {
        walk(&clipped.shape, f.size.x / 2.0, &mut out);
    }
    out
}

fn at(f: &Frames, text: &str) -> Rect {
    f.placed.iter().find(|(t, ..)| t == text).map(|p| p.1).unwrap_or_else(|| panic!("{text} in {:?}", f.drawn))
}

fn focused(f: &Frames) -> Rect {
    f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).expect("something has the focus").rect
}

#[test]
fn the_scan_asks_for_thirty_frames_a_second_at_most_and_stops_with_the_search() {
    let mut f = scanning();
    let before = rings(&f);
    assert_eq!(before.len(), 3, "{before:?}");
    let asked = asked(&f);
    for _ in 0..6 {
        f.frame(Vec::new());
    }
    let delays = std::mem::take(&mut *asked.lock().unwrap());
    assert!(!delays.is_empty(), "the pulse asks for its next frame");
    let frame = Duration::from_secs_f32(1.0 / 30.0);
    assert!(delays.iter().all(|delay| *delay >= frame), "never sooner than a thirtieth of a second: {delays:?}");
    assert_ne!(rings(&f), before, "the rings move");
    // The search stops: the rings stand, and the page asks for nothing.
    f.hosts.discovering = false;
    rest(&mut f);
    asked.lock().unwrap().clear();
    let standing = rings(&f);
    assert_eq!(standing.len(), 3, "{standing:?}");
    for _ in 0..4 {
        f.frame(Vec::new());
    }
    assert!(asked.lock().unwrap().is_empty(), "{:?}", asked.lock().unwrap());
    assert_eq!(rings(&f), standing);
}

#[test]
fn a_found_pc_takes_the_pulses_place_and_the_page_rests() {
    let mut f = scanning();
    f.hosts = found(1);
    f.frame(Vec::new());
    assert!(!rings(&f).is_empty(), "the pulse fades while the card rises");
    rest(&mut f);
    assert!(rings(&f).is_empty(), "and is gone when it has landed: {:?}", rings(&f));
    let asked = asked(&f);
    for _ in 0..4 {
        f.frame(Vec::new());
    }
    assert!(asked.lock().unwrap().is_empty(), "{:?}", asked.lock().unwrap());
    // The PC is gone again: so is its card, and the scan is back.
    let mut none = Hosts::without_discovery(Vec::new(), Arc::new(|| {}));
    none.discovering = true;
    f.hosts = none;
    rest(&mut f);
    assert_eq!(rings(&f).len(), 3);
    assert!(!shows(&f, "Pair"), "{:?}", f.drawn);
}

#[test]
fn with_reduced_motion_three_rings_stand_and_ask_for_nothing() {
    let mut f = first_run_frames(Vec::new());
    f.l.settings.reduce_motion = ReduceMotion::Reduce;
    f.l.go(Screen::Hosts);
    rest(&mut f);
    let standing = rings(&f);
    assert_eq!(standing.len(), 3, "{standing:?}");
    assert!(standing.windows(2).all(|w| w[0].0 == w[1].0 && w[1].1 > w[0].1 + 20.0), "one centre, apart from one another: {standing:?}");
    let asked = asked(&f);
    for _ in 0..4 {
        f.frame(Vec::new());
    }
    assert!(asked.lock().unwrap().is_empty(), "{:?}", asked.lock().unwrap());
    assert_eq!(rings(&f), standing);
}

#[test]
fn high_contrast_shows_one_plain_ring_that_stands() {
    let mut f = first_run_frames(Vec::new());
    f.l.settings.high_contrast = true;
    f.l.go(Screen::Hosts);
    rest(&mut f);
    let ring = rings(&f);
    assert_eq!(ring.len(), 1, "{ring:?}");
    assert_eq!((ring[0].2, ring[0].3), (2.0, design_tokens::HC_COLOR_RIM));
    let asked = asked(&f);
    for _ in 0..4 {
        f.frame(Vec::new());
    }
    assert!(asked.lock().unwrap().is_empty(), "{:?}", asked.lock().unwrap());
    assert_eq!(rings(&f), ring);
}

/// The pulse stands clear of the chrome and of its words, which are centred under it with Add PC
/// and the link, whole in the window, in the design's window, the smallest and an ultrawide.
#[test]
fn the_scan_and_its_words_fit_every_window() {
    for size in [vec2(1280.0, 800.0), vec2(720.0, 480.0), vec2(2560.0, 1080.0)] {
        let mut f = first_run_frames(Vec::new());
        f.l.settings.reduce_motion = ReduceMotion::Reduce;
        f.size = size;
        f.l.go(Screen::Hosts);
        rest(&mut f);
        let outer = rings(&f).into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).expect("the rings");
        let (words, add, link) = (at(&f, "Searching for PCs\u{2026}"), at(&f, "Add PC"), at(&f, "How to connect"));
        assert!(outer.0 - outer.1 >= chrome::PAGE_TOP - 0.5, "{size:?}: under the chrome, {outer:?}");
        assert!(outer.0 + outer.1 < words.top(), "{size:?}: over its words, {outer:?} {words:?}");
        assert!((words.center().x - size.x / 2.0).abs() < 1.0, "{size:?}: {words:?}");
        assert!(words.bottom() <= add.top() && add.bottom() <= link.top() && link.bottom() <= size.y, "{size:?}: {words:?} {add:?} {link:?}");
        assert!(focused(&f).contains(add.center()), "{size:?}: Add PC has the focus");
        // The link's words keep 12 points clear of that focus ring, at its widest (high contrast's).
        let ring = focused(&f).bottom() + design_tokens::HC_FOCUS_GAP + design_tokens::HC_FOCUS_RING_WIDTH;
        assert!(link.top() - ring >= 12.0, "{size:?}: {link:?} under a ring that ends at {ring}");
    }
}

#[test]
fn several_found_pcs_stack_with_the_one_amber_action_on_the_first() {
    let mut f = scanning();
    f.hosts = found(3);
    rest(&mut f);
    let ids: Vec<u64> = f.hosts.list.iter().map(|host| host.id).collect();
    let action = |f: &Frames, id: u64| f.ctx.read_response(Id::unique(("found-card", id)).with("pair")).expect("its action").rect;
    let actions: Vec<Rect> = ids.iter().map(|id| action(&f, *id)).collect();
    assert!(actions.windows(2).all(|w| w[1].top() > w[0].bottom() && w[1].left() == w[0].left() && w[1].width() == w[0].width()), "one column: {actions:?}");
    let names: Vec<Rect> = (0..3).map(|n| at(&f, &format!("PC {n}"))).collect();
    assert!(names.iter().zip(&actions).all(|(name, action)| name.right() <= action.left() && action.y_range().contains(name.center().y)), "{names:?} {actions:?}");
    // Amber once: the first card's Pair is the solid one, the others are amber words on a tint.
    let mut inks = Vec::new();
    for clipped in &f.shapes {
        galley_inks(&clipped.shape, "Pair", &mut inks);
    }
    assert_eq!(inks.len(), 3, "{:?}", f.drawn);
    assert_eq!(inks.iter().filter(|ink| **ink == glass::on_play()).count(), 1, "{inks:?}");
    assert!(focused(&f).contains(actions[0].center()), "the first has the focus");
    f.press(Key::ArrowDown);
    assert!(focused(&f).contains(action(&f, ids[1]).center()), "Down goes to the next card's");
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Pair(host) if *host == ids[1])), "and Enter pairs with that PC");
}

#[test]
fn a_found_card_says_how_the_pc_stands_and_where_and_takes_the_mouse() {
    let mut f = scanning();
    f.hosts = found(1);
    rest(&mut f);
    let id = f.hosts.list[0].id;
    assert!(shows(&f, "Not paired \u{b7} 192.0.2.20"), "{:?}", f.drawn);
    let card = f.ctx.read_response(Id::unique(("found-card", id))).expect("the card").rect;
    let (name, pair) = (at(&f, "PC 0"), at(&f, "Pair"));
    assert!(card.contains_rect(name) && card.contains_rect(pair), "{card:?} {name:?} {pair:?}");
    assert!((card.center().x - f.size.x / 2.0).abs() < 0.5 && card.width() <= 560.0, "{card:?}");
    // The card itself pairs too, and its options open beside it.
    let commands = f.click_at(name.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::Pair(host) if *host == id)), "a click on the card");
    let commands = f.click_at(pair.center(), egui::PointerButton::Primary);
    assert!(commands.iter().any(|c| matches!(c, Command::Pair(host) if *host == id)), "a click on Pair");
    f.click_at(name.center(), egui::PointerButton::Secondary);
    assert!(f.options_open(), "the other button opens the PC's options");
}

/// A window few points wide (the smallest one at the largest interface size) puts Pair under the
/// card's words instead of cutting them.
#[test]
fn a_found_card_in_a_narrow_window_puts_pair_under_its_words() {
    let mut f = first_run_frames(vec![record("Living Room PC", false, false)]);
    f.hosts.list[0].reach = Reach::Online;
    f.size = vec2(480.0, 480.0);
    f.l.go(Screen::Hosts);
    rest(&mut f);
    let (name, pair) = (at(&f, "Living Room PC"), at(&f, "Pair"));
    assert!(pair.top() >= name.bottom(), "{name:?} {pair:?}");
    assert!(pair.right() <= f.size.x && name.right() <= f.size.x, "{name:?} {pair:?}");
}
