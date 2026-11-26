//! Console projections built atop merged libraries: Continue playing, Recents, Starred,
//! game detail state, host lines, stream summaries, and classic-app reconciliation.

use super::hidden::is_hidden;
use super::merge::{Entry, Source};
use super::{Favourite, Recent};
use crate::http::native_api::library::Game;
use crate::http::native_api::session::{ActionId, Owner, Session, State};
use serde::{Deserialize, Serialize};

/// Whether a [`Continue`] item is actively streaming or suspended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Playing {
    Running,
    Suspended,
}

/// An active or suspended running game offered in "Continue playing".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Continue {
    pub host: u64,
    pub host_uid: String,
    pub host_name: String,
    pub app_id: u32,
    pub install_id: String,
    pub title: String,
    pub playing: Playing,
    pub suspended: bool,
    pub other_device: Option<String>,
    pub cover_path: Option<String>,
    pub cover_version: String,
}

/// Active/suspended games across online paired sources for "Continue playing".
pub fn continue_playing(sources: &[Source<'_>]) -> Vec<Continue> {
    let mut out = Vec::new();
    for source in sources {
        if !source.paired || !source.online {
            continue;
        }
        if let Some(session) = source.session
            && matches!(
                session.state,
                State::Preparing | State::Launching | State::Streaming | State::Suspended
            )
        {
            let suspended = session.state == State::Suspended;
            let playing = if suspended {
                Playing::Suspended
            } else {
                Playing::Running
            };
            if let Some(app) = &session.app {
                let (cover_path, cover_version) = cover_for_app(source, &app.install_id, app.app_id);
                let other_device = match session.owner {
                    Owner::OtherDevice => session
                        .playing_on
                        .as_ref()
                        .map(|p| p.device.clone())
                        .or_else(|| Some(String::new())),
                    _ => None,
                };
                out.push(Continue {
                    host: source.id,
                    host_uid: source.unique_id.to_owned(),
                    host_name: source.name.to_owned(),
                    app_id: app.app_id,
                    install_id: app.install_id.clone(),
                    title: app.title.clone(),
                    playing,
                    suspended,
                    other_device,
                    cover_path,
                    cover_version,
                });
                continue;
            }
            if let Some(playing_on) = &session.playing_on {
                let app = &playing_on.app;
                let (cover_path, cover_version) = cover_for_app(source, &app.install_id, app.app_id);
                out.push(Continue {
                    host: source.id,
                    host_uid: source.unique_id.to_owned(),
                    host_name: source.name.to_owned(),
                    app_id: app.app_id,
                    install_id: app.install_id.clone(),
                    title: app.title.clone(),
                    playing,
                    suspended,
                    other_device: Some(playing_on.device.clone()),
                    cover_path,
                    cover_version,
                });
                continue;
            }
        }
        if let Some(lib) = source.library {
            for game in &lib.games {
                if game.running {
                    out.push(Continue {
                        host: source.id,
                        host_uid: source.unique_id.to_owned(),
                        host_name: source.name.to_owned(),
                        app_id: game.app_id,
                        install_id: game.install_id.clone(),
                        title: game.title.clone(),
                        playing: Playing::Running,
                        suspended: false,
                        other_device: None,
                        cover_path: game.artwork.cover.clone(),
                        cover_version: game.artwork.version.clone(),
                    });
                }
            }
        }
    }
    out.sort_by_key(|c| c.playing != Playing::Running);
    out
}

fn cover_for_app(source: &Source<'_>, install_id: &str, app_id: u32) -> (Option<String>, String) {
    source
        .library
        .and_then(|l| {
            l.games.iter().find(|g| {
                (!install_id.is_empty() && g.install_id.eq_ignore_ascii_case(install_id))
                    || (app_id != 0 && g.app_id == app_id)
            })
        })
        .map_or((None, String::new()), |g| {
            (g.artwork.cover.clone(), g.artwork.version.clone())
        })
}

/// A recently played game item resolved against current sources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recently {
    pub host: u64,
    pub host_uid: String,
    pub host_name: String,
    pub online: bool,
    pub app_id: u32,
    pub install_id: String,
    pub title: String,
    pub at: u64,
}

