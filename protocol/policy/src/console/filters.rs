//! Library filter chips, Home row grouping, sorting, search narrowing, and Android Home projection.

use super::merge::{Copy, Entry, Merged, Source};
use super::projections::last_played;
use super::{Favourite, HomeGroup, HomeOrder, HomeSort, Recent};
use crate::library_order::{self, Facts};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

/// Active filter selection for Library / All games / Android Home.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LibraryFilter {
    All,
    Installed,
    NotInstalled,
    Store(String),
    Collection(String),
    NoCollection,
    Games,
    Other,
    Hidden,
}

impl LibraryFilter {
    pub fn key(&self) -> String {
        match self {
            Self::All => "filter:all".into(),
            Self::Installed => "filter:installed".into(),
            Self::NotInstalled => "filter:not_installed".into(),
            Self::Store(s) => format!("filter:store:{s}"),
            Self::Collection(c) => format!("filter:collection:{c}"),
            Self::NoCollection => "filter:no_collection".into(),
            Self::Games => "filter:games".into(),
            Self::Other => "filter:other".into(),
            Self::Hidden => "filter:hidden".into(),
        }
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        match self {
            Self::All | Self::Hidden => true,
            Self::Installed => entry.installed(),
            Self::NotInstalled => !entry.installed(),
            Self::Store(s) => entry.store().is_some_and(|st| st == s),
            Self::Collection(c) => entry.collections().iter().any(|col| col == c),
            Self::NoCollection => entry.collections().is_empty(),
            Self::Games => entry.store().is_some(),
            Self::Other => entry.store().is_none(),
        }
    }
}

/// Wire key representation used by Android `HomeModel` JNI calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HomeFilterKey {
    All,
    Installed,
    NotInstalled,
    Store { id: String },
    Collection { name: String },
    Hidden,
}

impl HomeFilterKey {
    pub fn to_filter(&self) -> LibraryFilter {
        match self {
            Self::All => LibraryFilter::All,
            Self::Installed => LibraryFilter::Installed,
            Self::NotInstalled => LibraryFilter::NotInstalled,
            Self::Store { id } => LibraryFilter::Store(id.clone()),
            Self::Collection { name } => LibraryFilter::Collection(name.clone()),
            Self::Hidden => LibraryFilter::Hidden,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterChip {
    pub filter: LibraryFilter,
    pub store_name: Option<String>,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowKind {
    Store { id: String, name: String },
    Collection { name: String },
    NoCollection,
    Games,
    Other,
    Installed,
    NotInstalled,
    Library,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomeRow<'a> {
    pub kind: RowKind,
    pub filter: LibraryFilter,
    pub games: Vec<&'a Entry>,
}

impl HomeRow<'_> {
    pub fn key(&self) -> String {
        match &self.kind {
            RowKind::Store { id, .. } => format!("row:store:{id}"),
            RowKind::Collection { name } => format!("row:collection:{}", name.to_lowercase()),
            RowKind::NoCollection => "row:no-collection".into(),
            RowKind::Games => "row:games".into(),
            RowKind::Other => "row:other".into(),
            RowKind::Installed => "row:installed".into(),
            RowKind::NotInstalled => "row:not-installed".into(),
            RowKind::Library => "row:library".into(),
        }
    }
}

/// Active sort pair for Home rows and All games.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sort {
    pub by: HomeSort,
    pub order: HomeOrder,
}

/// References used to compute each entry's latest played time during sorting.
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

/// Display name for a store slug: uses `name` if non-empty, else known product name,
/// else `key` with its first character capitalized.
pub fn store_name(key: &str, name: &str) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_owned();
    }
    match key.trim().to_ascii_lowercase().as_str() {
        "steam" => "Steam".into(),
        "epic" => "Epic Games".into(),
        "gog" => "GOG".into(),
        "ea" => "EA".into(),
        "ubisoft" => "Ubisoft Connect".into(),
        "microsoft" | "xbox" => "Xbox".into(),
        "battlenet" => "Battle.net".into(),
        "playnite" => "Playnite".into(),
        other => {
            let mut chars = other.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        }
    }
}

