//! The launch curtain on a stream (control message 0x5548; its rules are both apps',
//! `nightjar_policy::launch_curtain`). From a launch that asked for it until the host says the
//! game's window is in front, the overlay covers the picture with the launch scene's cover, ring
//! and stage word (`nightjar_desktop_ui::ui::launch_curtain`); the picture decodes and presents
//! under it as ever. It always lifts: on the host's lifting phase, on the quiet watch
//! (`Curtain::due`, which `wait` wakes the loop for), or on Show PC screen. While it is down the
//! pointer stays free for its buttons, Enter or A shows the PC's screen and Escape or B cancels;
//! once it lifts the stream takes the pointer as its start would have. A stream whose launch did
//! not ask, or whose PC did not offer it, has no curtain and runs as before.

use super::*;
use nightjar_policy::launch_curtain::{Curtain, Lift, View};
use rubylight_protocol::launch_curtain::CurtainState;

/// A stream's curtain, from its start until the stream has acted on the lift.
#[derive(Clone, Debug)]
pub(super) struct StreamCurtain {
    curtain: Curtain,
    /// The PC takes Show PC screen over its native API (`session.launch_curtain`) too.
    native: bool,
}

impl StreamCurtain {
    /// The curtain of a stream whose launch asked for one (`asked`), started `now`: down while
    /// the stream's DESCRIBE offered 0x5548 (`offered`); none otherwise (the PC's setting is off,
    /// or the PC is older), so the stream runs as without it.
    pub(super) fn start(asked: bool, offered: bool, native: bool, now: Instant) -> Option<Self> {
        if !asked {
            return None;
        }
        let mut curtain = Curtain::new(now);
        if let Some(lift) = curtain.offered(offered) {
            eprintln!("nightjar: launch curtain: none, {lift:?}");
            return None;
        }
        eprintln!("nightjar: launch curtain: down");
        Some(Self { curtain, native })
    }
}

/// What a key or button does on the curtain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Press {
    ShowScreen,
    Cancel,
}

/// Enter shows the PC's screen, Escape cancels.
pub(super) fn key_press(scancode: SDL_Scancode) -> Option<Press> {
    match scancode {
        SDL_SCANCODE_RETURN | SDL_SCANCODE_KP_ENTER => Some(Press::ShowScreen),
        SDL_SCANCODE_ESCAPE => Some(Press::Cancel),
        _ => None,
    }
}

/// A shows the PC's screen and B cancels, with the two swapped as the stream menu swaps them
/// (`Gamepads::swap_face_buttons`).
pub(super) fn pad_press(button: u8, swapped: bool) -> Option<Press> {
    let (confirm, back) = if swapped {
        (SDL_GAMEPAD_BUTTON_EAST, SDL_GAMEPAD_BUTTON_SOUTH)
    } else {
        (SDL_GAMEPAD_BUTTON_SOUTH, SDL_GAMEPAD_BUTTON_EAST)
    };
    match i32::from(button) {
        b if b == confirm.0 => Some(Press::ShowScreen),
        b if b == back.0 => Some(Press::Cancel),
        _ => None,
    }
}

/// The toast a lift leaves: none when the game is in front or the player chose it; after a
/// failure the PC's own words follow on a line of their own.
/// A controller's press, motion, touch or sensor reading, which the curtain keeps from the PC.
pub(super) fn pad_input(kind: SDL_EventType) -> bool {
    matches!(
        kind,
        SDL_EVENT_GAMEPAD_AXIS_MOTION
            | SDL_EVENT_GAMEPAD_BUTTON_DOWN
            | SDL_EVENT_GAMEPAD_BUTTON_UP
            | SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN
            | SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION
            | SDL_EVENT_GAMEPAD_TOUCHPAD_UP
            | SDL_EVENT_GAMEPAD_SENSOR_UPDATE
    )
}

pub(super) fn lift_toast(lift: &Lift) -> Option<String> {
    let line = tr(lift.notice()?.en);
    Some(match lift.reason().filter(|_| matches!(lift, Lift::Failed(_))) {
        Some(reason) => format!("{line}\n{reason}"),
        None => line.to_owned(),
    })
}

impl Streaming {
    /// Whether the launch curtain covers the picture now.
    pub fn curtain_down(&self) -> bool {
        self.curtain.as_ref().is_some_and(|c| c.curtain.down())
    }

