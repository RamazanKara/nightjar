//! What lies behind every page: a game's ambient, or on a page without art the stage, the page's
//! canvas under one wide, soft light from the top. Whatever shows cross-fades to the next. Chrome
//! samples the same textures in screen space.

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{
    pos2, vec2, Color32, Context, Pos2, Rect, Shape, TextureHandle, TextureId, TextureOptions,
};

use super::glass;
use crate::ambient;
use crate::theme::{look, shade, Look};

/// What the page wants behind it.
pub(super) enum Want {
    /// A cover's baked ambient, known by its key.
    Art {
        key: String,
        texture: TextureHandle,
        /// What its cover glows with, taken from the same bake.
        glow: Color32,
    },
    /// A game without art: its tile's gradient.
    Tile {
        title: String,
    },
    /// A cover still on its way, or a page that belongs to the last: what shows now stays.
    Keep,
    /// A page without art: the stage.
    Stage,
}

/// The ambient shows past the window's edges, so its soft edge is never seen.
const OVERSCAN: f32 = 0.14;
/// Where the page's own colour starts to rise over the backdrop, and where it nearly covers it.
const FADE_FROM: f32 = 0.30;
const FADE_NEAR: f32 = 0.58;
const FADE_NEAR_ALPHA: f32 = 0.95;
/// By day, fallback wash strength only when a synthetic test `Want::Art` has no Day bake.
/// Real covers and fallback tiles use [`ambient::bake_day`] and [`ambient::bake_tile_day`] directly.
const DAY_WASH: f32 = 0.65;
/// The stage light, in fractions of the window: its middle hangs above the top edge, so its
/// brightest part is never in view to read as a lamp, and it reaches wider than the window and
/// most of the way down it, so its end is never in view to read as a cone.
const LIGHT_AT: [f32; 2] = [0.5, -0.16];
const LIGHT_REACH: [f32; 2] = [0.78, 0.92];
/// How much of the token's glow the light has at its middle; on black it is fainter still, so the
/// page stays black.
const LIGHT_STRENGTH: f32 = 0.62;
const LIGHT_STRENGTH_OLED: f32 = 0.34;
/// How many fallback gradient tile textures are kept in memory at once.
const TILES_KEPT: usize = 32;
/// The veil where the page passes beneath the chrome: how far it reaches down the window, how far
/// below the capsules it stays whole (the frosted band), how much of the backdrop it brings behind
/// the brand, how far past the brand its pool goes and how far the page has scrolled when it is
/// whole. The least reach leaves the band an ease of 48 px under it.
const VEIL_FRACTION: f32 = 0.19;
const VEIL_LEAST: f32 = 126.0;
const VEIL_MOST: f32 = 190.0;
const BAND_BELOW: f32 = 10.0;
const BAND_ROWS: u32 = 4;
const VEIL_BRAND_ALPHA: f32 = 0.96;
const VEIL_BRAND_REACH: f32 = 54.0;
const VEIL_WHOLE_AT: f32 = 40.0;
const VEIL_ROWS: u32 = 24;
const VEIL_COLUMNS: u32 = 24;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shows {
    /// Baked ambient (Night or true Day bake): needs no canvas wash overlay.
    Art,
    /// Unbaked synthetic test texture shown in Day mode: falls back to [`DAY_WASH`].
    UnbakedDayArt,
    Stage,
}

#[derive(Clone, Default)]
enum LastWant {
    #[default]
    None,
    Art {
        key: String,
        texture: TextureHandle,
        glow: Color32,
    },
    Tile {
        title: String,
    },
}

#[derive(Default)]
pub(super) struct Ambient {
    fade: glass::CrossFade,
    shows: Option<Shows>,
    /// What showed before the fade now running, for fallback day wash over unbaked art to fade with it.
    before: Option<Shows>,
    key: u64,
    /// When the fade now running was asked for: a second change in that frame changes its mind, it
    /// does not fade from what never showed.
    shown_at: Option<f64>,
    tiles: HashMap<(String, bool), (TextureHandle, Color32)>,
    tile_order: VecDeque<(String, bool)>,
    day_art: HashMap<String, TextureHandle>,
    day_art_order: VecDeque<String>,
    last_want: LastWant,
    /// The stage, baked once for the look it shows in.
    stage: Option<(Look, TextureHandle)>,
    /// The cover's glow this frame, the one it fades to with what shows, and the one it fades from.
    glow: Color32,
    glow_to: Color32,
    glow_before: Color32,
}

