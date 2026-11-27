//! The ambient backdrop's and hero wash's bake: a cover squashed to a thumbnail, blurred,
//! saturated, and shaped to the active look's luminance envelope (dark obsidian cap by night,
//! lifted pastel Alabaster floor by day) so text reads cleanly over every cover with zero
//! ghost cover typography. Pure CPU work on small images; `hosts/covers.rs` runs [`bake`] off
//! the UI thread beside the cover's decode.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use egui::{Color32, ColorImage, Context, Id, TextureHandle, TextureId, TextureOptions};

use crate::theme::shade;
use crate::ui::design_tokens as t;

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// The cover glow's strength and how its colour is lit from the bake: `0 0 70px rgba(255,95,70,.28)`
/// in the target. A bake darker than the last dims the glow with it.
const GLOW_ALPHA: f32 = 0.28;
const GLOW_SATURATE: f32 = 1.5;
const GLOW_SATURATION_MOST: f32 = 0.74;
const GLOW_DARK_BELOW: f32 = 0.16;
const SIZE: [usize; 2] = [
    t::TIER_BAKED_THUMB_W as usize,
    t::TIER_BAKED_THUMB_H as usize,
];
/// Downsampled 2:3 grid for the hero stage's directional colour wash.
pub const HERO_SIZE: [usize; 2] = [96, 144];
/// Separable Gaussian sigma per pass on [`HERO_SIZE`] (two passes give effective sigma ~22.6 px,
/// kernel support radius 48 px per pass, dissolving cover lettering completely).
const HERO_BLUR_SIGMA: f32 = 16.0;
/// Bounded GPU texture ring for hero directional washes per [`Context`].
const HERO_WASHES_KEPT: usize = 32;

#[derive(Default)]
struct CoverThumbs {
    /// Squashed `HERO_SIZE` sRGB thumbnail indexed by uploaded cover `TextureId` (`Art::Ready`).
    by_art: HashMap<TextureId, Arc<[[u8; 3]]>>,
    /// Squashed `HERO_SIZE` sRGB thumbnail indexed by uploaded Night ambient `TextureId` (`Host::ambient`).
    by_ambient: HashMap<TextureId, Arc<[[u8; 3]]>>,
    /// Bounded FIFO cache of uploaded `HERO_SIZE` hero wash GPU textures keyed by `(source_id, day)`.
    hero_washes: HashMap<(TextureId, bool), TextureHandle>,
    hero_wash_order: VecDeque<(TextureId, bool)>,
}

fn cover_cache(ctx: &Context) -> Arc<Mutex<CoverThumbs>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<CoverThumbs>>>(Id::unique("nj-cover-thumbs"))
            .clone()
    })
}

fn thumb_bytes(cover: &ColorImage) -> Arc<[[u8; 3]]> {
    let vec: Vec<[u8; 3]> = squash_to(cover, HERO_SIZE)
        .into_iter()
        .map(|c| c.map(|v| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8))
        .collect();
    vec.into()
}

fn thumb_image(thumb: &[[u8; 3]]) -> ColorImage {
    ColorImage {
        size: HERO_SIZE,
        source_size: egui::Vec2::new(HERO_SIZE[0] as f32, HERO_SIZE[1] as f32),
        pixels: thumb
            .iter()
            .map(|&[r, g, b]| Color32::from_rgb(r, g, b))
            .collect(),
    }
}

/// Associates uploaded cover and/or Night ambient texture IDs with their source `cover` inside
/// `ctx` so Day ambient and Night/Day hero washes resolve deterministically per `egui::Context`
/// without cross-context `Managed(n)` collisions, `glow` collisions, or 256-entry FIFO eviction.
pub fn register_cover(
    ctx: &Context,
    art_id: Option<TextureId>,
    ambient_id: Option<TextureId>,
    cover: &ColorImage,
) {
    if art_id.is_none() && ambient_id.is_none() {
        return;
    }
    let thumb = thumb_bytes(cover);
    let cache = cover_cache(ctx);
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(id) = art_id {
        guard.by_art.insert(id, Arc::clone(&thumb));
    }
    if let Some(id) = ambient_id {
        guard.by_ambient.insert(id, thumb);
    }
}

