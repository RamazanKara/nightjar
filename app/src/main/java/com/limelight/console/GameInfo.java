package com.limelight.console;

import android.content.Context;
import android.content.SharedPreferences;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;

import androidx.annotation.VisibleForTesting;

import com.limelight.LimeLog;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarHttpJni;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ThreadPoolExecutor;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import java.util.function.Supplier;

/**
 * SteamGridDB covers and IGDB facts for the library. One background thread runs every lookup, so the services' pace and
 * IGDB's token are shared; records and cover images are files under the cache folder (the host's own box art is never
 * touched), the keys sit in their own preferences file, and the session (token, pace) stays in memory. With no key set
 * nothing here sends a request.
 */
public final class GameInfo {
    public static final String KEYS_FILE = "game_info_keys";
    public static final String KEY_STEAMGRIDDB = "steamgriddb";
    public static final String KEY_IGDB_ID = "igdb_client_id";
    public static final String KEY_IGDB_SECRET = "igdb_client_secret";

    private static GameInfo instance;

    /** The keys live in their own file, out of presets, profiles, exports and backups. */
    public static SharedPreferences keyPrefs(Context context) {
        return context.getApplicationContext().getSharedPreferences(KEYS_FILE, Context.MODE_PRIVATE);
    }

    public static GameInfoModel.Keys keys(Context context) {
        SharedPreferences prefs = keyPrefs(context);
        return new GameInfoModel.Keys(prefs.getString(KEY_STEAMGRIDDB, ""), prefs.getString(KEY_IGDB_ID, ""),
                prefs.getString(KEY_IGDB_SECRET, ""));
    }

    /** True when a SteamGridDB key is set: covers the host has none of may come from there. */
    public static boolean coverWanted(Context context) {
        return keys(context).art();
    }

    /** True when both IGDB keys are set: facts are looked up. */
    public static boolean factsWanted(Context context) {
        return keys(context).info();
    }

    public static synchronized GameInfo get(Context context) {
        if (instance == null) {
            Context app = context.getApplicationContext();
            instance = new GameInfo(new File(app.getCacheDir(), "game-info"), null, () -> keys(app), true);
        }
        return instance;
    }

    /** What names a game to the lookup: its title, its library game id and its store entry (a Steam one names the app id). */
    public static final class Game {
        public final String name;
        public final String gameId;
        public final String storeSource;
        public final String storeId;

        public Game(String name, String gameId, String storeSource, String storeId) {
            this.name = name;
            this.gameId = gameId;
            this.storeSource = storeSource;
            this.storeId = storeId;
        }

        public static Game of(NvApp app) {
            return new Game(app.getAppName(), app.getGameId(), app.getStoreSource(), app.getStoreId());
        }

        String id() {
            return name + "\n" + gameId + "\n" + storeSource + "\n" + storeId;
        }
    }

    /** What one game's lookup holds now: its record and, when a cover was wanted and found, the image file. */
    public static final class Entry {
        public final JSONObject record;
        public final File image;

        Entry(JSONObject record, File image) {
            this.record = record;
            this.image = image;
        }

        public JSONObject facts() {
            return record == null ? null : record.optJSONObject("facts");
        }
    }

    private final File dir;
    private final GameInfoModel.Transport transport;
    private GameInfoModel.Sleeper sleeper;
    private final Supplier<GameInfoModel.Keys> keys;
    /** Interactive lookups run ahead of background library prefetches. */
    private final ExecutorService executor;
    private final ExecutorService prefetchExecutor;
    /** Reads of what is kept on disk, off the main thread and away from the lookups; never touches the network. */
    private final ExecutorService readers;
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Map<String, JSONObject> records = new ConcurrentHashMap<>();
    private final Set<String> imagesFailed = ConcurrentHashMap.newKeySet();
    private final Set<String> factsAsked = ConcurrentHashMap.newKeySet();
    /** Who waits for a game's cover: one lookup per game, however many views ask. */
    private final Map<String, List<Consumer<File>>> coverWaiters = new HashMap<>();
    /** The key set the per-key memory (facts asked, images failed) belongs to. */
    private String keysSeen;
    /** In memory only: the token and the pace of the services, for the key set in {@link #sessionKeys}. */
    private final Object sessionLock = new Object();
    private JSONObject session;
    private String sessionKeys;
    private final AtomicInteger downloads = new AtomicInteger();

    @VisibleForTesting
    GameInfo(File dir, GameInfoModel.Transport transport, Supplier<GameInfoModel.Keys> keys, boolean background) {
        this.dir = dir;
        this.keys = keys;
        this.transport = transport != null ? transport : this::send;
        this.sleeper = Thread::sleep;
        this.executor = background ? Executors.newSingleThreadExecutor(r -> daemon(r, "game-info")) : null;
        this.prefetchExecutor = background ? Executors.newSingleThreadExecutor(r -> daemon(r, "game-info-prefetch")) : null;
        this.readers = background ? new ThreadPoolExecutor(1, 2, 10, TimeUnit.SECONDS, new LinkedBlockingQueue<>(256),
                r -> daemon(r, "game-info-read")) : null;
    }

