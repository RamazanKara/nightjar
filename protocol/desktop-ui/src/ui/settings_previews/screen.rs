//! The Video size picture: a 16:9 screen (neutral) and a wider video on it (amber), with a circle
//! in the video that shows whether its shape is kept. Fit leaves bars above and below, Fill runs
//! past the screen's sides (dashed, cut off), Stretch squeezes the circle, 1:1 pixels shows the
//! video at its own size.

use super::*;

const SCREEN: f32 = 16.0 / 9.0;
/// The video's shape, wider than the screen so Fit has bars to show.
const VIDEO: f32 = 2.4;
/// How wide Fill makes the video, in screen widths, and how tall Fit leaves it, in screen heights.
pub(super) const FILL_W: f32 = VIDEO / SCREEN;
pub(super) const FIT_H: f32 = SCREEN / VIDEO;
/// The video at its own size, smaller than the screen.
const OWN: f32 = 0.6;
/// The circle's radius, as a part of the video's height.
const CIRCLE: f32 = 0.3;

/// The video's width in screen widths, its height in screen heights, how far its middle sits from
/// the screen's (in screen heights, up is negative), and the circle's two radii in screen heights.
pub(super) fn pose(scaling: VideoScaling, top: bool) -> Pose {
    let (w, h, squeeze) = match scaling {
        VideoScaling::Fit => (1.0, FIT_H, 1.0),
        VideoScaling::Fill => (FILL_W, 1.0, 1.0),
        VideoScaling::Stretch => (1.0, 1.0, 1.0 / FILL_W),
        VideoScaling::Native => (OWN, OWN * FIT_H, 1.0),
    };
    // Only a video with room above it can sit at the top.
    let up = if top && h < 1.0 { -(1.0 - h) / 2.0 } else { 0.0 };
    let mut pose = [0.0; POSE];
    pose[..5].copy_from_slice(&[w, h, up, CIRCLE * h * squeeze, CIRCLE * h]);
    pose
}

pub(super) fn paint(pose: &Pose, rect: Rect, ink: &Ink, out: &mut Vec<Shape>) {
    // Room at the sides for what Fill cuts off.
    let width = (rect.width() / (FILL_W + 0.06)).min((rect.height() - 6.0) * SCREEN);
    let screen = Rect::from_center_size(rect.center(), vec2(width, width / SCREEN));
    let middle = screen.center() + vec2(0.0, pose[2] * screen.height());
    let video = Rect::from_center_size(middle, vec2(pose[0] * screen.width(), pose[1] * screen.height()));
    let seen = video.intersect(screen);
    // What runs past the screen is cut off.
    let cut = Stroke::new(1.5, if ink.high { ink.get } else { ink.get.gamma_multiply(0.6) });
    if video.left() < screen.left() - 1.0 {
        out.extend(Shape::dashed_line(&[pos2(screen.left(), video.top()), video.left_top(), video.left_bottom(), pos2(screen.left(), video.bottom())], cut, 4.0, 3.0));
    }
    if video.right() > screen.right() + 1.0 {
        out.extend(Shape::dashed_line(&[pos2(screen.right(), video.top()), video.right_top(), video.right_bottom(), pos2(screen.right(), video.bottom())], cut, 4.0, 3.0));
    }
    if seen.is_positive() {
        if !ink.high {
            out.push(Shape::rect_filled(seen, CornerRadius::same(2), ink.wash));
        }
        out.push(Shape::rect_stroke(seen, CornerRadius::same(2), Stroke::new(2.0, ink.get), StrokeKind::Inside));
        out.push(Shape::ellipse_stroke(middle, vec2(pose[3], pose[4]) * screen.height(), Stroke::new(1.5, ink.get)));
    }
    out.push(Shape::rect_stroke(screen, CornerRadius::same(4), Stroke::new(2.0, ink.reference), StrokeKind::Outside));
}
