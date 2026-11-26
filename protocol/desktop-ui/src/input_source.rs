//! The device in the hand, from the launcher's last SDL event: what the button hints name. A stick
//! at rest drifts, so it counts only past the point where it moves the focus; a device that SDL calls
//! a gamepad but has no stick is no pad.
//!
//! Which key or button a hint names is nightjar-policy's `input_glyphs::glyph`, shared with
//! Android (its controller labels are ported from Moonlight V+, streaming/input/gamepadglyphs.h);
//! this module turns the answer into drawn [`Piece`]s.

use nightjar_policy::input_glyphs::{self as shared, Glyph, Hint};

use crate::glyphs::{self, Piece, Style};
use crate::shortcuts::{Chord, Key};

/// How far a stick goes before it counts as the device in the hand: where it starts to move the focus.
pub const STICK_HELD: i32 = 24000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputSource {
    /// A mouse or a finger: no hints.
    #[default]
    None,
    Keyboard,
    Pad(Style),
}

impl InputSource {
    /// The key or button that does what has focus.
    pub fn go(self, swap_face_buttons: bool) -> Vec<Piece> {
        self.pieces(Hint::Go, swap_face_buttons)
    }

    /// The key or button that opens what has focus's options.
    pub fn more(self, swap_face_buttons: bool) -> Vec<Piece> {
        self.pieces(Hint::More, swap_face_buttons)
    }

    /// The key or button that leaves the top layer.
    pub fn back(self, swap_face_buttons: bool) -> Vec<Piece> {
        self.pieces(Hint::Back, swap_face_buttons)
    }

    /// The shared source this device is.
    pub fn shared(self) -> shared::Source {
        match self {
            InputSource::None => shared::Source::Mouse,
            InputSource::Keyboard => shared::Source::Keyboard,
            InputSource::Pad(style) => shared::Source::Pad(style.0),
        }
    }

    fn pieces(self, hint: Hint, swap: bool) -> Vec<Piece> {
        let key = |key| glyphs::chord(Chord::key(key), false);
        match shared::glyph(self.shared(), hint, swap) {
            None => Vec::new(),
            // A controller's hints are face buttons: Go, More (north) and Back.
            Some(Glyph::Button(label)) => vec![Piece::Face(label)],
            Some(Glyph::Enter) => key(Key::Enter),
            Some(Glyph::Escape) => key(Key::Escape),
            Some(Glyph::Key("F")) => key(Key::F),
            Some(Glyph::Key(other)) => vec![Piece::Key(crate::strings::tr(other).to_owned())],
            // The desktop has no TV remote.
            Some(Glyph::RemoteOk | Glyph::RemoteHoldOk | Glyph::RemoteBack) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::{self, Action, Button};

    /// The shared hints name the same buttons the launcher's own bindings act on.
    #[test]
    fn shared_hints_match_the_launcher_bindings() {
        for swap in [false, true] {
            for style in [Style::Xbox, Style::PlayStation, Style::Nintendo] {
                for (action, hint) in [(Action::Select, InputSource::Pad(style).go(swap)), (Action::Options, InputSource::Pad(style).more(swap))] {
                    let button = [Button::South, Button::East, Button::North, Button::West]
                        .into_iter()
                        .find(|button| shortcuts::pad_action(*button, swap) == Some(action))
                        .unwrap();
                    assert_eq!(hint, vec![style.piece(button)], "{style:?} {action:?} swap={swap}");
                }
            }
        }
        assert!(InputSource::None.go(false).is_empty() && InputSource::None.back(true).is_empty());
        assert_eq!(InputSource::Keyboard.go(false), glyphs::chord(Chord::key(Key::Enter), false));
        assert_eq!(InputSource::Keyboard.back(false), glyphs::chord(Chord::key(Key::Escape), false));
    }
}
