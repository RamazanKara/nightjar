//! The launch curtain both Nightjar apps draw over the stream while a game starts on the PC
//! (control message 0x5548, `docs/control-messages.md`, "Launch curtain"). The host says where
//! the start is; this decides whether the curtain is down and what it says, and lifts it on its
//! own when the host goes quiet, so a player is never stuck behind it. Each [`Words`] has a `key`
//! naming Android's `launch_curtain_<key>` string and the English desktop Nightjar translates with
//! `tr` (`trf` fills a launcher's name for `{}`; Android's string has `%1$s`).

use std::time::{Duration, Instant};

use rubylight_protocol::launch_curtain::{CurtainState, Phase, PromptKind};

pub use crate::quick_suspend::Words;

/// The curtain lifts once the picture has run this long without a word from the host.
pub const QUIET: Duration = Duration::from_secs(10);
/// It lifts in any case this long after the launch; the host gives up at 45 s.
pub const LIMIT: Duration = Duration::from_secs(60);

const fn w(key: &'static str, en: &'static str) -> Words {
    Words { key, en }
}

/// The stage words, one per phase the curtain stays down in.
pub const PREPARING: Words = w("preparing", "Preparing PC");
pub const STARTING: Words = w("starting", "Starting game");
pub const OPENING: Words = w("opening", "Opening window");
pub const FOCUSING: Words = w("focusing", "Focusing game");

/// The action card's line, by what wants the player; `{}` is the launcher's name.
pub const UPDATING: Words = w("updating", "{} is updating");
pub const UPDATING_ANY: Words = w("updating_any", "A launcher is updating");
pub const SIGN_IN: Words = w("sign_in", "{} wants you to sign in");
pub const SIGN_IN_ANY: Words = w("sign_in_any", "A launcher wants you to sign in");
pub const WAITING: Words = w("waiting", "{} is waiting for you");
pub const WAITING_ANY: Words = w("waiting_any", "A window on the PC is waiting for you");
pub const UAC: Words = w("uac", "Windows needs administrator approval on this PC");
/// Under the card's line.
pub const HINT: Words = w(
    "hint",
    "Show the PC screen to answer it from this device, or cancel the launch.",
);
/// The button that lifts the curtain; the other one is the app's own Cancel.
pub const SHOW_SCREEN: Words = w("show_screen", "Show PC screen");

/// The short notice once the curtain lifted before the game was in front.
pub const STILL_STARTING: Words = w(
    "still_starting",
    "Showing PC screen. The game is still starting.",
);
/// The notice once the PC said the start failed; the host's reason follows it.
pub const FAILED: Words = w("failed", "The game didn’t start on the PC.");

/// Every line, for the clients' tests.
pub const ALL: [Words; 15] = [
    PREPARING,
    STARTING,
    OPENING,
    FOCUSING,
    UPDATING,
    UPDATING_ANY,
    SIGN_IN,
    SIGN_IN_ANY,
    WAITING,
    WAITING_ANY,
    UAC,
    HINT,
    SHOW_SCREEN,
    STILL_STARTING,
    FAILED,
];

/// How the curtain lifted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lift {
    /// The game's window is in front, or the launch has no game window to wait for.
    Ready,
    /// The player chose to see the PC's screen.
    Dismissed,
    /// The PC gave up waiting for the game's window; its reason, or empty.
    TimedOut(String),
    /// The start failed on the PC; its reason, or empty.
    Failed(String),
    /// The stream does not carry the curtain: the host's setting is off, or it is older.
    NotOffered,
    /// The picture ran for [`QUIET`] without a word from the host, or the launch passed
    /// [`LIMIT`].
    Quiet,
}

impl Lift {
    /// The notice to show once the curtain lifted this way, or none when the game is in front
    /// or the player chose it.
    pub fn notice(&self) -> Option<Words> {
        match self {
            Self::TimedOut(_) | Self::Quiet => Some(STILL_STARTING),
            Self::Failed(_) => Some(FAILED),
            Self::Ready | Self::Dismissed | Self::NotOffered => None,
        }
    }

    /// The host's own words for a timeout or a failure, when it gave some.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::TimedOut(reason) | Self::Failed(reason) if !reason.is_empty() => Some(reason),
            _ => None,
        }
    }
}

/// The card shown while a launcher or Windows wants the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attention {
    /// The card's line; fill `{}` with [`Attention::launcher`].
    pub words: Words,
    /// The launcher's name, when the line has a `{}`.
    pub launcher: Option<&'static str>,
    /// The waiting window's title, or empty.
    pub title: String,
}

/// What the curtain shows while it is down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    /// The stage word; none before the host's first state, while the app's own connecting
    /// words still show.
    pub stage: Option<Words>,
    /// The action card, while a launcher or Windows wants the player.
    pub attention: Option<Attention>,
    /// Whether Show PC screen is offered: once the picture runs.
    pub dismissible: bool,
}

/// A launch's curtain on the client, from the launch that asked for it until it lifts.
#[derive(Clone, Debug)]
pub struct Curtain {
    started: Instant,
    state: Option<CurtainState>,
    heard: Option<Instant>,
    picture: Option<Instant>,
    lifted: Option<Lift>,
}