/// Filter chips offered for the non-hidden `entries` slice (`All`, `Installed`/`NotInstalled`,
/// individual stores, collections). Empty when `entries` is empty.
pub fn library_filters(entries: &[Entry]) -> Vec<FilterChip> {
    if entries.is_empty() {
        return Vec::new();
    }
    let mut chips = vec![FilterChip {
        filter: LibraryFilter::All,
        store_name: None,
        count: entries.len(),
    }];
    let installed_count = entries.iter().filter(|e| e.installed()).count();
    let not_installed_count = entries.len() - installed_count;
    if installed_count > 0 && not_installed_count > 0 {
        chips.push(FilterChip {
            filter: LibraryFilter::Installed,
            store_name: None,
            count: installed_count,
        });
    }
    if not_installed_count > 0 {
        chips.push(FilterChip {
            filter: LibraryFilter::NotInstalled,
            store_name: None,
            count: not_installed_count,
        });
    }

    let mut stores: BTreeMap<String, (String, usize)> = BTreeMap::new();
    let mut no_store = 0usize;
    let mut collections: BTreeMap<String, usize> = BTreeMap::new();
    for entry in entries {
        if let Some(s) = entry.store() {
            let name = entry
                .store_name()
                .map_or_else(|| store_name(s, ""), str::to_owned);
            let slot = stores.entry(s.to_owned()).or_insert((name, 0));
            slot.1 += 1;
        } else {
            no_store += 1;
        }
        for c in entry.collections() {
            *collections.entry(c).or_insert(0) += 1;
        }
    }
    if stores.len() > 1 || (!stores.is_empty() && no_store > 0) {
        if stores.len() > 1 {
            let total_games: usize = stores.values().map(|(_, count)| *count).sum();
            chips.push(FilterChip {
                filter: LibraryFilter::Games,
                store_name: None,
                count: total_games,
            });
        }
        let mut store_list: Vec<(String, String, usize)> = stores
            .into_iter()
            .map(|(id, (name, count))| (id, name, count))
            .collect();
        store_list.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
        for (id, name, count) in store_list {
            chips.push(FilterChip {
                filter: LibraryFilter::Store(id),
                store_name: Some(name),
                count,
            });
        }
        if no_store > 0 {
            chips.push(FilterChip {
                filter: LibraryFilter::Other,
                store_name: None,
                count: no_store,
            });
        }
    }
    if !collections.is_empty() {
        let without_collections = entries
            .iter()
            .filter(|e| e.collections().is_empty())
            .count();
        for (name, count) in collections {
            chips.push(FilterChip {
                filter: LibraryFilter::Collection(name.clone()),
                store_name: Some(name),
                count,
            });
        }
        if without_collections > 0 {
            chips.push(FilterChip {
                filter: LibraryFilter::NoCollection,
                store_name: None,
                count: without_collections,
            });
        }
    }
    chips
}

/// Which `HomeGroup` options make sense for `entries`.
pub fn offered_groups(entries: &[Entry]) -> Vec<HomeGroup> {
    let has_collections = entries.iter().any(|e| !e.collections().is_empty());
    let installed_count = entries.iter().filter(|e| e.installed()).count();
    let has_both_install = installed_count > 0 && installed_count < entries.len();
    let mut out = vec![HomeGroup::Store, HomeGroup::Type];
    if has_collections {
        out.push(HomeGroup::Collection);
    }
    if has_both_install {
        out.push(HomeGroup::Installation);
    }
    out.push(HomeGroup::None);
    out
}

/// Effective `HomeGroup` after falling back when `group` has no basis in `entries`.
pub fn shown_group(group: HomeGroup, entries: &[Entry]) -> HomeGroup {
    if offered_groups(entries).contains(&group) {
        group
    } else {
        HomeGroup::Store
    }
}

/// Which `HomeSort` options are offered for `entries`: Title and LastPlayed always;
/// Playtime only when some PC reports playtime.
pub fn offered_sorts(entries: &[Entry]) -> Vec<HomeSort> {
    let mut out = vec![HomeSort::Title, HomeSort::LastPlayed];
    if entries.iter().any(|e| e.playtime_minutes().is_some()) {
        out.push(HomeSort::Playtime);
    }
    out
}

