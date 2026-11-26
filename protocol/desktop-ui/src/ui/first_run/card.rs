//! A PC found on the first run, as a hardware card: a glass panel with the PC's mark in a well,
//! its name, one line of how it stands and where it is, and room for its one action.

use super::*;
use crate::ui::pcs::{display_well, status_words, PcLine};

/// The widest a card gets, and the gap between two in the list: room for a card's shadow to end.
pub(super) const WIDTH: f32 = 560.0;
pub(super) const GAP: f32 = 16.0;
/// The card tier's shadow, in every look that has one: closer under the card and tighter than a
/// panel's, which is made for one surface alone on the stage and fills the gap between two cards
/// as a grey band by day. It ends inside [`GAP`], and is the panel's shadow colour of the look at
/// this share of its strength.
const SHADOW_Y: f32 = 4.0;
const SHADOW_BLUR: f32 = 12.0;
const SHADOW_STRENGTH: f32 = 0.6;
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

/// The panel's glass at `rect` over the card tier's shadow in place of the panel's own.
fn surface(out: &mut Vec<Shape>, ctx: &egui::Context, rect: Rect) {
    let (r, material) = (t::RADIUS_PANEL, glass::Material::Panel);
    let recipe = glass::recipe(material, glass::tier(ctx));
    let shadow = glass::scaled(recipe.shadow, SHADOW_STRENGTH);
    glass::soft(out, ctx, rect.translate(vec2(0.0, SHADOW_Y)), r, SHADOW_BLUR, shadow);
    // A surface draws its shadow first, as one shape or as none where the look has none: the
    // panel's is counted here and left out of what the surface drew.
    let mut own = Vec::new();
    glass::soft(&mut own, ctx, rect.translate(vec2(0.0, recipe.shadow_y)), r, recipe.shadow_blur, recipe.shadow);
    let mut panel = Vec::new();
    glass::surface(&mut panel, ctx, rect, r, material);
    out.extend(panel.into_iter().skip(own.len()));
}

/// The card's own picture at `rect`: the panel, lifted while `lift` says it is pointed at or held,
/// the display in its well, the name and the line of how the PC stands and where it is. True when
/// the name was cut to fit.
pub(super) fn paint(ui: &Ui, out: &mut Vec<Shape>, rect: Rect, parts: &Parts, host: &Host, line: &PcLine, lift: (bool, bool)) -> bool {
    let ctx = ui.ctx();
    surface(out, ctx, rect);
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

    /// By day and at night a card's shadow ends before the next card starts, and a panel's own
    /// (which reaches several gaps down) is not under it; high contrast has no shadow at all.
    #[test]
    fn a_cards_shadow_ends_inside_the_gap_to_the_next_card() {
        use crate::theme::{own_look, Look};
        let rect = Rect::from_min_size(pos2(100.0, 200.0), vec2(WIDTH, 104.0));
        let reach = |look: Look| {
            own_look(look);
            let ctx = egui::Context::default();
            let mut drawn = Vec::new();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| surface(&mut drawn, ui.ctx(), rect));
            let mut delta = output.textures_delta;
            delta.clear();
            let meshes: Vec<Rect> = drawn.iter().filter_map(|shape| if let Shape::Mesh(mesh) = shape { Some(mesh.calc_bounds()) } else { None }).collect();
            assert!(!meshes.is_empty(), "{look:?}: the panel is drawn");
            meshes.into_iter().fold(rect, |all, mesh| all.union(mesh))
        };
        for look in [Look::Night, Look::Day] {
            let reach = reach(look);
            assert!((reach.bottom() - rect.bottom() - SHADOW_Y - SHADOW_BLUR).abs() < 0.5, "{look:?}: {reach:?}");
            assert!(reach.bottom() <= rect.bottom() + GAP + 0.5, "{look:?}: inside the gap, {reach:?}");
            assert!(rect.top() - reach.top() <= SHADOW_BLUR + 0.5 && rect.left() - reach.left() <= SHADOW_BLUR + 0.5, "{look:?}: {reach:?}");
        }
        let high = reach(Look::HighContrast);
        assert!(rect.expand(1.0).contains_rect(high), "no shadow in high contrast: {high:?}");
        own_look(Look::Night);
    }
}
