//! What the interface answers a press with, beyond what it draws: a short touch of the motor and,
//! once sounds exist, a short sound. One list of events for Android (`Haptics`) and the desktop
//! (`ui/feedback_hook.rs`), and one gate: while a stream plays the interface is silent and still,
//! because the motors and the speakers belong to the game.
//!
//! This is the contract only. Each platform maps a [`Haptic`] to what its hardware has, and the
//! sounds have no players yet: [`Gate::sounds`] stays off until a setting for them exists.

/// Something the interface did that the hand may feel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    /// Focus moved to another control.
    FocusTick,
    /// The focused control was pressed.
    Confirm,
    /// The top layer closed.
    Back,
    /// A switch changed.
    Toggle,
    /// A dragged control met the grid or a guide in the layout editor.
    SnapGrid,
    /// A press that cannot do anything: the end of a list, a disabled control.
    Reject,
    /// A game is starting.
    LaunchStart,
}

impl Event {
    pub const ALL: [Event; 7] = [Event::FocusTick, Event::Confirm, Event::Back, Event::Toggle, Event::SnapGrid, Event::Reject, Event::LaunchStart];

    /// A stable name for the JNI and logs.
    pub fn key(self) -> &'static str {
        match self {
            Event::FocusTick => "focus_tick",
            Event::Confirm => "confirm",
            Event::Back => "back",
            Event::Toggle => "toggle",
            Event::SnapGrid => "snap_grid",
            Event::Reject => "reject",
            Event::LaunchStart => "launch_start",
        }
    }

    pub fn parse(key: &str) -> Option<Event> {
        Event::ALL.into_iter().find(|event| event.key() == key)
    }

    /// The shortest time between two of this event that are both felt: a held D-pad moves focus
    /// faster than a motor can tick, and a drag crosses many grid lines.
    pub fn min_interval_ms(self) -> u32 {
        match self {
            Event::FocusTick => 45,
            Event::SnapGrid => 60,
            Event::Reject => 180,
            Event::Confirm | Event::Back | Event::Toggle | Event::LaunchStart => 0,
        }
    }
}

/// How strong a touch of the motor is, from the lightest up. A platform takes the closest effect
/// its hardware has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Haptic {
    /// The lightest tick there is.
    Tick,
    /// A light click.
    Click,
    /// A full click.
    Press,
    /// Two quick ticks: no.
    Double,
    /// A longer, rising touch: something starts.
    Rise,
}

/// A sound by its role. Wave 0 names them; nothing plays them yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sound {
    Tick,
    Select,
    Back,
    Toggle,
    Snap,
    Reject,
    Launch,
}

/// What may be felt and heard right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gate {
    /// A stream is on screen: everything is off, whatever else holds.
    pub stream_active: bool,
    /// The system and the user allow touch feedback.
    pub haptics: bool,
    /// The user turned interface sounds on. No setting exists yet, so this is `false` everywhere.
    pub sounds: bool,
}

/// What one event answers with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Feedback {
    pub haptic: Option<Haptic>,
    pub sound: Option<Sound>,
}

impl Feedback {
    pub const NONE: Feedback = Feedback { haptic: None, sound: None };

    pub fn is_none(self) -> bool {
        self == Feedback::NONE
    }
}

fn haptic(event: Event) -> Haptic {
    match event {
        Event::FocusTick | Event::SnapGrid => Haptic::Tick,
        Event::Back | Event::Toggle => Haptic::Click,
        Event::Confirm => Haptic::Press,
        Event::Reject => Haptic::Double,
        Event::LaunchStart => Haptic::Rise,
    }
}

fn sound(event: Event) -> Sound {
    match event {
        Event::FocusTick => Sound::Tick,
        Event::Confirm => Sound::Select,
        Event::Back => Sound::Back,
        Event::Toggle => Sound::Toggle,
        Event::SnapGrid => Sound::Snap,
        Event::Reject => Sound::Reject,
        Event::LaunchStart => Sound::Launch,
    }
}

/// What `event` answers with under `gate`. While a stream is active this is always
/// [`Feedback::NONE`]: the hard gate.
pub fn feedback(event: Event, gate: Gate) -> Feedback {
    if gate.stream_active {
        return Feedback::NONE;
    }
    Feedback { haptic: gate.haptics.then(|| haptic(event)), sound: gate.sounds.then(|| sound(event)) }
}

