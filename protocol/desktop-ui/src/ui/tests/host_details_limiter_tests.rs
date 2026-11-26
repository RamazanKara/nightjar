//! The read-only rows of a PC's details page. The frame limiter's: one group of rows, each with its
//! value at the trailing edge, and nothing at all for a PC that does not report its limiter. The
//! "PC" section's and the remote check's measurements stand in the same rows.

use super::*;

/// Where the topmost `text` of the last frame stands, and its clip.
fn topmost(f: &Frames, text: &str) -> (Rect, Rect) {
    f.placed.iter().filter(|(t, ..)| t == text).min_by(|a, b| a.1.top().total_cmp(&b.1.top())).map(|p| (p.1, p.2)).unwrap_or_else(|| panic!("{text} in {:?}", f.drawn))
}

/// Each (label, value) of `rows` stands in a row of one group: the value beside its label at the
/// rows' one trailing edge, and a row's height from one to the next.
fn assert_one_group(f: &Frames, rows: &[(&str, &str)]) -> Vec<(Rect, Rect)> {
    let rows: Vec<(Rect, Rect)> = rows.iter().map(|(label, value)| (topmost(f, label).0, topmost(f, value).0)).collect();
    for (label, value) in &rows {
        assert!((label.center().y - value.center().y).abs() < 1.0, "a value beside its label: {label:?} {value:?}");
        assert!((label.left() - rows[0].0.left()).abs() < 0.5 && (value.right() - rows[0].1.right()).abs() < 0.5, "one edge for the labels, one for the values: {rows:?}");
        assert!(label.right() + space::M <= value.left() + 0.5, "{label:?} {value:?}");
    }
    assert!(rows.windows(2).all(|w| (w[1].0.center().y - w[0].0.center().y - design_tokens::SIZE_ROW_HEIGHT).abs() < 0.5), "the rows join: {rows:?}");
    rows
}

#[test]
fn the_pc_section_is_one_group_of_rows_down_to_the_web_ui() {
    let mut f = details(Reach::Online, "SUNSHINE_SERVER_FREE", RUBYLIGHT);
    f.hosts.list[0].record.address = "192.0.2.7".into();
    f.hosts.list[0].info.as_mut().expect("the answer").gpu_type = Some("RX 7900 XT".into());
    f.settle();
    let rows = assert_one_group(&f, &[("Streaming app", "Rubylight 3.0.0"), ("Address", "192.0.2.7"), ("GPU", "RX 7900 XT")]);
    let heading = topmost(&f, "PC").0;
    assert!(heading.bottom() <= rows[0].0.top(), "the heading over its rows: {heading:?} {rows:?}");
    // The web UI's row is the group's last: it starts where the GPU's ends, with no gap between them.
    let ends = rows[2].0.center().y + design_tokens::SIZE_ROW_HEIGHT / 2.0;
    let web = topmost(&f, "The PC’s web UI").0;
    assert!(web.top() >= ends && web.top() - ends < 12.0, "{web:?} under a row that ends at {ends}");
    let mut inks = Vec::new();
    for clipped in &f.shapes {
        galley_inks(&clipped.shape, "RX 7900 XT", &mut inks);
    }
    assert_eq!(inks, vec![Color32::from(ON_SURFACE_VARIANT)], "a value in the quieter ink");
}

#[test]
fn what_the_remote_check_measured_stands_in_one_group_of_rows() {
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    let id = f.hosts.list[0].id;
    f.l.remote.state = Some((id, crate::host_page::CheckState::Done(crate::host_page::report(relay_check()))));
    f.settle();
    let rows = assert_one_group(&f, &[("Network path", "Relay (DERP)"), ("HTTPS request average", "12.0 ms"), ("Jitter", "2.0 ms"), ("Failed requests", "20 %")]);
    // What the check says stands over its measurements, clear of the focus ring of the check's button.
    let said = f.placed.iter().find(|(t, ..)| t.starts_with("The paired PC responded on this network.")).map(|p| p.1).expect("what the check says");
    let button = f.ctx.memory(|m| m.focused()).and_then(|id| f.ctx.read_response(id)).expect("the check has the focus").rect;
    let ring = button.bottom() + design_tokens::HC_FOCUS_GAP + design_tokens::HC_FOCUS_RING_WIDTH;
    assert!(said.top() - ring >= 12.0, "{said:?} under a ring that ends at {ring}");
    assert!(said.bottom() <= rows[0].0.top(), "{said:?} {rows:?}");
}

/// A value that has no room beside its label (a long address in a narrow window) goes under it,
/// whole, inside the row.
#[test]
fn a_value_too_long_for_its_row_goes_under_its_label() {
    let long = "a-rather-long-name-of-a-gaming-pc.in-some-tailnet-of-ours.example.net";
    let mut f = details(Reach::Online, "MJOLNIR_STATE_FREE", "");
    f.hosts.list[0].record.address = long.into();
    f.size = vec2(400.0, 480.0);
    f.settle();
    let ((label, _), (value, clip)) = (topmost(&f, "Address"), topmost(&f, long));
    assert!(value.top() >= label.bottom() && (value.left() - label.left()).abs() < 0.5, "under its label: {label:?} {value:?}");
    assert!(clip.left() <= value.left() && value.right() <= clip.right().min(f.size.x), "inside the page: {value:?} {clip:?}");
    assert!(value.height() > label.height() * 1.5, "on as many lines as it takes: {value:?}");
}

