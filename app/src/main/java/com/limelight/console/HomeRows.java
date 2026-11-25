/**
 * Package-private projection helpers for {@link HomeModel}: cross-host library shelf assembly,
 * installed-first ordering, chip filter counts, last-played timestamps, and running-session badges.
 */
package com.limelight.console;

import com.limelight.nativeapi.ConsoleSnapshot;
import com.limelight.nativeapi.NativeCapabilities;
import com.limelight.nativeapi.NativeSession;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

final class HomeRows {
    private HomeRows() {
    }

    static List<HomeModel.Row> libraryRows(ConsoleSnapshot snapshot, Set<String> starred,
                                           List<HomeModel.GameCard> allGames, Set<String> launchDenied,
                                           String only) {
        Map<String, List<HomeModel.GameCard>> byStore = new LinkedHashMap<>();
        Map<String, String> storeNames = new HashMap<>();
        List<HomeModel.GameCard> other = new ArrayList<>();
        List<HomeModel.GameCard> unadded = new ArrayList<>();
        List<HomeModel.GameCard> played = new ArrayList<>();
        for (ConsoleSnapshot.Title title : snapshot.titles) {
            ConsoleSnapshot.Source installed = null;
            ConsoleSnapshot.Source missing = null;
            ConsoleSnapshot.Source found = null;
            boolean addable = false;
            int hosts = 0;
            List<String> listing = new ArrayList<>();
            List<ConsoleSnapshot.Source> copies = new ArrayList<>();
            List<ConsoleSnapshot.Source> away = new ArrayList<>();
            List<String> awayHosts = new ArrayList<>();
            for (ConsoleSnapshot.Source source : title.sources) {
                if (only != null && !only.equals(source.hostUuid)) continue;
                if (source.hidden) {
                    away.add(source);
                    awayHosts.add(source.hostUuid);
                    continue;
                }
                if (!source.added) {
                    ConsoleSnapshot.Host host = snapshot.host(source.hostUuid);
                    if (found == null && host != null && host.has(NativeCapabilities.LIBRARY_UNADDED)) {
                        found = source;
                        addable = host.has(NativeCapabilities.LIBRARY_ADD) && !launchDenied.contains(host.uuid);
                    }
                    continue;
                }
                hosts++;
                listing.add(source.hostUuid);
                copies.add(source);
                if (Boolean.FALSE.equals(source.installed)) {
                    if (missing == null) missing = source;
                } else if (installed == null) {
                    installed = source;
                }
            }
            if (!away.isEmpty()) {
                ConsoleSnapshot.Source first = away.get(0);
                allGames.add(new HomeModel.GameCard(HomeModel.HIDDEN_KEY + title.key, title.title, first.hostUuid,
                        first.hostName, first.appId, badgeOf(snapshot, first), first, away.size(),
                        starred.contains(first.hostUuid + ":" + first.appId), false, awayHosts,
                        lastPlayed(title, only), away));
            }
            ConsoleSnapshot.Source best = installed != null ? installed : missing;
            if (best == null) {
                if (found != null) {
                    HomeModel.GameCard card = new HomeModel.GameCard("unadded:" + title.key, title.title,
                            found.hostUuid, found.hostName, found.appId, HomeModel.Badge.NONE, found, 1, false,
                            addable);
                    unadded.add(card);
                    allGames.add(card);
                }
                continue;
            }
            boolean star = starred.contains(best.hostUuid + ":" + best.appId);
            HomeModel.GameCard card = new HomeModel.GameCard("game:" + title.key, title.title, best.hostUuid,
                    best.hostName, best.appId, badgeOf(snapshot, best), best, hosts, star, false, listing,
                    lineLastPlayed(snapshot, title, only), copies);
            allGames.add(card);
            if (best.storeSource == null) {
                other.add(card);
            } else {
                byStore.computeIfAbsent(best.storeSource, k -> new ArrayList<>()).add(card);
                if (!storeNames.containsKey(best.storeSource) && best.storeName != null) {
                    storeNames.put(best.storeSource, best.storeName);
                }
            }
            Long last = lastPlayed(title, only);
            if (last != null && last > 0) {
                played.add(new HomeModel.GameCard("recent:" + title.key, title.title, best.hostUuid, best.hostName,
                        best.appId, card.badge, best, hosts, star, false, listing, last, copies));
            }
        }
        List<HomeModel.Row> rows = new ArrayList<>();
        if (!played.isEmpty()) {
            Map<String, Long> at = new HashMap<>();
            for (ConsoleSnapshot.Title title : snapshot.titles) {
                Long last = lastPlayed(title, only);
                if (last != null && last > 0) at.put("recent:" + title.key, last);
            }
            Collections.sort(played, (a, b) -> Long.compare(at.get(b.key), at.get(a.key)));
            if (played.size() > HomeModel.ROW_CAP) played = new ArrayList<>(played.subList(0, HomeModel.ROW_CAP));
            rows.add(new HomeModel.Row("shelf:recent", HomeModel.RowKind.RECENT, null, null, played,
                    HomeModel.FILTER_ALL, false));
        }
        List<String> stores = new ArrayList<>(byStore.keySet());
        Collections.sort(stores, (a, b) -> {
            int bySize = Integer.compare(byStore.get(b).size(), byStore.get(a).size());
            return bySize != 0 ? bySize : a.compareTo(b);
        });
        for (String store : stores) {
            rows.add(new HomeModel.Row("shelf:store:" + store, HomeModel.RowKind.STORE, store,
                    Stores.name(store, storeNames.get(store)), installedFirst(byStore.get(store)),
                    HomeModel.STORE_FILTER + store, true));
        }
        if (!other.isEmpty()) {
            boolean plain = stores.isEmpty();
            rows.add(new HomeModel.Row(plain ? HomeModel.LIBRARY_ROW : "shelf:other", HomeModel.RowKind.OTHER, null,
                    null, installedFirst(other), plain ? HomeModel.FILTER_ALL : HomeModel.FILTER_OTHER, true));
        }
        if (!unadded.isEmpty()) {
            rows.add(new HomeModel.Row("shelf:unadded", HomeModel.RowKind.UNADDED, null, null,
                    installedFirst(unadded), HomeModel.FILTER_UNADDED, true));
        }
        return rows;
    }

