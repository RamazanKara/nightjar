//! Pairing as one centred panel: the PC's mark and name, the PIN in large separate cells, one line
//! saying where to type it, and Cancel. When pairing failed the same panel says so with a mark in
//! the error colour and the app's own message, whole, and Try again as its amber action.

use super::*;
use crate::ui::design_tokens as t;
use crate::ui::pcs::{display_well, well_under};
use egui::Shape;

/// The panel's column.
const WIDTH: f32 = 440.0;
/// The well its mark stands in.
const WELL: f32 = 56.0;
/// A PIN's cell, the gap between two, and its digit.
const CELL: Vec2 = vec2(72.0, 92.0);
const CELL_GAP: f32 = 12.0;
const DIGIT: f32 = 44.0;
/// What a short window makes of the cells, so the words under them keep their room.
const SHORT: f32 = 0.75;
const SPINNER_R: f32 = 9.0;
/// Between the panel's two buttons: room for the focus ring of the one beside the other.
const BUTTON_GAP: f32 = space::L;
/// Each of the panel's buttons: two of them fill its column.
const BUTTON_W: f32 = (WIDTH - BUTTON_GAP) / 2.0;

/// The larger and the smaller gap between the panel's parts: closer in a short window.
fn gaps(ui: &Ui) -> (f32, f32) {
    if short_window(ui.ctx()) { (space::M, space::S) } else { (space::L, space::M) }
}

/// The mark at the panel's top, in its well. A short window does without: the PIN keeps its room.
fn mark(ui: &mut Ui, draw: impl FnOnce(&mut Vec<Shape>, &egui::Context, Rect)) {
    if short_window(ui.ctx()) {
        return;
    }
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), WELL), Sense::hover());
    let mut out = Vec::new();
    draw(&mut out, ui.ctx(), Rect::from_center_size(row.center(), Vec2::splat(WELL)));
    ui.painter().extend(out);
    ui.add_space(space::M);
}

/// An exclamation mark in a ring, in the middle of `well`.
fn alert(out: &mut Vec<Shape>, well: Rect, ink: Color32) {
    const BAR: f32 = 2.0;
    let middle = well.center();
    out.push(Shape::circle_stroke(middle, 14.0, Stroke::new(1.75, ink)));
    let bar = [pos2(middle.x, middle.y - 7.0), pos2(middle.x, middle.y + 1.5)];
    out.push(Shape::line_segment(bar, Stroke::new(BAR, ink)));
    out.extend(bar.map(|end| Shape::circle_filled(end, BAR / 2.0, ink)));
    out.push(Shape::circle_filled(pos2(middle.x, middle.y + 6.5), 1.4, ink));
}

/// The panel's heading, centred; a long PC name wraps.
fn heading(ui: &mut Ui, words: &str) {
    ui.vertical_centered(|ui| ui.label(RichText::new(words).font(semibold(text::SECTION)).color(ON_SURFACE)));
}

/// The PIN, a cell for each digit, centred. A screen reader hears it as one.
fn cells(ui: &mut Ui, pin: &str) {
    let digits: Vec<char> = pin.chars().collect();
    let count = digits.len() as f32;
    let room = ui.available_width();
    let gaps = CELL_GAP * (count - 1.0);
    let wanted = if short_window(ui.ctx()) { CELL.x * SHORT } else { CELL.x };
    // A long PIN gets narrower cells before it leaves the column.
    let width = wanted.min((room - gaps) / count);
    let scale = width / CELL.x;
    let size = vec2(width, CELL.y * scale);
    let (row, response) = ui.allocate_exact_size(vec2(room, size.y), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, pin));
    let left = row.center().x - (width * count + gaps) / 2.0;
    let mut out = Vec::new();
    for (n, digit) in digits.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(left + n as f32 * (width + CELL_GAP), row.top()), size);
        glass::control(&mut out, ui.ctx(), cell, t::RADIUS_CONTROL * scale);
        // Each digit in the middle of its own cell: the PIN reads in tabular figures.
        let galley = ui.painter().layout_no_wrap(digit.to_string(), semibold(DIGIT * scale), *ON_SURFACE);
        out.push(Shape::galley(cell.center() - galley.size() / 2.0, galley, *ON_SURFACE));
    }
    ui.painter().extend(out);
}