    private static Thread daemon(Runnable r, String name) {
        Thread t = new Thread(r, name);
        t.setDaemon(true);
        return t;
    }

    @VisibleForTesting
    void setSleeper(GameInfoModel.Sleeper replacement) {
        sleeper = replacement;
    }

    @VisibleForTesting
    int downloads() {
        return downloads.get();
    }

    private static long wallSecs() {
        return System.currentTimeMillis() / 1000;
    }

    /**
     * Forgets what was memoised for the old keys (games asked, images that failed) when the keys changed: a failure
     * with other keys is stale, and the library asks again. Cheap; call on each public entry.
     */
    private void noteKeys(GameInfoModel.Keys k) {
        String signature = k.json(true, true).toString();
        synchronized (factsAsked) {
            if (signature.equals(keysSeen)) return;
            if (keysSeen != null) {
                factsAsked.clear();
                imagesFailed.clear();
            }
            keysSeen = signature;
        }
    }

    // Requests: every lookup runs on the single worker thread.

    /**
     * Gives the game's cover image to {@code done} (on a worker thread), or null when there is none. A cover kept on
     * disk comes without any request; otherwise the lookup runs on the shared worker. Views asking for the same game
     * share one lookup. Does nothing without a SteamGridDB key.
     */
    public void cover(Game game, Consumer<File> done) {
        GameInfoModel.Keys k = keys.get();
        if (executor == null || !k.art()) return;
        noteKeys(k);
        String id = game.id();
        synchronized (coverWaiters) {
            List<Consumer<File>> waiting = coverWaiters.get(id);
            if (waiting != null) {
                waiting.add(done);
                return;
            }
            waiting = new ArrayList<>();
            waiting.add(done);
            coverWaiters.put(id, waiting);
        }
        try {
            readers.execute(() -> {
                // The waiters are always released, whatever a read or a lookup throws.
                boolean handedOn = false;
                File kept = null;
                try {
                    kept = cachedCover(game);
                    if (kept == null) {
                        executor.execute(() -> {
                            File image = null;
                            try {
                                Entry entry = resolve(game, true, true);
                                image = entry == null ? null : entry.image;
                            } finally {
                                finishCover(id, image);
                            }
                        });
                        handedOn = true;
                    }
                } catch (RejectedExecutionException e) {
                    // Shut down: nothing to look up.
                } finally {
                    if (!handedOn) finishCover(id, kept);
                }
            });
        } catch (RejectedExecutionException e) {
            finishCover(id, null);
        }
    }

    private void finishCover(String id, File file) {
        List<Consumer<File>> waiting;
        synchronized (coverWaiters) {
            waiting = coverWaiters.remove(id);
        }
        if (waiting == null) return;
        for (Consumer<File> done : waiting) {
            try {
                done.accept(file);
            } catch (RuntimeException e) {
                LimeLog.warning("Game info cover callback failed: " + e.getClass().getSimpleName());
            }
        }
    }

    /** Looks the game's facts up (no cover) and calls {@code done} on the main thread with them, or null. Nothing without IGDB keys. */
    public void facts(Game game, Consumer<JSONObject> done) {
        GameInfoModel.Keys k = keys.get();
        if (executor == null || !k.info()) return;
        noteKeys(k);
        executor.execute(() -> {
            Entry entry = resolve(game, false, true);
            JSONObject facts = entry == null ? null : entry.facts();
            main.post(() -> done.accept(facts));
        });
    }

    /**
     * Looks the facts of these games up in the background, asking {@code changed} on the main thread after each one
     * that found some. Failed lookups remain eligible on the next pass, and stale records refresh once expired.
     */
    public void prefetchFacts(List<Game> games, Runnable changed) {
        GameInfoModel.Keys k = keys.get();
        if (prefetchExecutor == null || !k.info()) return;
        noteKeys(k);
        for (Game game : games) {
            prefetchExecutor.execute(() -> {
                JSONObject cached = cachedRecord(game);
                try {
                    if (cached != null && GameInfoModel.fresh(cached, wallSecs(), k, false, true)
                            && factsAsked.contains(game.id())) {
                        return;
                    }
                } catch (JSONException | RuntimeException e) {
                    // Corrupt record on disk: attempt a fresh lookup below.
                }
                Entry entry = resolve(game, false, true);
                if (entry != null) {
                    factsAsked.add(game.id());
                    if (entry.facts() != null) main.post(changed);
                }
            });
        }
    }

