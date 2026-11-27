//! Console-grade Visual Controller Studio for Desktop `Controls` and `Controller` settings.
//! Renders a studio-hardware vector controller diagram adapting its silhouette and face-button
//! glyphs (`Xbox`, `PlayStation` DualSense + touchpad, `Nintendo` swapped layout, `Steam Deck`)
//! alongside a live Stick Deadzone & Response Curve graph driven by `Settings::deadzone`.

use crate::config::Settings;
use crate::glyphs::Style;
use crate::pads_view::{PadBrand, PadsView};
use crate::strings::tr;
use crate::theme::{
    high_contrast, look, radius, regular, semibold, shade, space, text, Look, MOON, ON_MOON,
    ON_SURFACE, ON_SURFACE_VARIANT, OUTLINE, OUTLINE_VARIANT, SURFACE, SURFACE_HIGH,
};
use crate::ui::settings_previews::Ink;
use crate::ui::{design_tokens, glass, spring};
use egui::{
    pos2, vec2, Align2, Color32, CornerRadius, Id, Painter, Rect, Sense, Shape, Stroke, StrokeKind,
    Ui,
};

const CARD_PAD: f32 = 14.0;
const PANEL_H: f32 = 186.0;

/// Pre-translated strings supplied by `controller_section.rs` (scanned by `strings/tests`).
#[derive(Clone, Copy)]
pub(in crate::ui) struct Labels<'a> {
    pub title: &'a str,
    pub scope: &'a str,
    pub stick_response: &'a str,
    pub stick_deadzone: &'a str,
    pub trigger_deadzone: &'a str,
    pub motion: &'a str,
    pub touchpad: &'a str,
    pub vibration: &'a str,
}

/// Controller hardware silhouette and face-button layout shown in the studio diagram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum StudioLayout {
    Xbox,
    PlayStation,
    Nintendo,
    SteamDeck,
}

impl StudioLayout {
    pub const ALL: [Self; 4] = [Self::Xbox, Self::PlayStation, Self::Nintendo, Self::SteamDeck];

    pub fn label(self) -> &'static str {
        match self {
            Self::Xbox => tr("Xbox"),
            Self::PlayStation => tr("PlayStation"),
            Self::Nintendo => tr("Nintendo"),
            Self::SteamDeck => "Steam Deck",
        }
    }

    /// `(south, east, west, north)` button labels for this layout and `swap_face_buttons`.
    pub fn face_labels(self, swapped: bool) -> [&'static str; 4] {
        match (self, swapped) {
            (Self::PlayStation, false) => ["×", "○", "□", "△"],
            (Self::PlayStation, true) => ["○", "×", "△", "□"],
            (Self::Nintendo, _) | (_, true) => ["B", "A", "Y", "X"],
            (Self::Xbox | Self::SteamDeck, false) => ["A", "B", "X", "Y"],
        }
    }

    pub fn detect(pads: &PadsView, style: Style, swapped: bool) -> Self {
        if pads.pads.iter().any(|p| {
            let lower = p.name.to_ascii_lowercase();
            lower.contains("steam") || lower.contains("deck")
        }) {
            return Self::SteamDeck;
        }
        if swapped {
            return Self::Nintendo;
        }
        match pads.pads.first().map(|p| p.brand) {
            Some(PadBrand::PlayStation) => Self::PlayStation,
            Some(PadBrand::Nintendo) => Self::Nintendo,
            Some(PadBrand::Xbox) => Self::Xbox,
            None => match style {
                Style::PlayStation => Self::PlayStation,
                Style::Nintendo => Self::Nintendo,
                Style::Xbox => Self::Xbox,
            },
        }
    }
}

/// Normalized stick response `(0.0..=1.0)` for input deflection `x` (`0.0..=1.0`) and `deadzone_pct`.
pub(in crate::ui) fn stick_response(x: f32, deadzone_pct: u32) -> f32 {
    let dz = (deadzone_pct as f32 / 100.0).clamp(0.0, 0.95);
    let x = x.clamp(0.0, 1.0);
    if x <= dz { 0.0 } else { ((x - dz) / (1.0 - dz)).clamp(0.0, 1.0) }
}

