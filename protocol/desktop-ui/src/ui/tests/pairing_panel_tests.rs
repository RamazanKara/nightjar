//! Pairing on the stage: one centred panel with the PIN a cell a digit under the PC's name, one
//! line saying where to type it and Cancel; a failed pairing says why, whole, in the error colour,
//! with Try again as the panel's amber action.

use super::*;

const WHERE: &str = "Enter this PIN in the PC’s settings or web UI, usually on its PIN or pairing page.";
const WHY: &str = "PIN rejected. Generate a new one-time PIN in the PC’s web UI and check the passphrase.";

/// The panel waiting for the PC with `pin` to type there, past its fade-in.
fn pairing(pin: &str) -> Frames {
    let mut f = Frames::new(record("Studio", false, false), Reach::Online);
    let id = f.hosts.list[0].id;
    f.l.dialog = Dialog::Pairing { host: id, pin: pin.into() };
    f.l.focus_dialog = true;
    for _ in 0..4 {
        f.settle();
    }
    f
}

/// The panel of a pairing that failed with [`WHY`], and the PC's id.
fn failed(one_time: bool) -> (Frames, u64) {
    let mut f = Frames::new(record("Studio", false, false), Reach::Online);
    let id = f.hosts.list[0].id;
    f.l.pair_failed(id, WHY, one_time);
    for _ in 0..4 {
        f.settle();
    }
    (f, id)
}

fn at(f: &Frames, text: &str) -> Rect {
    f.placed.iter().rev().find(|(t, ..)| t == text).map(|p| p.1).unwrap_or_else(|| panic!("{text} in {:?}", f.drawn))
}

fn focused(f: &Frames) -> Rect {
    f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).expect("something has the focus").rect
}

/// The colour each galley of `text` asks for itself.
fn own_inks(f: &Frames, text: &str) -> Vec<Color32> {
    fn walk(shape: &egui::Shape, text: &str, out: &mut Vec<Color32>) {
        match shape {
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, text, out)),
            egui::Shape::Text(t) if t.galley.text() == text => out.extend(t.galley.job.sections.first().map(|s| s.format.color)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for clipped in &f.shapes {
        walk(&clipped.shape, text, &mut out);
    }
    out
}

#[test]
fn the_pin_stands_a_cell_a_digit_centred_under_the_pcs_name() {
    let f = pairing("4071");
    let title = at(&f, "Pair with Studio");
    let digits: Vec<Rect> = ["4", "0", "7", "1"].iter().map(|d| at(&f, d)).collect();
    // A cell and its gap from one digit's middle to the next, whatever the digit's own width.
    assert!(digits.windows(2).all(|w| (w[1].center().x - w[0].center().x - 84.0).abs() < 1.0 && (w[1].center().y - w[0].center().y).abs() < 0.5), "{digits:?}");
    let middle = (digits[0].center().x + digits[3].center().x) / 2.0;
    let (line, waiting, cancel) = (at(&f, WHERE), at(&f, "Waiting for the PC\u{2026}"), at(&f, "Cancel"));
    for (what, rect) in [("the PC's name", title), ("where to type it", line), ("Cancel", cancel)] {
        assert!((rect.center().x - middle).abs() < 1.5, "{what} is centred on the PIN: {rect:?}, {middle}");
    }
    assert!((middle - f.size.x / 2.0).abs() < 1.5, "the panel in the window's middle: {middle}");
    assert!(title.bottom() <= digits[0].top() && digits[0].bottom() < line.top(), "{title:?} {digits:?} {line:?}");
    assert!(line.bottom() <= waiting.top() && waiting.bottom() <= cancel.top(), "{line:?} {waiting:?} {cancel:?}");
    assert!(line.width() <= 440.0 && line.height() > 30.0, "one line of words, wrapped in the panel's column: {line:?}");
}

#[test]
fn cancel_has_the_focus_and_stops_the_pairing() {
    let mut f = pairing("4071");
    assert!(focused(&f).contains(at(&f, "Cancel").center()), "Cancel has the focus");
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::CancelPair)), "Enter cancels the request");
    assert!(f.no_dialog());
    let mut f = pairing("4071");
    let commands = click_in_dialog(&mut f, "Cancel");
    assert!(commands.iter().any(|c| matches!(c, Command::CancelPair)), "a click does too");
    assert!(f.no_dialog());
}

/// A one-time PIN was typed here: there is none to show, and nothing says to enter one.
#[test]
fn a_pairing_with_no_pin_to_show_waits_with_the_pcs_name_alone() {
    let f = pairing("");
    for words in ["Pair with Studio", "Waiting for the PC\u{2026}", "Cancel"] {
        assert!(shows(&f, words), "{words}: {:?}", f.drawn);
    }
    assert!(!shows(&f, WHERE), "{:?}", f.drawn);
    assert!(focused(&f).contains(at(&f, "Cancel").center()));
}