    /** The facts a record on disk holds, without any request; call off the main thread. Null when there are none. */
    public JSONObject cachedFacts(Game game) {
        JSONObject record = cachedRecord(game);
        return record == null ? null : record.optJSONObject("facts");
    }

    /** The cover image a record on disk points at, without any request (nothing is downloaded); null when none is kept. */
    public File cachedCover(Game game) {
        try {
            GameInfoModel.Cleaned c = GameInfoModel.clean(game.name, game.gameId, game.storeSource, game.storeId);
            JSONObject record = read(c.key, new File(dir, GameInfoModel.files(c.key, null)[0]));
            return record == null ? null : imageFile(c.key, record, false);
        } catch (JSONException | RuntimeException | LinkageError e) {
            return null;
        }
    }

    private JSONObject cachedRecord(Game game) {
        try {
            GameInfoModel.Cleaned c = GameInfoModel.clean(game.name, game.gameId, game.storeSource, game.storeId);
            return read(c.key, new File(dir, GameInfoModel.files(c.key, null)[0]));
        } catch (JSONException | RuntimeException | LinkageError e) {
            return null;
        }
    }

    // The lookup, on the worker thread (or a test's).

    /**
     * The game's record, from disk while it holds and from a new lookup when it does not, and its cover image when one
     * is wanted. Null when no service wanted here has a key. Failures keep what was known before.
     */
    @VisibleForTesting
    Entry resolve(Game game, boolean wantCover, boolean wantFacts) {
        GameInfoModel.Keys k = keys.get();
        boolean art = wantCover && k.art();
        boolean info = wantFacts && k.info();
        if (!art && !info) return null;
        try {
            // A token and a pace belong to one set of keys.
            String signature = k.json(true, true).toString();
            JSONObject currentSession;
            synchronized (sessionLock) {
                if (!signature.equals(sessionKeys)) {
                    session = null;
                    sessionKeys = signature;
                }
                currentSession = session;
            }
            GameInfoModel.Cleaned c = GameInfoModel.clean(game.name, game.gameId, game.storeSource, game.storeId);
            File file = new File(dir, GameInfoModel.files(c.key, null)[0]);
            JSONObject record = read(c.key, file);
            if (record == null || !GameInfoModel.fresh(record, wallSecs(), k, art, info)) {
                JSONObject lookedUp = new JSONObject().put("name", c.clean).put("steam", c.steam == null ? JSONObject.NULL : c.steam);
                GameInfoModel.Finished finished = GameInfoModel.lookup(transport, sleeper, GameInfo::wallSecs,
                        SystemClock::elapsedRealtime, lookedUp, k, art, info, currentSession);
                if (finished == null) {
                    return null;
                }
                synchronized (sessionLock) {
                    if (signature.equals(sessionKeys)) {
                        session = finished.session;
                    }
                }
                for (String line : finished.log) LimeLog.warning("Game info: " + line);
                record = GameInfoModel.merge(finished.record, record);
                write(file, record);
                records.put(c.key, record);
            }
            if (record == null) return null;
            return new Entry(record, art ? imageFile(c.key, record, true) : null);
        } catch (JSONException | RuntimeException | LinkageError e) {
            LimeLog.warning("Game info could not look up a game: " + e.getClass().getSimpleName());
            return null;
        }
    }

    /**
     * The cover's image file; fetched first when it is not kept and {@code fetch} is set (lookups only; one fetch of a
     * URL at a time, as the worker runs one lookup at a time). Null when the record has none or it cannot be had.
     */
    private File imageFile(String key, JSONObject record, boolean fetch) throws JSONException {
        String url = record.isNull("cover") ? null : record.optString("cover", null);
        if (url == null || url.isEmpty()) return null;
        File image = new File(new File(dir, "img"), GameInfoModel.files(key, url)[1]);
        if (image.isFile() && image.length() > 0) return image;
        if (!fetch || imagesFailed.contains(url)) return null;
        if (!download(url, image)) {
            imagesFailed.add(url);
            return null;
        }
        return image;
    }

    private boolean download(String url, File target) {
        downloads.incrementAndGet();
        File temporary = null;
        try {
            File folder = target.getParentFile();
            if (folder != null && !folder.isDirectory() && !folder.mkdirs()) return false;
            temporary = File.createTempFile("cover-", ".tmp", folder);
            if (!NightjarHttpJni.gameInfoDownload(url, temporary.getAbsolutePath(), true)) return false;
            if (temporary.length() == 0) return false;
            replace(temporary, target);
            temporary = null;
            return true;
        } catch (IOException | RuntimeException | UnsatisfiedLinkError e) {
            return false;
        } finally {
            if (temporary != null) temporary.delete();
        }
    }

