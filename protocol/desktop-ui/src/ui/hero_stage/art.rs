//! The hero's art, as Android's `HeroStageView` lays it: the game's cover in the trailing share of
//! the stage, cropped to its upper part, behind a soft round mask whose middle stands on the stage's
//! end edge, and held to the stage light so the words over it need no scrim. A game without art
//! gets its tile's gradient there instead.
//!
//! Android bakes a held copy of the cover, pixel by pixel. The desktop has the cover only as a
//! texture, so it holds it with the mesh's colour: by night the whole cover is multiplied down to
//! the ambient's text-zone luminance (a white pixel lands on it exactly), by day it is washed with
//! the day's canvas until black lands on the day's text-zone luminance. Either way nothing in the
//! art is brighter (by night) or darker (by day) than the ambient may be. The tile's two colours are
//! held on the CPU, as Android holds every pixel.

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{pos2, vec2, Color32, Pos2, Rect, Shape, TextureId, Vec2};

use crate::theme::Look;
use crate::ui::design_tokens as t;

/// A wide window: the art's share of the stage's width, from its end edge.
pub(in crate::ui) const ART_SHARE: f32 = 0.62;
/// The mask's middle, down the art's height, on the end edge.
const MIDDLE_DOWN: f32 = 0.34;
/// A tall window: how far the mask reaches across, in window widths, and its middle down the art.
const TALL_REACH: f32 = 1.2;
const MIDDLE_DOWN_TALL: f32 = 0.25;
/// Of the cover's height that does not fit, this share is cut off above: the picture is in the
/// upper part.
const CROP_ABOVE: f32 = 0.22;
/// The mask: whole to half its reach, gone at it, along Android's stops.
const MASK: [(f32, f32); 6] = [(0.0, 1.0), (0.5, 1.0), (0.625, 0.843), (0.75, 0.502), (0.875, 0.157), (1.0, 0.0)];
/// How far past the stage's end the art's middle stands at most: a page's usual margin, so in a
/// usual window the art is whole at the window's edge as on Android, and on an ultrawide it fades
/// into the margin instead of stopping at the stage.
const END_REACH: f32 = 48.0;
/// The mesh the mask is drawn with: fine enough that its steps are never seen.
const COLUMNS: usize = 32;
const ROWS: usize = 16;

/// Where the art lies and how its mask falls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Place {
    pub rect: Rect,
    /// The mask's middle.
    pub middle: Pos2,
    /// How far the mask reaches toward the start, past its middle toward the end, and down.
    pub before: f32,
    pub after: f32,
    pub down: f32,
}

/// Where the art goes for a stage (its width, from the page's top to the hero's foot) in this
/// window: the trailing [`ART_SHARE`] of a wide one, lit from its end edge; across the top of a
/// tall one.
pub(in crate::ui) fn place(stage: Rect, window: Rect) -> Place {
    if window.width() > window.height() {
        let left = stage.right() - stage.width() * ART_SHARE;
        let middle_x = window.right().min(stage.right() + END_REACH);
        let rect = Rect::from_min_max(pos2(left, stage.top()), pos2(window.right(), stage.bottom()));
        let middle = pos2(middle_x, rect.top() + rect.height() * MIDDLE_DOWN);
        Place { rect, middle, before: middle_x - left, after: window.right() - middle_x, down: rect.height() * (1.0 - MIDDLE_DOWN) }
    } else {
        let rect = Rect::from_min_max(pos2(window.left(), stage.top()), pos2(window.right(), stage.bottom()));
        let middle = pos2(window.center().x, rect.top() + rect.height() * MIDDLE_DOWN_TALL);
        let reach = window.width() * TALL_REACH / 2.0;
        Place { rect, middle, before: reach, after: reach, down: rect.height() * (1.0 - MIDDLE_DOWN_TALL) }
    }
}

/// How much of the art shows at `p`: whole near the middle, gone at the reach.
pub(in crate::ui) fn mask(place: &Place, p: Pos2) -> f32 {
    let dx = p.x - place.middle.x;
    let across = if dx < 0.0 { place.before } else { place.after };
    let x = if across > 0.5 { dx / across } else if dx.abs() < 0.5 { 0.0 } else { 1.0 };
    let y = (p.y - place.middle.y) / place.down.max(1.0);
    let d = (x * x + y * y).sqrt();
    let mut last = MASK[0];
    for stop in MASK {
        if d <= stop.0 {
            let span = (stop.0 - last.0).max(1e-6);
            return last.1 + (stop.1 - last.1) * ((d - last.0) / span).clamp(0.0, 1.0);
        }
        last = stop;
    }
    0.0
}

fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 { channel / 12.92 } else { ((channel + 0.055) / 1.055).powf(2.4) }
}

fn encoded(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    if value <= 0.003_130_8 { value * 12.92 } else { 1.055 * value.powf(1.0 / 2.4) - 0.055 }
}

