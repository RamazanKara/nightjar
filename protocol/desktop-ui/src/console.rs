//! Thin UI adapter over `nightjar_policy::console`: binds Desktop's `Host`, `Settings`, and
//! localized UI strings (`tr`/`trf`) while keeping all cross-host merge, filter, group, sort,
//! hidden, and detail rules in `nightjar-policy`.

use std::collections::BTreeMap;
use std::rc::Rc;

use nightjar_policy::console as policy;
use nightjar_policy::http::native_api::library::{Game, Library};
use nightjar_policy::http::native_api::session::Session;
#[cfg(test)]
use nightjar_policy::http::native_api::session::State;

use crate::config::{Favourite, HomeGroup, HomeOrder, HomeSort, Recent, Settings};
use crate::hosts_model::{Host, Reach};
use crate::strings::{tr, trf};

pub use policy::{
    classic_app, classic_library, entry_key_of, fit_row, library_filters, neighbour,
    offered_groups, offered_sorts, recheck_running, saved_classic_app, shown_group, store_name,
    stream_summary, Copy, Detail, EndAction, Entry, FilterChip, GameState, LibraryFilter,
    Playing, Recently, RowKind, Running, Starred, CLASSIC_REVISION,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Continue {
    pub host: u64,
    pub host_name: String,
    pub app_id: u32,
    pub title: String,
    pub playing: Playing,
}

pub type LibraryRow<'a> = policy::HomeRow<'a>;

#[derive(Clone, Copy)]
pub struct Source<'a> {
    pub id: u64,
    pub unique_id: &'a str,
    pub name: &'a str,
    pub online: bool,
    pub paired: bool,
    pub library: Option<&'a Library>,
    pub classic: bool,
    pub last_played: Option<&'a BTreeMap<String, u64>>,
    pub playtime_minutes: Option<&'a BTreeMap<String, u64>>,
    pub collections: Option<&'a BTreeMap<String, Vec<String>>>,
    pub session: Option<&'a Session>,
    pub host_control: bool,
}

impl<'a> Source<'a> {
    pub fn of(host: &'a Host) -> Self {
        let library = host.native.library.as_ref().or(host.classic.as_ref());
        let classic = host.native.library.is_none() && host.classic.is_some();
        Self {
            id: host.id,
            unique_id: &host.record.unique_id,
            name: host.display_name(),
            online: host.reach == Reach::Online,
            paired: host.paired(),
            library,
            classic,
            last_played: (!host.native.last_played.is_empty()).then_some(&host.native.last_played),
            playtime_minutes: (!host.native.playtime_minutes.is_empty()).then_some(&host.native.playtime_minutes),
            collections: (!host.native.collections.is_empty()).then_some(&host.native.collections),
            session: host.native.session.as_ref(),
            host_control: host
                .native
                .capabilities
                .as_ref()
                .is_some_and(|c| c.capabilities.permissions.host_control),
        }
    }

    fn as_policy(&self) -> policy::Source<'a> {
        policy::Source {
            id: self.id,
            unique_id: self.unique_id,
            name: self.name,
            online: self.online,
            paired: self.paired,
            library: self.library,
            classic: self.classic,
            last_played: self.last_played,
            playtime_minutes: self.playtime_minutes,
            collections: self.collections,
            session: self.session,
            host_control: self.host_control,
        }
    }
}

fn to_policy<'a>(sources: &[Source<'a>]) -> Vec<policy::Source<'a>> {
    sources.iter().map(Source::as_policy).collect()
}