/// Retains only registered cover/ambient thumbnails and cached hero washes in `ctx` whose
/// `TextureId` satisfies `keep`.
pub fn retain_covers(ctx: &Context, mut keep: impl FnMut(TextureId) -> bool) {
    let cache = cover_cache(ctx);
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    let CoverThumbs {
        by_art,
        by_ambient,
        hero_washes,
        hero_wash_order,
    } = &mut *guard;
    by_art.retain(|&id, _| keep(id));
    by_ambient.retain(|&id, _| keep(id));
    hero_washes.retain(|&(id, _), _| keep(id));
    hero_wash_order.retain(|key| hero_washes.contains_key(key));
}

/// Clears all registered cover thumbnails and cached hero wash textures in `ctx` (e.g. on renderer swap).
pub fn clear_covers(ctx: &Context) {
    let cache = cover_cache(ctx);
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    *guard = CoverThumbs::default();
}

/// Lazily bakes a Day-mode ambient (`SIZE`) for a cover registered under `ambient_id` in `ctx`.
pub fn day_for(ctx: &Context, ambient_id: TextureId, _glow: Color32) -> Option<ColorImage> {
    let cache = cover_cache(ctx);
    let thumb = {
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        guard.by_ambient.get(&ambient_id).cloned()?
    };
    Some(bake_day(&thumb_image(&thumb)))
}

/// Lazily bakes a blurred hero directional wash (`HERO_SIZE`) for a cover registered under
/// `art_id` or `ambient_id` in `ctx`.
pub fn hero_for(
    ctx: &Context,
    art_id: TextureId,
    ambient_id: Option<TextureId>,
    _glow: Option<Color32>,
    day: bool,
) -> Option<ColorImage> {
    let cache = cover_cache(ctx);
    let thumb = {
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .by_art
            .get(&art_id)
            .or_else(|| ambient_id.and_then(|id| guard.by_ambient.get(&id)))
            .cloned()?
    };
    Some(bake_hero(&thumb_image(&thumb), day))
}

/// Returns a bounded-FIFO GPU `TextureHandle` for the blurred hero directional wash in `ctx`.
pub fn hero_wash_texture(
    ctx: &Context,
    name: &str,
    source_id: TextureId,
    ambient_id: Option<TextureId>,
    glow: Option<Color32>,
    day: bool,
) -> Option<TextureHandle> {
    let key = (source_id, day);
    let cache = cover_cache(ctx);
    {
        let guard = cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(found) = guard.hero_washes.get(&key) {
            return Some(found.clone());
        }
    }
    let baked = hero_for(ctx, source_id, ambient_id, glow, day)?;
    let handle = ctx.load_texture(name, baked, TextureOptions::LINEAR);
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    while guard.hero_washes.len() >= HERO_WASHES_KEPT {
        let Some(old) = guard.hero_wash_order.pop_front() else {
            guard.hero_washes.clear();
            break;
        };
        guard.hero_washes.remove(&old);
    }
    guard.hero_wash_order.push_back(key);
    guard.hero_washes.insert(key, handle.clone());
    Some(handle)
}

fn to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.max(0.0).powf(1.0 / 2.4) - 0.055
    }
}

fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h, if max <= 0.0 { 0.0 } else { d / max }, max]
}

fn hsv_to_rgb([h, s, v]: [f32; 3]) -> [f32; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = v - c;
    let [r, g, b] = match (h.rem_euclid(360.0) / 60.0) as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    [r + m, g + m, b + m]
}

