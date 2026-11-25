//! Pure controller source attribution and "Controllers is Off" notice helpers shared between
//! launch planning and the media stream layer.

use crate::config::{Overrides, Profile, Settings};
use crate::strings::{tr, trf};

/// The saved switches of "Controllers" (`nightjar_policy::controllers`).
const CONTROLLERS_KEYS: &[&str] = &["controllers", "multi_controller"];

/// Where a stream's "Controllers" choice was made: any settings layer can hold it, and the topmost
/// one wins, so the log and the start's notice name it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ControllersFrom {
    /// The main settings.
    #[default]
    Settings,
    /// The profile this network uses, by name.
    Profile(String),
    /// This PC's own settings.
    Pc,
    /// This game's own settings.
    Game,
    /// moonlight-qt's `--multi-controller` option, for this run's streams from its PC.
    CommandLine,
    /// The kind of stream: a microphone-only session or a second screen sends no controller.
    StreamKind,
}

impl ControllersFrom {
    /// The topmost layer holding the choice: for Off, the one that turned controllers off; else
    /// the one that holds either switch. The main settings when no layer above holds it.
    pub fn of_layers(
        off: bool,
        profile: Option<&Profile>,
        pc: Option<&Overrides>,
        game: Option<&Overrides>,
        command_line: bool,
    ) -> Self {
        let keys = if off { &CONTROLLERS_KEYS[..1] } else { CONTROLLERS_KEYS };
        let holds = |layer: Option<&Overrides>| layer.is_some_and(|own| own.has(keys));
        // The command line sets only the player switch, never Off.
        if command_line && !off {
            Self::CommandLine
        } else if holds(game) {
            Self::Game
        } else if holds(pc) {
            Self::Pc
        } else if let Some(profile) = profile.filter(|p| p.settings.has(keys)) {
            Self::Profile(profile.name.clone())
        } else {
            Self::Settings
        }
    }

    pub fn log_name(&self) -> String {
        match self {
            Self::Settings => "main settings".to_owned(),
            Self::Profile(name) => format!("profile \"{name}\""),
            Self::Pc => "this PC's settings".to_owned(),
            Self::Game => "this game's settings".to_owned(),
            Self::CommandLine => "command line".to_owned(),
            Self::StreamKind => "stream kind".to_owned(),
        }
    }

    /// What the stream's start says when Controllers Off made here leaves a controller on this
    /// device; None where the stream's kind turned controllers off, not the player.
    pub fn off_notice(&self) -> Option<String> {
        Some(match self {
            Self::Settings | Self::CommandLine => tr("Controllers is Off in Settings, so no controller reaches the PC.").to_owned(),
            Self::Profile(name) => trf("Controllers is Off in the profile {}, so no controller reaches the PC.", &[name]),
            Self::Pc => tr("Controllers is set to Off for this PC, so no controller reaches it.").to_owned(),
            Self::Game => tr("Controllers is set to Off for this game, so no controller reaches the PC.").to_owned(),
            Self::StreamKind => return None,
        })
    }
}

/// The launch's `controllers:` log line: the choice, where it was made, the controllers connected
/// now and the gamepad mask the launch tells the PC.
pub fn controllers_line(settings: &Settings, from: &ControllersFrom, connected: &[&str], mask: u32) -> String {
    let choice = nightjar_policy::controllers::Controllers::from_saved(settings.controllers, settings.multi_controller);
    let names = if connected.is_empty() { String::new() } else { format!(" ({})", connected.join(", ")) };
    format!("controllers: {} from {}, {} connected{names}, gamepad mask {mask:#x}", choice.key(), from.log_name(), connected.len())
}

/// The notice that Controllers is Off, said once a stream: at its start when a controller is
/// connected, else for the first controller connected during it.
#[derive(Debug, Default)]
pub struct ControllersOffNotice {
    said: bool,
}

impl ControllersOffNotice {
    pub fn take(&mut self, settings: &Settings, from: &ControllersFrom, connected: usize) -> Option<String> {
        if self.said {
            return None;
        }
        let text = controllers_notice(settings, from, connected)?;
        self.said = true;
        Some(text)
    }
}

/// The notice that the `connected` controllers stay on this device, as the "Controllers" choice
/// made in `from` is Off.
pub fn controllers_notice(settings: &Settings, from: &ControllersFrom, connected: usize) -> Option<String> {
    let choice = nightjar_policy::controllers::Controllers::from_saved(settings.controllers, settings.multi_controller);
    choice.leaves_connected_here(connected).then(|| from.off_notice()).flatten()
}
