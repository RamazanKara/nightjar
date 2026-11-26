//! The HDR picture: a highlight ramp. Across, how bright the game draws, dark to the sun at the
//! right; up, how bright the screen shows it. The dashed line is the whole ramp; the amber curve
//! follows it as far as the screen goes, to the top with HDR and bending flat where the highlights
//! are cut off without it.

use super::*;

/// Where the ramp goes flat without HDR, as a part of the whole, and how wide the bend into it is.
pub(super) const CLIP: f32 = 0.56;
const KNEE: f32 = 0.07;
/// With HDR the curve reaches the top before it would bend.
pub(super) const WHOLE: f32 = 1.0 + KNEE;

pub(super) fn pose(hdr: bool) -> Pose {
    let mut pose = [0.0; POSE];
    pose[0] = if hdr { WHOLE } else { CLIP };
    pose
}

/// What the screen shows of brightness `x` when it goes flat at `clip`.
pub(super) fn shown(x: f32, clip: f32) -> f32 {
    if x <= clip - KNEE {
        x
    } else if x >= clip + KNEE {
        clip
    } else {
        let t = (x - (clip - KNEE)) / (2.0 * KNEE);
        clip - KNEE + 2.0 * KNEE * (t - t * t / 2.0)
    }
}

pub(super) fn paint(pose: &Pose, rect: Rect, ink: &Ink, out: &mut Vec<Shape>) {
    const STEPS: usize = 40;
    let clip = pose[0].clamp(0.2, WHOLE);
    let scale = egui::Rangef::new(rect.bottom() - 9.0, rect.bottom() - 5.0);
    let plot = Rect::from_min_max(pos2(rect.left() + 1.0, rect.top() + 4.0), pos2(rect.right() - 12.0, scale.min - 8.0));
    let at = |x: f32, y: f32| pos2(plot.left() + x * plot.width(), plot.bottom() - y * plot.height());
    out.push(Shape::line(vec![plot.left_top(), plot.left_bottom(), plot.right_bottom()], Stroke::new(1.0, ink.quiet)));
    let curve: Vec<Pos2> = (0..=STEPS).map(|step| step as f32 / STEPS as f32).map(|x| at(x, shown(x, clip))).collect();
    if !ink.high {
        // What reaches the eye, under the curve.
        let mut under = curve.clone();
        under.push(at(1.0, 0.0));
        out.push(Shape::convex_polygon(under, ink.wash, Stroke::NONE));
    }
    out.extend(Shape::dashed_line(&[at(0.0, 0.0), at(1.0, 1.0)], Stroke::new(1.5, ink.reference), 5.0, 4.0));
    out.push(Shape::line(curve, Stroke::new(2.5, ink.get)));
    // Across: dark to bright, with a sun where the highlights are.
    let (left, right) = (plot.left(), plot.right() - 6.0);
    if ink.high {
        out.push(Shape::line_segment([pos2(left, scale.center()), pos2(right, scale.center())], Stroke::new(1.0, ink.quiet)));
    } else {
        let mut ramp = egui::Mesh::default();
        for (x, color) in [(left, Color32::TRANSPARENT), (right, ink.reference)] {
            ramp.colored_vertex(pos2(x, scale.min), color);
            ramp.colored_vertex(pos2(x, scale.max), color);
        }
        ramp.add_triangle(0, 1, 2);
        ramp.add_triangle(1, 2, 3);
        out.push(Shape::mesh(ramp));
    }
    let sun = pos2(plot.right() + 6.0, scale.center());
    out.push(Shape::circle_filled(sun, 2.5, ink.reference));
    for ray in 0..8 {
        let way = egui::Vec2::angled(ray as f32 * std::f32::consts::TAU / 8.0);
        out.push(Shape::line_segment([sun + way * 4.5, sun + way * 6.5], Stroke::new(1.0, ink.reference)));
    }
}