pub fn opens_on_home(enabled: bool, sources: &[Source<'_>]) -> bool {
    policy::opens_on_home(enabled, &to_policy(sources))
}

pub fn has_library(sources: &[Source<'_>]) -> bool {
    policy::has_library(&to_policy(sources))
}

pub fn has_native_library(sources: &[Source<'_>]) -> bool {
    policy::has_native_library(&to_policy(sources))
}

pub fn merge_library(sources: &[Source<'_>]) -> Vec<Entry> {
    policy::merge_library(&to_policy(sources))
}

pub fn continue_playing(sources: &[Source<'_>]) -> Vec<Continue> {
    policy::continue_playing(&to_policy(sources))
        .into_iter()
        .map(|c| Continue {
            host: c.host,
            host_name: c.host_name,
            app_id: c.app_id,
            title: c.title,
            playing: c.playing,
        })
        .collect()
}

pub fn recently_shown(
    sources: &[Source<'_>],
    recents: &[Recent],
    playing: &[Continue],
    hidden: &[Favourite],
    limit: usize,
) -> Vec<Recently> {
    let p: Vec<policy::Continue> = playing
        .iter()
        .map(|c| policy::Continue {
            host: c.host,
            host_uid: String::new(),
            host_name: c.host_name.clone(),
            app_id: c.app_id,
            install_id: String::new(),
            title: c.title.clone(),
            playing: c.playing,
            suspended: c.playing == Playing::Suspended,
            other_device: None,
            cover_path: None,
            cover_version: String::new(),
        })
        .collect();
    policy::recently_shown(&to_policy(sources), recents, &p, hidden, limit)
}

#[cfg(test)]
pub fn recently_played(
    sources: &[Source<'_>],
    recents: &[Recent],
    playing: &[Continue],
    limit: usize,
) -> Vec<Recently> {
    recently_shown(sources, recents, playing, &[], limit)
}

pub fn favourite_games(sources: &[Source<'_>], favourites: &[Favourite]) -> Vec<Starred> {
    policy::favourite_games(&to_policy(sources), favourites)
}

pub fn is_starred(source: &Source<'_>, game: &Game, favourites: &[Favourite]) -> bool {
    policy::is_starred(&source.as_policy(), game, favourites)
}

pub fn favourite_of(source: &Source<'_>, game: &Game) -> Option<Favourite> {
    policy::favourite_of(&source.as_policy(), game)
}

pub fn favourite_for(sources: &[Source<'_>], host: u64, app_id: u32) -> Option<Favourite> {
    policy::favourite_for(&to_policy(sources), host, app_id)
}

pub fn detail(source: &Source<'_>, app_id: u32, native: bool) -> Detail {
    policy::detail(&source.as_policy(), app_id, native)
}

pub fn active(source: &Source<'_>) -> bool {
    policy::active(&source.as_policy())
}

pub fn live(session: &Session) -> bool {
    policy::live(session)
}

pub fn is_hidden(source: &Source<'_>, install_id: &str, app_id: u32, hidden: &[Favourite]) -> bool {
    policy::is_hidden(&source.as_policy(), install_id, app_id, hidden)
}

pub fn not_hidden(sources: &[Source<'_>], host: u64, app_id: u32, hidden: &[Favourite]) -> bool {
    policy::not_hidden(&to_policy(sources), host, app_id, hidden)
}

pub fn split_hidden(entries: Vec<Entry>, sources: &[Source<'_>], hidden: &[Favourite]) -> (Vec<Entry>, Vec<Entry>) {
    policy::split_hidden(entries, &to_policy(sources), hidden)
}

pub fn hide_keys(sources: &[Source<'_>], host: u64, app_id: u32) -> Vec<Favourite> {
    policy::hide_keys(&to_policy(sources), host, app_id)
}

pub fn last_played(entry: &Entry, sources: &[Source<'_>], recents: &[Recent]) -> Option<u64> {
    policy::last_played(entry, &to_policy(sources), recents)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Merged(policy::Merged);

impl Merged {
    pub fn of(sources: &[Source<'_>], hidden: &[Favourite]) -> Self {
        let p = to_policy(sources);
        let all = policy::merge_library(&p);
        let (shown, away) = policy::split_hidden(all.clone(), &p, hidden);
        Self(policy::Merged::from_parts(all, shown, away))
    }

    pub fn all(&self) -> &[Entry] {
        self.0.all()
    }

    pub fn shown(&self) -> &[Entry] {
        self.0.shown()
    }

    pub fn hidden(&self) -> &[Entry] {
        self.0.hidden()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MemoKey {
    sources: Vec<SourceSig>,
    hidden: Vec<Favourite>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceSig {
    id: u64,
    unique_id: String,
    name: String,
    online: bool,
    paired: bool,
    classic: bool,
    host_control: bool,
    library: Option<Library>,
    last_played: Option<BTreeMap<String, u64>>,
    playtime_minutes: Option<BTreeMap<String, u64>>,
    collections: Option<BTreeMap<String, Vec<String>>>,
    session: Option<nightjar_policy::http::native_api::session::Session>,
}

#[derive(Default)]
pub struct LibraryMemo {
    cached: std::cell::RefCell<Option<(MemoKey, Rc<Merged>)>>,
    merges: std::cell::RefCell<u64>,
}

impl LibraryMemo {
    pub fn get(&self, sources: &[Source<'_>], hidden: &[Favourite]) -> Rc<Merged> {
        let key = MemoKey {
            sources: sources
                .iter()
                .map(|s| SourceSig {
                    id: s.id,
                    unique_id: s.unique_id.to_owned(),
                    name: s.name.to_owned(),
                    online: s.online,
                    paired: s.paired,
                    classic: s.classic,
                    host_control: s.host_control,
                    library: s.library.cloned(),
                    last_played: s.last_played.cloned(),
                    playtime_minutes: s.playtime_minutes.cloned(),
                    collections: s.collections.cloned(),
                    session: s.session.cloned(),
                })
                .collect(),
            hidden: hidden.to_vec(),
        };
        if let Some((prev_key, prev)) = self.cached.borrow().as_ref()
            && prev_key == &key
        {
            return Rc::clone(prev);
        }
        let merged = Rc::new(Merged::of(sources, hidden));
        *self.merges.borrow_mut() += 1;
        *self.cached.borrow_mut() = Some((key, Rc::clone(&merged)));
        merged
    }

    pub fn merges(&self) -> u64 {
        *self.merges.borrow()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sort {
    pub by: HomeSort,
    pub order: HomeOrder,
}

impl Sort {
    pub fn of(settings: &Settings) -> Self {
        Self {
            by: settings.home_sort,
            order: settings.home_order,
        }
    }

    pub fn shown(settings: &Settings, entries: &[Entry]) -> Self {
        let offered = offered_sorts(entries);
        let by = if offered.contains(&settings.home_sort) {
            settings.home_sort
        } else {
            HomeSort::Title
        };
        Self {
            by,
            order: settings.home_order,
        }
    }

    fn as_policy(self) -> policy::Sort {
        policy::Sort {
            by: self.by,
            order: self.order,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Played<'a> {
    pub sources: &'a [Source<'a>],
    pub recents: &'a [Recent],
}

impl<'a> Played<'a> {
    pub fn last(self, entry: &Entry) -> Option<u64> {
        last_played(entry, self.sources, self.recents)
    }
}

pub fn filtered<'a>(
    pool: &'a [Entry],
    filter: &LibraryFilter,
    sort: Sort,
    played: Played<'_>,
) -> Vec<&'a Entry> {
    let p = to_policy(played.sources);
    policy::filtered(
        pool,
        filter,
        sort.as_policy(),
        policy::Played {
            sources: &p,
            recents: played.recents,
        },
    )
}

pub fn home_rows<'a>(
    entries: &'a [Entry],
    group: HomeGroup,
    sort: Sort,
    played: Played<'_>,
) -> Vec<LibraryRow<'a>> {
    let p = to_policy(played.sources);
    policy::home_rows(
        entries,
        group,
        sort.as_policy(),
        policy::Played {
            sources: &p,
            recents: played.recents,
        },
    )
}

pub fn library_rows<'a>(
    entries: &'a [Entry],
    sort: Sort,
    played: Played<'_>,
) -> Vec<LibraryRow<'a>> {
    home_rows(entries, HomeGroup::Store, sort, played)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostLine {
    Games(usize),
    SavedGames(usize),
    Offline,
    NoLibrary,
}

impl HostLine {
    pub fn text(self) -> String {
        match self {
            Self::Games(n) => trf("Games: {}", &[&n.to_string()]),
            Self::SavedGames(n) => trf("Offline. Saved games: {}", &[&n.to_string()]),
            Self::Offline => tr("Offline").to_owned(),
            Self::NoLibrary => tr("Games not loaded yet").to_owned(),
        }
    }
}

pub fn host_line(source: &Source<'_>) -> HostLine {
    match policy::host_line(&source.as_policy()) {
        policy::HostLine::Games(n) => HostLine::Games(n),
        policy::HostLine::SavedGames(n) => HostLine::SavedGames(n),
        policy::HostLine::Offline => HostLine::Offline,
        policy::HostLine::NoLibrary => HostLine::NoLibrary,
    }
}

pub fn list_line(entry: &Entry, played: Option<u64>, now: u64) -> String {
    list_line_with_day(entry, played, now, crate::platform_hooks::hooks().local_day)
}

pub(crate) fn list_line_with_day(entry: &Entry, played: Option<u64>, now: u64, local_day: fn(u64) -> i64) -> String {
    let when = played.map(|at| {
        if at == 0 {
            tr("Not played yet").to_owned()
        } else {
            let days = (local_day(now) - local_day(at)).max(0);
            match days {
                0 => tr("Played today").to_owned(),
                1 => tr("Played yesterday").to_owned(),
                n => trf("Played {} days ago", &[&n.to_string()]),
            }
        }
    });
    match (entry.store_name(), when) {
        (Some(s), Some(w)) => format!("{s} · {w}"),
        (Some(s), None) => s.to_owned(),
        (None, Some(w)) => w,
        (None, None) => String::new(),
    }
}

#[cfg(test)]
mod tests;
