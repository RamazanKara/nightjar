//! Preview cards for the settings whose words nobody can picture (`client/design/README.md`,
//! Settings): Frame pacing as frames against the screen's refreshes, HDR as a highlight ramp,
//! Upscaling as an edge on the stream's pixels, Video size as the picture inside a screen. A card
//! shows the value chosen: its picture, its name, and what the picture's marks mean. Amber is what
//! the player gets and neutral is the reference. A picture is a few shapes made each frame, with
//! no image and no texture; it moves to a new value on a short spring (a cut with reduced motion)
//! and is still at rest. High contrast draws lines only, in the label colours. The pictures
//! illustrate: none of them claims a measured number. The words come translated from the caller.

use super::settings::FramePacingChoice;
use super::*;
use crate::config::{Upscaling, VideoScaling};
use egui::Shape;

mod edge;
mod pacing;
mod screen;
mod tone;
#[cfg(test)]
mod tests;

/// The numbers a picture is drawn from; a change of value springs each to its new place.
pub(in crate::ui) type Pose = [f32; POSE];
pub(in crate::ui) const POSE: usize = 16;

/// The picture's widest, and the least the words beside it keep.
const PICTURE_W: f32 = 288.0;
const WORDS_MIN: f32 = 150.0;
const PAD: f32 = 12.0;

/// What a card draws, for the value chosen.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(in crate::ui) enum Picture {
    /// Frames arriving and frames shown, against the screen's refreshes.
    Pacing(FramePacingChoice),
    /// A highlight ramp: cut off without HDR, whole with it.
    Tone { hdr: bool },
    /// An edge on the stream's pixels, as soft as the upscaler leaves it; `sharpness` is 0 to 1.
    Edge { upscaling: Upscaling, sharpness: f32 },
    /// The picture inside a screen of another shape; `top` sits it at the screen's top.
    Screen { scaling: VideoScaling, top: bool },
}

impl Picture {
    /// Where the picture's numbers rest for this value.
    pub(in crate::ui) fn pose(self) -> Pose {
        match self {
            Picture::Pacing(choice) => pacing::pose(choice),
            Picture::Tone { hdr } => tone::pose(hdr),
            Picture::Edge { upscaling, sharpness } => edge::pose(upscaling, sharpness),
            Picture::Screen { scaling, top } => screen::pose(scaling, top),
        }
    }

    /// How many of the pose's numbers the picture reads: only those have springs.
    fn numbers(self) -> usize {
        match self {
            Picture::Pacing(_) => 2 * pacing::FRAMES,
            Picture::Tone { .. } => 1,
            Picture::Edge { .. } => 2,
            Picture::Screen { .. } => 5,
        }
    }

    /// The picture at `pose`, inside `rect`.
    pub(in crate::ui) fn paint(self, pose: &Pose, rect: Rect, ink: &Ink, out: &mut Vec<Shape>) {
        match self {
            Picture::Pacing(_) => pacing::paint(pose, rect, ink, out),
            Picture::Tone { .. } => tone::paint(pose, rect, ink, out),
            Picture::Edge { .. } => edge::paint(pose, rect, ink, out),
            Picture::Screen { .. } => screen::paint(pose, rect, ink, out),
        }
    }
}

/// The colours a picture is drawn in, for the look in use.
#[derive(Clone, Copy, Debug)]
pub(in crate::ui) struct Ink {
    /// What the player gets: amber.
    pub get: Color32,
    /// Amber inside a shape or under a line. Clear in high contrast, which fills nothing.
    pub wash: Color32,
    /// The reference: neutral.
    pub reference: Color32,
    /// The reference's quiet lines: a grid, the refreshes.
    pub quiet: Color32,
    /// High contrast: lines only, in the label colours.
    pub high: bool,
}

impl Ink {
    pub(in crate::ui) fn now() -> Self {
        let high = high_contrast();
        Ink {
            get: *MOON,
            wash: if high { Color32::TRANSPARENT } else { MOON.gamma_multiply(0.2) },
            reference: *OUTLINE,
            quiet: if high { *OUTLINE_VARIANT } else { OUTLINE.gamma_multiply(0.34) },
            high,
        }
    }

    /// A shape of what the player gets: filled, or outlined where nothing is filled.
    fn block(&self, rect: Rect, round: u8, out: &mut Vec<Shape>) {
        if self.high {
            out.push(Shape::rect_stroke(rect, CornerRadius::same(round), Stroke::new(1.5, self.get), StrokeKind::Inside));
        } else {
            out.push(Shape::rect_filled(rect, CornerRadius::same(round), self.get));
        }
    }
}

/// A mark of a picture, as the card's legend shows it beside its meaning.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::ui) enum Mark {
    /// Amber, filled: a frame shown, the picture as enlarged.
    Bar,
    /// Amber, a line: the curve.
    Line,
    /// Amber, a frame: the video.
    Box,
    /// Neutral: a frame arriving.
    Dot,
    /// Neutral: a refresh.
    Tick,
    /// Neutral, dashed: the whole ramp.
    Dashed,
    /// Neutral: the stream's pixels.
    Grid,
    /// Neutral, a frame: the screen.
    Outline,
}

