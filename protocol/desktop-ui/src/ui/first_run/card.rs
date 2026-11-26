//! A PC found on the first run, as a hardware card: a glass panel with the PC's mark in a well,
//! its name, one line of how it stands and where it is, and room for its one action.

use super::*;
use crate::ui::pcs::{display_well, status_words, PcLine};

/// The widest a card gets, and the gap between two in the list: room for a panel's shadow to end.
pub(super) const WIDTH: f32 = 560.0;
pub(super) const GAP: f32 = 16.0;
const PAD: f32 = 20.0;
/// The tinted well the display stands in.
const WELL: f32 = 64.0;
const WELL_GAP: f32 = 16.0;
pub(super) const ACTION_H: f32 = 52.0;
const ACTION_GAP: f32 = 16.0;
/// The least room the words keep beside the action; with less the action goes under them.
const WORDS_LEAST: f32 = 150.0;
const NAME_TO_LINE: f32 = 6.0;
const LINE_H: f32 = 20.0;

/// Where a card's parts go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Parts {
    pub(super) well: Rect,
    pub(super) words: Rect,
    pub(super) action: Rect,
}

/// Whether a card `width` wide puts an action `action_w` wide under its words instead of beside them.
fn stacked(width: f32, action_w: f32) -> bool {
    width - 2.0 * PAD - WELL - WELL_GAP - ACTION_GAP - action_w < WORDS_LEAST
}

/// A card's height at `width` with an action `action_w` wide.
pub(super) fn height(width: f32, action_w: f32) -> f32 {
    if stacked(width, action_w) {
        PAD + WELL + ACTION_GAP + ACTION_H + PAD
    } else {
        PAD + WELL + PAD
    }
}

pub(super) fn parts(card: Rect, action_w: f32) -> Parts {
    let inner = card.shrink(PAD);
    let well = Rect::from_min_size(inner.min, Vec2::splat(WELL));
    let left = well.right() + WELL_GAP;
    if stacked(card.width(), action_w) {
        Parts {
            well,
            words: Rect::from_min_max(pos2(left, well.top()), pos2(inner.right(), well.bottom())),
            action: Rect::from_min_max(pos2(inner.left(), inner.bottom() - ACTION_H), inner.right_bottom()),
        }
    } else {
        let action = Rect::from_min_size(pos2(inner.right() - action_w, card.center().y - ACTION_H / 2.0), vec2(action_w, ACTION_H));
        Parts {
            well,
            words: Rect::from_min_max(pos2(left, well.top()), pos2(action.left() - ACTION_GAP, well.bottom())),
            action,
        }
    }
}

/// The card's own picture at `rect`: the panel, lifted while `lift` says it is pointed at or held,
/// the display in its well, the name and the line of how the PC stands and where it is. True when
/// the name was cut to fit.
pub(super) fn paint(ui: &Ui, out: &mut Vec<Shape>, rect: Rect, parts: &Parts, host: &Host, line: &PcLine, lift: (bool, bool)) -> bool {
    let ctx = ui.ctx();
    glass::surface(out, ctx, rect, t::RADIUS_PANEL, glass::Material::Panel);
    lift_shapes(out, ctx, rect, t::RADIUS_PANEL, lift.0, lift.1);
    let lit = host.reach != Reach::Offline;
    display_well(out, ctx, parts.well, if lit { *ON_SURFACE } else { *ON_SURFACE_VARIANT });
    let room = parts.words.width();
    let mut job = egui::text::LayoutJob::simple_singleline(host.display_name().to_owned(), semibold(t::TYPE_SECTION_SIZE), *ON_SURFACE);
    job.wrap = egui::text::TextWrapping::truncate_at_width(room);
    let name = ui.painter().layout_job(job);
    let cut = name.elided;
    let name_h = name.size().y;
    let top = parts.words.center().y - (name_h + NAME_TO_LINE + LINE_H) / 2.0;
    out.push(Shape::galley(pos2(parts.words.left(), top), name, *ON_SURFACE));
    let words = format!("{} · {}", line.words(), host.connect_address());
    status_words(ui, out, pos2(parts.words.left(), top + name_h + NAME_TO_LINE + LINE_H / 2.0), room, line, &words);
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_card_is_one_row_and_a_narrow_one_puts_its_action_under_the_words() {
        let action_w = 176.0;
        let wide = Rect::from_min_size(pos2(100.0, 200.0), vec2(WIDTH, height(WIDTH, action_w)));
        let p = parts(wide, action_w);
        assert_eq!(wide.height(), PAD + WELL + PAD);
        assert!(p.well.right() < p.words.left() && p.words.right() < p.action.left(), "{p:?}");
        assert!(p.words.width() >= WORDS_LEAST && wide.contains_rect(p.action), "{p:?}");
        assert_eq!(p.action.center().y, wide.center().y, "the action in the row's middle");
        let narrow = Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, height(360.0, action_w)));
        let p = parts(narrow, action_w);
        assert!(narrow.height() > wide.height());
        assert!(p.action.top() >= p.well.bottom() + ACTION_GAP - 0.5 && narrow.contains_rect(p.action), "{p:?}");
        assert_eq!((p.action.left(), p.action.right()), (PAD, 360.0 - PAD), "as wide as the card's inside");
        assert!(p.words.right() <= narrow.right() - PAD, "{p:?}");
    }
}
