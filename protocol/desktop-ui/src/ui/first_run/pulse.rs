//! The scan pulse of the first run: three thin rings leaving a point while the network is searched.
//! It alone may ask for frames while the page rests, at most thirty a second and only while it
//! searches: its rings stand still under reduced motion, and high contrast draws one plain ring.

use egui::{Color32, Context, Pos2, Shape, Stroke};

use crate::theme::{high_contrast, shade};
use crate::ui::design_tokens as t;
use crate::ui::spring;

const RINGS: usize = 3;
/// A ring's way from the point to the edge, in seconds; the rings leave a third of it apart.
const LOOP_SECONDS: f64 = 2.4;
const FRAME_SECONDS: f32 = 1.0 / 30.0;
const RING_WIDTH: f32 = 1.25;
/// How much more than a resting ring (`SKY_RING`) one shows as it leaves the point.
const GAIN: f32 = 3.6;
/// The share of its way over which a ring comes in, so none pops out of the point.
const COME_IN: f32 = 0.1;
const POINT: f32 = 3.0;
/// Standing rings: how far out each is and how much of it shows, from the point outwards.
const STILL: [(f32, f32); RINGS] = [(0.36, 0.82), (0.68, 0.5), (1.0, 0.26)];
const HC_RING_AT: f32 = 0.68;
const HC_RING_WIDTH: f32 = 2.0;

/// What the pulse does this frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Pulse {
    /// The search runs: the rings move and ask for their next frame.
    Scanning,
    /// A PC was found: the rings still move while they fade, on the frames the fade brings.
    Leaving,
    /// No search runs: the rings stand.
    Still,
}

/// Where a ring is `along` its way (0 at the point, 1 at the edge) as a share of the radius, and
/// how much of it shows: quick off the point, slower and fainter toward the edge, gone at it.
pub(super) fn ring_at(along: f32) -> (f32, f32) {
    let left = 1.0 - along.clamp(0.0, 1.0);
    let reach = 1.0 - left.powf(1.6);
    let came_in = (along / COME_IN).clamp(0.0, 1.0);
    (reach, came_in * left.powf(1.4))
}

/// The rings' ink at `strength` of a ring that has just left the point.
fn ink(strength: f32) -> Color32 {
    let by = GAIN * strength.clamp(0.0, 1.0);
    let [r, g, b, a] = shade::SKY_RING.get().to_array();
    let channel = |v: u8| (f32::from(v) * by).round().min(255.0) as u8;
    Color32::from_rgba_premultiplied(channel(r), channel(g), channel(b), channel(a))
}