/// Renders the Visual Controller Studio card at the top of `Controls` / `Controller`.
pub(in crate::ui) fn show(
    ui: &mut Ui,
    s: &Settings,
    style: Style,
    pads: &PadsView,
    layered: bool,
    labels: &Labels<'_>,
) {
    let pick_id = Id::unique("controller-studio-layout");
    let active_layout = ui
        .data(|d| d.get_temp(pick_id))
        .unwrap_or_else(|| StudioLayout::detect(pads, style, s.swap_face_buttons));

    let avail = ui.available_width().clamp(200.0, crate::ui::SETTINGS_W);
    let side_by_side = avail >= 560.0;
    let header_stacked = avail < 520.0;
    let header_h = if header_stacked { 52.0 } else { 28.0 };
    let body_h = if side_by_side { PANEL_H } else { PANEL_H * 2.0 + space::M };
    let card_h = CARD_PAD * 2.0 + header_h + space::S + body_h;
    let (card_rect, _) = ui.allocate_exact_size(vec2(avail, card_h), Sense::hover());

    if ui.is_rect_visible(card_rect) {
        let mut bg = Vec::new();
        glass::group(&mut bg, ui.ctx(), card_rect, f32::from(radius::PANEL), shade::TINT_GROUP_FILL.get());
        ui.painter().extend(bg);
    }

    let inner = card_rect.shrink(CARD_PAD);
    paint_header(ui, inner, header_stacked, layered, active_layout, pick_id, labels);

    let body_top = inner.top() + header_h + space::S;
    let (pad_rect, curve_rect) = if side_by_side {
        let gap = space::M;
        let pad_w = ((inner.width() - gap) * 0.58).round();
        let curve_w = (inner.width() - gap - pad_w).max(120.0);
        (
            Rect::from_min_size(pos2(inner.left(), body_top), vec2(pad_w, PANEL_H)),
            Rect::from_min_size(pos2(inner.left() + pad_w + gap, body_top), vec2(curve_w, PANEL_H)),
        )
    } else {
        (
            Rect::from_min_size(pos2(inner.left(), body_top), vec2(inner.width(), PANEL_H)),
            Rect::from_min_size(pos2(inner.left(), body_top + PANEL_H + space::M), vec2(inner.width(), PANEL_H)),
        )
    };

    if ui.is_rect_visible(card_rect) {
        paint_controller_panel(ui, pad_rect, s, active_layout, labels);
        paint_curve_panel(ui, curve_rect, s, labels);
    }
}

