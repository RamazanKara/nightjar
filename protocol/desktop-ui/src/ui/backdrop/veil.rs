//! What the backdrop lays over itself and over the page: the page colour rising toward the foot,
//! the day's wash over art, and the veil a scrolled page dissolves into beneath the chrome.

use super::*;

/// What the last frame painted of the backdrop, for a test to follow a fade: how far the fade is,
/// and whether one runs.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::ui) struct Painted {
    pub along: f32,
    pub fading: bool,
}

#[cfg(test)]
pub(super) fn painted_id() -> egui::Id {
    egui::Id::unique("backdrop-painted")
}

#[cfg(test)]
pub(in crate::ui) fn painted(ctx: &Context) -> Painted {
    ctx.data(|d| d.get_temp(painted_id())).unwrap_or(Painted {
        along: 1.0,
        fading: false,
    })
}

pub(super) fn smoothstep(from: f32, to: f32, x: f32) -> f32 {
    let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Where the frosted band under the chrome ends: a little below the capsules. Down to here the page
/// never shows, so nothing reads between the capsules or behind the brand.
pub(in crate::ui) fn band_foot(screen: Rect) -> f32 {
    screen.top() + super::super::chrome::BAR_TOP + super::super::chrome::BAR_H + BAND_BELOW
}

/// How much of the backdrop lies over the page at `p`, with the page fully scrolled: all of it in
/// the band behind the chrome, then a soft ease to nothing, and a pool behind the brand, which has
/// no glass of its own.
pub(super) fn veil_at(screen: Rect, brand: Rect, p: Pos2) -> f32 {
    let foot = screen.top() + veil_height(screen);
    let ramp = 1.0 - smoothstep(band_foot(screen), foot, p.y);
    let reach = brand.size() / 2.0 + vec2(VEIL_BRAND_REACH, VEIL_BRAND_REACH * 0.7);
    let away = (p - brand.center()) / reach;
    let pool = VEIL_BRAND_ALPHA * (1.0 - smoothstep(0.5, 1.0, away.length()));
    ramp.max(pool)
}

/// The ramp ends above where the page colour starts to rise, so the veil never has to bring that
/// back too.
pub(super) fn veil_height(screen: Rect) -> f32 {
    (screen.height() * VEIL_FRACTION).clamp(VEIL_LEAST, VEIL_MOST)
}

impl Ambient {
    /// Where the page passes beneath the chrome it dissolves into the backdrop: the same textures in
    /// the same place, laid over the page, so with nothing passing there it cannot be seen at all.
    /// Behind the capsules it is whole, a frosted band, so nothing shows between them; below them it
    /// eases away, never a hard edge. `brand` keeps its words readable. None with the page at its
    /// start.
    pub(in crate::ui) fn veil(
        &self,
        ctx: &Context,
        screen: Rect,
        brand: Rect,
        scrolled: f32,
    ) -> Vec<Shape> {
        let strength = (scrolled / VEIL_WHOLE_AT).min(1.0);
        if strength <= 0.0 {
            return Vec::new();
        }
        let alpha = |p: Pos2| veil_at(screen, brand, p) * strength;
        let foot = screen.top() + veil_height(screen).max(brand.bottom() + VEIL_BRAND_REACH);
        // A few rows through the band, which is level, and rows close enough for the ease below it;
        // columns only where the pool bends.
        let band = band_foot(screen).min(foot);
        let rows: Vec<f32> = (0..BAND_ROWS)
            .map(|i| egui::lerp(screen.top()..=band, i as f32 / BAND_ROWS as f32))
            .chain((0..=VEIL_ROWS).map(|i| egui::lerp(band..=foot, i as f32 / VEIL_ROWS as f32)))
            .collect();
        let mut columns: Vec<f32> = (0..=VEIL_COLUMNS)
            .map(|i| {
                let end = brand.right() + 2.0 * VEIL_BRAND_REACH;
                egui::lerp(screen.left()..=end, i as f32 / VEIL_COLUMNS as f32)
            })
            .collect();
        columns.push(screen.right());
        let grid = |texture: TextureId, area: Option<Rect>, layer: f32, plain: Color32| {
            let mut mesh = Mesh::with_texture(texture);
            for y in &rows {
                for x in &columns {
                    let pos = pos2(*x, *y);
                    let uv = area.map_or(WHITE_UV, |a| {
                        pos2(
                            (pos.x - a.min.x) / a.width(),
                            (pos.y - a.min.y) / a.height(),
                        )
                    });
                    mesh.vertices.push(Vertex {
                        pos,
                        uv,
                        color: plain.gamma_multiply(alpha(pos) * layer),
                    });
                }
            }
            let across = columns.len() as u32;
            for row in 0..rows.len() as u32 - 1 {
                for column in 0..across - 1 {
                    let a = row * across + column;
                    mesh.indices
                        .extend([a, a + 1, a + across + 1, a, a + across + 1, a + across]);
                }
            }
            Shape::mesh(mesh)
        };
        if crate::theme::high_contrast() {
            return vec![grid(TextureId::default(), None, 1.0, *crate::theme::NIGHT)];
        }
        let behind = self.fade.layers(ctx);
        let mut out = Vec::new();
        if behind.base.is_none() && behind.top.is_none() {
            out.push(grid(TextureId::default(), None, 1.0, shade::COLOR_CANVAS.get()));
        }
        for layer in [behind.base, behind.top].into_iter().flatten() {
            out.push(grid(
                layer.texture,
                Some(layer.area),
                layer.alpha,
                Color32::WHITE,
            ));
        }
        // Fallback wash only for synthetic unbaked textures in Day mode; true Day-baked covers
        // and tiles (`Shows::Art`) carry lifted Alabaster colours directly.
        let unbaked = self.unbaked_day_share(&behind);
        if look() == Look::Day && unbaked > 0.0 {
            out.push(grid(
                TextureId::default(),
                None,
                DAY_WASH * unbaked,
                shade::COLOR_CANVAS.get(),
            ));
        }
        out
    }
}

/// Fallback Day wash only when `unbaked > 0.0` (synthetic textures without [`ambient::bake_day`]).
pub(super) fn day_wash(screen: Rect, unbaked: f32) -> Option<Shape> {
    (look() == Look::Day && unbaked > 0.0).then(|| {
        Shape::rect_filled(
            screen,
            0.0,
            shade::COLOR_CANVAS.get().gamma_multiply(DAY_WASH * unbaked),
        )
    })
}

/// The page's colour rising over the backdrop toward the window's foot, so shelves and rows sit on
/// calm ground whatever shows above them.
pub(super) fn fade_over(screen: Rect) -> Shape {
    let mut mesh = Mesh::default();
    let row = |mesh: &mut Mesh, y: f32, color: Color32| {
        for x in [screen.left(), screen.right()] {
            mesh.vertices.push(Vertex {
                pos: pos2(x, y),
                uv: WHITE_UV,
                color,
            });
        }
    };
    let at = |f: f32| screen.top() + screen.height() * f;
    row(&mut mesh, at(FADE_FROM), Color32::TRANSPARENT);
    row(
        &mut mesh,
        at(FADE_NEAR),
        shade::TINT_FADE.get().gamma_multiply(FADE_NEAR_ALPHA),
    );
    row(&mut mesh, at(1.0), shade::TINT_FADE.get());
    for r in 0..2u32 {
        let a = r * 2;
        mesh.indices.extend([a, a + 1, a + 3, a, a + 3, a + 2]);
    }
    Shape::mesh(mesh)
}
