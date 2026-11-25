package com.limelight.nativeapi;

import android.content.Context;
import android.content.SharedPreferences;

import com.limelight.AppView;
import com.limelight.LimeLog;
import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarConsoleJni;

import java.io.File;
import java.io.IOException;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Map;
import java.util.Set;

/**
 * The games this device hides, per PC, backed by Rust's {@code hidden_apps.json} via
 * {@link NightjarConsoleJni}. Tracks stable IDs across host app-id renumberings and performs a
 * one-time migration from legacy {@code HiddenApps} {@link SharedPreferences}.
 */
public interface HiddenStore {
    Object WRITES = new Object();

    /** A host's listed games by stable id; duplicates are omitted because the id is ambiguous. */
    final class Listing {
        private final Map<String, Integer> games = new HashMap<>();
        private final Set<String> twice = new HashSet<>();

        public void add(String stableId, int appId) {
            if (stableId == null || stableId.isEmpty() || twice.contains(stableId)) return;
            if (games.put(stableId, appId) != null) {
                games.remove(stableId);
                twice.add(stableId);
            }
        }

        public Map<String, Integer> games() {
            return games;
        }
    }

    /** The app ids hidden on this host. */
    Set<Integer> of(String hostUuid);

    /** The stable ids this host's hidden games were last seen with, and their app ids then. */
    Map<String, Integer> ids(String hostUuid);

    void set(String hostUuid, int appId, String stableId, boolean hidden);

    Set<Integer> following(String hostUuid, Map<String, Integer> games);

    Set<Integer> changed(String hostUuid, Set<Integer> shown, Set<Integer> hidden);

    static void setApp(long handle, String hostUuid, int appId, String stableId, boolean hidden) {
        if (!NightjarConsoleJni.repoHiddenSetApp(handle, hostUuid, appId, stableId == null ? "" : stableId, hidden)) {
            LimeLog.warning("Hidden games: failed to store hidden state for app " + appId);
        }
    }

    /** Rust-backed persistent store in {@code context.getFilesDir()/console/hidden_apps.json}. */
    final class Preferences implements HiddenStore {
        private static final String IDS_SUFFIX = "#ids";
        private final Context app;
        private final SharedPreferences legacyPrefs;

        public Preferences(Context context) {
            this.app = context.getApplicationContext();
            this.legacyPrefs = app.getSharedPreferences(AppView.HIDDEN_APPS_PREF_FILENAME, Context.MODE_PRIVATE);
            migrateLegacyIfPresent();
        }

        /** The key of a host's stable ids in legacy preferences. */
        public static String idsKey(String hostUuid) {
            return hostUuid + IDS_SUFFIX;
        }

        private long handle() {
            migrateLegacyIfPresent();
            return ConsoleRepository.handleFor(app.getFilesDir());
        }

        private void migrateLegacyIfPresent() {
            migrateLegacy(app, ConsoleRepository.handleFor(app.getFilesDir()));
        }

        static void migrateLegacy(Context app, long handle) {
            if (handle == 0L) return;
            SharedPreferences prefs = app.getSharedPreferences(AppView.HIDDEN_APPS_PREF_FILENAME, Context.MODE_PRIVATE);
            synchronized (WRITES) {
                Map<String, ?> all = prefs.getAll();
                if (all.isEmpty()) return;
                Json.Arr entries = collectLegacyEntries(prefs, all);
                if (entries.length() > 0) {
                    try {
                        NightjarConsoleJni.repoImportLegacy(handle, "", "", entries.toString());
                    } catch (RuntimeException | UnsatisfiedLinkError e) {
                        com.limelight.LimeLog.warning("Legacy hidden apps import failed: " + e.getMessage());
                        return;
                    }
                }
                prefs.edit().clear().commit();
            }
        }

        @Override
        public Set<Integer> of(String hostUuid) {
            return parseIntSet(NightjarConsoleJni.repoHiddenOf(handle(), hostUuid));
        }

        @Override
        public Map<String, Integer> ids(String hostUuid) {
            return parseIdMap(NightjarConsoleJni.repoHiddenIds(handle(), hostUuid));
        }

        @Override
        public void set(String hostUuid, int appId, String stableId, boolean hidden) {
            HiddenStore.setApp(handle(), hostUuid, appId, stableId, hidden);
        }

        @Override
        public Set<Integer> following(String hostUuid, Map<String, Integer> games) {
            Json.Obj map = Json.obj();
            for (Map.Entry<String, Integer> e : games.entrySet()) {
                if (e.getKey() != null && !e.getKey().isEmpty() && e.getValue() != null) {
                    map.put(e.getKey(), e.getValue());
                }
            }
            return parseIntSet(NightjarConsoleJni.repoHiddenFollowing(handle(), hostUuid, map.toString()));
        }