fn paint_header(
    ui: &mut Ui,
    inner: Rect,
    stacked: bool,
    layered: bool,
    active: StudioLayout,
    pick_id: Id,
    labels: &Labels<'_>,
) {
    let painter = ui.painter();
    let title = painter.layout_no_wrap(labels.title.to_owned(), semibold(text::LABEL), *ON_SURFACE);
    let title_pos = pos2(inner.left(), inner.top() + 4.0);
    painter.galley(title_pos, title.clone(), *ON_SURFACE);

    if !layered {
        let scope = painter.layout_no_wrap(labels.scope.to_owned(), regular(text::CAPTION), *ON_SURFACE_VARIANT);
        let sx = title_pos.x + title.size().x + 10.0;
        if sx + scope.size().x <= inner.right() {
            painter.circle_filled(pos2(sx + 3.0, title_pos.y + title.size().y / 2.0), 2.5, *MOON);
            painter.galley(pos2(sx + 9.0, title_pos.y + (title.size().y - scope.size().y) / 2.0), scope, *ON_SURFACE_VARIANT);
        }
    }

    let pill_y = if stacked { inner.top() + 26.0 } else { inner.top() + 2.0 };
    let mut x = if stacked { inner.left() } else { inner.right() };
    let order = if stacked {
        StudioLayout::ALL
    } else {
        [StudioLayout::SteamDeck, StudioLayout::Nintendo, StudioLayout::PlayStation, StudioLayout::Xbox]
    };

    for layout in order {
        let words = layout.label();
        let g = ui.painter().layout_no_wrap(words.to_owned(), semibold(text::CAPTION), *ON_SURFACE);
        let w = g.size().x + 16.0;
        let rect = if stacked {
            let r = Rect::from_min_size(pos2(x, pill_y), vec2(w, 24.0));
            x += w + 6.0;
            r
        } else {
            x -= w;
            let r = Rect::from_min_size(pos2(x, pill_y), vec2(w, 24.0));
            x -= 6.0;
            r
        };
        if rect.left() < inner.left() - 1.0 || rect.right() > inner.right() + 1.0 {
            continue;
        }
        let selected = layout == active;
        let resp = ui.interact(rect, pick_id.with(words), Sense::click());
        resp.widget_info(move || egui::WidgetInfo::selected(egui::Role::Button, true, selected, words));
        if resp.clicked() {
            ui.data_mut(|d| d.insert_temp(pick_id, layout));
        }
        let round = CornerRadius::same(12);
        let (fill, stroke, ink) = if selected {
            let f = if high_contrast() { Color32::TRANSPARENT } else { MOON.gamma_multiply(0.18) };
            (f, Stroke::new(1.3, *MOON), *MOON)
        } else {
            let f = if resp.hovered() { *SURFACE_HIGH } else { *SURFACE };
            (f, Stroke::new(1.0, *OUTLINE_VARIANT), *ON_SURFACE_VARIANT)
        };
        ui.painter().rect(rect, round, fill, stroke, StrokeKind::Inside);
        ui.painter().galley(pos2(rect.center().x - g.size().x / 2.0, rect.center().y - g.size().y / 2.0), g, ink);
    }
}

fn panel_box(painter: &Painter, rect: Rect) {
    let fill = match look() {
        Look::Day => *SURFACE,
        Look::Night => Color32::from_rgb(0x11, 0x13, 0x19),
        Look::Oled | Look::HighContrast => Color32::BLACK,
    };
    painter.rect(rect, CornerRadius::same(radius::CONTROL), fill, Stroke::new(1.0, *OUTLINE_VARIANT), StrokeKind::Inside);
}