/// Rate-limits events of one kind: the caller keeps one of these per platform and asks before it
/// plays anything.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Limiter {
    last: [Option<u64>; Event::ALL.len()],
}

impl Limiter {
    /// What `event` at `now_ms` answers with: [`feedback`], unless the same event was felt less
    /// than its [`Event::min_interval_ms`] ago. An event the gate silenced does not count as felt.
    pub fn feedback(&mut self, event: Event, gate: Gate, now_ms: u64) -> Feedback {
        let answer = feedback(event, gate);
        if answer.is_none() {
            return answer;
        }
        let slot = &mut self.last[Event::ALL.iter().position(|e| *e == event).unwrap_or(0)];
        if slot.is_some_and(|last| now_ms.saturating_sub(last) < u64::from(event.min_interval_ms())) {
            return Feedback::NONE;
        }
        *slot = Some(now_ms);
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: Gate = Gate { stream_active: false, haptics: true, sounds: true };

    #[test]
    fn a_stream_silences_every_event() {
        for event in Event::ALL {
            for haptics in [false, true] {
                for sounds in [false, true] {
                    let gate = Gate { stream_active: true, haptics, sounds };
                    assert_eq!(feedback(event, gate), Feedback::NONE, "{event:?}");
                    assert!(Limiter::default().feedback(event, gate, 0).is_none());
                }
            }
        }
    }

    #[test]
    fn every_event_is_felt_and_heard_outside_a_stream() {
        for event in Event::ALL {
            let answer = feedback(event, OPEN);
            assert!(answer.haptic.is_some() && answer.sound.is_some(), "{event:?}");
            assert_eq!(Event::parse(event.key()), Some(event));
        }
        assert_eq!(Event::parse("volume"), None);
    }

    #[test]
    fn each_half_follows_its_own_switch() {
        let felt = feedback(Event::Confirm, Gate { sounds: false, ..OPEN });
        assert_eq!((felt.haptic, felt.sound), (Some(Haptic::Press), None));
        let heard = feedback(Event::Confirm, Gate { haptics: false, ..OPEN });
        assert_eq!((heard.haptic, heard.sound), (None, Some(Sound::Select)));
        assert!(feedback(Event::Confirm, Gate::default()).is_none(), "nothing is on by default");
    }

    #[test]
    fn a_focus_tick_is_the_lightest_and_a_launch_the_longest() {
        assert_eq!(feedback(Event::FocusTick, OPEN).haptic, Some(Haptic::Tick));
        assert_eq!(feedback(Event::SnapGrid, OPEN).haptic, Some(Haptic::Tick));
        assert_eq!(feedback(Event::Reject, OPEN).haptic, Some(Haptic::Double));
        assert_eq!(feedback(Event::LaunchStart, OPEN).haptic, Some(Haptic::Rise));
        assert!(Haptic::Tick < Haptic::Click && Haptic::Click < Haptic::Press);
    }

    #[test]
    fn a_held_dpad_ticks_no_faster_than_a_motor_can() {
        let mut limiter = Limiter::default();
        assert!(!limiter.feedback(Event::FocusTick, OPEN, 1000).is_none());
        assert!(limiter.feedback(Event::FocusTick, OPEN, 1020).is_none());
        assert!(!limiter.feedback(Event::Confirm, OPEN, 1021).is_none(), "another event has its own clock");
        assert!(!limiter.feedback(Event::Confirm, OPEN, 1022).is_none(), "a press is never dropped");
        assert!(!limiter.feedback(Event::FocusTick, OPEN, 1045).is_none());
    }

    #[test]
    fn a_silenced_event_does_not_start_the_clock() {
        let mut limiter = Limiter::default();
        let stream = Gate { stream_active: true, ..OPEN };
        assert!(limiter.feedback(Event::Reject, stream, 0).is_none());
        assert!(!limiter.feedback(Event::Reject, OPEN, 10).is_none(), "the stream ended: the next reject is felt");
        assert!(limiter.feedback(Event::Reject, OPEN, 100).is_none());
    }
}