    private JSONObject read(String key, File file) {
        JSONObject cached = records.get(key);
        if (cached != null) return cached;
        try {
            if (!file.isFile()) return null;
            JSONObject record = new JSONObject(new String(readAll(file), StandardCharsets.UTF_8));
            records.put(key, record);
            return record;
        } catch (IOException | JSONException e) {
            return null;
        }
    }

    // java.nio.file needs API 26 and the app supports API 21, so plain java.io. On Android a
    // rename within one folder replaces the target atomically, as the nio move did.
    private static void replace(File from, File to) throws IOException {
        if (from.renameTo(to)) return;
        if (to.exists() && !to.delete()) throw new IOException("cannot replace " + to.getName());
        if (!from.renameTo(to)) throw new IOException("cannot move to " + to.getName());
    }

    private static byte[] readAll(File file) throws IOException {
        try (InputStream in = new FileInputStream(file); ByteArrayOutputStream out = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[16 * 1024];
            int n;
            while ((n = in.read(buffer)) > 0) out.write(buffer, 0, n);
            return out.toByteArray();
        }
    }

    private static void write(File file, JSONObject record) {
        File temporary = null;
        try {
            File folder = file.getParentFile();
            if (folder != null && !folder.isDirectory() && !folder.mkdirs()) return;
            temporary = File.createTempFile("record-", ".tmp", folder);
            try (OutputStream out = new FileOutputStream(temporary)) {
                out.write(record.toString().getBytes(StandardCharsets.UTF_8));
            }
            replace(temporary, file);
            temporary = null;
        } catch (IOException | RuntimeException e) {
            LimeLog.warning("Game info could not keep a record");
        } finally {
            if (temporary != null) temporary.delete();
        }
    }

    private static String joined(JSONObject facts, String field) {
        org.json.JSONArray array = facts.optJSONArray(field);
        StringBuilder out = new StringBuilder();
        for (int i = 0; array != null && i < array.length(); i++) {
            String value = array.optString(i, "");
            if (value.isEmpty()) continue;
            if (out.length() > 0) out.append(" · ");
            out.append(value);
        }
        return out.toString();
    }

    /**
     * The lines a game's facts show: Genres, Themes and Time to beat ("Story 20 h · Story + extras 24 h · Everything
     * 90 h"), each only when there is something to say. Empty when there is nothing.
     */
    public static List<String> describe(Context context, JSONObject facts) {
        List<String> lines = new ArrayList<>();
        if (facts == null) return lines;
        String genres = joined(facts, "genres");
        if (!genres.isEmpty()) lines.add(context.getString(com.limelight.R.string.game_info_genres) + ": " + genres);
        String themes = joined(facts, "themes");
        if (!themes.isEmpty()) lines.add(context.getString(com.limelight.R.string.game_info_themes) + ": " + themes);
        try {
            GameInfoModel.TimeToBeat t = GameInfoModel.timeToBeat(facts);
            StringBuilder time = new StringBuilder();
            String[] hours = {t.hastily, t.normally, t.completely};
            int[] labels = {com.limelight.R.string.game_info_story, com.limelight.R.string.game_info_extras,
                    com.limelight.R.string.game_info_everything};
            for (int i = 0; i < hours.length; i++) {
                if (hours[i] == null) continue;
                if (time.length() > 0) time.append(" · ");
                time.append(context.getString(labels[i])).append(' ').append(context.getString(com.limelight.R.string.game_info_hours, hours[i]));
            }
            if (time.length() > 0) lines.add(context.getString(com.limelight.R.string.game_info_time_to_beat) + ": " + time);
        } catch (JSONException | RuntimeException | LinkageError e) {
            // No time to beat when the library cannot say.
        }
        return lines;
    }

    /** One planned request over HTTPS; never logs the URL, the headers or the body, which can hold keys. */
    @VisibleForTesting
    GameInfoModel.Reply send(JSONObject planned) {
        try {
            String method = planned.getString("method");
            String url = planned.getString("url");
            org.json.JSONArray headers = planned.optJSONArray("headers");
            String body = planned.isNull("body") ? null : planned.getString("body");
            byte[] bodyBytes = "GET".equalsIgnoreCase(method) ? null
                    : (body == null ? new byte[0] : body.getBytes(StandardCharsets.UTF_8));
            String raw = NightjarHttpJni.gameInfoExchange(method, url,
                    headers == null ? "[]" : headers.toString(), bodyBytes, true);
            Json.Obj res = Json.parseObject(raw);
            int status = res.optInt("status", 0);
            Long retry = res.has("retry_after") && !res.isNull("retry_after") ? res.optLong("retry_after") : null;
            return new GameInfoModel.Reply(status, res.optString("body", ""), retry);
        } catch (JSONException | RuntimeException | UnsatisfiedLinkError e) {
            return new GameInfoModel.Reply(0, "", null);
        }
    }
}