fn hash_of(value: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn full_uv() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

/// The stage in `look`'s colours, for the overscanned area every backdrop layer covers.
fn stage_bake(look: Look) -> egui::ColorImage {
    let span = 1.0 + 2.0 * OVERSCAN;
    let light = LIGHT_AT.map(|at| (at + OVERSCAN) / span);
    let reach = LIGHT_REACH.map(|reach| reach / span);
    let strength = if look == Look::Oled {
        LIGHT_STRENGTH_OLED
    } else {
        LIGHT_STRENGTH
    };
    ambient::bake_stage(light, reach, strength)
}

fn layer_mesh(texture: TextureId, area: Rect, alpha: f32) -> Shape {
    let mut mesh = Mesh::with_texture(texture);
    mesh.add_rect_with_uv(area, full_uv(), Color32::WHITE.gamma_multiply(alpha));
    Shape::mesh(mesh)
}

impl Ambient {
    fn show(
        &mut self,
        ctx: &Context,
        key: u64,
        texture: TextureHandle,
        shows: Shows,
        glow: Color32,
    ) {
        if self.shows.is_some() && self.key == key {
            return;
        }
        let now = ctx.input(|i| i.time);
        if self.shown_at != Some(now) {
            self.before = self.shows;
            self.glow_before = self.glow;
            self.shown_at = Some(now);
        }
        self.shows = Some(shows);
        self.key = key;
        self.glow_to = glow;
        self.fade.show(ctx, key, texture);
    }

    /// A new renderer (Linux's switch to the GPU renderer): the bakes went with the old one and are
    /// made again as they are asked for; what the page wants shows again on the next frame.
    pub(super) fn renderer_changed(&mut self) {
        self.tiles.clear();
        self.tile_order.clear();
        self.day_art.clear();
        self.day_art_order.clear();
        self.last_want = LastWant::None;
        self.stage = None;
        self.fade = glass::CrossFade::default();
        self.shows = None;
        self.before = None;
    }

    /// Whether a game's ambient shows, for a test.
    #[cfg(test)]
    pub(super) fn shows_art(&self) -> bool {
        matches!(self.shows, Some(Shows::Art | Shows::UnbakedDayArt))
    }

    /// The cover's glow this frame, fading as the ambient does. Nothing on the stage and in high
    /// contrast.
    pub(super) fn glow(&self) -> Color32 {
        self.glow
    }

    fn day_art_texture(
        &mut self,
        ctx: &Context,
        key: &str,
        night: &TextureHandle,
        glow: Color32,
    ) -> Option<TextureHandle> {
        if let Some(cached) = self.day_art.get(key) {
            return Some(cached.clone());
        }
        let baked = ambient::day_for(night.id(), glow)?;
        while self.day_art.len() >= TILES_KEPT {
            let Some(old) = self.day_art_order.pop_front() else {
                self.day_art.clear();
                break;
            };
            self.day_art.remove(&old);
        }
        let handle = ctx.load_texture(
            format!("ambient-day-{key}"),
            baked,
            TextureOptions::LINEAR,
        );
        self.day_art_order.push_back(key.to_owned());
        self.day_art.insert(key.to_owned(), handle.clone());
        Some(handle)
    }

    fn tile(&mut self, ctx: &Context, title: &str, day: bool) -> (TextureHandle, Color32) {
        let cache_key = (title.to_owned(), day);
        if let Some(cached) = self.tiles.get(&cache_key) {
            return cached.clone();
        }
        while self.tiles.len() >= TILES_KEPT {
            let Some(old) = self.tile_order.pop_front() else {
                self.tiles.clear();
                break;
            };
            self.tiles.remove(&old);
        }
        let pair = nightjar_policy::tile::tile_pair(nightjar_policy::tile::title_hash(title));
        let (top, bottom) = super::cards::tile_colors(pair);
        let night_baked = ambient::bake_tile(top, bottom);
        let glow = ambient::glow_of(&night_baked);
        let baked = if day {
            ambient::bake_tile_day(top, bottom)
        } else {
            night_baked
        };
        let entry = (
            ctx.load_texture("ambient-tile", baked, TextureOptions::LINEAR),
            glow,
        );
        self.tile_order.push_back(cache_key.clone());
        self.tiles.insert(cache_key, entry.clone());
        entry
    }

    fn show_art(&mut self, ctx: &Context, key: String, night: TextureHandle, glow: Color32) {
        let is_day = look() == Look::Day;
        self.last_want = LastWant::Art {
            key: key.clone(),
            texture: night.clone(),
            glow,
        };
        if is_day {
            if let Some(day_tex) = self.day_art_texture(ctx, &key, &night, glow) {
                self.show(ctx, hash_of(("art-day", &key)), day_tex, Shows::Art, glow);
            } else {
                self.show(
                    ctx,
                    hash_of(("art-unbaked-day", &key)),
                    night,
                    Shows::UnbakedDayArt,
                    glow,
                );
            }
        } else {
            self.show(ctx, hash_of(("art", &key)), night, Shows::Art, glow);
        }
    }

    fn show_tile(&mut self, ctx: &Context, title: String) {
        let is_day = look() == Look::Day;
        self.last_want = LastWant::Tile {
            title: title.clone(),
        };
        let (texture, glow) = self.tile(ctx, &title, is_day);
        self.show(
            ctx,
            hash_of(("tile", &title, is_day)),
            texture,
            Shows::Art,
            glow,
        );
    }

    /// Shows the stage: baked for the same overscanned area as an ambient, so one fades into the
    /// other, once for the look it shows in. It is the same on every page, so going from one page
    /// without art to the next changes nothing behind them.
    fn show_stage(&mut self, ctx: &Context) {
        self.last_want = LastWant::None;
        let look = look();
        let texture = match &self.stage {
            Some((baked, texture)) if *baked == look => texture.clone(),
            _ => {
                let texture = ctx.load_texture("stage", stage_bake(look), TextureOptions::LINEAR);
                self.stage = Some((look, texture.clone()));
                texture
            }
        };
        self.show(ctx, hash_of(("stage", look)), texture, Shows::Stage, Color32::TRANSPARENT);
    }

    /// The backdrop's shapes this frame, all at once, for a test (the launcher goes through
    /// [`Self::prepare`] and [`Self::finish`]).
    #[cfg(test)]
    pub(super) fn paint(&mut self, ctx: &Context, want: Want) -> Vec<Shape> {
        self.prepare(ctx, want);
        self.finish(ctx)
    }

    /// Takes what the page wants behind it, before the page is drawn, and tells glass what to
    /// sample. [`Self::finish`] paints it once the page is drawn, under it.
    pub(super) fn prepare(&mut self, ctx: &Context, want: Want) {
        if crate::theme::high_contrast() {
            // Ambient and stage light are off: black, and solid glass needs no backdrop.
            glass::set_backdrop(ctx, glass::Backdrop::default());
            self.glow = Color32::TRANSPARENT;
            return;
        }
        match want {
            Want::Art { key, texture, glow } => self.show_art(ctx, key, texture, glow),
            Want::Tile { title } => self.show_tile(ctx, title),
            Want::Keep if matches!(self.shows, Some(Shows::Art | Shows::UnbakedDayArt)) => {
                match self.last_want.clone() {
                    LastWant::Art { key, texture, glow } => self.show_art(ctx, key, texture, glow),
                    LastWant::Tile { title } => self.show_tile(ctx, title),
                    LastWant::None => {}
                }
            }
            // Nothing to keep yet: the stage rather than a bare canvas. A stage that is kept is
            // shown again, for the look may have changed under it.
            Want::Keep | Want::Stage => self.show_stage(ctx),
        }
        self.layers(ctx, ctx.content_rect());
    }

    /// This frame's layers, handed to glass, with the cover's glow along them.
    fn layers(&mut self, ctx: &Context, screen: Rect) -> glass::Backdrop {
        let area = screen.expand2(screen.size() * OVERSCAN);
        let backdrop = self.fade.backdrop(ctx, area);
        let along = backdrop.top.map_or(1.0, |top| top.alpha);
        self.glow = if backdrop.base.is_some() {
            self.glow_before.lerp_to_gamma(self.glow_to, along)
        } else {
            self.glow_to
        };
        backdrop
    }

    /// Share of unbaked synthetic art in Day mode (0.0 for all real Day-baked covers and tiles).
    fn unbaked_day_share(&self, behind: &glass::Backdrop) -> f32 {
        let is_unbaked =
            |shows: Option<Shows>| f32::from(u8::from(shows == Some(Shows::UnbakedDayArt)));
        match (behind.base, behind.top) {
            (Some(_), Some(top)) => {
                egui::lerp(is_unbaked(self.before)..=is_unbaked(self.shows), top.alpha)
            }
            _ => is_unbaked(self.shows),
        }
    }

    /// The backdrop's shapes, once the page [`Self::prepare`] was for is drawn. In high contrast
    /// plain black: no ambient and no light.
    pub(super) fn finish(&mut self, ctx: &Context) -> Vec<Shape> {
        let screen = ctx.content_rect();
        let mut out = Vec::new();
        if crate::theme::high_contrast() {
            out.push(Shape::rect_filled(screen, 0.0, *crate::theme::NIGHT));
            return out;
        }
        out.push(Shape::rect_filled(screen, 0.0, shade::COLOR_CANVAS.get()));
        let backdrop = self.layers(ctx, screen);
        if let Some(base) = backdrop.base {
            out.push(layer_mesh(base.texture, base.area, 1.0));
        }
        if let Some(top) = backdrop.top {
            out.push(layer_mesh(top.texture, top.area, top.alpha));
        }
        if let Some(wash) = day_wash(screen, self.unbaked_day_share(&backdrop)) {
            out.push(wash);
        }
        // The page's colour rises over art and stage alike, so it holds still through a fade
        // between them.
        out.push(fade_over(screen));
        #[cfg(test)]
        ctx.data_mut(|d| {
            d.insert_temp(
                painted_id(),
                Painted {
                    along: backdrop.top.map_or(1.0, |top| top.alpha),
                    fading: backdrop.base.is_some(),
                },
            )
        });
        out
    }
}

mod veil;
use veil::{day_wash, fade_over};
#[cfg(test)]
pub(super) use veil::{band_foot, painted, Painted};
#[cfg(test)]
use veil::{painted_id, veil_at, veil_height};

#[cfg(test)]
mod tests;
