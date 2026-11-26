//! Hidden-game key extraction, splitting visible vs hidden entries, hidden listings per host,
//! incremental tracking, and starred-item matching across library renumberings.

use super::merge::{Entry, Source};
use super::Favourite;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A hidden game formatted for display (e.g. Settings -> Manage hidden games).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenEntry {
    pub host: String,
    pub install_id: String,
    pub app_id: u32,
    pub title: String,
}

/// A host's hidden games grouped for the hidden-games management sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HiddenListing {
    pub host_id: String,
    pub host_name: String,
    pub online: bool,
    pub games: Vec<HiddenEntry>,
}

/// Tracks snapshots of hidden keys (`host + install_id/app_id`) to detect incremental changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HiddenTracker {
    tracked: Vec<Favourite>,
}

impl HiddenTracker {
    /// Records the initial set of hidden keys to watch.
    pub fn following(keys: Vec<Favourite>) -> Self {
        Self { tracked: keys }
    }

    /// Replaces the tracked set with `keys`.
    pub fn set(&mut self, keys: Vec<Favourite>) {
        self.tracked = keys;
    }

    /// Returns whether `current` matches differently from the tracked set, and if changed updates.
    pub fn changed(&mut self, current: &[Favourite]) -> bool {
        let same_set = self.tracked.len() == current.len()
            && self
                .tracked
                .iter()
                .all(|a| current.iter().any(|b| a.same_game_or_bare(b)))
            && current
                .iter()
                .all(|b| self.tracked.iter().any(|a| a.same_game_or_bare(b)));
        if same_set {
            false
        } else {
            self.tracked = current.to_vec();
            true
        }
    }

    pub fn tracked(&self) -> &[Favourite] {
        &self.tracked
    }
}

