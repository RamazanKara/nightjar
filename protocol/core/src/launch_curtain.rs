//! Launch Curtain state (`0x5548`), host to client, reliable: where the start of a game on the
//! PC is, so a client can keep its own calm screen over the stream until the game's window is
//! in front instead of showing the Windows desktop, store windows and splash screens.
//!
//! Frame, little-endian: version u8 ([`VERSION`]), phase u8 ([`Phase`]), flags u8
//! ([`flags`]), prompt kind u8 ([`PromptKind`]), sequence u32, elapsed ms u32, launcher length
//! u8, title length u16, reason length u16, then the launcher id, the prompt window's title and
//! the reason as UTF-8 ([`HEADER_BYTES`] + at most [`MAX_LAUNCHER_BYTES`],
//! [`MAX_TITLE_BYTES`] and [`MAX_REASON_BYTES`]).
//!
//! The sequence grows with every frame of a launch, so a client keeps the newest; the host
//! repeats its state every couple of seconds while the curtain is up, so a client that hears
//! nothing for long knows to lift it. A phase this version does not know reads as
//! [`Phase::Ready`]: an unknown phase never keeps a player behind a curtain. Version 0 is
//! refused; trailing bytes are ignored so a newer host can append fields. A host sends it only
//! on a stream whose launch asked with [`LAUNCH_PARAMETER`] and announced the id in
//! `a=x-rl-control`.

use alloc::string::String;
use alloc::vec::Vec;

/// Control-stream message type (`client/protocol/core/src/ids.rs`).
pub const LAUNCH_CURTAIN_STATE_MESSAGE_TYPE: u16 = 0x5548;
/// The serverinfo element of a host that follows a game's start for a curtain.
pub const SERVERINFO_FIELD: &str = "LaunchCurtainVersion";
/// Its value: this frame version.
pub const SERVERINFO_VERSION: u8 = 1;
/// The `/launch` and `/resume` parameter a client sends, with `1`, to get the state.
pub const LAUNCH_PARAMETER: &str = "rlLaunchCurtain";
/// The frame version this crate writes.
pub const VERSION: u8 = 1;
/// Size of the fixed part every frame starts with.
pub const HEADER_BYTES: usize = 17;
/// The longest launcher id (`steam`, `epic`, `windows`).
pub const MAX_LAUNCHER_BYTES: usize = 32;
/// The longest prompt window title.
pub const MAX_TITLE_BYTES: usize = 256;
/// The longest reason.
pub const MAX_REASON_BYTES: usize = 256;
/// The longest frame.
pub const MAX_BYTES: usize = HEADER_BYTES + MAX_LAUNCHER_BYTES + MAX_TITLE_BYTES + MAX_REASON_BYTES;

/// Whether a launch parameter's value asks for the curtain.
pub fn requested(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Where the start is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    /// The PC prepares the stream.
    #[default]
    Preparing = 0,
    /// The game's program or its store is starting.
    StartingApp = 1,
    /// The game runs; its window has not come up.
    WaitingWindow = 2,
    /// The game's window is up behind another or minimised; the host brings it forward.
    FocusingWindow = 3,
    /// A launcher or Windows wants the player ([`PromptKind`]).
    AttentionNeeded = 4,
    /// The game's window is in front: the curtain lifts.
    Ready = 5,
    /// The player chose to see the PC's screen.
    Dismissed = 6,
    /// The game took too long; the curtain lifts and says so.
    TimedOut = 7,
    /// The start failed; the reason says why.
    Failed = 8,
}

impl Phase {
    /// A phase from the wire; one this version does not know is [`Phase::Ready`].
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Preparing,
            1 => Self::StartingApp,
            2 => Self::WaitingWindow,
            3 => Self::FocusingWindow,
            4 => Self::AttentionNeeded,
            6 => Self::Dismissed,
            7 => Self::TimedOut,
            8 => Self::Failed,
            _ => Self::Ready,
        }
    }

    /// Whether the curtain is down for good in this phase.
    pub fn lifts(self) -> bool {
        matches!(
            self,
            Self::Ready | Self::Dismissed | Self::TimedOut | Self::Failed
        )
    }
}

/// What a launcher or Windows wants in [`Phase::AttentionNeeded`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum PromptKind {
    #[default]
    None = 0,
    /// An update, install or download is in the way.
    Update = 1,
    /// A sign in or a verification code.
    SignIn = 2,
    /// Another dialog; the title names it.
    Prompt = 3,
    /// Windows asks for administrator permission on the secure desktop.
    Uac = 4,
}

impl PromptKind {
    /// A kind from the wire; one this version does not know is [`PromptKind::Prompt`].
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::None,
            1 => Self::Update,
            2 => Self::SignIn,
            4 => Self::Uac,
            _ => Self::Prompt,
        }
    }
}

