//! The Frame pacing picture: frames as they arrive from the network (dots, above), the frames
//! shown (bars, below) and the screen's refreshes (lines). The stream runs at half the screen's
//! rate, so a frame is due every two refreshes; they arrive unevenly, one late with the next right
//! behind it, which is where the choices part. A line from each dot to its bar leans as far as the
//! frame waited.

use super::*;

/// The refreshes across the picture, and the frames it follows.
const REFRESHES: f32 = 16.0;
pub(super) const FRAMES: usize = 8;
/// The refreshes a frame is due: the screen refreshes twice for each.
const DUE: f32 = 2.0;

/// When each frame arrives, in refreshes: the third late, the fourth right behind it.
pub(super) const ARRIVES: [f32; FRAMES] = [0.6, 2.7, 5.35, 5.8, 8.7, 10.6, 12.8, 14.5];

/// When each frame is shown, in refreshes, then whether it is shown at all (1) or a newer one
/// took its turn (0).
pub(super) fn pose(choice: FramePacingChoice) -> Pose {
    let mut shown = [0.0; FRAMES];
    let mut kept = [1.0; FRAMES];
    let refresh = |frame: usize| ARRIVES[frame].ceil();
    for frame in 0..FRAMES {
        let before = frame.checked_sub(1).map(|earlier| shown[earlier]);
        shown[frame] = match choice {
            // At once, wherever the screen is in its refresh.
            FramePacingChoice::Latency => ARRIVES[frame] + 0.1,
            // At the next refresh, and only the newest: a frame with a newer one behind it by then is passed over.
            FramePacingChoice::Balanced => {
                if frame + 1 < FRAMES && refresh(frame + 1) == refresh(frame) {
                    kept[frame] = 0.0;
                }
                refresh(frame)
            }
            // At the next refresh, each in turn.
            FramePacingChoice::BalancedEvery => before.map_or(refresh(frame), |earlier| refresh(frame).max(earlier + 1.0)),
            // No sooner after the one before than a frame lasts.
            FramePacingChoice::CapFps => before.map_or(refresh(frame), |earlier| refresh(frame).max(earlier + DUE)),
            // A frame held back, so each takes its whole turn.
            FramePacingChoice::Smoothness => before.map_or(refresh(frame) + DUE, |earlier| earlier + DUE),
            // As far apart as the game drew them, a little after.
            FramePacingChoice::HostTimes => DUE * frame as f32 + 1.9,
        };
    }
    let mut pose = [0.0; POSE];
    pose[..FRAMES].copy_from_slice(&shown);
    pose[FRAMES..2 * FRAMES].copy_from_slice(&kept);
    pose
}

pub(super) fn paint(pose: &Pose, rect: Rect, ink: &Ink, out: &mut Vec<Shape>) {
    const GAP: f32 = 2.0;
    let (shown, kept) = (&pose[..FRAMES], &pose[FRAMES..2 * FRAMES]);
    let x = |refresh: f32| rect.left() + refresh / REFRESHES * rect.width();
    let dots = rect.top() + 12.0;
    let bars = egui::Rangef::new(rect.bottom() - 36.0, rect.bottom() - 12.0);
    // The screen's refreshes, behind the frames they time.
    for refresh in 0..=REFRESHES as usize {
        let at = x(refresh as f32).round() + 0.5;
        out.push(Shape::line_segment([pos2(at, bars.min - 10.0), pos2(at, bars.max + 8.0)], Stroke::new(1.0, ink.quiet)));
    }
    for frame in 0..FRAMES {
        let here = kept[frame].clamp(0.0, 1.0);
        let arrives = pos2(x(ARRIVES[frame]), dots);
        // From where it arrives to where it shows.
        if here > 0.02 {
            let line = if ink.high { ink.reference } else { ink.reference.gamma_multiply(0.55 * here) };
            out.push(Shape::line_segment([arrives + vec2(0.0, 5.0), pos2(x(shown[frame]), bars.min - 3.0)], Stroke::new(1.0, line)));
        }
        if here > 0.5 {
            out.push(Shape::circle_filled(arrives, 3.5, ink.reference));
        } else {
            out.push(Shape::circle_stroke(arrives, 3.0, Stroke::new(1.5, ink.reference)));
        }
        if here <= 0.02 {
            continue;
        }
        // On screen until the next frame shown takes over; the last one for as long as it is due.
        let until = (frame + 1..FRAMES).find(|&next| kept[next] > 0.5).map_or(shown[frame] + DUE, |next| shown[next]);
        let (left, right) = (x(shown[frame]), (x(until) - GAP).max(x(shown[frame]) + 2.0));
        let half = (bars.max - bars.min) / 2.0 * here;
        let middle = (bars.min + bars.max) / 2.0;
        ink.block(Rect::from_min_max(pos2(left, middle - half), pos2(right, middle + half)), 4, out);
    }
}