/// WCAG's relative luminance of an opaque colour.
#[cfg(test)]
pub(in crate::ui) fn luminance(color: Color32) -> f32 {
    let [r, g, b] = [color.r(), color.g(), color.b()].map(|c| linear(f32::from(c) / 255.0));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// What the cover is multiplied by at night: white comes out at the ambient's text-zone luminance.
pub(in crate::ui) fn night_hold() -> f32 {
    encoded(t::AMBIENT_TEXT_ZONE_MAX_LUMA)
}

/// How much of the day's canvas washes over the cover by day: black comes out at the day's
/// text-zone luminance.
pub(in crate::ui) fn day_wash() -> f32 {
    let canvas = t::DAY_COLOR_CANVAS;
    let least = f32::from(canvas.r().min(canvas.g()).min(canvas.b())) / 255.0;
    (encoded(t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA) / least).min(1.0)
}

/// An opaque colour in the stage's light: held down to the night's text-zone luminance, or lifted
/// toward white to the day's, keeping its hue.
pub(in crate::ui) fn held(color: Color32, day: bool) -> Color32 {
    let rgb = [color.r(), color.g(), color.b()].map(|c| linear(f32::from(c) / 255.0));
    let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    let out = if day {
        let goal = t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA;
        let toward = if luma < goal { (goal - luma) / (1.0 - luma).max(1e-6) } else { 0.0 };
        rgb.map(|c| c + (1.0 - c) * toward)
    } else {
        let most = t::AMBIENT_TEXT_ZONE_MAX_LUMA;
        let by = if luma > most { most / luma } else { 1.0 };
        rgb.map(|c| c * by)
    };
    let [r, g, b] = out.map(|c| (encoded(c) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

/// What the art is made of.
pub(in crate::ui) enum Source {
    /// Pre-blurred and look-graded hero colour wash (`ambient::bake_hero`), ready to paint with
    /// zero ghost cover typography and zero muddy Day wash.
    Wash(TextureId, Vec2),
    /// Fallback held cover texture when a synthetic unit-test texture has no baked wash.
    Cover(TextureId, Vec2),
    /// A game without art: its tile's two colours, top and bottom.
    Tile(Color32, Color32),
}

/// Where in the cover each point of `rect` samples: it covers the rect, centred across, and most of
/// what does not fit is cut off below.
fn cover_uv(size: Vec2, rect: Rect) -> impl Fn(Pos2) -> Pos2 {
    let size = size.max(Vec2::splat(1.0));
    let scale = (rect.width() / size.x).max(rect.height() / size.y);
    let shown = size * scale;
    let origin = pos2(rect.left() + (rect.width() - shown.x) / 2.0, rect.top() - (shown.y - rect.height()) * CROP_ABOVE);
    move |p: Pos2| pos2((p.x - origin.x) / shown.x, (p.y - origin.y) / shown.y)
}

/// A grid over the art's rect, each vertex coloured by `color` (given the point and the mask there)
/// and sampling `texture` at `uv`.
fn grid(place: &Place, texture: TextureId, uv: &dyn Fn(Pos2) -> Pos2, color: &dyn Fn(Pos2, f32) -> Color32) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    let rect = place.rect;
    for row in 0..=ROWS {
        for column in 0..=COLUMNS {
            let p = rect.min + vec2(rect.width() * column as f32 / COLUMNS as f32, rect.height() * row as f32 / ROWS as f32);
            mesh.vertices.push(Vertex { pos: p, uv: uv(p), color: color(p, mask(place, p)) });
        }
    }
    let stride = (COLUMNS + 1) as u32;
    for row in 0..ROWS as u32 {
        for column in 0..COLUMNS as u32 {
            let a = row * stride + column;
            mesh.indices.extend([a, a + 1, a + stride + 1, a, a + stride + 1, a + stride]);
        }
    }
    mesh
}

/// The art's shapes in `look`, `alpha` of it shown (it fades in once its cover arrives). Nothing in
/// high contrast, whose page is solid.
pub(in crate::ui) fn shapes(place: &Place, source: &Source, look: Look, alpha: f32) -> Vec<Shape> {
    if look == Look::HighContrast || alpha <= 0.0 || place.rect.width() < 1.0 || place.rect.height() < 1.0 {
        return Vec::new();
    }
    let day = look == Look::Day;
    let white = TextureId::default();
    match source {
        Source::Tile(top, bottom) => {
            let (top, bottom) = (held(*top, day), held(*bottom, day));
            let fill = crate::ui::glass::gradient(place.rect, 165.0, &[(0.0, top), (1.0, bottom)]);
            vec![Shape::mesh(grid(place, white, &|_| WHITE_UV, &|p, m| fill(p).gamma_multiply(m * alpha)))]
        }
        Source::Wash(texture, size) => {
            let uv = cover_uv(*size, place.rect);
            let oled_dim = if look == Look::Oled { 0.78 } else { 1.0 };
            vec![Shape::mesh(grid(place, *texture, &uv, &|_, m| {
                Color32::WHITE.gamma_multiply(m * alpha * oled_dim)
            }))]
        }
        Source::Cover(texture, size) => {
            let uv = cover_uv(*size, place.rect);
            if day {
                let wash = day_wash();
                let keep = Color32::from_gray(((1.0 - wash) * 255.0).round() as u8);
                let canvas = t::DAY_COLOR_CANVAS;
                let add = move |m: f32| {
                    let by = wash * m * alpha;
                    let c = |v: u8| (f32::from(v) * by).round() as u8;
                    Color32::from_rgba_premultiplied(c(canvas.r()), c(canvas.g()), c(canvas.b()), 0)
                };
                vec![
                    Shape::mesh(grid(place, *texture, &uv, &|_, m| keep.gamma_multiply(m * alpha))),
                    Shape::mesh(grid(place, white, &|_| WHITE_UV, &|_, m| add(m))),
                ]
            } else {
                let hold = Color32::from_gray((night_hold() * 255.0).round() as u8);
                vec![Shape::mesh(grid(place, *texture, &uv, &|_, m| hold.gamma_multiply(m * alpha)))]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide() -> (Rect, Rect) {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, 800.0));
        (Rect::from_min_max(pos2(48.0, 0.0), pos2(1232.0, 420.0)), window)
    }

    #[test]
    fn the_art_takes_the_trailing_share_and_is_whole_at_the_end_edge() {
        let (stage, window) = wide();
        let at = place(stage, window);
        assert!((at.rect.left() - (stage.right() - stage.width() * ART_SHARE)).abs() < 0.5, "{at:?}");
        assert_eq!(at.rect.right(), window.right(), "a usual window: the art runs to its edge");
        assert!(mask(&at, at.middle) > 0.999 && mask(&at, pos2(window.right(), at.rect.top())) > 0.9, "whole at the end edge");
        assert!(mask(&at, pos2(at.rect.left(), at.middle.y)) < 0.001, "gone where it starts");
        assert!(mask(&at, pos2(at.middle.x, at.rect.bottom())) < 0.001, "gone at the hero's foot");
        let halfway = mask(&at, pos2(at.middle.x - at.before * 0.75, at.middle.y));
        assert!((halfway - 0.502).abs() < 0.01, "along Android's stops: {halfway}");
    }

    #[test]
    fn on_an_ultrawide_the_art_fades_into_the_margin_instead_of_stopping() {
        let window = Rect::from_min_size(Pos2::ZERO, vec2(2560.0, 1080.0));
        let stage = Rect::from_min_max(pos2(400.0, 0.0), pos2(2160.0, 500.0));
        let at = place(stage, window);
        assert_eq!(at.middle.x, stage.right() + END_REACH);
        assert!(mask(&at, pos2(window.right(), at.middle.y)) < 0.001, "gone at the window's edge");
        assert!(mask(&at, pos2(stage.right(), at.middle.y)) > 0.999, "whole at the stage's end");
    }

    #[test]
    fn held_colours_keep_to_the_stage_light() {
        for c in [Color32::WHITE, Color32::from_rgb(255, 210, 40), Color32::from_rgb(164, 69, 28), Color32::BLACK] {
            assert!(luminance(held(c, false)) <= t::AMBIENT_TEXT_ZONE_MAX_LUMA + 0.003, "{c:?} by night");
            assert!(luminance(held(c, true)) >= t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA - 0.01, "{c:?} by day");
        }
        let dark = Color32::from_rgb(20, 30, 60);
        assert_eq!(held(dark, false), dark, "what is already dark stays");
        // The cover's own hold: white multiplied down, and black washed up.
        let white_at_night = Color32::from_gray((night_hold() * 255.0).round() as u8);
        assert!(luminance(white_at_night) <= t::AMBIENT_TEXT_ZONE_MAX_LUMA + 0.003);
        let wash = day_wash();
        let canvas = t::DAY_COLOR_CANVAS;
        let black_by_day = Color32::from_rgb(
            (f32::from(canvas.r()) * wash) as u8,
            (f32::from(canvas.g()) * wash) as u8,
            (f32::from(canvas.b()) * wash) as u8,
        );
        assert!(luminance(black_by_day) >= t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA - 0.01, "{black_by_day:?}");
    }

    #[test]
    fn high_contrast_shows_no_art() {
        let (stage, window) = wide();
        let at = place(stage, window);
        let tile = Source::Tile(Color32::RED, Color32::BLUE);
        assert!(shapes(&at, &tile, Look::HighContrast, 1.0).is_empty());
        assert_eq!(shapes(&at, &tile, Look::Night, 1.0).len(), 1);
        assert_eq!(shapes(&at, &Source::Cover(TextureId::Managed(7), vec2(300.0, 450.0)), Look::Day, 1.0).len(), 2);
    }

    #[test]
    fn the_cover_is_cropped_from_its_upper_part() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 400.0));
        let uv = cover_uv(vec2(300.0, 450.0), rect);
        let (top, bottom) = (uv(rect.left_top()), uv(rect.left_bottom()));
        assert!(top.x.abs() < 1e-4 && (uv(rect.right_top()).x - 1.0).abs() < 1e-4, "it covers the width");
        let cut = 1.0 - (bottom.y - top.y);
        assert!((top.y - cut * CROP_ABOVE).abs() < 1e-3, "{top:?} {bottom:?}");
    }
}