impl Mark {
    /// The mark at legend size, in `rect`.
    fn paint(self, rect: Rect, ink: &Ink, out: &mut Vec<Shape>) {
        let c = rect.center();
        match self {
            Mark::Bar => ink.block(Rect::from_center_size(c, vec2(rect.width(), 8.0)), 3, out),
            Mark::Line => out.push(Shape::line_segment([pos2(rect.left(), c.y), pos2(rect.right(), c.y)], Stroke::new(2.5, ink.get))),
            Mark::Box => {
                let frame = Rect::from_center_size(c, vec2(rect.width(), 10.0));
                out.push(Shape::rect_filled(frame, CornerRadius::same(2), ink.wash));
                out.push(Shape::rect_stroke(frame, CornerRadius::same(2), Stroke::new(1.5, ink.get), StrokeKind::Inside));
            }
            Mark::Dot => out.push(Shape::circle_filled(c, 3.5, ink.reference)),
            Mark::Tick => out.push(Shape::line_segment([pos2(c.x, rect.top()), pos2(c.x, rect.bottom())], Stroke::new(1.0, ink.reference))),
            Mark::Dashed => out.extend(Shape::dashed_line(&[pos2(rect.left(), c.y), pos2(rect.right(), c.y)], Stroke::new(1.5, ink.reference), 4.0, 3.0)),
            Mark::Grid => {
                let cell = Rect::from_center_size(c, Vec2::splat(12.0));
                out.push(Shape::rect_stroke(cell, CornerRadius::ZERO, Stroke::new(1.0, ink.reference), StrokeKind::Inside));
                out.push(Shape::line_segment([pos2(c.x, cell.top()), pos2(c.x, cell.bottom())], Stroke::new(1.0, ink.reference)));
                out.push(Shape::line_segment([pos2(cell.left(), c.y), pos2(cell.right(), c.y)], Stroke::new(1.0, ink.reference)));
            }
            Mark::Outline => {
                let frame = Rect::from_center_size(c, vec2(rect.width(), 10.0));
                out.push(Shape::rect_stroke(frame, CornerRadius::same(2), Stroke::new(1.5, ink.reference), StrokeKind::Inside));
            }
        }
    }
}

/// A row's preview card. The words are in the player's language already.
#[derive(Clone, Copy)]
pub(in crate::ui) struct Card<'a> {
    /// Names the card's springs: one per row.
    pub id: Id,
    /// The setting's name, for a screen reader.
    pub title: &'a str,
    /// The chosen value's name.
    pub caption: &'a str,
    /// What the picture's marks mean, what the player gets first.
    pub legend: &'a [(Mark, &'a str)],
    pub picture: Picture,
}

/// The card's own tint on the row's: one step off its surface, with the group's rim.
fn fill() -> Color32 {
    shade::TINT_GROUP_FILL.get()
}

/// The card, as wide as the row and `size.previewCard` tall: the picture on the left, the value's
/// name and the legend beside it. It takes no focus: the row's control above it sets the value.
pub(in crate::ui) fn card(ui: &mut Ui, card: &Card<'_>) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), design_tokens::SIZE_PREVIEW_CARD), Sense::hover());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, format!("{}: {}", card.title, card.caption)));
    // The springs step whether or not the card is on screen: one scrolled into view is at its value.
    let mut pose = card.picture.pose();
    for (at, value) in pose.iter_mut().enumerate().take(card.picture.numbers()) {
        *value = spring::animate(ui.ctx(), card.id.with(at), *value, spring::FOCUS);
    }
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let ink = Ink::now();
    let mut shapes = Vec::new();
    glass::group(&mut shapes, ui.ctx(), rect, f32::from(radius::CONTROL), fill());
    ui.painter().extend(shapes);
    let inner = rect.shrink(PAD);
    let width = (inner.width() - space::L - WORDS_MIN).clamp(96.0, PICTURE_W);
    let picture = Rect::from_min_size(inner.min, vec2(width, inner.height()));
    let mut drawn = Vec::new();
    card.picture.paint(&pose, picture, &ink, &mut drawn);
    ui.painter().with_clip_rect(picture.expand(1.0).intersect(ui.clip_rect())).extend(drawn);
    words(ui, card, Rect::from_min_max(pos2(picture.right() + space::L, inner.top()), inner.max), &ink);
    response
}

/// The value's name and the legend, centred beside the picture.
fn words(ui: &Ui, card: &Card<'_>, rect: Rect, ink: &Ink) {
    const SWATCH: Vec2 = vec2(18.0, 14.0);
    const LINE_GAP: f32 = 5.0;
    let painter = ui.painter();
    let name = painter.layout(card.caption.to_owned(), semibold(text::LABEL), *ON_SURFACE, rect.width());
    let room = (rect.width() - SWATCH.x - space::S).max(40.0);
    let lines: Vec<_> = card.legend.iter()
        .map(|(mark, meaning)| (*mark, painter.layout((*meaning).to_owned(), regular(text::CAPTION), *ON_SURFACE_VARIANT, room)))
        .collect();
    let tall = name.size().y + space::S + lines.iter().map(|(_, g)| g.size().y.max(SWATCH.y) + LINE_GAP).sum::<f32>() - LINE_GAP;
    let mut y = (rect.center().y - tall / 2.0).max(rect.top());
    painter.galley(pos2(rect.left(), y), name.clone(), *ON_SURFACE);
    y += name.size().y + space::S;
    let mut marks = Vec::new();
    for (mark, meaning) in lines {
        // The mark on the meaning's first line.
        let first = meaning.rows.first().map_or(meaning.size().y, |row| row.height());
        let line = first.max(SWATCH.y);
        mark.paint(Rect::from_center_size(pos2(rect.left() + SWATCH.x / 2.0, y + line / 2.0), SWATCH), ink, &mut marks);
        painter.galley(pos2(rect.left() + SWATCH.x + space::S, y + (line - first) / 2.0), meaning.clone(), *ON_SURFACE_VARIANT);
        y += meaning.size().y.max(SWATCH.y) + LINE_GAP;
    }
    painter.extend(marks);
}
