//! Compact studio Visual Preview Card strip for the `Recommended` Settings landing page.
//! Surfaces live, spring-animated vector diagrams for Smoothness / Frame Pacing, HDR vs SDR,
//! and Upscaling so players immediately see what the recommended profile delivers.

use super::{fill, Ink, Picture};
use crate::config::Settings;
use crate::strings::tr;
use crate::theme::{high_contrast, radius, regular, semibold, space, text, MOON, ON_SURFACE, ON_SURFACE_VARIANT, OUTLINE_VARIANT};
use crate::ui::settings::{frame_pacing_choice, simple_caps, simple_hdr_name, simple_smoothness_name, simple_state, FramePacingChoice};
use crate::ui::{design_tokens, glass, spring, Display};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Id, Rect, Sense, Shape, Stroke, StrokeKind, Ui};

const STRIP_CARD_H: f32 = 132.0;
const DIAGRAM_H: f32 = 68.0;
const CARD_PAD: f32 = 12.0;

struct StripItem<'a> {
    key: &'static str,
    title: &'a str,
    value: String,
    picture: Picture,
}

/// Draws the 3-up (responsive 3 -> 2 -> 1 column) Visual Preview Card strip at the top of `Recommended`.
pub(in crate::ui) fn recommended_strip(
    ui: &mut Ui,
    s: &Settings,
    display: &Display,
    vrr_offered: bool,
    hdr_display: bool,
) {
    let caps = simple_caps(display, hdr_display, vrr_offered);
    let (choices, custom) = simple_state(s, display, &caps, vrr_offered);

    let smoothness_val = if custom {
        tr("Custom").to_owned()
    } else {
        tr(simple_smoothness_name(choices.smoothness)).to_owned()
    };
    let pacing_pic = if s.vrr && vrr_offered {
        FramePacingChoice::HostTimes
    } else {
        frame_pacing_choice(s)
    };

    let hdr_mode = if custom {
        if s.hdr { tr("On") } else { tr("Off") }
    } else {
        tr(simple_hdr_name(choices.hdr))
    };
    let hdr_effective = s.hdr || (!custom && choices.hdr == nightjar_policy::simple_profile::Hdr::Auto && hdr_display);
    let hdr_val = format!("{} · {hdr_mode}", tr("HDR"));

    let items = [
        StripItem {
            key: "smoothness",
            title: tr("Frame pacing"),
            value: smoothness_val,
            picture: Picture::Pacing(pacing_pic),
        },
        StripItem {
            key: "hdr",
            title: tr("Picture"),
            value: hdr_val,
            picture: Picture::Tone { hdr: hdr_effective },
        },
        StripItem {
            key: "upscaling",
            title: tr("Upscaling"),
            value: tr(s.upscaling.label()).to_owned(),
            picture: Picture::Edge {
                upscaling: s.upscaling,
                sharpness: s.upscale_sharpness as f32 / 100.0,
            },
        },
    ];

    let avail = ui.available_width().clamp(180.0, crate::ui::SETTINGS_W);
    let gap = space::M;
    let cols = if avail >= 660.0 {
        3
    } else if avail >= 440.0 {
        2
    } else {
        1
    };
    let rows_count = items.len().div_ceil(cols);
    let total_h = rows_count as f32 * STRIP_CARD_H + (rows_count.saturating_sub(1)) as f32 * gap;
    let (strip_rect, _) = ui.allocate_exact_size(vec2(avail, total_h), Sense::hover());

    for (idx, item) in items.iter().enumerate() {
        let row = idx / cols;
        let col = idx % cols;
        let row_cols = if row + 1 == rows_count {
            items.len() - row * cols
        } else {
            cols
        };
        let card_w = (avail - (row_cols.saturating_sub(1)) as f32 * gap) / row_cols as f32;
        let x = strip_rect.left() + col as f32 * (card_w + gap);
        let y = strip_rect.top() + row as f32 * (STRIP_CARD_H + gap);
        let rect = Rect::from_min_size(pos2(x, y), vec2(card_w, STRIP_CARD_H));
        paint_strip_card(ui, rect, item);
    }
}

fn paint_strip_card(ui: &mut Ui, rect: Rect, item: &StripItem<'_>) {
    let id = Id::unique(("recommended-preview", item.key));
    let item_resp = ui.interact(rect, id.with("a11y"), Sense::hover());
    let a11y_label = format!("{}: {}", item.title, item.value);
    item_resp.widget_info(move || egui::WidgetInfo::labeled(egui::Role::Image, true, &a11y_label));

    let mut pose = item.picture.pose();
    for (at, value) in pose.iter_mut().enumerate().take(item.picture.numbers()) {
        *value = spring::animate(ui.ctx(), id.with(at), *value, spring::FOCUS);
    }
    if !ui.is_rect_visible(rect) {
        return;
    }

    let ink = Ink::now();
    let mut shapes = Vec::new();
    glass::group(&mut shapes, ui.ctx(), rect, f32::from(radius::PANEL), fill());
    ui.painter().extend(shapes);

    let inner = rect.shrink(CARD_PAD);
    let stage = Rect::from_min_size(inner.min, vec2(inner.width(), DIAGRAM_H));
    let stage_round = CornerRadius::same(radius::SMALL);
    if high_contrast() {
        ui.painter().rect_stroke(
            stage,
            stage_round,
            Stroke::new(1.0, *OUTLINE_VARIANT),
            StrokeKind::Inside,
        );
    } else {
        ui.painter().rect_filled(
            stage,
            stage_round,
            design_tokens::TINT_TRACK_FILL.gamma_multiply(0.55),
        );
    }

    let plot = stage.shrink2(vec2(10.0, 8.0));
    let mut drawn: Vec<Shape> = Vec::new();
    item.picture.paint(&pose, plot, &ink, &mut drawn);
    ui.painter()
        .with_clip_rect(stage.intersect(ui.clip_rect()))
        .extend(drawn);

    // Title and current choice badge underneath the diagram.
    let footer_top = stage.bottom() + 8.0;
    let text_w = inner.width().max(40.0);
    let painter = ui.painter();
    let mut title_job = egui::text::LayoutJob::simple_singleline(
        item.title.to_owned(),
        semibold(text::LABEL),
        *ON_SURFACE,
    );
    title_job.wrap = egui::text::TextWrapping::truncate_at_width(text_w);
    let title_g = painter.layout_job(title_job);
    let title_h = title_g.size().y;
    painter.galley(pos2(inner.left(), footer_top), title_g, *ON_SURFACE);

    let val_y = footer_top + title_h + 2.0;
    let dot_c = pos2(inner.left() + 3.5, val_y + 7.0);
    painter.circle_filled(dot_c, 3.0, *MOON);
    let mut val_job = egui::text::LayoutJob::simple_singleline(
        item.value.clone(),
        regular(text::CAPTION),
        *ON_SURFACE_VARIANT,
    );
    val_job.wrap = egui::text::TextWrapping::truncate_at_width((text_w - 12.0).max(24.0));
    let val_g = painter.layout_job(val_job);
    painter.galley(pos2(inner.left() + 11.0, val_y), val_g, *ON_SURFACE_VARIANT);
    let _ = Align2::LEFT_TOP;
    let _ = Color32::TRANSPARENT;
}