pub fn sort_entries<'a>(entries: &mut Vec<&'a Entry>, sort: Sort, played: Played<'_>) {
    let keys: Vec<Facts> = entries
        .iter()
        .map(|entry| Facts {
            title: entry.title.clone(),
            last_played: last_played(entry, played.sources, played.recents),
            playtime_minutes: entry.playtime_minutes(),
        })
        .collect();
    let order = library_order::order(&keys, sort.by.key(), sort.order.direction());
    let sorted: Vec<&'a Entry> = order.into_iter().map(|idx| entries[idx]).collect();
    *entries = sorted;
}

/// Filters `pool` by `filter` and sorts using `sort` + `played`.
pub fn filtered<'a>(
    pool: &'a [Entry],
    filter: &LibraryFilter,
    sort: Sort,
    played: Played<'_>,
) -> Vec<&'a Entry> {
    let mut out: Vec<&'a Entry> = pool.iter().filter(|e| filter.matches(e)).collect();
    sort_entries(&mut out, sort, played);
    out
}

/// Groups `entries` into ordered Home shelves according to `group`, sorting each shelf with
/// installed games first and each partition in `sort` order.
pub fn home_rows<'a>(
    entries: &'a [Entry],
    group: HomeGroup,
    sort: Sort,
    played: Played<'_>,
) -> Vec<HomeRow<'a>> {
    let sort_vec = |refs: Vec<&'a Entry>| -> Vec<&'a Entry> {
        let mut installed: Vec<&'a Entry> = refs.iter().copied().filter(|e| e.installed()).collect();
        let mut not_installed: Vec<&'a Entry> = refs.into_iter().filter(|e| !e.installed()).collect();
        sort_entries(&mut installed, sort, played);
        sort_entries(&mut not_installed, sort, played);
        installed.into_iter().chain(not_installed).collect()
    };
    match shown_group(group, entries) {
        HomeGroup::None => {
            if entries.is_empty() {
                Vec::new()
            } else {
                vec![HomeRow {
                    kind: RowKind::Library,
                    filter: LibraryFilter::All,
                    games: sort_vec(entries.iter().collect()),
                }]
            }
        }
        HomeGroup::Type => {
            let mut games = Vec::new();
            let mut other = Vec::new();
            for entry in entries {
                if entry.store().is_some() {
                    games.push(entry);
                } else {
                    other.push(entry);
                }
            }
            let mut rows = Vec::new();
            if !games.is_empty() {
                rows.push(HomeRow {
                    kind: RowKind::Games,
                    filter: LibraryFilter::Games,
                    games: sort_vec(games),
                });
                if !other.is_empty() {
                    rows.push(HomeRow {
                        kind: RowKind::Other,
                        filter: LibraryFilter::Other,
                        games: sort_vec(other),
                    });
                }
            } else if !other.is_empty() {
                rows.push(HomeRow {
                    kind: RowKind::Library,
                    filter: LibraryFilter::All,
                    games: sort_vec(other),
                });
            }
            rows
        }
        HomeGroup::Installation => {
            let mut installed = Vec::new();
            let mut not_installed = Vec::new();
            for entry in entries {
                if entry.installed() {
                    installed.push(entry);
                } else {
                    not_installed.push(entry);
                }
            }
            let mut rows = Vec::new();
            if !installed.is_empty() {
                rows.push(HomeRow {
                    kind: RowKind::Installed,
                    filter: LibraryFilter::Installed,
                    games: sort_vec(installed),
                });
            }
            if !not_installed.is_empty() {
                rows.push(HomeRow {
                    kind: RowKind::NotInstalled,
                    filter: LibraryFilter::NotInstalled,
                    games: sort_vec(not_installed),
                });
            }
            rows
        }
        HomeGroup::Collection => {
            let mut cols: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
            let mut none = Vec::new();
            for entry in entries {
                let entry_cols = entry.collections();
                if entry_cols.is_empty() {
                    none.push(entry);
                } else {
                    for c in entry_cols {
                        cols.entry(c).or_default().push(entry);
                    }
                }
            }
            let mut rows: Vec<HomeRow> = cols
                .into_iter()
                .map(|(name, list)| HomeRow {
                    filter: LibraryFilter::Collection(name.clone()),
                    kind: RowKind::Collection { name },
                    games: sort_vec(list),
                })
                .collect();
            if !none.is_empty() {
                rows.push(HomeRow {
                    kind: RowKind::NoCollection,
                    filter: LibraryFilter::NoCollection,
                    games: sort_vec(none),
                });
            }
            rows
        }
        HomeGroup::Store => {
            let mut stores: BTreeMap<String, (String, Vec<&Entry>)> = BTreeMap::new();
            let mut other = Vec::new();
            for entry in entries {
                if let Some(id) = entry.store() {
                    let name = entry
                        .store_name()
                        .map_or_else(|| store_name(id, ""), str::to_owned);
                    stores
                        .entry(id.to_owned())
                        .or_insert_with(|| (name, Vec::new()))
                        .1
                        .push(entry);
                } else {
                    other.push(entry);
                }
            }
            let mut store_rows: Vec<HomeRow<'a>> = stores
                .into_iter()
                .map(|(id, (name, list))| HomeRow {
                    filter: LibraryFilter::Store(id.clone()),
                    kind: RowKind::Store { id, name },
                    games: sort_vec(list),
                })
                .collect();
            store_rows.sort_by_key(|a| std::cmp::Reverse(a.games.len()));
            if !other.is_empty() {
                store_rows.push(HomeRow {
                    kind: RowKind::Other,
                    filter: LibraryFilter::Other,
                    games: sort_vec(other),
                });
            }
            if store_rows.len() == 1
                && matches!(store_rows[0].kind, RowKind::Other)
                && let Some(only) = store_rows.first_mut()
            {
                only.kind = RowKind::Library;
                only.filter = LibraryFilter::All;
            }
            store_rows
        }
    }
}

