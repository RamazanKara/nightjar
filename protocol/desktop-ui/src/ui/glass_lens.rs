//! The lens tier's refracted edge band on the desktop (`client/design/README.md`, "Glass recipe"):
//! a ring of quads along the outer `lensWidth` of a glass shape, sampling the live chain's output
//! ([`LIVE_BACKDROP`]) with displaced UVs and brightened by `lensBrightness`. No shader: the same
//! screen-space meshes the live tier draws, so it costs one more ring per surface and no pass.
//!
//! Only on [`Tier::Lens`], and only for a material whose tokens give it a band (`lensWidth > 0`:
//! chrome and panel). It lies over the fill and under the sheen and rim, as the recipe orders them.

use egui::epaint::{Mesh, Vertex};
use egui::{pos2, Color32, Context, Pos2, Shape};

use super::design_tokens as t;
use super::glass::{fit_radius, outline, ppp, Material, Recipe, Tier, LIVE_BACKDROP};
use crate::theme::{look, Look};

/// How far in the band's outer edge reads, in band widths: content from deeper inside, mirrored,
/// as an edge of glass bends what lies under it.
const REACH: f32 = 2.0;
/// The band on the desktop is this many `lensWidth`s wide. Android's shader bends light over its
/// `lensWidth`; quads with no per-pixel falloff need a wider ramp to read as the same edge.
pub const WIDTH_FACTOR: f32 = 2.25;
/// The band's opacity at its brightest, at the top edge: what it shows of the undimmed backdrop
/// over the dim glass. It does not follow the fill, so the band reads on dense and light glass alike.
pub const EDGE_STRENGTH: f32 = 0.6;
/// Down the sides and along the bottom the band keeps this share of its top strength: the edge is
/// lit from the top, as the rim is.
pub const SIDE_SHARE: f32 = 0.45;
/// Where across the band its peak sits, from the outer edge: the outermost pixels stay a little
/// softer, so the band never draws as a stroke.
const PEAK_AT: f32 = 0.3;
/// The outer edge's share of the peak.
const OUTER_SHARE: f32 = 0.7;

/// The band of `material` on `tier`: its width in points and its brightness, `None` where its
/// tokens give it none.
pub fn band(material: Material, tier: Tier) -> Option<(f32, f32)> {
    if tier != Tier::Lens {
        return None;
    }
    let day = look() == Look::Day;
    let (width, brightness) = match (material, day) {
        (Material::Chrome, false) => (t::GLASS_CHROME_LENS_LENS_WIDTH, t::GLASS_CHROME_LENS_LENS_BRIGHTNESS),
        (Material::Chrome, true) => (t::DAY_GLASS_CHROME_LENS_LENS_WIDTH, t::DAY_GLASS_CHROME_LENS_LENS_BRIGHTNESS),
        (Material::Panel, false) => (t::GLASS_PANEL_LENS_LENS_WIDTH, t::GLASS_PANEL_LENS_LENS_BRIGHTNESS),
        (Material::Panel, true) => (t::DAY_GLASS_PANEL_LENS_LENS_WIDTH, t::DAY_GLASS_PANEL_LENS_LENS_BRIGHTNESS),
        (Material::Menu, _) => (t::GLASS_MENU_LENS_LENS_WIDTH, t::GLASS_MENU_LENS_LENS_BRIGHTNESS),
        (Material::Chip, false) => (t::GLASS_CHIP_LENS_LENS_WIDTH, t::GLASS_CHIP_LENS_LENS_BRIGHTNESS),
        (Material::Chip, true) => (t::DAY_GLASS_CHIP_LENS_LENS_WIDTH, t::DAY_GLASS_CHIP_LENS_LENS_BRIGHTNESS),
    };
    (width > 0.0).then_some((width, brightness))
}