/// A spinner and what is waited for, centred as one.
fn waiting_line(ui: &mut Ui, words: &str) {
    let font = regular(text::BODY);
    let words_w = ui.painter().layout_no_wrap(words.to_owned(), font.clone(), *ON_SURFACE_VARIANT).size().x;
    let room = ui.available_width();
    let lead = ((room - 2.0 * SPINNER_R - space::S - words_w) / 2.0).max(0.0);
    ui.allocate_ui_with_layout(vec2(room, 2.0 * SPINNER_R + 4.0), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = space::S;
        ui.add_space(lead);
        spinner(ui, SPINNER_R);
        ui.add(egui::Label::new(RichText::new(words).font(font).color(ON_SURFACE_VARIANT)).truncate());
    });
}

/// The panel while the PC is waited for: its PIN, where to type it, and Cancel, which it returns.
/// A pairing with no PIN to show (a one-time PIN was typed here) waits with its name alone.
pub(super) fn waiting(ui: &mut Ui, name: &str, pin: &str) -> Response {
    ui.set_width(WIDTH);
    let (large, small) = gaps(ui);
    mark(ui, |out, ctx, well| display_well(out, ctx, well, *ON_SURFACE));
    heading(ui, &trf("Pair with {}", &[&name]));
    if !pin.is_empty() {
        ui.add_space(large);
        cells(ui, pin);
        ui.add_space(small);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr("Enter this PIN in the PC’s settings or web UI, usually on its PIN or pairing page.")).color(ON_SURFACE_VARIANT));
        });
    }
    ui.add_space(small);
    waiting_line(ui, tr("Waiting for the PC…"));
    ui.add_space(large);
    ui.vertical_centered(|ui| button_at_least(ui, None, None, tr("Cancel"), None, Tone::Quiet, Size::CONTROL, BUTTON_W)).inner
}

/// What the failed panel's two buttons are.
pub(super) struct Failed {
    pub(super) retry: Response,
    pub(super) close: Response,
}

/// The panel when pairing failed: `why` is the app's own message, shown whole in the error colour
/// under a mark in it, then Try again (amber, the first in the focus order) beside Close.
pub(super) fn failed(ui: &mut Ui, name: &str, why: &str) -> Failed {
    ui.set_width(WIDTH);
    let (large, small) = gaps(ui);
    mark(ui, |out, ctx, well| {
        well_under(out, ctx, well);
        alert(out, well, *ERROR);
    });
    heading(ui, &trf("Pairing with {} failed", &[&name]));
    ui.add_space(small);
    ui.vertical_centered(|ui| ui.label(RichText::new(why).color(ERROR)));
    ui.add_space(large);
    ui.allocate_ui_with_layout(vec2(WIDTH, Size::CONTROL.height), Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = BUTTON_GAP;
        let retry = button_at_least(ui, None, None, tr("Try again"), None, Tone::Primary, Size::CONTROL, BUTTON_W);
        let close = button_at_least(ui, None, None, tr("Close"), None, Tone::Quiet, Size::CONTROL, BUTTON_W);
        Failed { retry, close }
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_alert_is_one_ink_inside_its_well() {
        let well = Rect::from_min_size(pos2(10.0, 10.0), Vec2::splat(WELL));
        let ink = Color32::from_rgb(9, 8, 7);
        let mut out = Vec::new();
        alert(&mut out, well, ink);
        for shape in &out {
            assert!(well.shrink(8.0).contains_rect(shape.visual_bounding_rect()), "{shape:?}");
            let one_ink = match shape {
                Shape::Circle(c) => (c.fill == ink) != (c.stroke.color == ink && c.fill == Color32::TRANSPARENT),
                Shape::LineSegment { stroke, .. } => stroke.color == ink,
                _ => false,
            };
            assert!(one_ink, "{shape:?}");
        }
    }
}