/// One thumbnail pixel's colour by night: saturation lifted, yellow moved toward amber (it dims
/// to olive otherwise), luminance capped in linear light so a white cover comes out a safe grey.
fn grade(rgb: [f32; 3]) -> [f32; 3] {
    let [mut h, s, v] = rgb_to_hsv(rgb);
    let band = ((h - 25.0) / 15.0).clamp(0.0, 1.0) * ((95.0 - h) / 15.0).clamp(0.0, 1.0);
    h += (22.0 - h) * band;
    let lifted = hsv_to_rgb([h, (s * t::AMBIENT_SATURATE).min(1.0), v]);
    let linear = lifted.map(to_linear);
    let luma: f32 = (0..3).map(|i| linear[i] * LUMA[i]).sum();
    let scale = (t::AMBIENT_TEXT_ZONE_MAX_LUMA / luma.max(1e-5)).min(1.0);
    linear.map(|c| to_srgb(c * scale).clamp(0.0, 1.0))
}

/// One thumbnail pixel's colour by day: saturation lifted (`DAY_AMBIENT_SATURATE`, 1.3) and linear
/// luminance lifted toward Alabaster white up to `DAY_AMBIENT_TEXT_ZONE_MIN_LUMA` (0.56) so dark
/// text reads cleanly over bright pastel colour instead of a muddy grey wash.
fn grade_day(rgb: [f32; 3]) -> [f32; 3] {
    let [h, s, v] = rgb_to_hsv(rgb);
    let lifted = hsv_to_rgb([h, (s * t::DAY_AMBIENT_SATURATE).min(1.0), v]);
    let linear = lifted.map(to_linear);
    let luma: f32 = (0..3).map(|i| linear[i] * LUMA[i]).sum();
    let floor = t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA;
    let toward = if luma < floor {
        ((floor - luma) / (1.0 - luma).max(1e-5)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    linear.map(|c| to_srgb((c + (1.0 - c) * toward).clamp(0.0, 1.0)))
}

/// The area average of `cover` over each cell of a `[tw, th]` thumbnail: all of the cover, squashed.
fn squash_to(cover: &ColorImage, [tw, th]: [usize; 2]) -> Vec<[f32; 3]> {
    let [w, h] = cover.size;
    let mut out = vec![[0.0f32; 3]; tw * th];
    if w == 0 || h == 0 || tw == 0 || th == 0 {
        return out;
    }
    let mut counts = vec![0u32; tw * th];
    for y in 0..h {
        let ty = y * th / h;
        for x in 0..w {
            let at = ty * tw + x * tw / w;
            let c = cover.pixels[y * w + x];
            out[at][0] += f32::from(c.r());
            out[at][1] += f32::from(c.g());
            out[at][2] += f32::from(c.b());
            counts[at] += 1;
        }
    }
    // A cover smaller than the thumbnail leaves cells empty: they take the nearest source pixel.
    for ty in 0..th {
        for tx in 0..tw {
            let at = ty * tw + tx;
            if counts[at] == 0 {
                let c = cover.pixels[(ty * h / th) * w + tx * w / tw];
                out[at] = [f32::from(c.r()), f32::from(c.g()), f32::from(c.b())];
                counts[at] = 1;
            }
            out[at] = out[at].map(|v| v / counts[at] as f32 / 255.0);
        }
    }
    out
}

fn squash(cover: &ColorImage) -> Vec<[f32; 3]> {
    squash_to(cover, SIZE)
}

/// A separable Gaussian of `sigma` thumbnail pixels; edges repeat.
fn gaussian(pixels: &[[f32; 3]], [w, h]: [usize; 2], sigma: f32) -> Vec<[f32; 3]> {
    let reach = (sigma * 3.0).ceil() as isize;
    let weights: Vec<f32> = (-reach..=reach)
        .map(|i| (-(i as f32).powi(2) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    let line = |get: &dyn Fn(isize) -> [f32; 3]| {
        let mut sum = [0.0f32; 3];
        for (i, weight) in (-reach..=reach).zip(&weights) {
            let c = get(i);
            for k in 0..3 {
                sum[k] += c[k] * weight;
            }
        }
        sum.map(|v| v / total)
    };
    let mut across = vec![[0.0f32; 3]; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            across[y * w + x] =
                line(&|i| pixels[y * w + (x as isize + i).clamp(0, w as isize - 1) as usize]);
        }
    }
    let mut down = vec![[0.0f32; 3]; pixels.len()];
    for y in 0..h {
        for x in 0..w {
            down[y * w + x] =
                line(&|i| across[(y as isize + i).clamp(0, h as isize - 1) as usize * w + x]);
        }
    }
    down
}

/// To bytes with one level of ordered noise, so wide dark gradients do not band.
fn finish(pixels: &[[f32; 3]], size: [usize; 2]) -> ColorImage {
    let mut image = ColorImage::filled(size, Color32::BLACK);
    for (at, c) in pixels.iter().enumerate() {
        let noise = ((at.wrapping_mul(2_654_435_761)) >> 24 & 0xFF) as f32 / 255.0 - 0.5;
        let [r, g, b] = c.map(|v| (v * 255.0 + noise + 0.5).clamp(0.0, 255.0) as u8);
        image.pixels[at] = Color32::from_rgb(r, g, b);
    }
    image
}

/// The Night ambient of a cover: one call per cover, off the UI thread.
pub fn bake(cover: &ColorImage) -> ColorImage {
    let small = squash(cover);
    let soft = gaussian(&small, SIZE, SIZE[0] as f32 / 8.0);
    let graded: Vec<[f32; 3]> = soft.into_iter().map(grade).collect();
    finish(&graded, SIZE)
}

/// The Day-mode ambient of a cover: lifted pastel Alabaster luminance (`>= 0.56`) with `1.3x`
/// saturation so dark text reads cleanly with zero muddy grey wash.
pub fn bake_day(cover: &ColorImage) -> ColorImage {
    let small = squash(cover);
    let soft = gaussian(&small, SIZE, SIZE[0] as f32 / 8.0);
    let graded: Vec<[f32; 3]> = soft.into_iter().map(grade_day).collect();
    finish(&graded, SIZE)
}

/// Multi-pass blurred 2:3 hero directional wash (`HERO_SIZE`), graded for Night/OLED (`day == false`)
/// or lifted Alabaster Day (`day == true`) so box-art lettering dissolves completely into colour.
pub fn bake_hero(cover: &ColorImage, day: bool) -> ColorImage {
    let small = squash_to(cover, HERO_SIZE);
    let pass1 = gaussian(&small, HERO_SIZE, HERO_BLUR_SIGMA);
    let pass2 = gaussian(&pass1, HERO_SIZE, HERO_BLUR_SIGMA);
    let map_fn: fn([f32; 3]) -> [f32; 3] = if day { grade_day } else { grade };
    let graded: Vec<[f32; 3]> = pass2.into_iter().map(map_fn).collect();
    finish(&graded, HERO_SIZE)
}

/// What the cover glows with on its page: the bake's own colour lit up, its hue kept, weighted to
/// where the bake is most coloured, as strong as the spec's glow. A grey cover glows pale and a black
/// one not at all. Premultiplied.
pub fn glow_of(baked: &ColorImage) -> Color32 {
    let (mut sum, mut weights, mut brightest) = ([0.0f32; 3], 0.0f32, 0.0f32);
    for pixel in &baked.pixels {
        let c = [pixel.r(), pixel.g(), pixel.b()].map(|v| f32::from(v) / 255.0);
        let [_, s, v] = rgb_to_hsv(c);
        let weight = s * v + 1e-3;
        for k in 0..3 {
            sum[k] += c[k] * weight;
        }
        weights += weight;
        brightest = brightest.max(v);
    }
    if weights <= 0.0 {
        return Color32::TRANSPARENT;
    }
    let [h, s, _] = rgb_to_hsv(sum.map(|v| v / weights));
    let lit = hsv_to_rgb([h, (s * GLOW_SATURATE).min(GLOW_SATURATION_MOST), 1.0]);
    let strength = GLOW_ALPHA * (brightest / GLOW_DARK_BELOW).min(1.0);
    let [r, g, b] = lit.map(|v| (v * strength * 255.0).round() as u8);
    Color32::from_rgba_premultiplied(r, g, b, (strength * 255.0).round() as u8)
}

/// The ambient of a game without art: its tile's gradient, under the Night luminance cap.
pub fn bake_tile(top: Color32, bottom: Color32) -> ColorImage {
    bake_tile_with(top, bottom, grade)
}

/// The Day-mode ambient of a game without art: its tile's gradient lifted to the Day pastel floor.
pub fn bake_tile_day(top: Color32, bottom: Color32) -> ColorImage {
    bake_tile_with(top, bottom, grade_day)
}

fn bake_tile_with(
    top: Color32,
    bottom: Color32,
    grade_fn: fn([f32; 3]) -> [f32; 3],
) -> ColorImage {
    let [w, h] = SIZE;
    let channel = |c: Color32| [c.r(), c.g(), c.b()].map(|v| f32::from(v) / 255.0);
    let (top, bottom) = (channel(top), channel(bottom));
    // The tile's 165 degree gradient.
    let (dx, dy) = (165f32.to_radians().sin(), -165f32.to_radians().cos());
    let span = dx.abs() * w as f32 + dy.abs() * h as f32;
    let mut pixels = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        for x in 0..w {
            let along =
                ((x as f32 - w as f32 / 2.0) * dx + (y as f32 - h as f32 / 2.0) * dy) / span + 0.5;
            let mix = std::array::from_fn(|k| top[k] + (bottom[k] - top[k]) * along);
            pixels[y * w + x] = grade_fn(mix);
        }
    }
    finish(&pixels, SIZE)
}

/// The stage's bake. A cover's ambient is a blur, which hides how small it is; the stage is one
/// wide, faint gradient, where a bake that coarse would show as bands in the dark. This fine, the
/// noise [`finish`] adds stays near a pixel's size on screen and the gradient reads as continuous.
const STAGE_SIZE: [usize; 2] = [640, 360];
/// Where the stage reaches its middle colour, the page's canvas, top to bottom.
const STAGE_MID_AT: f32 = 0.46;

/// The stage light at `d` of its reach: full in the middle and nothing at its reach, where its
/// slope and its curvature are nothing too, so there is no edge to see, neither a disc's nor a
/// cone's.
pub fn stage_light(d: f32) -> f32 {
    let inside = (1.0 - d * d).max(0.0);
    inside * inside * inside
}

/// The stage a page without art stands on: its colours, top to bottom, under one wide light around
/// `light` (fractions of the bake, `reach` likewise), `strength` of the token's. What glass samples
/// there.
pub fn bake_stage(light: [f32; 2], reach: [f32; 2], strength: f32) -> ColorImage {
    let [w, h] = STAGE_SIZE;
    let channel = |c: Color32| [c.r(), c.g(), c.b(), c.a()].map(|v| f32::from(v) / 255.0);
    let (top, mid, bottom, glow) = (
        channel(shade::SKY_TOP.get()),
        channel(shade::SKY_MID.get()),
        channel(shade::SKY_BOTTOM.get()),
        channel(shade::SKY_GLOW.get()),
    );
    let mut pixels = vec![[0.0f32; 3]; w * h];
    for y in 0..h {
        let v = (y as f32 + 0.5) / h as f32;
        let (from, to, along) = if v < STAGE_MID_AT {
            (top, mid, v / STAGE_MID_AT)
        } else {
            (mid, bottom, (v - STAGE_MID_AT) / (1.0 - STAGE_MID_AT))
        };
        // Eased, so the gradient has no crease where it reaches the canvas.
        let along = along * along * (3.0 - 2.0 * along);
        for x in 0..w {
            let d = (((x as f32 + 0.5) / w as f32 - light[0]) / reach[0])
                .hypot((v - light[1]) / reach[1]);
            let lit = strength * stage_light(d);
            pixels[y * w + x] = std::array::from_fn(|k| {
                let floor = from[k] + (to[k] - from[k]) * along;
                floor * (1.0 - glow[3] * lit) + glow[k] * lit
            });
        }
    }
    finish(&pixels, STAGE_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(c: Color32) -> f32 {
        let linear = [c.r(), c.g(), c.b()].map(|v| to_linear(f32::from(v) / 255.0));
        (0..3).map(|i| linear[i] * LUMA[i]).sum()
    }

    #[test]
    fn a_white_cover_bakes_to_the_safe_grey_the_spec_names() {
        let white = ColorImage::filled([600, 900], Color32::WHITE);
        let out = bake(&white);
        assert_eq!(out.size, SIZE);
        for pixel in &out.pixels {
            assert!(
                luma(*pixel) <= t::AMBIENT_TEXT_ZONE_MAX_LUMA + 0.003,
                "{pixel:?}"
            );
            let grey = (to_srgb(t::AMBIENT_TEXT_ZONE_MAX_LUMA) * 255.0).round() as i32;
            assert!((i32::from(pixel.r()) - grey).abs() <= 1, "{pixel:?}");
        }
    }

    #[test]
    fn a_dark_cover_keeps_its_colour_and_a_black_one_stays_black() {
        let dark = ColorImage::filled([300, 450], Color32::from_rgb(40, 10, 60));
        let c = bake(&dark).pixels[500];
        assert!(c.b() > c.r() && c.r() > c.g(), "{c:?}");
        let black = bake(&ColorImage::filled([300, 450], Color32::BLACK));
        assert!(black
            .pixels
            .iter()
            .all(|p| p.r() <= 1 && p.g() <= 1 && p.b() <= 1));
    }

    #[test]
    fn yellow_moves_toward_amber() {
        let [h, ..] = rgb_to_hsv(grade([0.9, 0.85, 0.1]));
        assert!(h < 45.0, "{h}");
    }

    #[test]
    fn the_bake_sees_the_whole_cover_squashed_not_a_crop() {
        // Red top, blue bottom: both ends of the cover reach the thumbnail.
        let mut image = ColorImage::filled([200, 300], Color32::from_rgb(90, 0, 0));
        for pixel in &mut image.pixels[200 * 150..] {
            *pixel = Color32::from_rgb(0, 0, 90);
        }
        let out = bake(&image);
        let (top, bottom) = (out.pixels[48], out.pixels[96 * 53 + 48]);
        assert!(top.r() > top.b(), "{top:?}");
        assert!(bottom.b() > bottom.r(), "{bottom:?}");
    }

    #[test]
    fn a_warm_cover_glows_warm_a_blue_one_blue_and_a_black_one_not_at_all() {
        let warm = glow_of(&bake(&ColorImage::filled(
            [200, 300],
            Color32::from_rgb(200, 70, 50),
        )));
        assert!(warm.r() > warm.g() * 2 && warm.r() > warm.b() * 2, "{warm:?}");
        let alpha = f32::from(warm.a()) / 255.0;
        assert!((alpha - GLOW_ALPHA).abs() < 0.02, "the spec's strength: {alpha}");
        let blue = glow_of(&bake(&ColorImage::filled(
            [200, 300],
            Color32::from_rgb(30, 60, 200),
        )));
        assert!(blue.b() > blue.r() * 2, "{blue:?}");
        let black = glow_of(&bake(&ColorImage::filled([200, 300], Color32::BLACK)));
        assert!(black.a() < 16, "{black:?}");
    }

    #[test]
    fn a_tile_gradient_stays_under_the_cap() {
        for (top, bottom) in [
            (t::TILE_PAIR6_TOP, t::TILE_PAIR6_BOTTOM),
            (t::TILE_PAIR0_TOP, t::TILE_PAIR0_BOTTOM),
        ] {
            let out = bake_tile(top, bottom);
            assert!(out
                .pixels
                .iter()
                .all(|p| luma(*p) <= t::AMBIENT_TEXT_ZONE_MAX_LUMA + 0.003));
        }
    }

    /// The stage reads as light on a matte floor, never as a disc or a cone: brightest under the
    /// light, dimmer every way from there, no step anywhere larger than the noise, and the floor's
    /// own colour where the light ends.
    #[test]
    fn the_stage_light_falls_off_with_no_edge_and_leaves_the_floor() {
        crate::theme::own_palette();
        let baked = bake_stage([0.5, 0.02], [0.6, 0.66], 0.6);
        let [w, h] = baked.size;
        assert_eq!([w, h], STAGE_SIZE);
        let at = |x: usize, y: usize| f32::from(baked.pixels[y * w + x].g());
        // Over a few texels, so the half level of noise does not count.
        let smooth = |x: usize, y: usize| (0..16).map(|k| at(x + k, y)).sum::<f32>() / 16.0;
        let middle = w / 2 - 8;
        let lit = smooth(middle, 0);
        assert!(lit > f32::from(shade::SKY_TOP.get().g()) + 8.0, "the light shows: {lit}");
        assert!(lit > smooth(0, 0) + 4.0, "less to the sides: {lit} and {}", smooth(0, 0));
        let mut last = lit;
        for y in 1..h {
            let here = smooth(middle, y);
            assert!(here <= last + 0.5, "row {y}: it only falls, {last} to {here}");
            last = here;
        }
        for y in 0..h {
            for x in 0..w {
                let here = at(x, y);
                if x + 1 < w {
                    assert!((at(x + 1, y) - here).abs() <= 2.0, "an edge at {x}, {y}");
                }
                if y + 1 < h {
                    assert!((at(x, y + 1) - here).abs() <= 2.0, "an edge at {x}, {y}");
                }
            }
        }
        let floor = f32::from(shade::SKY_BOTTOM.get().g());
        assert!((smooth(middle, h - 1) - floor).abs() <= 1.0, "the floor at the foot");
        assert_eq!(stage_light(1.0), 0.0);
        assert_eq!(stage_light(0.0), 1.0);
    }

    #[test]
    fn tiny_covers_bake_without_gaps() {
        let out = bake(&ColorImage::filled([2, 3], Color32::from_rgb(10, 60, 90)));
        assert!(
            out.pixels.iter().all(|p| p.b() > 20),
            "no empty cell stays black"
        );
    }

    #[test]
    fn bake_day_lifts_dark_and_coloured_covers_to_pastel_alabaster_floor() {
        let black = bake_day(&ColorImage::filled([300, 450], Color32::BLACK));
        assert!(
            black
                .pixels
                .iter()
                .all(|p| luma(*p) >= t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA - 0.01),
            "black cover lifts cleanly to Day floor"
        );
        let cobalt = bake_day(&ColorImage::filled(
            [300, 450],
            Color32::from_rgb(20, 60, 210),
        ));
        let mid = cobalt.pixels[500];
        assert!(
            luma(mid) >= t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA - 0.01 && mid.b() > mid.r() + 18,
            "pastel chroma kept: {mid:?}"
        );
    }

    #[test]
    fn bake_hero_dissolves_high_contrast_lettering_into_smooth_wash() {
        let mut cover = ColorImage::filled([300, 450], Color32::from_rgb(20, 40, 120));
        // Sharp white horizontal/vertical bars simulating giant title lettering in the upper third.
        for y in 40..110 {
            for x in 30..270 {
                if (x / 12) % 2 == 0 || (y / 14) % 2 == 0 {
                    cover.pixels[y * 300 + x] = Color32::WHITE;
                }
            }
        }
        let night = bake_hero(&cover, false);
        let day = bake_hero(&cover, true);
        assert_eq!(night.size, HERO_SIZE);
        assert_eq!(day.size, HERO_SIZE);
        let [w, h] = HERO_SIZE;
        for y in 0..h {
            for x in 0..w {
                let p = night.pixels[y * w + x];
                assert!(luma(p) <= t::AMBIENT_TEXT_ZONE_MAX_LUMA + 0.004, "{p:?}");
                if x + 1 < w {
                    let next = night.pixels[y * w + x + 1];
                    let step = (i32::from(p.r()) - i32::from(next.r()))
                        .abs()
                        .max((i32::from(p.g()) - i32::from(next.g())).abs())
                        .max((i32::from(p.b()) - i32::from(next.b())).abs());
                    assert!(step <= 4, "no sharp letter edge survives at ({x},{y}): step={step}");
                }
                let dp = day.pixels[y * w + x];
                assert!(luma(dp) >= t::DAY_AMBIENT_TEXT_ZONE_MIN_LUMA - 0.01, "{dp:?}");
            }
        }
    }

    #[test]
    fn dark_covers_and_large_libraries_resolve_by_texture_id_without_collision_or_eviction() {
        let ctx = Context::default();
        let other_ctx = Context::default();
        // Two dark/monochrome covers with identical `glow == TRANSPARENT` but distinct top/bottom luminance
        // and >300 registered covers never cross-contaminate or evict live entries, nor leak across Contexts.
        let mut cover_a = ColorImage::filled([120, 180], Color32::BLACK);
        for p in &mut cover_a.pixels[..120 * 60] {
            *p = Color32::from_rgb(38, 38, 38);
        }
        let mut cover_b = ColorImage::filled([120, 180], Color32::BLACK);
        for p in &mut cover_b.pixels[120 * 120..] {
            *p = Color32::from_rgb(220, 40, 40);
        }
        let art_a = TextureId::Managed(0);
        let amb_a = TextureId::Managed(1);
        let art_b = TextureId::Managed(2);
        let amb_b = TextureId::Managed(3);
        register_cover(&ctx, Some(art_a), Some(amb_a), &cover_a);
        for idx in 0..300u64 {
            let filler = ColorImage::filled([24, 36], Color32::from_rgb((idx % 200) as u8, 10, 20));
            register_cover(
                &ctx,
                Some(TextureId::User(910_000 + idx * 2)),
                Some(TextureId::User(910_001 + idx * 2)),
                &filler,
            );
        }
        register_cover(&ctx, Some(art_b), Some(amb_b), &cover_b);

        // Separate `egui::Context` never sees `ctx`'s `Managed(0..1)` entries.
        assert!(day_for(&other_ctx, amb_a, Color32::TRANSPARENT).is_none());
        assert!(day_for(&ctx, TextureId::User(999_999), Color32::TRANSPARENT).is_none());
        assert!(hero_for(&ctx, TextureId::User(999_998), None, Some(Color32::TRANSPARENT), false).is_none());

        // `cover_a` survives >256 later registrations and keeps its neutral top-lit wash (not `cover_b`'s red bottom).
        let hero_a = hero_for(&ctx, art_a, Some(amb_a), Some(Color32::TRANSPARENT), false).expect("hero_a");
        let hero_b = hero_for(&ctx, art_b, Some(amb_b), Some(Color32::TRANSPARENT), false).expect("hero_b");
        let bottom_idx = HERO_SIZE[0] * (HERO_SIZE[1] - 8) + HERO_SIZE[0] / 2;
        assert!(
            hero_b.pixels[bottom_idx].r() > hero_b.pixels[bottom_idx].b() + 15,
            "cover_b keeps red bottom: {:?}",
            hero_b.pixels[bottom_idx]
        );
        assert!(
            (i32::from(hero_a.pixels[bottom_idx].r()) - i32::from(hero_a.pixels[bottom_idx].b())).abs() <= 2,
            "cover_a stays neutral rather than colliding with cover_b: {:?}",
            hero_a.pixels[bottom_idx]
        );
        assert!(day_for(&ctx, amb_a, Color32::TRANSPARENT).is_some(), "day_for survives >256 covers");

        // Even if `art_a` is dropped (`Art::Dropped`), `amb_a` still resolves `hero_for`.
        retain_covers(&ctx, |id| id == amb_a || id == art_b || id == amb_b);
        assert!(
            hero_for(&ctx, TextureId::User(888_888), Some(amb_a), None, true).is_some(),
            "retained ambient_id still resolves Day hero wash after cover texture drop"
        );
    }
}
