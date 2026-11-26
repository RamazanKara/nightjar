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
/// `recipe` is the surface's: the band shows the backdrop at its brightness times the band's, and
/// lets through what the fill lets through, fading out toward the inside.
pub fn paint(out: &mut Vec<Shape>, ctx: &Context, rect: egui::Rect, r: f32, material: Material, tier: Tier, recipe: &Recipe) {
    let Some((width, brightness)) = band(material, tier) else { return };
    let width = width.min(rect.width() / 2.0).min(rect.height() / 2.0);
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
    let through = 1.0 - f32::from(recipe.fill.a()) / 255.0;
    let gray = (recipe.brightness * brightness).clamp(0.0, 1.0);
    let edge = {
        let a = through.clamp(0.0, 1.0);
        let v = (gray * a * 255.0).round() as u8;
        Color32::from_rgba_premultiplied(v, v, v, (a * 255.0).round() as u8)
    };
    // Inside the surface's own feathered edge, which stays the fill's.
    let feather = 1.0 / ppp;
    let mut mesh = Mesh::with_texture(LIVE_BACKDROP);
    let nodes = outline(rect, r, ppp);
    for node in &nodes {
        let outer = node.c + node.n * (r - feather / 2.0);
        let inner = node.c + node.n * (r - width);
        mesh.vertices.push(Vertex { pos: outer, uv: uv(node.c + node.n * (r - REACH * width)), color: edge });
        mesh.vertices.push(Vertex { pos: inner, uv: uv(inner), color: Color32::TRANSPARENT });
    }
    let count = nodes.len() as u32;
    for i in 0..count {
        let (a, b) = (i * 2, (i + 1) % count * 2);
        mesh.indices.extend([a, b, b + 1, a, b + 1, a + 1]);
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
}