    /// What the curtain shows, while it is down.
    pub fn curtain_view(&self) -> Option<View> {
        self.curtain.as_ref().filter(|c| c.curtain.down()).map(|c| c.curtain.view())
    }

    /// Whether the window's pointer goes to the curtain's buttons: it is down and the stream has
    /// not taken the pointer (its capture chord still can).
    pub fn curtain_takes_pointer(&self) -> bool {
        self.curtain_down() && !self.captured
    }

    /// A state from the host. A lifting phase is acted on by the next `follow_curtain`.
    pub(super) fn curtain_heard(&mut self, state: CurtainState) {
        if let Some(c) = self.curtain.as_mut() {
            c.curtain.heard(Instant::now(), state);
            self.needs_redraw = true;
        }
    }

    /// Once a pass: the first shown picture starts the quiet watch, the watch lifts the curtain
    /// when the host went quiet, and a lift (however it came) shows its toast and gives the
    /// stream the pointer, as its start would have.
    pub fn follow_curtain(&mut self, window: *mut SDL_Window) {
        let Some(c) = self.curtain.as_mut() else { return };
        let now = Instant::now();
        if self.first_frame {
            c.curtain.picture(now);
        }
        c.curtain.tick(now);
        let Some(lift) = c.curtain.lifted().cloned() else { return };
        self.curtain = None;
        eprintln!("nightjar: launch curtain: lifted, {lift:?}");
        if let Some(toast) = lift_toast(&lift) {
            self.add_toast(toast);
        }
        // An open stream menu takes the pointer itself as it closes.
        if !self.menu_visible {
            // SAFETY: a live window.
            let focused = unsafe { SDL_GetWindowFlags(window) & SDL_WINDOW_INPUT_FOCUS != 0 };
            if focused {
                self.set_capture(window, true);
            } else {
                self.capture_on_focus();
            }
        }
        self.needs_redraw = true;
    }

    /// When the quiet watch lifts the curtain unless the host speaks first, for the loop's wait.
    pub(super) fn curtain_wake_in(&self, now: Instant) -> Option<Duration> {
        let due = self.curtain.as_ref()?.curtain.due()?;
        Some(due.saturating_duration_since(now))
    }

    /// Show PC screen: the curtain lifts at once. A PC that takes it hears so on a worker, which
    /// never holds the stream up; a failure there changes nothing here.
    pub fn curtain_show_screen(&mut self) {
        let Some(c) = self.curtain.as_mut() else { return };
        let tell = c.native && c.curtain.host_dismissible();
        if c.curtain.dismiss().is_none() {
            return;
        }
        self.needs_redraw = true;
        let Some(client) = lock(&self.shared.host_client).clone().filter(|_| tell) else { return };
        let spawned = std::thread::Builder::new().name("nightjar curtain".into()).spawn(move || {
            if let Err(error) = client.dismiss_launch_curtain() {
                eprintln!("nightjar: launch curtain: the PC did not take Show PC screen: {error}");
            }
        });
        if let Err(error) = spawned {
            eprintln!("nightjar: launch curtain: could not tell the PC: {error}");
        }
    }

    /// Cancel: the stream ends as the stream menu's Disconnect ends it.
    pub fn curtain_cancel(&mut self) {
        if self.curtain_down() {
            self.exit = Some(Exit::User);
        }
    }