        @Override
        public Set<Integer> changed(String hostUuid, Set<Integer> shown, Set<Integer> hidden) {
            return parseIntSet(NightjarConsoleJni.repoHiddenChanged(
                    handle(), hostUuid, intArrayJson(shown), intArrayJson(hidden)));
        }

        static Json.Arr collectLegacyEntries(SharedPreferences prefs, Map<String, ?> all) {
            Set<String> hosts = new HashSet<>();
            for (String key : all.keySet()) {
                if (key.endsWith(IDS_SUFFIX)) {
                    hosts.add(key.substring(0, key.length() - IDS_SUFFIX.length()));
                } else {
                    hosts.add(key);
                }
            }
            Json.Arr out = Json.arr();
            for (String host : hosts) {
                Map<Integer, String> appToStable = new HashMap<>();
                for (String entry : prefs.getStringSet(idsKey(host), Collections.emptySet())) {
                    int at = entry.indexOf(':');
                    if (at <= 0) {
                        LimeLog.warning("Hidden games: not an id: " + entry);
                        continue;
                    }
                    try {
                        int appId = Integer.parseInt(entry.substring(0, at));
                        String stableId = entry.substring(at + 1);
                        if (!stableId.isEmpty()) appToStable.put(appId, stableId);
                    } catch (NumberFormatException e) {
                        LimeLog.warning("Hidden games: not an id: " + entry);
                    }
                }
                for (String idStr : prefs.getStringSet(host, Collections.emptySet())) {
                    try {
                        int appId = Integer.parseInt(idStr);
                        String stableId = appToStable.getOrDefault(appId, "");
                        out.add(Json.obj()
                                .put("host", host)
                                .put("install_id", stableId)
                                .put("app_id", appId)
                                .put("title", ""));
                    } catch (NumberFormatException e) {
                        LimeLog.warning("Hidden games: not an app id: " + idStr);
                    }
                }
            }
            return out;
        }

        static Set<Integer> parseIntSet(String raw) {
            Set<Integer> out = new HashSet<>();
            if (raw == null || raw.isEmpty()) return out;
            Json.Arr arr = Json.parseArray(raw);
            for (int i = 0; i < arr.length(); i++) {
                out.add(arr.getInt(i));
            }
            return out;
        }

        static Map<String, Integer> parseIdMap(String raw) {
            Map<String, Integer> out = new HashMap<>();
            if (raw == null || raw.isEmpty()) return out;
            Json.Obj obj = Json.parseObject(raw);
            for (Iterator<String> it = obj.keys(); it.hasNext(); ) {
                String k = it.next();
                out.put(k, obj.getInt(k));
            }
            return out;
        }

        static String intArrayJson(Set<Integer> ids) {
            Json.Arr arr = Json.arr();
            for (Integer id : ids) {
                if (id != null) arr.add(id);
            }
            return arr.toString();
        }
    }

    /** Ephemeral directory-backed store for isolated unit tests. */
    final class InMemory implements HiddenStore {
        private final File dir;

        public InMemory() {
            try {
                dir = File.createTempFile("nightjar-hidden-mem", "");
                if (!dir.delete() || !dir.mkdirs()) {
                    throw new IOException("Could not create temp directory " + dir);
                }
                dir.deleteOnExit();
            } catch (IOException e) {
                throw new IllegalStateException("Failed to create temp hidden dir", e);
            }
        }

        private long handle() {
            return ConsoleRepository.handleFor(dir);
        }

        @Override
        public Set<Integer> of(String hostUuid) {
            return Preferences.parseIntSet(NightjarConsoleJni.repoHiddenOf(handle(), hostUuid));
        }

        @Override
        public Map<String, Integer> ids(String hostUuid) {
            return Preferences.parseIdMap(NightjarConsoleJni.repoHiddenIds(handle(), hostUuid));
        }

        @Override
        public void set(String hostUuid, int appId, String stableId, boolean hidden) {
            HiddenStore.setApp(handle(), hostUuid, appId, stableId, hidden);
        }

        @Override
        public Set<Integer> following(String hostUuid, Map<String, Integer> games) {
            Json.Obj map = Json.obj();
            for (Map.Entry<String, Integer> e : games.entrySet()) {
                if (e.getKey() != null && !e.getKey().isEmpty() && e.getValue() != null) {
                    map.put(e.getKey(), e.getValue());
                }
            }
            return Preferences.parseIntSet(NightjarConsoleJni.repoHiddenFollowing(handle(), hostUuid, map.toString()));
        }

        @Override
        public Set<Integer> changed(String hostUuid, Set<Integer> shown, Set<Integer> hidden) {
            return Preferences.parseIntSet(NightjarConsoleJni.repoHiddenChanged(
                    handle(), hostUuid, Preferences.intArrayJson(shown), Preferences.intArrayJson(hidden)));
        }
    }
}