fn paint_controller_panel(ui: &mut Ui, rect: Rect, s: &Settings, layout: StudioLayout, labels: &Labels<'_>) {
    let painter = ui.painter();
    panel_box(painter, rect);

    let stage = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - 26.0));
    let c = stage.center() + vec2(0.0, 4.0);
    let u = (stage.width() / 248.0).min(stage.height() / 136.0).clamp(0.55, 1.25);
    let p = |dx: f32, dy: f32| c + vec2(dx * u, dy * u);

    let body_fill = match look() {
        Look::Day => *SURFACE_HIGH,
        Look::Night => Color32::from_rgb(0x18, 0x1B, 0x24),
        Look::Oled => Color32::from_rgb(0x10, 0x12, 0x18),
        Look::HighContrast => Color32::BLACK,
    };
    let rim = Stroke::new(1.5, *OUTLINE);
    let quiet_rim = Stroke::new(1.1, *OUTLINE_VARIANT);
    let active_trig = 1.0 - (s.trigger_deadzone as f32 / 100.0).clamp(0.0, 0.9);

    for side in [-1.0_f32, 1.0] {
        let trig = Rect::from_center_size(p(side * 60.0, -49.0), vec2(32.0 * u, 11.0 * u));
        painter.rect(trig, CornerRadius::same(4), body_fill, quiet_rim, StrokeKind::Inside);
        let fill_w = trig.width() * active_trig;
        let bar = if side < 0.0 {
            Rect::from_min_size(pos2(trig.right() - fill_w, trig.top()), vec2(fill_w, trig.height()))
        } else {
            Rect::from_min_size(trig.min, vec2(fill_w, trig.height()))
        };
        if !high_contrast() {
            painter.rect_filled(bar.shrink(2.0), CornerRadius::same(2), MOON.gamma_multiply(0.55));
        }
        let bump = Rect::from_center_size(p(side * 58.0, -37.0), vec2(38.0 * u, 8.0 * u));
        painter.rect(bump, CornerRadius::same(3), body_fill, rim, StrokeKind::Inside);
    }

    if layout == StudioLayout::SteamDeck {
        let deck = Rect::from_center_size(p(0.0, 2.0), vec2(204.0 * u, 84.0 * u));
        painter.rect(deck, CornerRadius::same((18.0 * u) as u8), body_fill, rim, StrokeKind::Inside);
        let screen = Rect::from_center_size(p(0.0, 2.0), vec2(90.0 * u, 56.0 * u));
        painter.rect(screen, CornerRadius::same(4), design_tokens::TINT_TRACK_FILL, Stroke::new(1.2, *MOON), StrokeKind::Inside);
        for side in [-1.0_f32, 1.0] {
            let pad = Rect::from_center_size(p(side * 64.0, 18.0), vec2(20.0 * u, 20.0 * u));
            painter.rect(pad, CornerRadius::same(3), *SURFACE, Stroke::new(1.1, *MOON), StrokeKind::Inside);
        }
    } else {
        painter.add(Shape::convex_polygon(vec![p(-68.0, -32.0), p(68.0, -32.0), p(76.0, 18.0), p(-76.0, 18.0)], body_fill, Stroke::NONE));
        for side in [-1.0_f32, 1.0] {
            painter.add(Shape::convex_polygon(
                vec![p(side * 40.0, -20.0), p(side * 86.0, -14.0), p(side * 95.0, 32.0), p(side * 80.0, 47.0), p(side * 60.0, 43.0), p(side * 40.0, 18.0)],
                body_fill,
                Stroke::NONE,
            ));
        }
        painter.add(Shape::closed_line(
            vec![p(-66.0, -32.0), p(66.0, -32.0), p(86.0, -14.0), p(96.0, 32.0), p(82.0, 48.0), p(62.0, 44.0), p(42.0, 18.0), p(-42.0, 18.0), p(-62.0, 44.0), p(-82.0, 48.0), p(-96.0, 32.0), p(-86.0, -14.0)],
            rim,
        ));
    }

    if layout == StudioLayout::PlayStation {
        let tp = Rect::from_center_size(p(0.0, -15.0), vec2(54.0 * u, 24.0 * u));
        let tp_stroke = if s.gamepad_touchpad_as_mouse { Stroke::new(1.6, *MOON) } else { quiet_rim };
        let tp_fill = if s.gamepad_touchpad_as_mouse && !high_contrast() { MOON.gamma_multiply(0.16) } else { *SURFACE };
        painter.rect(tp, CornerRadius::same(4), tp_fill, tp_stroke, StrokeKind::Inside);
        painter.circle_stroke(p(0.0, 6.0), 4.5 * u, Stroke::new(1.3, *MOON));
    } else if layout != StudioLayout::SteamDeck {
        painter.circle_stroke(p(0.0, -14.0), 6.0 * u, Stroke::new(1.5, *MOON));
        painter.circle_filled(p(-18.0, -12.0), 2.3 * u, *OUTLINE);
        painter.circle_filled(p(18.0, -12.0), 2.3 * u, *OUTLINE);
    }

    let (l_stick, dpad, r_stick) = match layout {
        StudioLayout::PlayStation => (p(-28.0, 10.0), p(-56.0, -10.0), p(28.0, 10.0)),
        StudioLayout::SteamDeck => (p(-64.0, -14.0), p(-86.0, -4.0), p(64.0, -14.0)),
        StudioLayout::Xbox | StudioLayout::Nintendo => (p(-54.0, -10.0), p(-30.0, 8.0), p(30.0, 8.0)),
    };

    let stick_r = 12.5 * u;
    let dz_frac = (s.deadzone as f32 / 50.0).clamp(0.14, 0.85);
    for center in [l_stick, r_stick] {
        painter.circle(center, stick_r, *SURFACE, quiet_rim);
        if !high_contrast() {
            painter.circle_filled(center, stick_r * dz_frac, MOON.gamma_multiply(0.22));
        }
        painter.circle_stroke(center, stick_r * dz_frac, Stroke::new(1.5, *MOON));
    }

    let (arm, thick) = (7.5 * u, 4.6 * u);
    painter.rect_filled(Rect::from_center_size(dpad, vec2(arm * 2.0, thick)), CornerRadius::same(2), *OUTLINE);
    painter.rect_filled(Rect::from_center_size(dpad, vec2(thick, arm * 2.0)), CornerRadius::same(2), *OUTLINE);

    let cluster = if layout == StudioLayout::SteamDeck { p(84.0, -6.0) } else { p(56.0, -10.0) };
    let step = if layout == StudioLayout::SteamDeck { 8.5 * u } else { 11.5 * u };
    let btn_r = if layout == StudioLayout::SteamDeck { 4.8 * u } else { 6.2 * u };
    let glyphs = layout.face_labels(s.swap_face_buttons);
    let offsets = [vec2(0.0, step), vec2(step, 0.0), vec2(-step, 0.0), vec2(0.0, -step)];
    let highlight_all = s.swap_face_buttons || layout == StudioLayout::Nintendo;
    for (idx, (off, glyph)) in offsets.into_iter().zip(glyphs).enumerate() {
        let bc = cluster + off;
        let accent = idx == 0 || highlight_all;
        let fill = if idx == 0 && !high_contrast() { *MOON } else { *SURFACE };
        let stroke = if accent { Stroke::new(1.4, *MOON) } else { Stroke::new(1.1, *OUTLINE) };
        painter.circle(bc, btn_r, fill, stroke);
        let ink = if idx == 0 && !high_contrast() { *ON_MOON } else if accent { *MOON } else { *ON_SURFACE };
        painter.text(bc, Align2::CENTER_CENTER, glyph, semibold((9.5 * u).clamp(8.0, 11.5)), ink);
    }

    let pills = [
        (labels.motion, s.controller_motion),
        (labels.touchpad, s.gamepad_touchpad_as_mouse || layout == StudioLayout::PlayStation),
        (labels.vibration, !s.disable_rumble),
    ];
    let mut x = rect.left() + 10.0;
    let foot_y = rect.bottom() - 21.0;
    for (word, active) in pills {
        let g = painter.layout_no_wrap(word.to_owned(), regular(text::CAPTION), *ON_SURFACE);
        let w = g.size().x + 16.0;
        if x + w > rect.right() - 8.0 {
            break;
        }
        let pill = Rect::from_min_size(pos2(x, foot_y), vec2(w, 16.0));
        painter.circle_filled(pos2(pill.left() + 6.0, pill.center().y), 2.6, if active { *MOON } else { *OUTLINE_VARIANT });
        let ink = if active { *ON_SURFACE } else { *ON_SURFACE_VARIANT };
        painter.galley(pos2(pill.left() + 12.0, pill.center().y - g.size().y / 2.0), g, ink);
        x += w + 8.0;
    }
}