/// The rings around `center` out to `radius`, `shown` of them (1 whole, 0 gone).
pub(super) fn paint(ctx: &Context, out: &mut Vec<Shape>, center: Pos2, radius: f32, shown: f32, pulse: Pulse) {
    if shown <= 0.0 || radius <= 0.0 {
        return;
    }
    if high_contrast() {
        let rim = t::HC_COLOR_RIM.gamma_multiply(shown.min(1.0));
        out.push(Shape::circle_stroke(center, radius * HC_RING_AT, Stroke::new(HC_RING_WIDTH, rim)));
        out.push(Shape::circle_filled(center, POINT, rim));
        return;
    }
    let moving = pulse != Pulse::Still && spring::motion(ctx);
    if moving {
        let turn = (ctx.input(|i| i.time) / LOOP_SECONDS).fract() as f32;
        for n in 0..RINGS {
            let (reach, strength) = ring_at((turn + n as f32 / RINGS as f32).fract());
            out.push(Shape::circle_stroke(center, radius * reach, Stroke::new(RING_WIDTH, ink(strength * shown))));
        }
        if pulse == Pulse::Scanning {
            ctx.request_repaint_after(std::time::Duration::from_secs_f32(FRAME_SECONDS));
        }
    } else {
        for (reach, strength) in STILL {
            out.push(Shape::circle_stroke(center, radius * reach, Stroke::new(RING_WIDTH, ink(strength * shown))));
        }
    }
    out.push(Shape::circle_filled(center, POINT, ink(shown)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Look;
    use egui::{pos2, vec2, Rect};

    /// One frame of the pulse at `time`: its shapes, and how soon it asked for the next frame.
    fn frame(ctx: &Context, time: f64, shown: f32, pulse: Pulse) -> (Vec<Shape>, std::time::Duration) {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), time: Some(time), ..Default::default() };
        let mut shapes = Vec::new();
        let mut output = ctx.run_ui(input, |ui| paint(ui.ctx(), &mut shapes, pos2(400.0, 300.0), 160.0, shown, pulse));
        output.textures_delta.clear();
        (shapes, output.viewport_output[&egui::ViewportId::ROOT].repaint_delay)
    }

    fn rings(shapes: &[Shape]) -> Vec<(f32, Color32)> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Circle(c) if c.fill == Color32::TRANSPARENT => Some((c.radius, c.stroke.color)),
                _ => None,
            })
            .collect()
    }

    /// A context past its first frames, which ask for the next one themselves.
    fn warm(still: bool) -> Context {
        let ctx = Context::default();
        if still {
            let mut style = (*ctx.global_style()).clone();
            style.animation_time = 0.0;
            ctx.set_global_style(style);
        }
        for n in 0..3 {
            frame(&ctx, f64::from(n) * 0.05, 1.0, Pulse::Still);
        }
        ctx
    }

    #[test]
    fn a_ring_leaves_the_point_unseen_and_is_gone_at_the_edge() {
        assert_eq!(ring_at(0.0), (0.0, 0.0));
        let (reach, strength) = ring_at(1.0);
        assert!((reach - 1.0).abs() < 1e-6 && strength == 0.0, "{reach} {strength}");
        let mut last = 0.0;
        for step in 1..=40 {
            let (reach, strength) = ring_at(step as f32 / 40.0);
            assert!(reach > last && (0.0..=1.0).contains(&strength), "step {step}: {reach} {strength}");
            last = reach;
        }
    }

    #[test]
    fn while_it_scans_three_rings_move_and_ask_for_thirty_frames_a_second_at_most() {
        crate::theme::own_palette();
        let ctx = warm(false);
        let (first, delay) = frame(&ctx, 1.0, 1.0, Pulse::Scanning);
        assert_eq!(rings(&first).len(), RINGS);
        // A frame a thirtieth of a second on, never one at once.
        assert!(delay >= std::time::Duration::from_secs_f32(1.0 / 30.0) && delay < std::time::Duration::from_millis(50), "{delay:?}");
        let (later, _) = frame(&ctx, 1.4, 1.0, Pulse::Scanning);
        assert_ne!(rings(&first), rings(&later), "the rings moved");
        // The same place in the loop is the same picture.
        let (again, _) = frame(&ctx, 1.0 + LOOP_SECONDS, 1.0, Pulse::Scanning);
        for (a, b) in rings(&first).iter().zip(rings(&again)) {
            assert!((a.0 - b.0).abs() < 0.01, "{a:?} {b:?}");
        }
    }

    #[test]
    fn a_pulse_that_leaves_or_stands_asks_for_nothing() {
        crate::theme::own_palette();
        let ctx = warm(false);
        for pulse in [Pulse::Leaving, Pulse::Still] {
            let (shapes, delay) = frame(&ctx, 2.0, 1.0, pulse);
            assert_eq!(rings(&shapes).len(), RINGS, "{pulse:?}");
            assert_eq!(delay, std::time::Duration::MAX, "{pulse:?}");
        }
        let (gone, delay) = frame(&ctx, 2.1, 0.0, Pulse::Scanning);
        assert!(gone.is_empty() && delay == std::time::Duration::MAX, "a pulse that is gone draws and asks nothing");
        let still = |time| rings(&frame(&ctx, time, 1.0, Pulse::Still).0);
        assert_eq!(still(3.0), still(3.7), "standing rings stand");
    }

    #[test]
    fn with_reduced_motion_three_rings_stand_and_nothing_is_asked() {
        crate::theme::own_palette();
        let ctx = warm(true);
        let (a, delay) = frame(&ctx, 1.0, 1.0, Pulse::Scanning);
        let (b, _) = frame(&ctx, 1.7, 1.0, Pulse::Scanning);
        assert_eq!(rings(&a).len(), RINGS);
        assert_eq!(rings(&a), rings(&b), "the same rings whatever the time");
        assert_eq!(delay, std::time::Duration::MAX);
        let radii: Vec<f32> = rings(&a).iter().map(|r| r.0).collect();
        assert!(radii.windows(2).all(|w| w[1] > w[0] + 20.0), "apart from one another: {radii:?}");
    }

    #[test]
    fn high_contrast_draws_one_plain_ring_that_stands() {
        crate::theme::own_look(Look::HighContrast);
        let ctx = warm(false);
        let (shapes, delay) = frame(&ctx, 1.0, 1.0, Pulse::Scanning);
        assert_eq!(rings(&shapes), vec![(160.0 * HC_RING_AT, t::HC_COLOR_RIM)]);
        assert_eq!(delay, std::time::Duration::MAX);
    }

    #[test]
    fn the_rings_take_each_looks_ring_colour() {
        for look in [Look::Night, Look::Day, Look::Oled] {
            crate::theme::own_look(look);
            let ring = shade::SKY_RING.get();
            let strongest = ink(1.0);
            assert!(strongest.a() > ring.a() && strongest.a() < 128, "{look:?}: {strongest:?} from {ring:?}");
            assert_eq!(ink(0.0), Color32::TRANSPARENT, "{look:?}");
            // The same hue as the token: each channel in the token's share of its alpha.
            for (got, token) in strongest.to_array().iter().zip(ring.to_array()) {
                assert!((f32::from(*got) - f32::from(token) * GAIN).abs() <= 0.5, "{look:?}: {strongest:?} from {ring:?}");
            }
        }
    }
}
