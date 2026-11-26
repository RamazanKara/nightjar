//! The one place the desktop interface reports what it did that a hand may feel
//! (`nightjar_policy::ui_feedback_contract`): focus moves, confirm, back, toggle. It keeps the
//! contract's rate `Limiter` and its hard gate, so nothing answers while a stream is on screen.
//!
//! The desktop plays nothing yet: the launcher's egui has no way to the controller's motor outside
//! a stream (only the stream's gamepad path in `nightjar-desktop-media` rumbles, for the PC), and
//! the contract's sounds have no players. The answer is returned and the last one kept, so the
//! wiring is tested and a player can be added here alone.

use egui::{Context, Id};
use nightjar_policy::ui_feedback_contract::{Event, Feedback, Gate, Limiter};

#[derive(Clone, Debug, Default)]
struct Hook {
    limiter: Limiter,
    stream_active: bool,
    last: Option<(Event, Feedback)>,
}

fn id() -> Id {
    Id::unique("nightjar-feedback-hook")
}

fn with<R>(ctx: &Context, f: impl FnOnce(&mut Hook) -> R) -> R {
    ctx.data_mut(|d| f(d.get_temp_mut_or_default::<Hook>(id())))
}

/// Whether a stream is on screen: while it is, every event answers with nothing.
pub fn set_stream_active(ctx: &Context, active: bool) {
    with(ctx, |hook| hook.stream_active = active);
}

/// Reports `event` and returns what it answers with: [`Feedback::NONE`] while a stream is on
/// screen, or when the same event was felt too recently.
pub fn report(ctx: &Context, event: Event) -> Feedback {
    let now_ms = (ctx.input(|i| i.time).max(0.0) * 1000.0) as u64;
    with(ctx, |hook| {
        // Touch feedback would be allowed (the desktop has no system switch for it); sounds have
        // no setting yet, so they stay off as the contract says.
        let gate = Gate { stream_active: hook.stream_active, haptics: true, sounds: false };
        let feedback = hook.limiter.feedback(event, gate, now_ms);
        if !feedback.is_none() {
            hook.last = Some((event, feedback));
        }
        feedback
    })
}

/// The last event that answered with something, and its answer.
pub fn last(ctx: &Context) -> Option<(Event, Feedback)> {
    with(ctx, |hook| hook.last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nightjar_policy::ui_feedback_contract::Haptic;

    fn at(ctx: &Context, seconds: f64) {
        let input = egui::RawInput { time: Some(seconds), ..Default::default() };
        ctx.run_ui(input, |_| {}).textures_delta.clear();
    }

    #[test]
    fn events_answer_by_the_contract_and_are_kept() {
        let ctx = Context::default();
        at(&ctx, 1.0);
        assert_eq!(report(&ctx, Event::Confirm).haptic, Some(Haptic::Press));
        assert_eq!(report(&ctx, Event::Back).haptic, Some(Haptic::Click));
        assert_eq!(last(&ctx).map(|(event, _)| event), Some(Event::Back));
        assert_eq!(report(&ctx, Event::Toggle).sound, None, "no sounds without a setting");
    }

    #[test]
    fn a_held_d_pad_ticks_no_faster_than_the_limit() {
        let ctx = Context::default();
        at(&ctx, 1.0);
        assert!(!report(&ctx, Event::FocusTick).is_none());
        assert!(report(&ctx, Event::FocusTick).is_none(), "the same instant");
        at(&ctx, 1.1);
        assert!(!report(&ctx, Event::FocusTick).is_none());
    }

    #[test]
    fn nothing_answers_while_a_stream_is_on_screen() {
        let ctx = Context::default();
        at(&ctx, 1.0);
        set_stream_active(&ctx, true);
        for event in Event::ALL {
            assert!(report(&ctx, event).is_none(), "{event:?}");
        }
        assert_eq!(last(&ctx), None);
        set_stream_active(&ctx, false);
        assert!(!report(&ctx, Event::Confirm).is_none());
    }
}