/// Bits of [`CurtainState::flags`].
pub mod flags {
    /// The launch starts no game window to wait for (the desktop, a launcher's own window, a
    /// resumed game): the curtain lifts on the first picture.
    pub const DESKTOP_OR_LAUNCHER: u8 = 0x01;
    /// The game's window has been seen.
    pub const FIRST_FRAME_SEEN: u8 = 0x02;
    /// Windows shows its secure desktop (a UAC prompt).
    pub const UAC_SECURE_DESKTOP: u8 = 0x04;
    /// The player may lift the curtain from the client.
    pub const DISMISSIBLE: u8 = 0x08;
}

/// One state of a launch's curtain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CurtainState {
    pub phase: Phase,
    pub flags: u8,
    pub prompt: PromptKind,
    /// Grows with every frame of a launch.
    pub sequence: u32,
    /// Since the launch started; never shown as progress.
    pub elapsed_ms: u32,
    /// The launcher's id (`steam`, `epic`, `ea`, `ubisoft`, `xbox`, `windows`), or empty.
    pub launcher: String,
    /// The prompt window's title, or empty.
    pub title: String,
    /// Why the curtain timed out or the start failed, in plain words, or empty.
    pub reason: String,
}

impl CurtainState {
    /// Whether a flag of [`flags`] is set.
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    /// The frame, or none when a text is longer than the frame carries ([`clip`] it first).
    pub fn encode(&self) -> Option<Vec<u8>> {
        let (launcher, title, reason) = (
            self.launcher.as_bytes(),
            self.title.as_bytes(),
            self.reason.as_bytes(),
        );
        if launcher.len() > MAX_LAUNCHER_BYTES
            || title.len() > MAX_TITLE_BYTES
            || reason.len() > MAX_REASON_BYTES
        {
            return None;
        }
        let mut out =
            Vec::with_capacity(HEADER_BYTES + launcher.len() + title.len() + reason.len());
        out.extend_from_slice(&[VERSION, self.phase as u8, self.flags, self.prompt as u8]);
        out.extend_from_slice(&self.sequence.to_le_bytes());
        out.extend_from_slice(&self.elapsed_ms.to_le_bytes());
        out.push(launcher.len() as u8);
        out.extend_from_slice(&(title.len() as u16).to_le_bytes());
        out.extend_from_slice(&(reason.len() as u16).to_le_bytes());
        out.extend_from_slice(launcher);
        out.extend_from_slice(title);
        out.extend_from_slice(reason);
        Some(out)
    }

    /// A frame, or none for version 0, a short frame, a length over its bound or text that is
    /// not UTF-8.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < HEADER_BYTES || bytes[0] == 0 {
            return None;
        }
        let u32_at = |at: usize| {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let u16_at = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]) as usize;
        let (launcher_len, title_len, reason_len) = (bytes[12] as usize, u16_at(13), u16_at(15));
        if launcher_len > MAX_LAUNCHER_BYTES
            || title_len > MAX_TITLE_BYTES
            || reason_len > MAX_REASON_BYTES
        {
            return None;
        }
        let texts =
            bytes.get(HEADER_BYTES..HEADER_BYTES + launcher_len + title_len + reason_len)?;
        let (launcher, rest) = texts.split_at(launcher_len);
        let (title, reason) = rest.split_at(title_len);
        let text = |b: &[u8]| core::str::from_utf8(b).ok().map(String::from);
        Some(Self {
            phase: Phase::from_u8(bytes[1]),
            flags: bytes[2],
            prompt: PromptKind::from_u8(bytes[3]),
            sequence: u32_at(4),
            elapsed_ms: u32_at(8),
            launcher: text(launcher)?,
            title: text(title)?,
            reason: text(reason)?,
        })
    }
}