/// Adds the band of `material` just inside `rect`'s rounded outline, if it has one on `tier`.
/// The band is the live backdrop undimmed, times `lensBrightness` (clamped to what a vertex colour
/// carries), sampled mirrored from deeper inside and laid over the fill at up to [`EDGE_STRENGTH`].
/// Over bright or colourful art it is clearly lighter than the interior, which shows the backdrop
/// at the recipe's brightness under the fill; over dark art it all but vanishes. It is strongest at
/// the top, rises from the outer edge to a peak, fades out inward and stays inside the shape. The
/// recipe is the surface's; the band does not follow its fill.
pub fn paint(out: &mut Vec<Shape>, ctx: &Context, rect: egui::Rect, r: f32, material: Material, tier: Tier, _recipe: &Recipe) {
    let Some((width, brightness)) = band(material, tier) else { return };
    let width = (width * WIDTH_FACTOR).min(rect.width() / 2.0).min(rect.height() / 2.0);
    if width <= 0.0 {
        return;
    }
    let ppp = ppp(ctx);
    let r = fit_radius(rect, r);
    let screen = ctx.content_rect();
    let uv = |p: Pos2| {
        pos2(
            (p.x - screen.min.x) / screen.width().max(1e-3),
            (p.y - screen.min.y) / screen.height().max(1e-3),
        )
    };
    let gray = brightness.clamp(0.0, 1.0);
    let shade = |alpha: f32| {
        let a = alpha.clamp(0.0, 1.0);
        let v = (gray * a * 255.0).round() as u8;
        Color32::from_rgba_premultiplied(v, v, v, (a * 255.0).round() as u8)
    };
    // Inside the surface's own feathered edge, which stays the fill's.
    let feather = 1.0 / ppp;
    // Across the band, from the outer edge in: (depth, share of the peak).
    let rows = [(feather, OUTER_SHARE), ((PEAK_AT * width).max(1.5 * feather), 1.0), (width, 0.0)];
    // The mirror: the outer edge reads `REACH` widths in, the inner edge reads itself.
    let sample = |depth: f32| REACH * width + (1.0 - REACH) * depth;
    let mut mesh = Mesh::with_texture(LIVE_BACKDROP);
    let nodes = outline(rect, r, ppp);
    for node in &nodes {
        let lit = SIDE_SHARE + (1.0 - SIDE_SHARE) * (-node.n.y).max(0.0);
        for (depth, share) in rows {
            let pos = node.c + node.n * (r - depth);
            let at = node.c + node.n * (r - sample(depth));
            mesh.vertices.push(Vertex { pos, uv: uv(at), color: shade(EDGE_STRENGTH * lit * share) });
        }
    }
    let count = nodes.len() as u32;
    for i in 0..count {
        let (a, b) = (i * 3, (i + 1) % count * 3);
        for k in 0..2 {
            mesh.indices.extend([a + k, b + k, b + k + 1, a + k, b + k + 1, a + k + 1]);
        }
    }
    out.push(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_chrome_and_panel_have_a_band_and_only_on_lens() {
        crate::theme::own_palette();
        for material in [Material::Chrome, Material::Panel] {
            let (width, brightness) = band(material, Tier::Lens).expect("a band");
            assert!(width > 0.0 && brightness > 1.0, "{material:?}");
            for tier in [Tier::Live, Tier::Baked, Tier::Solid] {
                assert_eq!(band(material, tier), None, "{material:?} on {tier:?}");
            }
        }
        assert_eq!(band(Material::Menu, Tier::Lens), None);
        assert_eq!(band(Material::Chip, Tier::Lens), None);
    }

    #[test]
    fn the_band_samples_the_live_chain_inside_the_shape() {
        crate::theme::own_palette();
        let ctx = Context::default();
        let rect = egui::Rect::from_min_size(pos2(40.0, 20.0), egui::vec2(400.0, 48.0));
        let recipe = super::super::glass::recipe(Material::Chrome, Tier::Lens);
        let mut out = Vec::new();
        paint(&mut out, &ctx, rect, 24.0, Material::Chip, Tier::Lens, &recipe);
        paint(&mut out, &ctx, rect, 24.0, Material::Chrome, Tier::Live, &recipe);
        assert!(out.is_empty(), "no band for a chip, none on live");
        paint(&mut out, &ctx, rect, 24.0, Material::Chrome, Tier::Lens, &recipe);
        let [Shape::Mesh(mesh)] = out.as_slice() else { panic!("one ring: {out:?}") };
        assert_eq!(mesh.texture_id, LIVE_BACKDROP);
        assert!(mesh.vertices.iter().all(|v| rect.expand(0.01).contains(v.pos)), "inside the shape");
        assert!(mesh.vertices.iter().any(|v| v.color.a() > 0), "the outer edge shows");
    }

    #[test]
    fn the_band_is_brighter_and_more_opaque_at_the_edge_than_inside_and_strongest_on_top() {
        crate::theme::own_palette();
        let ctx = Context::default();
        let rect = egui::Rect::from_min_size(pos2(100.0, 100.0), egui::vec2(400.0, 300.0));
        let recipe = super::super::glass::recipe(Material::Panel, Tier::Lens);
        let mut out = Vec::new();
        paint(&mut out, &ctx, rect, 24.0, Material::Panel, Tier::Lens, &recipe);
        let [Shape::Mesh(mesh)] = out.as_slice() else { panic!("one ring") };
        // Three rows a node: the outer edge, the peak, the inner edge.
        let rows: Vec<_> = mesh.vertices.chunks(3).collect();
        for row in &rows {
            let (outer, peak, inner) = (row[0].color, row[1].color, row[2].color);
            assert!(outer.a() > inner.a() && outer.r() > inner.r(), "{outer:?} over {inner:?}");
            assert!(peak.a() >= outer.a(), "a soft outer edge, no stroke");
            assert_eq!(inner, Color32::TRANSPARENT, "it fades into the interior");
        }
        let top = rows.iter().find(|row| (row[0].pos.y - rect.top()).abs() < 2.0 && (row[0].pos.x - rect.center().x).abs() < 60.0).unwrap();
        let side = rows.iter().find(|row| (row[0].pos.x - rect.left()).abs() < 2.0 && (row[0].pos.y - rect.center().y).abs() < 60.0).unwrap();
        assert!(top[1].color.a() > side[1].color.a(), "lit from the top");
        assert!(f32::from(top[1].color.a()) / 255.0 > 0.5, "over the fill, whatever its alpha");
    }
}