/// How many items of a row fit before an "All" tile, given `columns`.
pub fn fit_row(total: usize, columns: usize) -> (usize, bool) {
    if total <= columns {
        (total, false)
    } else {
        (columns.saturating_sub(1), true)
    }
}

#[derive(PartialEq, Eq)]
struct MemoSourceSig {
    id: u64,
    unique_id: String,
    name: String,
    online: bool,
    paired: bool,
    classic: bool,
    host_control: bool,
    library: Option<crate::http::native_api::library::Library>,
    last_played: Option<BTreeMap<String, u64>>,
    playtime_minutes: Option<BTreeMap<String, u64>>,
    collections: Option<BTreeMap<String, Vec<String>>>,
    session: Option<crate::http::native_api::session::Session>,
}

type LibraryMemoEntry = (Vec<MemoSourceSig>, Vec<Favourite>, Merged);

/// Simple frame-level memo cache for merged + hidden-split library on Desktop UI.
#[derive(Default)]
pub struct LibraryMemo {
    cached: RefCell<Option<LibraryMemoEntry>>,
    merges: RefCell<u64>,
}

impl LibraryMemo {
    pub fn get(&self, sources: &[Source<'_>], hidden: &[Favourite]) -> Merged {
        let sig: Vec<MemoSourceSig> = sources
            .iter()
            .map(|s| MemoSourceSig {
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
            .collect();
        if let Some((prev_sig, prev_hidden, prev_merged)) = self.cached.borrow().as_ref()
            && prev_sig == &sig
            && prev_hidden == hidden
        {
            return prev_merged.clone();
        }
        let all = super::merge::merge_library(sources);
        let (shown, hidden_entries) = super::hidden::split_hidden(all.clone(), sources, hidden);
        let merged = Merged::from_parts(all, shown, hidden_entries);
        *self.merges.borrow_mut() += 1;
        *self.cached.borrow_mut() = Some((sig, hidden.to_vec(), merged.clone()));
        merged
    }

    pub fn merges(&self) -> u64 {
        *self.merges.borrow()
    }
}

/// Serialized filter chip DTO for Android `HomeModel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedChip {
    pub key: String,
    pub label: String,
    pub count: usize,
    pub selected: bool,
}

/// Serialized library card DTO for Android `HomeModel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectedCard {
    pub key: String,
    pub title: String,
    pub host_id: String,
    pub host_name: String,
    pub host_online: bool,
    pub install_id: String,
    pub app_id: u32,
    pub added: bool,
    pub installed: bool,
    pub running: bool,
    pub host_count: usize,
    pub store_source: Option<String>,
    pub store_name: Option<String>,
    pub cover_path: Option<String>,
    pub cover_version: String,
    pub last_played: Option<u64>,
    pub playtime_minutes: Option<u64>,
    pub copies: Vec<Copy>,
}

/// Complete Home projection produced in Rust for Android `HomeModel`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeProjection {
    pub active_filter_key: String,
    pub chips: Vec<ProjectedChip>,
    pub cards: Vec<ProjectedCard>,
    pub hidden_count: usize,
    pub available_sorts: Vec<HomeSort>,
}