    /// A key or button of the curtain's (`key_press`, `pad_press`) while it is down; true when it
    /// acted, so it goes no further. Show PC screen waits for the picture, as its button does; an
    /// open stream menu keeps its keys.
    pub(super) fn curtain_press(&mut self, press: Option<Press>) -> bool {
        let (Some(press), Some(view), false) = (press, self.curtain_view(), self.menu_visible) else { return false };
        match press {
            Press::ShowScreen if view.dismissible => self.curtain_show_screen(),
            // Before the first picture there is no PC screen to show; the press goes nowhere.
            Press::ShowScreen => {}
            Press::Cancel => self.curtain_cancel(),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strings::{with_lang, Lang};
    use nightjar_policy::launch_curtain::{FAILED, QUIET, STILL_STARTING};
    use rubylight_protocol::launch_curtain::Phase;

    fn state(sequence: u32, phase: Phase, reason: &str) -> CurtainState {
        CurtainState { phase, sequence, reason: reason.into(), ..Default::default() }
    }

    #[test]
    fn no_curtain_unless_the_launch_asked_and_the_stream_offered_it() {
        let now = Instant::now();
        assert!(StreamCurtain::start(false, true, true, now).is_none(), "a launch that did not ask");
        assert!(StreamCurtain::start(true, false, true, now).is_none(), "a PC whose setting is off");
        let down = StreamCurtain::start(true, true, false, now).expect("asked and offered");
        assert!(down.curtain.down() && !down.native);
    }

    #[test]
    fn a_lift_says_why_only_when_the_game_is_not_in_front() {
        with_lang(Lang::En, || {
            assert_eq!(lift_toast(&Lift::Ready), None);
            assert_eq!(lift_toast(&Lift::Dismissed), None);
            assert_eq!(lift_toast(&Lift::NotOffered), None);
            assert_eq!(lift_toast(&Lift::Quiet).as_deref(), Some(STILL_STARTING.en));
            assert_eq!(lift_toast(&Lift::TimedOut("Still waiting.".into())).as_deref(), Some(STILL_STARTING.en));
            assert_eq!(lift_toast(&Lift::Failed(String::new())).as_deref(), Some(FAILED.en));
            assert_eq!(
                lift_toast(&Lift::Failed("The game closed.".into())),
                Some(format!("{}\nThe game closed.", FAILED.en))
            );
        });
        let german = with_lang(Lang::De, || lift_toast(&Lift::Quiet)).unwrap();
        assert_ne!(german, STILL_STARTING.en, "the toast is in the user's language");
    }

    #[test]
    fn the_host_and_the_quiet_watch_lift_the_curtain_with_their_toast() {
        let start = Instant::now();
        let mut c = StreamCurtain::start(true, true, true, start).unwrap();
        assert!(c.curtain.heard(start, state(1, Phase::WaitingWindow, "")).is_none());
        c.curtain.picture(start);
        let due = c.curtain.due().expect("the watch runs while it is down");
        assert_eq!(due, start + QUIET);
        assert_eq!(c.curtain.tick(due), Some(Lift::Quiet));
        assert_eq!(with_lang(Lang::En, || lift_toast(&Lift::Quiet)).as_deref(), Some(STILL_STARTING.en));

        let mut c = StreamCurtain::start(true, true, true, start).unwrap();
        let lift = c.curtain.heard(start, state(1, Phase::Failed, "Steam closed it."));
        let toast = with_lang(Lang::En, || lift_toast(lift.as_ref().unwrap()));
        assert_eq!(toast, Some(format!("{}\nSteam closed it.", FAILED.en)));
        assert!(c.curtain.due().is_none(), "nothing wakes the loop once it lifted");
    }

    #[test]
    fn enter_and_a_show_the_screen_escape_and_b_cancel() {
        assert_eq!(key_press(SDL_SCANCODE_RETURN), Some(Press::ShowScreen));
        assert_eq!(key_press(SDL_SCANCODE_KP_ENTER), Some(Press::ShowScreen));
        assert_eq!(key_press(SDL_SCANCODE_ESCAPE), Some(Press::Cancel));
        assert_eq!(key_press(SDL_SCANCODE_W), None);
        let (south, east) = (SDL_GAMEPAD_BUTTON_SOUTH.0 as u8, SDL_GAMEPAD_BUTTON_EAST.0 as u8);
        assert_eq!(pad_press(south, false), Some(Press::ShowScreen));
        assert_eq!(pad_press(east, false), Some(Press::Cancel));
        assert_eq!(pad_press(south, true), Some(Press::Cancel), "swapped face buttons");
        assert_eq!(pad_press(east, true), Some(Press::ShowScreen));
        assert_eq!(pad_press(SDL_GAMEPAD_BUTTON_NORTH.0 as u8, false), None);
    }

    #[test]
    fn every_controller_input_is_kept_from_the_pc_under_the_curtain() {
        for kind in [
            SDL_EVENT_GAMEPAD_BUTTON_DOWN,
            SDL_EVENT_GAMEPAD_BUTTON_UP,
            SDL_EVENT_GAMEPAD_AXIS_MOTION,
            SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION,
            SDL_EVENT_GAMEPAD_SENSOR_UPDATE,
        ] {
            assert!(pad_input(kind), "{}", kind.0);
        }
        // A controller coming or going is still the stream's to handle.
        assert!(!pad_input(SDL_EVENT_GAMEPAD_ADDED));
        assert!(!pad_input(SDL_EVENT_GAMEPAD_REMOVED));
        assert!(!pad_input(SDL_EVENT_KEY_DOWN));
    }
}