/// `text` cut to at most `max_bytes` bytes on a character boundary.
pub fn clip(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    fn waiting() -> CurtainState {
        CurtainState {
            phase: Phase::AttentionNeeded,
            flags: flags::DISMISSIBLE | flags::FIRST_FRAME_SEEN,
            prompt: PromptKind::SignIn,
            sequence: 7,
            elapsed_ms: 8_420,
            launcher: "steam".into(),
            title: "Steam Guard - Computer Authorization Required".into(),
            reason: String::new(),
        }
    }

    #[test]
    fn launch_curtain_state_round_trips_with_its_layout() {
        let state = waiting();
        let wire = state.encode().unwrap();
        assert_eq!(&wire[..4], &[1, 4, 0x0A, 2]);
        assert_eq!(&wire[4..8], &7u32.to_le_bytes());
        assert_eq!(&wire[8..12], &8_420u32.to_le_bytes());
        assert_eq!(wire[12], 5);
        assert_eq!(&wire[13..15], &(state.title.len() as u16).to_le_bytes());
        assert_eq!(&wire[15..17], &0u16.to_le_bytes());
        assert_eq!(&wire[17..22], b"steam");
        assert_eq!(CurtainState::decode(&wire), Some(state));
    }

    #[test]
    fn launch_curtain_every_phase_and_kind_round_trips() {
        for phase in 0..=8u8 {
            for kind in 0..=4u8 {
                let state = CurtainState {
                    phase: Phase::from_u8(phase),
                    prompt: PromptKind::from_u8(kind),
                    reason: "The game took longer than 45 seconds to start.".into(),
                    ..waiting()
                };
                assert_eq!(state.phase as u8, phase);
                assert_eq!(state.prompt as u8, kind);
                let wire = state.encode().unwrap();
                assert_eq!(CurtainState::decode(&wire), Some(state));
            }
        }
    }

    #[test]
    fn launch_curtain_unknown_phases_lift_and_unknown_kinds_are_prompts() {
        let mut wire = waiting().encode().unwrap();
        for unknown in [9u8, 42, 0xFF] {
            wire[1] = unknown;
            wire[3] = unknown;
            let state = CurtainState::decode(&wire).unwrap();
            assert_eq!(state.phase, Phase::Ready);
            assert!(state.phase.lifts());
            assert_eq!(state.prompt, PromptKind::Prompt);
        }
        for phase in [
            Phase::Ready,
            Phase::Dismissed,
            Phase::TimedOut,
            Phase::Failed,
        ] {
            assert!(phase.lifts(), "{phase:?}");
        }
        for phase in [
            Phase::Preparing,
            Phase::StartingApp,
            Phase::WaitingWindow,
            Phase::FocusingWindow,
            Phase::AttentionNeeded,
        ] {
            assert!(!phase.lifts(), "{phase:?}");
        }
    }

    #[test]
    fn launch_curtain_refuses_version_zero_short_frames_long_texts_and_bad_utf8() {
        let wire = waiting().encode().unwrap();
        let mut zero = wire.clone();
        zero[0] = 0;
        assert_eq!(CurtainState::decode(&zero), None);
        for cut in 0..wire.len() {
            assert_eq!(CurtainState::decode(&wire[..cut]), None, "cut at {cut}");
        }
        let mut long = wire.clone();
        long[12] = (MAX_LAUNCHER_BYTES + 1) as u8;
        assert_eq!(CurtainState::decode(&long), None);
        let mut long = wire.clone();
        long[13..15].copy_from_slice(&((MAX_TITLE_BYTES + 1) as u16).to_le_bytes());
        assert_eq!(CurtainState::decode(&long), None);
        let mut bad = wire.clone();
        bad[17] = 0xFF;
        assert_eq!(CurtainState::decode(&bad), None);
        let too_long = CurtainState {
            title: "x".repeat(MAX_TITLE_BYTES + 1),
            ..waiting()
        };
        assert_eq!(too_long.encode(), None);
    }

    #[test]
    fn launch_curtain_newer_versions_and_trailing_bytes_still_read() {
        let mut wire = waiting().encode().unwrap();
        wire[0] = 2;
        wire.extend_from_slice(&[1, 2, 3]);
        let state = CurtainState::decode(&wire).unwrap();
        assert_eq!(state.title, waiting().title);
    }

    #[test]
    fn launch_curtain_largest_frame_fits_its_bound() {
        let state = CurtainState {
            launcher: "l".repeat(MAX_LAUNCHER_BYTES),
            title: "t".repeat(MAX_TITLE_BYTES),
            reason: "r".repeat(MAX_REASON_BYTES),
            ..waiting()
        };
        let wire = state.encode().unwrap();
        assert_eq!(wire.len(), MAX_BYTES);
        assert!(MAX_BYTES <= 562);
        assert_eq!(CurtainState::decode(&wire), Some(state));
    }

    #[test]
    fn launch_curtain_clip_keeps_whole_characters() {
        assert_eq!(clip("steam", 32), "steam");
        let title = "ä".repeat(200);
        let clipped = clip(&title, MAX_TITLE_BYTES);
        assert_eq!(clipped.len(), MAX_TITLE_BYTES);
        assert_eq!(clip("aä", 2), "a");
        assert_eq!(clip("", 0), "");
        assert!(requested(Some("1")));
        assert!(!requested(Some("0")));
        assert!(!requested(None));
        assert_eq!("1".to_string(), SERVERINFO_VERSION.to_string());
    }
}