/// Filters/searches/sorts `merged` entries for Android `HomeModel`.
pub fn project_home(
    shown: &[Entry],
    hidden: &[Entry],
    sources: &[Source<'_>],
    recents: &[Recent],
    requested_filter: &LibraryFilter,
    query: &str,
    sort: Sort,
) -> HomeProjection {
    let mut raw_chips = library_filters(shown);
    if !hidden.is_empty() {
        raw_chips.push(FilterChip {
            filter: LibraryFilter::Hidden,
            store_name: None,
            count: hidden.len(),
        });
    }
    let active_filter = if raw_chips.iter().any(|c| &c.filter == requested_filter) {
        requested_filter.clone()
    } else {
        raw_chips
            .first()
            .map_or(LibraryFilter::All, |c| c.filter.clone())
    };
    let words = query.trim().to_lowercase();
    let matches_query = |entry: &Entry| -> bool {
        if words.is_empty() {
            return true;
        }
        entry.title.to_lowercase().contains(&words)
            || entry
                .store_name()
                .is_some_and(|s| s.to_lowercase().contains(&words))
            || entry
                .copies
                .iter()
                .any(|c| c.host_name.to_lowercase().contains(&words))
            || entry
                .collections()
                .iter()
                .any(|c| c.to_lowercase().contains(&words))
    };

    let pool = if active_filter == LibraryFilter::Hidden {
        hidden
    } else {
        shown
    };
    let played = Played { sources, recents };
    let mut visible: Vec<&Entry> = pool
        .iter()
        .filter(|e| active_filter.matches(e) && matches_query(e))
        .collect();
    sort_entries(&mut visible, sort, played);

    let chips: Vec<ProjectedChip> = raw_chips
        .into_iter()
        .map(|c| {
            let label = match &c.filter {
                LibraryFilter::All => "All".into(),
                LibraryFilter::Installed => "Installed".into(),
                LibraryFilter::NotInstalled => "Not installed".into(),
                LibraryFilter::Hidden => "Hidden".into(),
                LibraryFilter::Other => "Other".into(),
                LibraryFilter::NoCollection => "No collection".into(),
                LibraryFilter::Games => "Games".into(),
                LibraryFilter::Store(id) => c.store_name.unwrap_or_else(|| store_name(id, "")),
                LibraryFilter::Collection(name) => c.store_name.unwrap_or_else(|| name.clone()),
            };
            ProjectedChip {
                key: c.filter.key(),
                label,
                count: c.count,
                selected: c.filter == active_filter,
            }
        })
        .collect();

    let mut host_ids_seen = BTreeSet::new();
    let cards: Vec<ProjectedCard> = visible
        .into_iter()
        .map(|entry| {
            host_ids_seen.clear();
            for copy in &entry.copies {
                host_ids_seen.insert(copy.host_uid.clone());
            }
            let best = entry.best();
            let lp = last_played(entry, sources, recents);
            ProjectedCard {
                key: entry.key.clone(),
                title: entry.title.clone(),
                host_id: best.host_uid.clone(),
                host_name: best.host_name.clone(),
                host_online: best.online,
                install_id: best.install_id.clone(),
                app_id: best.app_id,
                added: entry.added(),
                installed: entry.installed(),
                running: entry.running(),
                host_count: host_ids_seen.len().max(1),
                store_source: entry.store().map(str::to_owned),
                store_name: entry.store_name().map(str::to_owned),
                cover_path: entry.cover().and_then(|c| c.cover_path.clone()),
                cover_version: entry
                    .cover()
                    .map_or_else(String::new, |c| c.cover_version.clone()),
                last_played: lp,
                playtime_minutes: entry.playtime_minutes(),
                copies: entry.copies.clone(),
            }
        })
        .collect();

    HomeProjection {
        active_filter_key: active_filter.key(),
        chips,
        cards,
        hidden_count: hidden.len(),
        available_sorts: offered_sorts(shown),
    }
}
