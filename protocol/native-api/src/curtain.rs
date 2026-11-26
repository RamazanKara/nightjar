//! `GET /session/curtain` and `POST /session/curtain/dismiss`: where the start of a game on the
//! PC is, for a client that keeps a launch curtain over the stream, while the host lists
//! [`crate::feature::LAUNCH_CURTAIN`]. The same state travels on the control stream as message
//! `0x5548` (`rubylight_protocol::launch_curtain`).
use crate::session::LauncherKind;
use serde::{Deserialize, Serialize};

/// Where the start is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CurtainPhase {
    Preparing,
    StartingApp,
    WaitingWindow,
    FocusingWindow,
    /// A launcher or Windows wants the player: `prompt` says what.
    AttentionNeeded,
    /// The game's window is in front; also the phase of a launch with no curtain.
    #[default]
    Ready,
    /// A device chose to see the PC's screen.
    Dismissed,
    /// The game took too long; `reason` says so.
    TimedOut,
    /// The start failed; `reason` says why.
    Failed,
    /// A phase from a newer host: clients lift the curtain.
    #[serde(other)]
    Unknown,
}

/// What a launcher or Windows shows in place of the game.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurtainPrompt {
    /// `steam`, `epic`, `ea`, `ubisoft`, `xbox`, `battlenet`, `gog`, `rockstar`, `riot`,
    /// `hoyoplay`, `windows`, or one from a newer host.
    pub launcher: String,
    pub kind: LauncherKind,
    /// The window's title, at most 120 characters; empty for a device that does not own the
    /// session.
    #[serde(default)]
    pub window_title: String,
    /// When the host first saw it.
    pub since_unix_ms: u64,
}

/// `GET /session/curtain`, and the answer of `POST /session/curtain/dismiss`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchCurtainSnapshot {
    /// Grows with every change.
    pub revision: u64,
    /// Whether a curtain is up: a launch asked for one and it has not lifted.
    pub active: bool,
    pub phase: CurtainPhase,
    /// Whether this device may lift the curtain for everyone (`POST .../dismiss`): it owns
    /// the session and the curtain is up.
    pub dismissible: bool,
    /// Since the launch started; never progress.
    pub elapsed_ms: u64,
    #[serde(default)]
    pub prompt: Option<CurtainPrompt>,
    /// Why it timed out or failed, in plain words.
    #[serde(default)]
    pub reason: Option<String>,
}

/// The body of `POST /session/curtain/dismiss`: empty, an unknown field is refused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurtainDismissRequest {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curtain_snapshot_reads_the_documented_shape_and_newer_values() {
        let snapshot: LaunchCurtainSnapshot = serde_json::from_str(
            r#"{"revision":12,"active":true,"phase":"attention_needed","dismissible":true,
            "elapsed_ms":8420,"prompt":{"launcher":"steam","kind":"sign_in",
            "window_title":"Steam Guard - Computer Authorization Required",
            "since_unix_ms":1795221890123},"reason":null}"#,
        )
        .unwrap();
        assert_eq!(snapshot.phase, CurtainPhase::AttentionNeeded);
        assert_eq!(snapshot.prompt.unwrap().kind, LauncherKind::SignIn);
        let newer: LaunchCurtainSnapshot = serde_json::from_str(
            r#"{"revision":1,"active":true,"phase":"rendering_shaders","dismissible":false,
            "elapsed_ms":1,"prompt":{"launcher":"x","kind":"region_lock","since_unix_ms":1},
            "extra":true}"#,
        )
        .unwrap();
        assert_eq!(newer.phase, CurtainPhase::Unknown);
        assert_eq!(newer.prompt.unwrap().kind, LauncherKind::Unknown);
        let uac = serde_json::to_value(LauncherKind::Uac).unwrap();
        assert_eq!(uac, "uac");
        assert!(serde_json::from_str::<CurtainDismissRequest>("{}").is_ok());
        assert!(serde_json::from_str::<CurtainDismissRequest>(r#"{"all":true}"#).is_err());
    }
}