impl Curtain {
    /// The curtain of a launch that asked for it, started now.
    pub fn new(now: Instant) -> Self {
        Self {
            started: now,
            state: None,
            heard: None,
            picture: None,
            lifted: None,
        }
    }

    /// Whether the stream's DESCRIBE offered 0x5548; without it the curtain lifts at once.
    pub fn offered(&mut self, offered: bool) -> Option<Lift> {
        if offered {
            None
        } else {
            self.lift(Lift::NotOffered)
        }
    }

    /// A state from the host. A repeated or older sequence only counts as a word from the host;
    /// a lifting phase lifts the curtain and returns how.
    pub fn heard(&mut self, now: Instant, state: CurtainState) -> Option<Lift> {
        if self.lifted.is_some() {
            return None;
        }
        self.heard = Some(now);
        if self
            .state
            .as_ref()
            .is_some_and(|last| state.sequence <= last.sequence)
        {
            return None;
        }
        let lift = match state.phase {
            Phase::Ready => Some(Lift::Ready),
            Phase::Dismissed => Some(Lift::Dismissed),
            Phase::TimedOut => Some(Lift::TimedOut(state.reason.clone())),
            Phase::Failed => Some(Lift::Failed(state.reason.clone())),
            _ => None,
        };
        self.state = Some(state);
        lift.and_then(|lift| self.lift(lift))
    }

    /// The stream's first picture; the quiet watch starts here.
    pub fn picture(&mut self, now: Instant) {
        self.picture.get_or_insert(now);
    }

    /// Lifts the curtain when the host went quiet or the launch passed [`LIMIT`]; returns the
    /// lift when it happened now.
    pub fn tick(&mut self, now: Instant) -> Option<Lift> {
        match self.due() {
            Some(due) if now >= due => self.lift(Lift::Quiet),
            _ => None,
        }
    }

    /// When [`Curtain::tick`] lifts the curtain unless the host speaks first.
    pub fn due(&self) -> Option<Instant> {
        if self.lifted.is_some() {
            return None;
        }
        let limit = self.started + LIMIT;
        let quiet = self.picture.map(|picture| {
            let last = self.heard.map_or(picture, |heard| heard.max(picture));
            last + QUIET
        });
        Some(quiet.map_or(limit, |quiet| quiet.min(limit)))
    }

    /// The player chose Show PC screen: the curtain lifts at once, before the host answers.
    pub fn dismiss(&mut self) -> Option<Lift> {
        self.lift(Lift::Dismissed)
    }

    /// Whether the curtain is down.
    pub fn down(&self) -> bool {
        self.lifted.is_none()
    }

    /// How it lifted, once it did.
    pub fn lifted(&self) -> Option<&Lift> {
        self.lifted.as_ref()
    }

    /// Whether the host takes a dismissal for this launch (`POST /session/curtain/dismiss`).
    pub fn host_dismissible(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|state| state.has(rubylight_protocol::launch_curtain::flags::DISMISSIBLE))
    }

    /// What the curtain shows while it is down.
    pub fn view(&self) -> View {
        let state = self.state.as_ref();
        View {
            stage: state.and_then(|state| stage(state.phase)),
            attention: state
                .filter(|state| state.phase == Phase::AttentionNeeded)
                .map(attention),
            dismissible: self.picture.is_some(),
        }
    }

    fn lift(&mut self, lift: Lift) -> Option<Lift> {
        if self.lifted.is_some() {
            return None;
        }
        self.lifted = Some(lift.clone());
        Some(lift)
    }
}

/// The stage word of a phase; the card's phase keeps the start's word under it.
fn stage(phase: Phase) -> Option<Words> {
    match phase {
        Phase::Preparing => Some(PREPARING),
        Phase::StartingApp | Phase::AttentionNeeded => Some(STARTING),
        Phase::WaitingWindow => Some(OPENING),
        Phase::FocusingWindow => Some(FOCUSING),
        Phase::Ready | Phase::Dismissed | Phase::TimedOut | Phase::Failed => None,
    }
}

fn attention(state: &CurtainState) -> Attention {
    let launcher = launcher_name(&state.launcher);
    let (named, any) = match state.prompt {
        PromptKind::Uac => (UAC, UAC),
        PromptKind::Update => (UPDATING, UPDATING_ANY),
        PromptKind::SignIn => (SIGN_IN, SIGN_IN_ANY),
        PromptKind::Prompt | PromptKind::None => (WAITING, WAITING_ANY),
    };
    let named_line = named.en.contains("{}");
    Attention {
        words: if launcher.is_some() || !named_line {
            named
        } else {
            any
        },
        launcher: launcher.filter(|_| named_line),
        title: state.title.clone(),
    }
}

/// A launcher's name from the host's id (`steam`, `epic`, ...); none for one this version does
/// not know.
pub fn launcher_name(id: &str) -> Option<&'static str> {
    Some(match id {
        "steam" => "Steam",
        "epic" => "Epic Games",
        "ea" => "EA app",
        "ubisoft" => "Ubisoft Connect",
        "xbox" => "Xbox app",
        "battlenet" => "Battle.net",
        "gog" => "GOG Galaxy",
        "riot" => "Riot Client",
        "windows" => "Windows",
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
