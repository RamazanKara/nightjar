//! Pixel bounds of tessellated stream-overlay primitives, so software KMS planes clear and copy
//! only the area egui drew instead of the whole 4K surface.

use egui::ClippedPrimitive;

/// Pixel bounding box `[x0, y0, x1, y1]` (`x0 < x1`, `y0 < y1`) covering everything [`SdlStage`]
/// can rasterize for `primitives` at `ppp` pixels per point, or `None` when nothing draws.
pub(super) fn primitive_bounds(primitives: &[ClippedPrimitive], ppp: f32) -> Option<[i32; 4]> {
    if !ppp.is_finite() || ppp <= 0.0 {
        return None;
    }
    let mut acc: Option<[i32; 4]> = None;
    for ClippedPrimitive { clip_rect, primitive } in primitives {
        if !clip_rect.min.x.is_finite()
            || !clip_rect.min.y.is_finite()
            || !clip_rect.max.x.is_finite()
            || !clip_rect.max.y.is_finite()
        {
            continue;
        }
        let clip_x = (clip_rect.min.x * ppp).round() as i32;
        let clip_y = (clip_rect.min.y * ppp).round() as i32;
        let clip_w = ((clip_rect.max.x - clip_rect.min.x) * ppp).round() as i32;
        let clip_h = ((clip_rect.max.y - clip_rect.min.y) * ppp).round() as i32;
        if clip_w <= 0 || clip_h <= 0 {
            continue;
        }
        let clip = [
            clip_x.saturating_sub(1),
            clip_y.saturating_sub(1),
            clip_x.saturating_add(clip_w).saturating_add(1),
            clip_y.saturating_add(clip_h).saturating_add(1),
        ];
        let rect = match primitive {
            egui::epaint::Primitive::Mesh(mesh) => {
                let Some(raw) = mesh_bounds(mesh, ppp) else { continue };
                let Some(clipped) = intersect(raw, clip) else { continue };
                clipped
            }
            egui::epaint::Primitive::Callback(_) => clip,
        };
        acc = Some(match acc {
            Some(prev) => union(prev, rect),
            None => rect,
        });
    }
    acc
}

fn mesh_bounds(mesh: &egui::epaint::Mesh, ppp: f32) -> Option<[i32; 4]> {
    if mesh.indices.is_empty() || mesh.vertices.is_empty() {
        return None;
    }
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for v in &mesh.vertices {
        let (x, y) = (v.pos.x * ppp, v.pos.y * ppp);
        if !x.is_finite() || !y.is_finite() {
            continue;
        }
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let x0 = (min_x.floor() as i32).saturating_sub(2);
    let y0 = (min_y.floor() as i32).saturating_sub(2);
    let x1 = (max_x.ceil() as i32).saturating_add(2);
    let y1 = (max_y.ceil() as i32).saturating_add(2);
    (x0 < x1 && y0 < y1).then_some([x0, y0, x1, y1])
}

fn intersect(a: [i32; 4], b: [i32; 4]) -> Option<[i32; 4]> {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

fn union(a: [i32; 4], b: [i32; 4]) -> [i32; 4] {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

#[cfg(test)]
mod tests {
    use super::primitive_bounds;
    use egui::epaint::{ClippedPrimitive, Color32, Mesh, Primitive, Vertex};
    use egui::{Pos2, Rect};

    fn quad(min: (f32, f32), max: (f32, f32)) -> Mesh {
        let mut mesh = Mesh::default();
        let uv = Pos2::ZERO;
        let c = Color32::WHITE;
        mesh.vertices = vec![
            Vertex { pos: Pos2::new(min.0, min.1), uv, color: c },
            Vertex { pos: Pos2::new(max.0, min.1), uv, color: c },
            Vertex { pos: Pos2::new(max.0, max.1), uv, color: c },
            Vertex { pos: Pos2::new(min.0, max.1), uv, color: c },
        ];
        mesh.indices = vec![0, 1, 2, 0, 2, 3];
        mesh
    }

    #[test]
    fn empty_primitives_yield_no_bounds() {
        assert_eq!(primitive_bounds(&[], 2.0), None);
        let empty = ClippedPrimitive {
            clip_rect: Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1080.0)),
            primitive: Primitive::Mesh(Mesh::default()),
        };
        assert_eq!(primitive_bounds(&[empty], 2.0), None);
    }

    #[test]
    fn corner_capsule_stays_tight_inside_fullscreen_clip_rect() {
        let capsule = ClippedPrimitive {
            clip_rect: Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1080.0)),
            primitive: Primitive::Mesh(quad((16.0, 16.0), (240.0, 42.0))),
        };
        let bounds = primitive_bounds(&[capsule], 2.0).expect("non-empty capsule");
        assert_eq!(bounds, [30, 30, 482, 86]);
    }

    #[test]
    fn clip_rect_truncates_oversized_mesh_and_unions_multiple_elements() {
        let clipped = ClippedPrimitive {
            clip_rect: Rect::from_min_max(Pos2::new(20.0, 20.0), Pos2::new(100.0, 50.0)),
            primitive: Primitive::Mesh(quad((0.0, 0.0), (500.0, 500.0))),
        };
        let toast = ClippedPrimitive {
            clip_rect: Rect::from_min_max(Pos2::ZERO, Pos2::new(1920.0, 1080.0)),
            primitive: Primitive::Mesh(quad((800.0, 980.0), (1120.0, 1020.0))),
        };
        assert_eq!(primitive_bounds(std::slice::from_ref(&clipped), 1.0), Some([19, 19, 101, 51]));
        assert_eq!(primitive_bounds(&[clipped, toast], 1.0), Some([19, 19, 1122, 1022]));
    }
}
