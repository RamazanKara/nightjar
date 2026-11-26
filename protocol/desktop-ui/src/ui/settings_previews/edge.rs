//! The Upscaling picture: the edge of something bright, enlarged over the stream's pixels (the
//! grid). The plain enlargement leaves it soft, a blend over more than a pixel; the upscalers that
//! sharpen narrow it as far as their sharpness goes, and too much of it lights a halo along the
//! edge.

use super::*;

/// The grid's rows: the stream's pixels as the picture enlarges them.
const ROWS: f32 = 5.0;
/// How wide the edge's blend is, in stream pixels, as enlarged plainly and as sharp as it gets.
pub(super) const SOFT: f32 = 1.5;
pub(super) const CRISP: f32 = 0.12;
/// The sharpness from which a halo starts to show.
const HALO_FROM: f32 = 0.7;

/// The blend's width in stream pixels, then how strong the halo is (0 to 1).
pub(super) fn pose(upscaling: Upscaling, sharpness: f32) -> Pose {
    let sharpness = sharpness.clamp(0.0, 1.0);
    let mut pose = [0.0; POSE];
    (pose[0], pose[1]) = if upscaling.is_shader() {
        (1.1 + (CRISP - 1.1) * sharpness, ((sharpness - HALO_FROM) / (1.0 - HALO_FROM)).max(0.0))
    } else if upscaling == Upscaling::Bilinear {
        (SOFT, 0.0)
    } else {
        // The vendor's own enhancement: sharp, with no strength of ours to set.
        (0.3, 0.0)
    };
    pose
}

pub(super) fn paint(pose: &Pose, rect: Rect, ink: &Ink, out: &mut Vec<Shape>) {
    let cell_h = rect.height() / ROWS;
    let columns = (rect.width() / cell_h).round().max(1.0);
    let cell_w = rect.width() / columns;
    let blend = pose[0].max(0.02) * cell_h;
    let halo = pose[1].clamp(0.0, 1.0);
    // The edge runs through the middle, rising to the right; the bright side is under it.
    let along = vec2(0.84, -0.54).normalized();
    let across = vec2(-along.y, along.x);
    let far = rect.width() + rect.height();
    let at = |run: f32, off: f32| rect.center() + along * run + across * off;
    let grid = |out: &mut Vec<Shape>| {
        // The window onto the pixels, then the pixels.
        out.push(Shape::rect_stroke(rect, CornerRadius::ZERO, Stroke::new(1.0, ink.quiet), StrokeKind::Inside));
        for column in 1..columns as usize {
            let x = (rect.left() + column as f32 * cell_w).round() + 0.5;
            out.push(Shape::line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, ink.quiet)));
        }
        for row in 1..ROWS as usize {
            let y = (rect.top() + row as f32 * cell_h).round() + 0.5;
            out.push(Shape::line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, ink.quiet)));
        }
    };
    if ink.high {
        // Lines only: the blend's two sides, closing to one as it sharpens, and the bright side hatched.
        grid(out);
        for off in [-blend / 2.0, blend / 2.0] {
            out.push(Shape::line_segment([at(-far, off), at(far, off)], Stroke::new(2.0, ink.get)));
        }
        let mut off = blend / 2.0 + 9.0;
        while off < far {
            out.push(Shape::line_segment([at(-far, off), at(far, off)], Stroke::new(1.0, ink.get)));
            off += 9.0;
        }
        return;
    }
    let bright = ink.get.gamma_multiply(0.78);
    let mut mesh = egui::Mesh::default();
    let mut quad = |corners: [Pos2; 4], colors: [Color32; 4]| {
        let first = mesh.vertices.len() as u32;
        for (corner, color) in corners.into_iter().zip(colors) {
            mesh.colored_vertex(corner, color);
        }
        mesh.add_triangle(first, first + 1, first + 2);
        mesh.add_triangle(first, first + 2, first + 3);
    };
    quad([at(-far, -blend / 2.0), at(far, -blend / 2.0), at(far, blend / 2.0), at(-far, blend / 2.0)], [Color32::TRANSPARENT, Color32::TRANSPARENT, bright, bright]);
    quad([at(-far, blend / 2.0), at(far, blend / 2.0), at(far, far), at(-far, far)], [bright; 4]);
    out.push(Shape::mesh(mesh));
    if halo > 0.01 {
        let off = blend / 2.0 + 2.0;
        out.push(Shape::line_segment([at(-far, off), at(far, off)], Stroke::new(2.0, tint(ink.get, Color32::WHITE, 0.7).gamma_multiply(halo))));
    }
    grid(out);
}