/// Resolves `recents` against live `sources`, skipping games already in `playing` or `hidden`.
pub fn recently_shown(
    sources: &[Source<'_>],
    recents: &[Recent],
    playing: &[Continue],
    hidden: &[Favourite],
    limit: usize,
) -> Vec<Recently> {
    let mut out: Vec<Recently> = Vec::new();
    for recent in recents {
        let Some(source) = sources
            .iter()
            .find(|s| s.paired && !s.unique_id.is_empty() && s.unique_id == recent.host)
        else {
            continue;
        };
        let Some(g) = source.library.and_then(|l| {
            l.games.iter().find(|g| {
                (!recent.install_id.is_empty()
                    && !g.install_id.is_empty()
                    && g.install_id.eq_ignore_ascii_case(&recent.install_id))
                    || ((recent.install_id.is_empty() || g.install_id.is_empty())
                        && recent.app_id != 0
                        && g.app_id == recent.app_id)
            })
        }) else {
            continue;
        };
        if !g.installed.unwrap_or(true) {
            continue;
        }
        let (app_id, install_id, title) = (g.app_id, g.install_id.clone(), g.title.clone());
        if is_hidden(source, &install_id, app_id, hidden) {
            continue;
        }
        let already_playing = playing.iter().any(|p| {
            p.host == source.id
                && ((!install_id.is_empty() && p.install_id.eq_ignore_ascii_case(&install_id))
                    || (app_id != 0 && p.app_id == app_id))
        });
        if already_playing
            || out.iter().any(|r| {
                r.host == source.id
                    && ((!install_id.is_empty() && r.install_id.eq_ignore_ascii_case(&install_id))
                        || (app_id != 0 && r.app_id == app_id))
            })
        {
            continue;
        }
        out.push(Recently {
            host: source.id,
            host_uid: source.unique_id.to_owned(),
            host_name: source.name.to_owned(),
            online: source.online,
            app_id,
            install_id,
            title,
            at: recent.at,
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

/// A starred game resolved against `sources`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Starred {
    pub host: u64,
    pub host_name: String,
    pub online: bool,
    pub app_id: u32,
    pub title: String,
    pub favourite: Favourite,
}

/// Key to bookmark `(source, game)` in favourites.
pub fn favourite_of(source: &Source<'_>, game: &Game) -> Option<Favourite> {
    if source.unique_id.is_empty() {
        return None;
    }
    Some(Favourite {
        host: source.unique_id.to_owned(),
        install_id: game.install_id.clone(),
        app_id: game.app_id,
        title: game.title.clone(),
    })
}

/// Key to bookmark `(host, app_id)` from `sources`.
pub fn favourite_for(sources: &[Source<'_>], host: u64, app_id: u32) -> Option<Favourite> {
    let source = sources.iter().find(|s| s.id == host)?;
    let game = source
        .library?
        .games
        .iter()
        .find(|g| g.app_id == app_id)?;
    favourite_of(source, game)
}

/// Whether `(source, game)` is in `favourites`.
pub fn is_starred(source: &Source<'_>, game: &Game, favourites: &[Favourite]) -> bool {
    let Some(target) = favourite_of(source, game) else {
        return false;
    };
    favourites.iter().any(|f| f.same_game(&target))
}

/// Resolves stored `favourites` against `sources`, updating app_id/title from live libraries.
pub fn favourite_games(sources: &[Source<'_>], favourites: &[Favourite]) -> Vec<Starred> {
    let mut out = Vec::new();
    let mut seen: Vec<Favourite> = Vec::new();
    for fav in favourites {
        let Some(source) = sources
            .iter()
            .find(|s| s.paired && !s.unique_id.is_empty() && s.unique_id == fav.host)
        else {
            continue;
        };
        if seen.iter().any(|prev| prev.same_game(fav)) {
            continue;
        }
        let (app_id, title, install_id) = if let Some(lib) = source.library {
            let Some(g) = lib.games.iter().find(|g| {
                (!fav.install_id.is_empty()
                    && !g.install_id.is_empty()
                    && g.install_id.eq_ignore_ascii_case(&fav.install_id))
                    || ((fav.install_id.is_empty() || g.install_id.is_empty())
                        && fav.app_id != 0
                        && g.app_id == fav.app_id)
            }) else {
                continue;
            };
            if !g.installed.unwrap_or(true) {
                continue;
            }
            (g.app_id, g.title.clone(), g.install_id.clone())
        } else {
            (fav.app_id, fav.title.clone(), fav.install_id.clone())
        };
        let resolved = Favourite {
            host: fav.host.clone(),
            install_id,
            app_id,
            title: title.clone(),
        };
        seen.push(resolved.clone());
        out.push(Starred {
            host: source.id,
            host_name: source.name.to_owned(),
            online: source.online,
            app_id,
            title,
            favourite: resolved,
        });
    }
    out
}

/// Looks up the merged entry key owning `(host, app_id)`.
pub fn entry_key_of(entries: &[Entry], host: u64, app_id: u32) -> Option<&String> {
    entries
        .iter()
        .find(|e| e.copies.iter().any(|c| c.host == host && c.app_id == app_id))
        .map(|e| &e.key)
}

/// Best known unix timestamp (seconds) when `entry` was last played across stores and local recents.
pub fn last_played(entry: &Entry, sources: &[Source<'_>], recents: &[Recent]) -> Option<u64> {
    let recent_ts = recents
        .iter()
        .filter(|r| {
            entry.copies.iter().any(|c| {
                let host_matches = sources
                    .iter()
                    .find(|s| s.id == c.host)
                    .is_some_and(|s| s.unique_id == r.host)
                    || c.host_uid == r.host;
                host_matches
                    && ((!c.install_id.is_empty()
                        && !r.install_id.is_empty()
                        && c.install_id.eq_ignore_ascii_case(&r.install_id))
                        || ((c.install_id.is_empty() || r.install_id.is_empty())
                            && r.app_id != 0
                            && c.app_id == r.app_id))
            })
        })
        .map(|r| r.at)
        .max();
    entry.last_played().into_iter().chain(recent_ts).max()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    Idle,
    Running,
    Suspended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndAction {
    None,
    CloseGame,
    Quit,
}

/// Derived state for launching/resuming/stopping `(source, app_id)` on a game detail screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detail {
    pub state: GameState,
    pub can_start: bool,
    pub installed: bool,
    pub end: EndAction,
    pub other_running: Option<String>,
    pub held_by_other: bool,
}

impl Detail {
    pub fn resumes(&self) -> bool {
        !self.held_by_other && matches!(self.state, GameState::Running | GameState::Suspended)
    }
}

/// Computes detail action state for `(source, app_id)`.
pub fn detail(source: &Source<'_>, app_id: u32, native: bool) -> Detail {
    let game = source
        .library
        .and_then(|l| l.games.iter().find(|g| g.app_id == app_id));
    let installed = game.and_then(|g| g.installed).unwrap_or(true);
    if !source.paired || !source.online {
        return Detail {
            state: GameState::Idle,
            can_start: false,
            installed,
            end: EndAction::None,
            other_running: None,
            held_by_other: false,
        };
    }
    let session = source.session;
    let active_session = session.is_some_and(|s| {
        matches!(
            s.state,
            State::Preparing | State::Launching | State::Streaming | State::Suspended
        )
    });
    let session_matches_game = session.and_then(|s| s.app.as_ref()).is_some_and(|a| {
        a.app_id == app_id
            || game.is_some_and(|g| {
                !g.install_id.is_empty()
                    && !a.install_id.is_empty()
                    && a.install_id.eq_ignore_ascii_case(&g.install_id)
            })
    });
    let game_running = game.is_some_and(|g| g.running);
    let close_game_allowed = session.is_none_or(|s| {
        s.actions
            .iter()
            .find(|a| a.id == ActionId::CloseGame)
            .is_some_and(|a| a.allowed)
    });
    let end_for_owner = if native && source.host_control && close_game_allowed {
        EndAction::CloseGame
    } else {
        EndAction::Quit
    };

    let (state, end, other_running, held_by_other) = if active_session && session_matches_game {
        let s = session.expect("active_session");
        match s.owner {
            Owner::OtherDevice => (
                GameState::Running,
                if native && source.host_control && close_game_allowed {
                    EndAction::CloseGame
                } else {
                    EndAction::None
                },
                None,
                true,
            ),
            _ if s.state == State::Suspended => (GameState::Suspended, end_for_owner, None, false),
            _ => (GameState::Running, end_for_owner, None, false),
        }
    } else if active_session {
        let other_title = session
            .and_then(|s| s.app.as_ref().map(|a| a.title.clone()))
            .or_else(|| {
                session.and_then(|s| s.playing_on.as_ref().map(|p| p.app.title.clone()))
            });
        (GameState::Idle, EndAction::None, other_title, false)
    } else if game_running {
        (GameState::Running, end_for_owner, None, false)
    } else {
        let other_flag = source
            .library
            .and_then(|l| l.games.iter().find(|g| g.running && g.app_id != app_id))
            .map(|g| g.title.clone());
        (GameState::Idle, EndAction::None, other_flag, false)
    };

    let resumes = !held_by_other && matches!(state, GameState::Running | GameState::Suspended);
    let can_start = !held_by_other && (installed || resumes);
    Detail {
        state,
        can_start,
        installed,
        end,
        other_running,
        held_by_other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostLine {
    Games(usize),
    SavedGames(usize),
    Offline,
    NoLibrary,
}

pub fn host_line(source: &Source<'_>) -> HostLine {
    match (source.online, source.library) {
        (true, Some(lib)) => HostLine::Games(lib.games.len()),
        (false, Some(lib)) => HostLine::SavedGames(lib.games.len()),
        (false, None) => HostLine::Offline,
        (true, None) => HostLine::NoLibrary,
    }
}

/// Whether `session` is currently active (`Preparing`, `Launching`, `Streaming`, or `Suspended`).
pub fn live(session: &Session) -> bool {
    crate::http::session_is_active(session)
}

/// Whether `source` has a live native library loaded (used by UI refresh gating).
pub fn has_live_library(source: &Source<'_>) -> bool {
    source.paired && !source.classic && source.library.is_some()
}

/// Whether `source` represents an active running session.
pub fn active(source: &Source<'_>) -> bool {
    source.online
        && source.session.is_some_and(|s| {
            matches!(
                s.state,
                State::Preparing | State::Launching | State::Streaming | State::Suspended
            )
        })
}

/// Formats a compact stream summary (`1920 × 1080, 60 FPS, 20 Mbps`).
pub fn stream_summary(width: u32, height: u32, fps: u32, bitrate_kbps: u32) -> String {
    let mbps = bitrate_kbps as f32 / 1000.0;
    if (mbps - mbps.round()).abs() < 0.05 {
        format!("{width} \u{00d7} {height}, {fps} FPS, {:.0} Mbps", mbps)
    } else {
        format!("{width} \u{00d7} {height}, {fps} FPS, {mbps:.1} Mbps")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Running {
    Same,
    Changed,
    Gone,
}

/// Compares what a user thought was running (`named`) against what is running `now`.
pub fn recheck_running(named: Option<i32>, now: Option<i32>) -> Running {
    match (named, now) {
        (Some(a), Some(b)) if a == b => Running::Same,
        (Some(_), Some(_)) => Running::Changed,
        (None, Some(_)) => Running::Same,
        _ => Running::Gone,
    }
}

/// Adjacent index when removing an item at `index` from a list of length `len`.
pub fn neighbour(len: usize, index: usize) -> Option<usize> {
    if len <= 1 || index >= len {
        None
    } else if index + 1 < len {
        Some(index + 1)
    } else {
        Some(index - 1)
    }
}

/// Matches a native/library `(install_id, app_id)` against a live classic app list `(id, uuid)`.
pub fn classic_app(apps: &[(i32, &str)], install_id: &str, app_id: u32) -> Option<i32> {
    if !install_id.is_empty()
        && let Some((id, _)) = apps
            .iter()
            .find(|(_, uuid)| !uuid.is_empty() && uuid.eq_ignore_ascii_case(install_id))
    {
        return Some(*id);
    }
    let id = i32::try_from(app_id).ok().filter(|&id| id > 0)?;
    apps.iter().find(|(cid, _)| *cid == id).map(|(cid, _)| *cid)
}

/// Matches against a saved classic app list: if `install_id` is non-empty and the entry in `apps`
/// has a non-empty UUID that doesn't match `install_id`, refuses the `app_id` fallback.
pub fn saved_classic_app(apps: &[(i32, &str)], install_id: &str, app_id: u32) -> Option<i32> {
    if !install_id.is_empty()
        && let Some((id, _)) = apps
            .iter()
            .find(|(_, uuid)| !uuid.is_empty() && uuid.eq_ignore_ascii_case(install_id))
    {
        return Some(*id);
    }
    let id = i32::try_from(app_id).ok().filter(|&id| id > 0)?;
    apps.iter()
        .find(|(cid, uuid)| *cid == id && (install_id.is_empty() || uuid.is_empty()))
        .map(|(cid, _)| *cid)
}
