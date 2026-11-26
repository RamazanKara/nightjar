//! A PC as a mark: a small display on its stand, drawn in line in one ink, in a tinted well. The
//! card of a PC found on the first run shows it, and the panel that pairs with it.

use egui::{pos2, vec2, Color32, Context, CornerRadius, Rect, Shape, Stroke, StrokeKind};

use crate::ui::glass;

const STROKE: f32 = 1.75;
/// The well's corner as a share of its side: the control radius at the card's size.
const CORNER: f32 = 0.25;

/// A display on its stand in the middle of `well`, in one ink.
fn display(out: &mut Vec<Shape>, well: Rect, ink: Color32) {
    let stroke = Stroke::new(STROKE, ink);
    let middle = well.center();
    let screen = Rect::from_center_size(pos2(middle.x, middle.y - 4.0), vec2(34.0, 23.0));
    out.push(Shape::rect_stroke(screen, CornerRadius::same(4), stroke, StrokeKind::Middle));
    let foot = screen.bottom() + 7.0;
    out.push(Shape::line_segment([pos2(middle.x, screen.bottom()), pos2(middle.x, foot)], stroke));
    let ends = [pos2(middle.x - 8.0, foot), pos2(middle.x + 8.0, foot)];
    out.push(Shape::line_segment(ends, stroke));
    // Round ends, as the screen's corners are.
    out.extend(ends.map(|end| Shape::circle_filled(end, STROKE / 2.0, ink)));
}

/// The tinted well at `well` alone, for another mark of its size (pairing's, when it failed).
pub(in crate::ui) fn well_under(out: &mut Vec<Shape>, ctx: &Context, well: Rect) {
    glass::control(out, ctx, well, well.width() * CORNER);
}

/// The tinted well at `well` (a square of 52 points or more) with the display in it.
pub(in crate::ui) fn display_well(out: &mut Vec<Shape>, ctx: &Context, well: Rect, ink: Color32) {
    well_under(out, ctx, well);
    display(out, well, ink);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_display_is_drawn_in_one_ink_inside_its_well() {
        for side in [52.0, 64.0] {
            let well = Rect::from_min_size(pos2(20.0, 20.0), vec2(side, side));
            let ink = Color32::from_rgb(1, 2, 3);
            let mut out = Vec::new();
            display(&mut out, well, ink);
            assert!(out.len() >= 3, "a screen, a neck and a foot");
            for shape in &out {
                assert!(well.shrink(6.0).contains_rect(shape.visual_bounding_rect()), "{side}: {shape:?}");
                let one_ink = match shape {
                    Shape::Rect(r) => r.fill == Color32::TRANSPARENT && r.stroke.color == ink,
                    Shape::LineSegment { stroke, .. } => stroke.color == ink,
                    Shape::Circle(c) => c.fill == ink,
                    _ => false,
                };
                assert!(one_ink, "{shape:?}");
            }
        }
    }
}