/// Keys to store when hiding/unhiding `(host, app_id)` on Desktop: includes every copy of
/// the same merged title across paired hosts with a known `unique_id`.
pub fn hide_keys(sources: &[Source<'_>], host: u64, app_id: u32) -> Vec<Favourite> {
    let entries = super::merge::merge_library(sources);
    let Some(entry) = entries
        .iter()
        .find(|e| e.copies.iter().any(|c| c.host == host && c.app_id == app_id))
    else {
        return sources
            .iter()
            .find(|s| s.id == host)
            .map_or_else(Vec::new, |s| hide_keys_in_source(s, app_id));
    };
    let mut out = Vec::new();
    for copy in &entry.copies {
        if copy.host_uid.is_empty() {
            continue;
        }
        let ambiguous = sources
            .iter()
            .find(|s| s.id == copy.host)
            .is_some_and(|s| is_ambiguous_install_id(s, &copy.install_id));
        if ambiguous && copy.host == host && copy.app_id != app_id {
            continue;
        }
        let fav = Favourite {
            host: copy.host_uid.clone(),
            install_id: if ambiguous {
                String::new()
            } else {
                copy.install_id.clone()
            },
            app_id: copy.app_id,
            title: entry.title.clone(),
        };
        if !out.iter().any(|existing: &Favourite| existing.same_game_or_bare(&fav)) {
            out.push(fav);
        }
    }
    out
}

/// Keys to store when hiding/unhiding `(host_id, app_id)` by host `unique_id`.
pub fn hide_keys_by_id(sources: &[Source<'_>], host_id: &str, app_id: u32) -> Vec<Favourite> {
    let Some(source) = sources.iter().find(|s| s.unique_id == host_id) else {
        return Vec::new();
    };
    hide_keys_in_source(source, app_id)
}

fn hide_keys_in_source(source: &Source<'_>, app_id: u32) -> Vec<Favourite> {
    let Some(lib) = source.library else {
        return Vec::new();
    };
    if source.unique_id.is_empty() {
        return Vec::new();
    }
    let Some(game) = lib.games.iter().find(|g| g.app_id == app_id) else {
        return Vec::new();
    };
    let ambiguous = is_ambiguous_install_id(source, &game.install_id);
    vec![Favourite {
        host: source.unique_id.to_owned(),
        install_id: if ambiguous {
            String::new()
        } else {
            game.install_id.clone()
        },
        app_id: game.app_id,
        title: game.title.clone(),
    }]
}

fn is_ambiguous_install_id(source: &Source<'_>, install_id: &str) -> bool {
    !install_id.is_empty()
        && source.library.is_some_and(|l| {
            l.games
                .iter()
                .filter(|g| g.install_id.eq_ignore_ascii_case(install_id))
                .count()
                > 1
        })
}

/// Whether `(source, install_id, app_id)` is matched by any entry in `hidden`.
///
/// If a host lists the same non-empty `install_id` on multiple entries in its library, that
/// identifier is ambiguous and must hide neither app.
pub fn is_hidden(
    source: &Source<'_>,
    install_id: &str,
    app_id: u32,
    hidden: &[Favourite],
) -> bool {
    if source.unique_id.is_empty() {
        return false;
    }
    if is_ambiguous_install_id(source, install_id) {
        return hidden.iter().any(|h| {
            h.host == source.unique_id
                && h.install_id.is_empty()
                && h.app_id != 0
                && h.app_id == app_id
        });
    }
    let key = Favourite {
        host: source.unique_id.to_owned(),
        install_id: install_id.to_owned(),
        app_id,
        title: String::new(),
    };
    hidden.iter().any(|h| {
        h.same_game_or_bare(&key) || classic_uuid_fallback(source, install_id, app_id, h)
    })
}

fn classic_uuid_fallback(
    source: &Source<'_>,
    install_id: &str,
    app_id: u32,
    h: &Favourite,
) -> bool {
    if h.host != source.unique_id || h.app_id == 0 || h.app_id != app_id {
        return false;
    }
    // Preserve classic `/applist` UUID fallback only when `source.library` does not identify
    // `install_id` as a different native game or place `h.install_id` at another `app_id`.
    let Some(lib) = source.library else {
        return true;
    };
    let install_id_is_other_native = !install_id.is_empty()
        && lib.games.iter().any(|g| {
            !g.install_id.is_empty()
                && g.install_id.eq_ignore_ascii_case(install_id)
                && !g.install_id.eq_ignore_ascii_case(&h.install_id)
        });
    let hidden_id_is_elsewhere = !h.install_id.is_empty()
        && lib.games.iter().any(|g| {
            !g.install_id.is_empty()
                && g.install_id.eq_ignore_ascii_case(&h.install_id)
                && g.app_id != h.app_id
        });
    !install_id_is_other_native && !hidden_id_is_elsewhere
}

/// Whether `(host_id, install_id, app_id)` is matched by any entry in `hidden`.
pub fn is_hidden_entry(hidden: &[Favourite], host_id: &str, install_id: &str, app_id: u32) -> bool {
    if host_id.is_empty() {
        return false;
    }
    let key = Favourite {
        host: host_id.to_owned(),
        install_id: install_id.to_owned(),
        app_id,
        title: String::new(),
    };
    hidden.iter().any(|h| h.same_game_or_bare(&key))
}

/// Whether `(host, app_id)` in `sources` is NOT hidden.
pub fn not_hidden(sources: &[Source<'_>], host: u64, app_id: u32, hidden: &[Favourite]) -> bool {
    let Some(source) = sources.iter().find(|s| s.id == host) else {
        return true;
    };
    let install_id = source
        .library
        .and_then(|l| l.games.iter().find(|g| g.app_id == app_id))
        .map_or("", |g| g.install_id.as_str());
    !is_hidden(source, install_id, app_id, hidden)
}

/// Whether `(host_id, app_id)` in `sources` is NOT hidden.
pub fn not_hidden_by_id(
    sources: &[Source<'_>],
    host_id: &str,
    app_id: u32,
    hidden: &[Favourite],
) -> bool {
    let Some(source) = sources.iter().find(|s| s.unique_id == host_id) else {
        return true;
    };
    let install_id = source
        .library
        .and_then(|l| l.games.iter().find(|g| g.app_id == app_id))
        .map_or("", |g| g.install_id.as_str());
    !is_hidden(source, install_id, app_id, hidden)
}

/// Splits merged `entries` into `(shown, hidden)` by filtering each copy against `hidden`.
/// An entry appears in `shown` with its non-hidden copies, and in `hidden` with its hidden copies.
pub fn split_hidden(
    entries: Vec<Entry>,
    sources: &[Source<'_>],
    hidden: &[Favourite],
) -> (Vec<Entry>, Vec<Entry>) {
    let mut shown = Vec::new();
    let mut away = Vec::new();
    for entry in entries {
        let mut kept = Vec::new();
        let mut gone = Vec::new();
        for copy in entry.copies {
            let source = sources.iter().find(|s| s.id == copy.host);
            let is_copy_hidden = source.map_or_else(
                || is_hidden_entry(hidden, &copy.host_uid, &copy.install_id, copy.app_id),
                |s| is_hidden(s, &copy.install_id, copy.app_id, hidden),
            );
            if is_copy_hidden {
                gone.push(copy);
            } else {
                kept.push(copy);
            }
        }
        if !kept.is_empty() {
            shown.push(Entry {
                key: entry.key.clone(),
                title: entry.title.clone(),
                copies: kept,
            });
        }
        if !gone.is_empty() {
            away.push(Entry {
                key: entry.key,
                title: entry.title,
                copies: gone,
            });
        }
    }
    (shown, away)
}

/// Groups all stored `hidden` items by host, enriching titles & online state from `sources`.
pub fn build_hidden_listings(
    hidden: &[Favourite],
    sources: &[Source<'_>],
) -> Vec<HiddenListing> {
    let mut by_host: BTreeMap<String, Vec<HiddenEntry>> = BTreeMap::new();
    for item in hidden {
        let live_title = sources
            .iter()
            .find(|s| s.unique_id == item.host)
            .and_then(|s| s.library)
            .and_then(|l| {
                l.games.iter().find(|g| {
                    if !item.install_id.is_empty() && !g.install_id.is_empty() {
                        g.install_id.eq_ignore_ascii_case(&item.install_id)
                    } else {
                        g.app_id == item.app_id
                    }
                })
            })
            .map(|g| g.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| item.title.clone());
        by_host
            .entry(item.host.clone())
            .or_default()
            .push(HiddenEntry {
                host: item.host.clone(),
                install_id: item.install_id.clone(),
                app_id: item.app_id,
                title: live_title,
            });
    }

    let mut out = Vec::with_capacity(by_host.len());
    for (host_id, mut games) in by_host {
        games.sort_by_key(|a| a.title.to_lowercase());
        let (host_name, online) = sources
            .iter()
            .find(|s| s.unique_id == host_id)
            .map_or_else(|| (host_id.clone(), false), |s| (s.name.to_owned(), s.online));
        out.push(HiddenListing {
            host_id,
            host_name,
            online,
            games,
        });
    }
    out.sort_by_key(|a| a.host_name.to_lowercase());
    out
}

/// Reconciles stored `favourites` against live `sources` to repair classic `app_id` renumberings
/// when `install_id` matches in the current library. Returns updated favourites if any changed.
pub fn reconcile_starred(
    favourites: &[Favourite],
    sources: &[Source<'_>],
) -> Option<Vec<Favourite>> {
    let mut changed = false;
    let mut updated = Vec::with_capacity(favourites.len());
    for fav in favourites {
        let mut next = fav.clone();
        if !fav.install_id.is_empty()
            && let Some(source) = sources.iter().find(|s| s.unique_id == fav.host)
            && let Some(lib) = source.library
            && let Some(game) = lib
                .games
                .iter()
                .find(|g| g.install_id.eq_ignore_ascii_case(&fav.install_id))
        {
            if game.app_id != 0 && game.app_id != fav.app_id {
                next.app_id = game.app_id;
                changed = true;
            }
            if !game.title.is_empty() && game.title != fav.title {
                next.title = game.title.clone();
                changed = true;
            }
        }
        updated.push(next);
    }
    changed.then_some(updated)
}