const LIMITER: &str = "<FrameLimiterSupported>1</FrameLimiterSupported><FrameLimiterEnabled>1</FrameLimiterEnabled>\
    <VirtualDisplayFrameLimiterEnabled>0</VirtualDisplayFrameLimiterEnabled><FrameLimiterFpsLimitMilliHz>119880</FrameLimiterFpsLimitMilliHz>";
const LABELS: [&str; 3] = ["Frame limiter", "Frame limiter on the virtual display", "Frame limit"];

/// Where each row's label and its value stand, with the value's clip, in the language the page
/// was drawn in. The section's heading has the first row's words: the row is the lower of the two.
fn rows(f: &Frames) -> Vec<(Rect, Rect, Rect)> {
    let lowest = |text: &str| f.placed.iter().filter(|(t, ..)| t == text).max_by(|a, b| a.1.top().total_cmp(&b.1.top())).map(|p| (p.1, p.2)).unwrap_or_else(|| panic!("{text} in {:?}", f.drawn));
    let fps = if crate::strings::current().decimal_comma() { "119,88" } else { "119.88" };
    let values = [tr("On").to_owned(), tr("Off").to_owned(), trf("{} fps", &[&fps])];
    LABELS.iter().zip(values).map(|(label, value)| (lowest(tr(label)).0, lowest(&value).0, lowest(&value).1)).collect()
}

#[test]
fn the_limiter_rows_are_one_group_with_each_value_at_its_trailing_edge() {
    let f = details(Reach::Online, "SUNSHINE_SERVER_FREE", LIMITER);
    let heading = f.placed.iter().find(|(t, ..)| t == "Frame limiter").map(|p| p.1).expect("the section's heading");
    let rows = rows(&f);
    assert!(heading.bottom() <= rows[0].0.top(), "the heading over its rows: {heading:?} {rows:?}");
    for (label, value, _) in &rows {
        assert!((label.center().y - value.center().y).abs() < 1.0, "a value beside its label: {label:?} {value:?}");
        assert!((label.left() - rows[0].0.left()).abs() < 0.5 && (value.right() - rows[0].1.right()).abs() < 0.5, "one edge for the labels, one for the values: {rows:?}");
        assert!(label.right() + space::M <= value.left() + 0.5, "{label:?} {value:?}");
    }
    // The rows join: a row's height from one to the next, with no gap between them.
    assert!(rows.windows(2).all(|w| (w[1].0.center().y - w[0].0.center().y - design_tokens::SIZE_ROW_HEIGHT).abs() < 0.5), "{rows:?}");
    // The group is the page's column: its values end near where the row under it ends its button.
    let button = f.placed.iter().find(|(t, ..)| t == "Test this PC").map(|p| p.1).expect("the page's check");
    assert!(rows[0].1.right() > button.right() && rows[0].1.right() - button.right() < 40.0, "{rows:?} {button:?}");
    let mut inks = Vec::new();
    for clipped in &f.shapes {
        galley_inks(&clipped.shape, "Off", &mut inks);
    }
    assert_eq!(inks, vec![Color32::from(ON_SURFACE_VARIANT)], "a value in the quieter ink");
}

#[test]
fn a_pc_that_does_not_report_its_limiter_has_no_such_rows() {
    let f = details(Reach::Online, "SUNSHINE_SERVER_FREE", "");
    for words in LABELS {
        assert!(!shows(&f, words), "{words}: {:?}", f.drawn);
    }
}

/// German at the largest interface size: the page fits, and where the window leaves a long label
/// no room it takes a second line before it reaches its value, which stays whole on its one line.
#[test]
fn a_long_label_wraps_beside_its_value_at_the_largest_interface_size() {
    let mut f = details(Reach::Online, "SUNSHINE_SERVER_FREE", LIMITER);
    fit_150(&mut f);
    assert_fits(&f, "the details page with the limiter rows", false);
    f.size = vec2(400.0, 480.0);
    f.settle();
    let rows = rows(&f);
    let (short, long) = (rows[0], rows[1]);
    assert!(long.0.height() > short.0.height() * 1.5, "the long label wraps: {long:?}");
    assert!((long.1.height() - short.1.height()).abs() < 0.5, "its value is one line: {long:?}");
    assert!(long.0.right() + space::M <= long.1.left() + 0.5, "the label ends before its value: {long:?}");
    assert!((long.0.center().y - long.1.center().y).abs() < 1.0, "the value in the middle of its row: {long:?}");
    for (label, value, clip) in &rows {
        assert!(clip.left() <= label.left() && value.right() <= clip.right().min(f.size.x), "the row keeps inside the page: {label:?} {value:?} {clip:?}");
    }
}