#[test]
fn a_failed_pairing_says_why_whole_in_the_error_colour() {
    let (mut f, id) = failed(false);
    let (title, why) = (at(&f, "Pairing with Studio failed"), at(&f, WHY));
    assert_eq!(own_inks(&f, WHY), vec![Color32::from(ERROR)], "the app's own message, in the error colour");
    assert!(why.width() <= 440.0 && title.bottom() <= why.top(), "whole, wrapped under the heading: {title:?} {why:?}");
    let (close, retry) = (at(&f, "Close"), at(&f, "Try again"));
    assert!(why.bottom() <= retry.top() && (close.center().y - retry.center().y).abs() < 0.5 && close.right() < retry.left(), "{why:?} {close:?} {retry:?}");
    // Try again is the amber one and has the focus; the two are as wide as each other.
    let ring = focused(&f);
    assert!(ring.contains(retry.center()), "{ring:?} {retry:?}");
    assert_eq!(own_inks(&f, "Try again"), vec![glass::on_play()], "the words of an amber button");
    f.press(Key::ArrowLeft);
    let other = focused(&f);
    assert!(other.contains(close.center()) && (other.width() - ring.width()).abs() < 0.5, "{other:?} {ring:?}");
    f.press(Key::ArrowRight);
    let commands = f.press(Key::Enter);
    assert!(commands.iter().any(|c| matches!(c, Command::Pair(host) if *host == id)), "Try again pairs again");
    assert!(f.no_dialog());
}

/// The amber of the panel's action is the same by day, so its words stay dark on it: the day
/// look's own ink for amber is white, which amber does not carry.
#[test]
fn try_again_keeps_its_dark_words_on_amber_by_day() {
    let (mut f, _) = failed(false);
    f.l.settings.theme = crate::config::Theme::Light;
    f.settle();
    assert_eq!(Color32::from(ON_SURFACE), design_tokens::DAY_COLOR_LABEL, "the day look is on");
    assert_eq!(own_inks(&f, "Try again"), vec![design_tokens::COLOR_ON_MOON]);
    assert_eq!(own_inks(&f, WHY), vec![design_tokens::DAY_COLOR_ERROR], "and the reason in the day's error colour");
}

#[test]
fn close_leaves_a_failed_pairing_and_a_one_time_pin_is_typed_again() {
    let (mut f, _) = failed(false);
    let commands = click_in_dialog(&mut f, "Close");
    assert!(commands.is_empty() && f.no_dialog(), "Close asks for nothing: {} commands", commands.len());
    let (mut f, id) = failed(true);
    let commands = click_in_dialog(&mut f, "Try again");
    assert!(!commands.iter().any(|c| matches!(c, Command::Pair(_))), "a one-time PIN is not sent again");
    assert!(matches!(&f.l.dialog, Dialog::Otp { host, pin, .. } if *host == id && pin.is_empty()), "back to the PIN form");
}

/// A short window drops the panel's mark and has smaller cells; the largest interface size in
/// German keeps every word of both panels in the window.
#[test]
fn the_panels_fit_a_short_window_and_the_largest_interface_size() {
    let mut f = pairing("4071");
    let cell = |f: &Frames| at(f, "0").center().x - at(f, "4").center().x;
    // How far under the panel's top its heading starts: its mark and the margin, or the margin.
    let over_the_name = |f: &Frames| at(f, "Pair with Studio").top() - f.ctx.memory(|m| m.area_rect(Id::unique("pairing"))).expect("the panel").top();
    assert!((cell(&f) - 84.0).abs() < 1.0 && over_the_name(&f) > 90.0, "{} {}", cell(&f), over_the_name(&f));
    f.size = vec2(720.0, 480.0);
    f.settle();
    assert!((cell(&f) - 66.0).abs() < 1.0, "smaller cells: {}", cell(&f));
    assert!(over_the_name(&f) < 40.0, "no mark over the name: {}", over_the_name(&f));
    assert_fits(&f, "pairing in the smallest window", true);
    let mut f = Frames::new(record("A long name for a gaming PC", false, false), Reach::Online);
    fit_150(&mut f);
    let id = f.hosts.list[0].id;
    for dialog in [Dialog::Pairing { host: id, pin: "1234".into() }, Dialog::PairError { host: id, body: WHY.into(), one_time: true }] {
        let name = format!("{:?}", std::mem::discriminant(&dialog));
        f.l.dialog = dialog;
        f.settle();
        assert_fits(&f, &format!("{name} at 150 %"), true);
    }
}