fn paint_curve_panel(ui: &mut Ui, rect: Rect, s: &Settings, labels: &Labels<'_>) {
    let painter = ui.painter();
    panel_box(painter, rect);

    let inner = rect.shrink(10.0);
    let heading = painter.layout_no_wrap(labels.stick_response.to_owned(), semibold(text::CAPTION), *ON_SURFACE);
    let head_h = heading.size().y;
    painter.galley(inner.min, heading, *ON_SURFACE);

    let dz_anim = spring::animate(
        ui.ctx(),
        Id::unique("studio-dz"),
        (s.deadzone as f32 / 100.0).clamp(0.0, 0.95),
        spring::FOCUS,
    )
    .clamp(0.0, 0.95);

    let foot_str = format!("{}: {}%  ·  {}: {}%", labels.stick_deadzone, s.deadzone, labels.trigger_deadzone, s.trigger_deadzone);
    let mut foot_job = egui::text::LayoutJob::simple_singleline(foot_str, regular(text::CAPTION), *ON_SURFACE_VARIANT);
    foot_job.wrap = egui::text::TextWrapping::truncate_at_width(inner.width().max(40.0));
    let foot = painter.layout_job(foot_job);
    let foot_h = foot.size().y;
    painter.galley(pos2(inner.left(), inner.bottom() - foot_h), foot, *ON_SURFACE_VARIANT);

    let plot = Rect::from_min_max(pos2(inner.left() + 2.0, inner.top() + head_h + 6.0), pos2(inner.right() - 2.0, inner.bottom() - foot_h - 6.0));
    if plot.width() < 24.0 || plot.height() < 24.0 {
        return;
    }

    let ink = Ink::now();
    for step in 0..=4 {
        let t = step as f32 / 4.0;
        let gx = egui::lerp(plot.left()..=plot.right(), t);
        let gy = egui::lerp(plot.top()..=plot.bottom(), t);
        painter.line_segment([pos2(gx, plot.top()), pos2(gx, plot.bottom())], Stroke::new(1.0, ink.quiet));
        painter.line_segment([pos2(plot.left(), gy), pos2(plot.right(), gy)], Stroke::new(1.0, ink.quiet));
    }

    painter.extend(Shape::dashed_line(&[plot.left_bottom(), plot.right_top()], Stroke::new(1.1, ink.reference.gamma_multiply(0.55)), 4.0, 3.0));

    let knee_x = egui::lerp(plot.left()..=plot.right(), dz_anim);
    if dz_anim > 0.005 && !ink.high {
        painter.rect_filled(Rect::from_min_max(pos2(plot.left(), plot.top()), pos2(knee_x, plot.bottom())), CornerRadius::ZERO, MOON.gamma_multiply(0.10));
    }
    if dz_anim > 0.005 {
        painter.extend(Shape::dashed_line(&[pos2(knee_x, plot.top()), pos2(knee_x, plot.bottom())], Stroke::new(1.1, *MOON), 3.0, 3.0));
    }
    if !ink.high {
        painter.add(Shape::convex_polygon(vec![pos2(knee_x, plot.bottom()), plot.right_top(), plot.right_bottom()], ink.wash, Stroke::NONE));
    }
    let dz_pct = (dz_anim * 100.0).round() as u32;
    let pts: Vec<_> = [0.0_f32, dz_anim, 0.5_f32.max(dz_anim), 1.0]
        .into_iter()
        .map(|x| pos2(egui::lerp(plot.left()..=plot.right(), x), egui::lerp(plot.bottom()..=plot.top(), stick_response(x, dz_pct))))
        .collect();
    painter.add(Shape::line(pts, Stroke::new(2.4, ink.get)));
    painter.circle_filled(pos2(knee_x, plot.bottom()), 3.2, ink.get);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layouts_adapt_face_labels_and_nintendo_swap() {
        assert_eq!(StudioLayout::Xbox.face_labels(false), ["A", "B", "X", "Y"]);
        assert_eq!(StudioLayout::Xbox.face_labels(true), ["B", "A", "Y", "X"]);
        assert_eq!(StudioLayout::PlayStation.face_labels(false), ["×", "○", "□", "△"]);
        assert_eq!(StudioLayout::Nintendo.face_labels(false), ["B", "A", "Y", "X"]);
        assert_eq!(StudioLayout::SteamDeck.face_labels(false), ["A", "B", "X", "Y"]);
    }

    #[test]
    fn stick_response_holds_zero_in_deadzone_and_ramps_to_one() {
        assert_eq!(stick_response(0.0, 15), 0.0);
        assert_eq!(stick_response(0.15, 15), 0.0);
        assert!((stick_response(0.575, 15) - 0.5).abs() < 1e-4);
        assert!((stick_response(1.0, 15) - 1.0).abs() < 1e-4);
    }
}