    static Long lineLastPlayed(ConsoleSnapshot snapshot, ConsoleSnapshot.Title title, String only) {
        Long hosts = lastPlayed(title, only);
        if (hosts != null) return hosts;
        Long newest = null;
        for (ConsoleSnapshot.Continue recent : snapshot.continuePlaying) {
            if (recent.kind != ConsoleSnapshot.ContinueKind.RECENT || recent.playedAtMs <= 0) continue;
            for (ConsoleSnapshot.Source source : title.sources) {
                if (only != null && !only.equals(source.hostUuid)) continue;
                if (source.added && source.hostUuid.equals(recent.hostUuid) && source.appId == recent.appId) {
                    long at = recent.playedAtMs / 1000;
                    if (newest == null || at > newest) newest = at;
                }
            }
        }
        return newest;
    }

    static Long lastPlayed(ConsoleSnapshot.Title title, String only) {
        Long newest = null;
        for (ConsoleSnapshot.Source source : title.sources) {
            if (only != null && !only.equals(source.hostUuid)) continue;
            if (source.added && source.lastPlayed != null && (newest == null || source.lastPlayed > newest)) {
                newest = source.lastPlayed;
            }
        }
        return newest;
    }

    static List<HomeModel.GameCard> installedFirst(List<HomeModel.GameCard> cards) {
        List<HomeModel.GameCard> sorted = new ArrayList<>(cards);
        Collections.sort(sorted, (a, b) -> Boolean.compare(a.notInstalled(), b.notInstalled()));
        return sorted;
    }

    static List<HomeModel.Filter> filtersOf(List<HomeModel.GameCard> allGames, List<CollectionGroups.Group> groups) {
        int added = 0, installed = 0, other = 0, unadded = 0, hidden = 0;
        Map<String, Integer> stores = new LinkedHashMap<>();
        Map<String, String> names = new HashMap<>();
        for (HomeModel.GameCard card : allGames) {
            if (card.hidden()) { hidden++; continue; }
            if (card.notAdded()) { unadded++; continue; }
            added++;
            if (!card.notInstalled()) installed++;
            String store = card.store();
            if (store == null) {
                other++;
            } else {
                Integer count = stores.get(store);
                stores.put(store, count == null ? 1 : count + 1);
                if (!names.containsKey(store)) names.put(store, card.storeName());
            }
        }
        List<HomeModel.Filter> filters = new ArrayList<>();
        if (added == 0 && unadded == 0 && hidden == 0) return filters;
        filters.add(new HomeModel.Filter(HomeModel.FILTER_ALL, null, added));
        if (installed < added) filters.add(new HomeModel.Filter(HomeModel.FILTER_INSTALLED, null, installed));
        if (installed < added && installed > 0) filters.add(new HomeModel.Filter(HomeModel.FILTER_NOT_INSTALLED, null, added - installed));
        if (other > 0 && !stores.isEmpty()) filters.add(new HomeModel.Filter(HomeModel.FILTER_GAMES, null, added - other));
        List<String> order = new ArrayList<>(stores.keySet());
        Collections.sort(order, (a, b) -> {
            int bySize = Integer.compare(stores.get(b), stores.get(a));
            return bySize != 0 ? bySize : a.compareTo(b);
        });
        for (String store : order) {
            filters.add(new HomeModel.Filter(HomeModel.STORE_FILTER + store, names.get(store), stores.get(store)));
        }
        if (other > 0 && !stores.isEmpty()) filters.add(new HomeModel.Filter(HomeModel.FILTER_OTHER, null, other));
        for (CollectionGroups.Group group : groups) {
            filters.add(group.name == null
                    ? new HomeModel.Filter(HomeModel.FILTER_NO_COLLECTION, null, group.games.length)
                    : new HomeModel.Filter(HomeModel.COLLECTION_FILTER + group.name, group.name, group.games.length));
        }
        if (unadded > 0) filters.add(new HomeModel.Filter(HomeModel.FILTER_UNADDED, null, unadded));
        if (hidden > 0) filters.add(new HomeModel.Filter(HomeModel.FILTER_HIDDEN, null, hidden));
        return filters;
    }

    static HomeModel.Badge badgeOf(ConsoleSnapshot snapshot, ConsoleSnapshot.Source source) {
        ConsoleSnapshot.Host host = snapshot.host(source.hostUuid);
        NativeSession session = host == null ? null : host.session;
        if (session != null && session.isActive() && session.app.appId == source.appId) {
            return session.state == NativeSession.State.SUSPENDED ? HomeModel.Badge.SUSPENDED : HomeModel.Badge.RUNNING;
        }
        return source.running ? HomeModel.Badge.RUNNING : HomeModel.Badge.NONE;
    }

    static HomeModel.Badge badgeOf(ConsoleSnapshot snapshot, List<ConsoleSnapshot.Source> copies) {
        HomeModel.Badge best = HomeModel.Badge.NONE;
        for (ConsoleSnapshot.Source copy : copies) {
            HomeModel.Badge badge = badgeOf(snapshot, copy);
            if (badge == HomeModel.Badge.RUNNING) {
                return badge;
            }
            if (badge == HomeModel.Badge.SUSPENDED) {
                best = badge;
            }
        }
        return best;
    }
}
