package com.limelight.nativeapi;

import android.content.Context;
import android.content.SharedPreferences;

import com.limelight.AppView;
import com.limelight.LimeLog;
import com.limelight.binding.PlatformBinding;
import com.limelight.computers.HostCapabilityCache;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.PairingManager;
import com.limelight.nvstream.jni.Json;
import com.limelight.nvstream.jni.NightjarConsoleJni;
import com.limelight.nvstream.wol.WakeTargets;

import java.io.File;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;

/**
 * Thin Android facade over Rust's cross-host {@code ConsoleRepository} ({@link NightjarConsoleJni}),
 * handling Android host descriptors, classic applist bridging, one-time {@link SharedPreferences}
 * migration, and per-handle lifecycle cleanup across Robolectric tests.
 */
public final class ConsoleRepository {
    public interface EndpointFactory {
        /** The endpoint of the host, or null when it cannot be reached with what is known. */
        HostEndpoint endpoint(ComputerDetails computer);
    }

    public interface Clock {
        long nowMs();
    }

    private static final String LEGACY_RECENTS_PREFS = "console_recents";
    private static final String LEGACY_RECENTS_KEY = "recents";
    private static final String LEGACY_FAVOURITES_PREFS = "console_favourites";
    private static final String LEGACY_FAVOURITES_KEY = "favourites";

    private static final ConcurrentHashMap<String, Long> HANDLES = new ConcurrentHashMap<>();
    private static final Set<String> MIGRATED_DIRS = ConcurrentHashMap.newKeySet();

    private final long handle;
    private final NativeApiTransport transport;
    private final EndpointFactory endpoints;
    private final ClassicAppSource classic;
    private final Clock clock;
    private final WakeTargets wakeTargets;
    private final byte[] certDer;
    private final byte[] keyPkcs8;
    private final String uniqueId;
    private final boolean useRustTransport;

    public ConsoleRepository(File baseDir, NativeApiTransport transport, EndpointFactory endpoints,
                             ClassicAppSource classic, Clock clock, WakeTargets wakeTargets) {
        this(handleFor(baseDir), transport, endpoints, classic, clock, wakeTargets, null, null, "", false);
    }

    private ConsoleRepository(long handle, NativeApiTransport transport, EndpointFactory endpoints,
                              ClassicAppSource classic, Clock clock, WakeTargets wakeTargets,
                              byte[] certDer, byte[] keyPkcs8, String uniqueId, boolean useRustTransport) {
        this.handle = handle;
        this.transport = transport;
        this.endpoints = endpoints;
        this.classic = classic;
        this.clock = clock;
        this.wakeTargets = wakeTargets;
        this.certDer = certDer;
        this.keyPkcs8 = keyPkcs8;
        this.uniqueId = uniqueId == null ? "" : uniqueId;
        this.useRustTransport = useRustTransport;
    }

    static long handleFor(File baseDir) {
        String key = baseDir.getAbsolutePath();
        synchronized (HANDLES) {
            return HANDLES.computeIfAbsent(key, NightjarConsoleJni::repoCreate);
        }
    }

    /** Destroys only the Rust repository handle associated with {@code baseDir} (tests). */
    public static void closeForTests(File baseDir) {
        if (baseDir == null) return;
        String key = baseDir.getAbsolutePath();
        synchronized (HANDLES) {
            Long h = HANDLES.remove(key);
            MIGRATED_DIRS.remove(key);
            if (h != null && h != 0L) {
                NightjarConsoleJni.repoDestroy(h);
            }
        }
    }

    /** Clears all Rust repository handles and migration markers between Robolectric tests. */
    public static void resetForTests() {
        synchronized (HANDLES) {
            for (Long h : new ArrayList<>(HANDLES.values())) {
                if (h != null && h != 0L) {
                    NightjarConsoleJni.repoDestroy(h);
                }
            }
            HANDLES.clear();
            MIGRATED_DIRS.clear();
        }
    }

    /** Evicts in-memory cached libraries/capabilities for {@code baseDir} (tests). */
    public static void evictCacheForTests(File baseDir) {
        long h = handleFor(baseDir);
        NightjarConsoleJni.repoClearCache(h);
    }

    /** The repository of the app: Rust transport/stores, classic applist reader, and clock. */
    public static ConsoleRepository create(Context context, String uniqueId) {
        Context app = context.getApplicationContext();
        long h = openAndMigrate(app);
        com.limelight.nvstream.http.LimelightCryptoProvider crypto = PlatformBinding.getCryptoProvider(app);
        byte[] cert = crypto.getClientCertificate() != null ? derCert(crypto) : null;
        byte[] key = crypto.getClientPrivateKey() != null ? crypto.getClientPrivateKey().getEncoded() : null;
        return new ConsoleRepository(h, new JniNativeApiTransport(),
                computer -> HostEndpoint.of(computer, uniqueId, crypto),
                new CachedAppListSource(app.getCacheDir()), System::currentTimeMillis,
                new WakeTargets(app), cert, key, uniqueId, cert != null && key != null);
    }

    private static byte[] derCert(com.limelight.nvstream.http.LimelightCryptoProvider crypto) {
        try {
            return crypto.getClientCertificate().getEncoded();
        } catch (Exception e) {
            return null;
        }
    }

    private static long openAndMigrate(Context app) {
        File dir = app.getFilesDir();
        long h = handleFor(dir);
        if (MIGRATED_DIRS.add(dir.getAbsolutePath())) {
            migrateLegacy(app, h);
        }
        return h;
    }

    private static void migrateLegacy(Context app, long h) {
        if (h == 0L) return;
        SharedPreferences recPrefs = app.getSharedPreferences(LEGACY_RECENTS_PREFS, Context.MODE_PRIVATE);
        String rawRecents = recPrefs.getString(LEGACY_RECENTS_KEY, null);
        String recentsPayload = "";
        if (rawRecents != null && !rawRecents.isEmpty()) {
            recentsPayload = translateLegacyRecents(rawRecents);
        }

        SharedPreferences favPrefs = app.getSharedPreferences(LEGACY_FAVOURITES_PREFS, Context.MODE_PRIVATE);
        String rawFavs = favPrefs.getString(LEGACY_FAVOURITES_KEY, null);
        String favsPayload = "";
        if (rawFavs != null && !rawFavs.isEmpty()) {
            favsPayload = translateLegacyFavourites(rawFavs);
        }

        if (!recentsPayload.isEmpty() || !favsPayload.isEmpty()) {
            try {
                NightjarConsoleJni.repoImportLegacy(h, recentsPayload, favsPayload, "");
            } catch (RuntimeException | UnsatisfiedLinkError e) {
                LimeLog.warning("Legacy console import failed: " + e.getMessage());
                HiddenStore.Preferences.migrateLegacy(app, h);
                return;
            }
            if (rawRecents != null) recPrefs.edit().remove(LEGACY_RECENTS_KEY).commit();
            if (rawFavs != null) favPrefs.edit().remove(LEGACY_FAVOURITES_KEY).commit();
        }
        HiddenStore.Preferences.migrateLegacy(app, h);
    }

    private static String translateLegacyRecents(String raw) {
        try {
            Json.Arr in = Json.parseArray(raw);
            Json.Arr out = Json.arr();
            for (int i = 0; i < in.length(); i++) {
                Json.Obj r = in.optObject(i);
                if (r == null) continue;
                out.add(Json.obj()
                        .put("host", r.optString("host", ""))
                        .put("install_id", r.optString("stable_id", ""))
                        .put("app_id", r.optInt("app", 0))
                        .put("title", r.optString("title", ""))
                        .put("at", r.optLong("played_ms", 0L)));
            }
            return out.toString();
        } catch (Json.SyntaxException e) {
            return "";
        }
    }

    private static String translateLegacyFavourites(String raw) {
        try {
            Json.Arr in = Json.parseArray(raw);
            Json.Arr out = Json.arr();
            for (int i = 0; i < in.length(); i++) {
                Json.Obj f = in.optObject(i);
                if (f == null || !f.has("app")) {
                    out.add(in.raw(i));
                    continue;
                }
                out.add(Json.obj()
                        .put("host", f.optString("host", ""))
                        .put("install_id", f.optString("key", ""))
                        .put("app_id", f.getInt("app"))
                        .put("title", f.optString("title", "")));
            }
            return out.toString();
        } catch (Json.SyntaxException e) {
            // Preserve damaged raw text so Rust writes `.bak`.
            return raw;
        }
    }

    public static boolean hasAnyLibrary(Context context) {
        return NightjarConsoleJni.repoHasAnyLibrary(context.getApplicationContext().getFilesDir().getAbsolutePath());
    }

    public static boolean hasClassicAppList(Context context) {
        Context app = context.getApplicationContext();
        File[] lists = new File(app.getCacheDir(), "applist").listFiles(file -> file.isFile() && file.length() > 0);
        if (lists == null) return false;
        for (File list : lists) {
            if (!HostCapabilityCache.load(app, list.getName()).has(HostCapabilities.Feature.NATIVE_API)) return true;
        }
        return false;
    }

    public static boolean hasLibrary(Context context, String hostUuid) {
        return NightjarConsoleJni.repoHasLibrary(
                context.getApplicationContext().getFilesDir().getAbsolutePath(), hostUuid);
    }

    /** A game's cover entry from the host's saved native library. */
    public static final class CachedLibraryCover {
        /** True when the saved library explicitly listed this game with no cover of its own. */
        public final boolean explicitlyNoCover;
        /** The cover cache key matching {@code CoverLoaders.appOf}, or null when the game has none. */
        public final String artKey;

        private CachedLibraryCover(boolean explicitlyNoCover, String artKey) {
            this.explicitlyNoCover = explicitlyNoCover;
            this.artKey = artKey;
        }
    }

    /**
     * Looks up an added game in this host's saved native library to recover its cover cache key
     * (matching {@code CoverLoaders.appOf}), or null when no saved library lists the game.
     */
    public static CachedLibraryCover findCachedLibraryCover(Context context, String hostUuid, int appId, String installId) {
        if (context == null || hostUuid == null || hostUuid.isEmpty()) return null;
        String raw = NightjarConsoleJni.repoFindCachedLibraryCover(
                context.getApplicationContext().getFilesDir().getAbsolutePath(),
                hostUuid,
                appId,
                installId == null ? "" : installId);
        if (raw == null || raw.isEmpty()) return null;
        try {
            Json.Obj obj = Json.parseObject(raw);
            if (obj.optBoolean("explicitly_no_cover", false)) {
                return new CachedLibraryCover(true, null);
            }
            // Matches CoverLoaders.appOf: native sources have empty appUuid and coverVersion as artVersion.
            NvApp listed = new NvApp(obj.optString("title", ""), obj.optInt("app_id", 0), false);
            listed.setArtVersion(obj.optString("cover_version", ""));
            return new CachedLibraryCover(false, listed.getAssetCacheKey());
        } catch (Json.SyntaxException e) {
            LimeLog.warning("Unreadable cached library for " + hostUuid + ": " + e.getMessage());
            return null;
        }
    }

    /** Drops what the app stores for a removed host without needing a repository. */
    public static void forgetHost(Context context, String hostUuid) {
        Context app = context.getApplicationContext();
        NightjarConsoleJni.repoForgetHost(openAndMigrate(app), hostUuid);
        app.getSharedPreferences(AppView.HIDDEN_APPS_PREF_FILENAME, Context.MODE_PRIVATE)
                .edit().remove(hostUuid).remove(HiddenStore.Preferences.idsKey(hostUuid)).apply();
        new WakeTargets(app).forgetHost(hostUuid);
        com.limelight.console.OpenPlay.forgetHost(app, hostUuid);
    }

    /** Records a game stream launch into Rust's {@code console_recents.json} without constructing a full repository. */
    public static void recordLaunch(Context context, String hostUuid, int appId, String title, String stableId) {
        recordLaunch(context, hostUuid, appId, title, stableId, System.currentTimeMillis());
    }

    /** Records a game stream launch at {@code playedAtMs} into Rust's {@code console_recents.json}. */
    public static void recordLaunch(Context context, String hostUuid, int appId, String title, String stableId,
                                    long playedAtMs) {
        recordLaunch(context, hostUuid, "", appId, title, stableId, playedAtMs);
    }

    /** Records a game stream launch with an explicit host display name into Rust's {@code console_recents.json}. */
    public static void recordLaunch(Context context, String hostUuid, String hostName, int appId, String title,
                                    String stableId, long playedAtMs) {
        Context app = context.getApplicationContext();
        long h = openAndMigrate(app);
        Json.Obj entry = Json.obj()
                .put("host", hostUuid == null ? "" : hostUuid)
                .put("host_name", hostName == null ? "" : hostName)
                .put("install_id", stableId == null ? "" : stableId)
                .put("app_id", appId)
                .put("title", title == null ? "" : title)
                .put("at", playedAtMs);
        NightjarConsoleJni.repoRecordLaunch(h, entry.toString());
    }

    /** Records a {@link NvApp} stream launch if its role is {@link NvApp.Role#STREAM}. */
    public static void recordLaunch(Context context, ComputerDetails computer, NvApp nvApp) {
        if (nvApp != null && nvApp.getRole() == NvApp.Role.STREAM) {
            recordLaunch(context, computer.uuid, computer.displayName(), nvApp.getAppId(), nvApp.getAppName(),
                    nvApp.getStableId(), System.currentTimeMillis());
        }
    }

    /** Returns persisted recent launches newest-first from Rust's {@code console_recents.json}. */
    public static List<RecentsStore.Recent> loadRecents(Context context) {
        Context app = context.getApplicationContext();
        String raw = NightjarConsoleJni.repoRecents(openAndMigrate(app));
        List<RecentsStore.Recent> out = new ArrayList<>();
        if (raw == null || raw.isEmpty()) return out;
        Json.Arr arr = Json.parseArray(raw);
        for (int i = 0; i < arr.length(); i++) {
            Json.Obj r = arr.getObject(i);
            out.add(new RecentsStore.Recent(r.optString("host", ""), r.optString("host_name", ""),
                    r.optInt("app_id", 0), r.optString("title", ""), r.optLong("at", 0L),
                    r.optString("install_id", "")));
        }
        return out;
    }

    public ConsoleSnapshot cached(List<ComputerDetails> computers) {
        Map<String, List<NvApp>> classicHdr = new HashMap<>();
        String raw = NightjarConsoleJni.repoCached(handle, hostsJson(computers, false, null, classicHdr));
        return ConsoleSnapshot.fromRustJson(raw, computers, Collections.emptyMap(), classicHdr);
    }

    public ConsoleSnapshot refresh(List<ComputerDetails> computers) throws InterruptedException {
        if (Thread.currentThread().isInterrupted()) throw new InterruptedException();
        Map<String, ComputerDetails> byUuid = new HashMap<>();
        Map<String, HostEndpoint> endpointMap = new HashMap<>();
        for (ComputerDetails c : computers) {
            if (c != null && c.uuid != null) {
                byUuid.put(c.uuid, c);
                if (c.state == ComputerDetails.State.ONLINE && c.caps.has(HostCapabilities.Feature.NATIVE_API)) {
                    endpointMap.put(c.uuid, endpoints.endpoint(c));
                }
            }
        }
        Map<String, NativeApiException> errors = new ConcurrentHashMap<>();
        NightjarConsoleJni.TransportCallback cb = useRustTransport ? null : (hostUuid, op, arg) -> {
            ComputerDetails c = byUuid.get(hostUuid);
            HostEndpoint ep = endpointMap.get(hostUuid);
            if (c == null || ep == null) {
                return Json.obj().put("error", Json.obj().put("kind", "unavailable").put("message", "No address")).toString();
            }
            try {
                return transport.request(ep, op, arg);
            } catch (NativeApiException e) {
                if ("library".equals(op)) errors.put(hostUuid, e);
                Json.Obj err = Json.obj()
                        .put("kind", e.getKind().name().toLowerCase(java.util.Locale.ROOT))
                        .put("message", e.getMessage() == null ? "error" : e.getMessage())
                        .put("http_status", e.getHttpStatus())
                        .put("retry_after_ms", e.getRetryAfterMs());
                if (e.getCode() != null) err.put("code", e.getCode());
                if (e.getSetting() != null) err.put("setting", e.getSetting());
                return Json.obj().put("error", err).toString();
            }
        };
        Map<String, List<NvApp>> classicHdr = new HashMap<>();
        String raw = NightjarConsoleJni.repoRefresh(
                handle, hostsJson(computers, true, endpointMap, classicHdr), certDer, keyPkcs8, uniqueId, clock.nowMs(), cb);
        ConsoleSnapshot snapshot = ConsoleSnapshot.fromRustJson(raw, computers, errors, classicHdr);
        if (wakeTargets != null) {
            for (ConsoleSnapshot.Host host : snapshot.hosts) {
                if (!host.capabilitiesFetched) continue;
                HostEndpoint ep = endpointMap.get(host.uuid);
                if (ep != null) {
                    wakeTargets.refresh(host.uuid, host.has(NativeCapabilities.WAKE), new NativeApiClient(transport, ep));
                }
            }
        }
        return snapshot;
    }

    public boolean classicListHas(ComputerDetails computer, int appId) {
        return classicListHas(computer, appId, "");
    }

    public boolean classicListHas(ComputerDetails computer, int appId, String stableId) {
        List<NvApp> apps = classic.apps(computer);
        if (apps != null) {
            for (NvApp app : apps) {
                if (app.getAppId() == appId && (stableId.isEmpty() || app.getAppUuid().isEmpty()
                        || stableId.equalsIgnoreCase(app.getAppUuid()))) return true;
            }
        }
        return false;
    }

    public NativeCapabilities capabilities(ComputerDetails computer) {
        if (!computer.caps.has(HostCapabilities.Feature.NATIVE_API)) return null;
        HostEndpoint endpoint = endpoints.endpoint(computer);
        if (endpoint == null) return null;
        try {
            return new NativeApiClient(transport, endpoint).capabilities();
        } catch (NativeApiException e) {
            LimeLog.info("No capabilities from " + computer.name + ": " + e.getMessage());
            return null;
        }
    }

    public ConsoleSnapshot.AddedGame addToLibrary(ComputerDetails computer, String gameId) throws NativeApiException {
        HostEndpoint endpoint = endpoints.endpoint(computer);
        if (endpoint == null) {
            throw new NativeApiException(NativeApiException.Kind.UNAVAILABLE, computer.name + " has no address");
        }
        NightjarConsoleJni.TransportCallback cb = useRustTransport ? null : (hostUuid, op, arg) -> {
            try {
                return transport.request(endpoint, op, arg);
            } catch (NativeApiException e) {
                Json.Obj err = Json.obj()
                        .put("kind", e.getKind().name().toLowerCase(java.util.Locale.ROOT))
                        .put("message", e.getMessage() == null ? "error" : e.getMessage())
                        .put("http_status", e.getHttpStatus());
                if (e.getCode() != null) err.put("code", e.getCode());
                return Json.obj().put("error", err).toString();
            }
        };
        String hostJson = hostDescriptor(computer, endpoint, false, null).toString();
        String reply = NightjarConsoleJni.repoAddToLibrary(handle, hostJson, gameId, certDer, keyPkcs8, uniqueId, cb);
        Json.Obj env;
        try {
            env = Json.parseObject(reply);
        } catch (RuntimeException e) {
            throw new NativeApiException(NativeApiException.Kind.MALFORMED,
                    "Malformed add-to-library reply: " + e.getMessage());
        }
        Json.Obj err = env.optObject("error");
        if (err != null) {
            throw new NativeApiException(NativeApiException.Kind.of(err.optString("kind", "api")),
                    err.optString("message", "library_add failed"));
        }
        Json.Obj ok = env.optObject("ok");
        return ok == null ? null : ConsoleSnapshot.AddedGame.fromJson(ok);
    }

    public void recordLaunch(ComputerDetails computer, NvApp app) {
        if (app.getRole() != NvApp.Role.STREAM) return;
        Json.Obj entry = Json.obj()
                .put("host", computer.uuid)
                .put("host_name", computer.displayName())
                .put("install_id", app.getStableId())
                .put("app_id", app.getAppId())
                .put("title", app.getAppName())
                .put("at", clock.nowMs());
        if (handle == 0L) return;
        try {
            NightjarConsoleJni.repoRecordLaunch(handle, entry.toString());
        } catch (RuntimeException | UnsatisfiedLinkError e) {
            LimeLog.warning("Console recordLaunch failed: " + e.getMessage());
        }
    }

    public void forgetHost(String hostUuid) {
        if (handle == 0L) return;
        try {
            NightjarConsoleJni.repoForgetHost(handle, hostUuid);
        } catch (RuntimeException | UnsatisfiedLinkError e) {
            LimeLog.warning("Console forgetHost failed: " + e.getMessage());
        }
    }

    public boolean isFavourite(String hostUuid, String key, int appId) {
        if (handle == 0L) return false;
        try {
            return NightjarConsoleJni.repoIsFavourite(handle, hostUuid, key == null ? "" : key, appId);
        } catch (RuntimeException | UnsatisfiedLinkError e) {
            LimeLog.warning("Console isFavourite failed: " + e.getMessage());
            return false;
        }
    }

    public boolean isFavourite(String hostUuid, int appId) {
        return isFavourite(hostUuid, "", appId);
    }

    public void setFavourite(String hostUuid, String hostName, String key, int appId, String title, boolean starred) {
        if (handle == 0L) return;
        Json.Obj entry = Json.obj()
                .put("host", hostUuid)
                .put("install_id", key == null ? "" : key)
                .put("app_id", appId)
                .put("title", title == null ? "" : title);
        try {
            // TODO(integrator-642): check boolean return once repoSetFavourite returns boolean on NightjarConsoleJni.
            NightjarConsoleJni.repoSetFavourite(handle, entry.toString(), starred);
        } catch (RuntimeException | UnsatisfiedLinkError e) {
            LimeLog.warning("Console setFavourite failed: " + e.getMessage());
        }
    }

    public void setFavourite(String hostUuid, String hostName, int appId, String title, boolean starred) {
        setFavourite(hostUuid, hostName, "", appId, title, starred);
    }

    public void setHidden(List<ConsoleSnapshot.Source> copies, boolean hide) {
        if (handle == 0L) return;
        Json.Arr keys = Json.arr();
        for (ConsoleSnapshot.Source copy : copies) {
            keys.add(Json.obj()
                    .put("host", copy.hostUuid)
                    .put("install_id", copy.stableId())
                    .put("app_id", copy.appId)
                    .put("title", copy.title));
        }
        try {
            NightjarConsoleJni.repoSetHidden(handle, keys.toString(), hide);
        } catch (RuntimeException | UnsatisfiedLinkError e) {
            LimeLog.warning("Console setHidden failed: " + e.getMessage());
        }
    }

    public String projectHome(List<ComputerDetails> computers, String filterJson, String query,
                              String sortBy, String sortOrder) {
        return NightjarConsoleJni.repoProjectHome(handle, hostsJson(computers, false, null, null),
                filterJson, query, sortBy, sortOrder);
    }

    public String gameDetail(List<ComputerDetails> computers, String hostId, int appId, boolean nativeApi) {
        return NightjarConsoleJni.repoGameDetail(handle, hostsJson(computers, false, null, null), hostId, appId, nativeApi);
    }

    private String hostsJson(List<ComputerDetails> computers, boolean network,
                             Map<String, HostEndpoint> endpointMap, Map<String, List<NvApp>> classicHdrOut) {
        Json.Arr arr = Json.arr();
        if (computers != null) {
            for (ComputerDetails c : computers) {
                if (c == null || c.uuid == null) continue;
                HostEndpoint ep = network && endpointMap != null ? endpointMap.get(c.uuid) : null;
                arr.add(hostDescriptor(c, ep, network, classicHdrOut));
            }
        }
        return arr.toString();
    }

    private Json.Obj hostDescriptor(ComputerDetails c, HostEndpoint ep, boolean network,
                                    Map<String, List<NvApp>> classicHdrOut) {
        boolean isNative = c.caps.has(HostCapabilities.Feature.NATIVE_API);
        boolean online = c.state == ComputerDetails.State.ONLINE && (!network || !isNative || ep != null);
        Json.Obj obj = Json.obj()
                .put("uuid", c.uuid)
                .put("name", c.displayName())
                .put("online", online)
                .put("paired", c.pairState == PairingManager.PairState.PAIRED)
                .put("is_native", isNative)
                .put("address", ep != null && ep.address != null ? ep.address : "")
                .put("http_port", ep != null ? ep.httpPort : (c.activeAddress != null ? c.activeAddress.port : 47989))
                .put("https_port", ep != null ? ep.httpsPort : c.httpsPort);
        if (ep != null && ep.serverCertificatePem != null) {
            obj.put("server_cert_pem", ep.serverCertificatePem);
        }
        if (!isNative && c.pairState == PairingManager.PairState.PAIRED) {
            List<NvApp> apps = classic.apps(c);
            if (apps != null) {
                if (classicHdrOut != null) classicHdrOut.put(c.uuid, apps);
                Json.Arr appArr = Json.arr();
                for (NvApp app : apps) {
                    appArr.add(Json.obj()
                            .put("id", app.getAppId())
                            .put("name", app.getAppName())
                            .put("uuid", app.getAppUuid())
                            .put("art_version", app.getArtVersion()));
                }
                obj.put("classic_apps", Json.obj().put("version", 1).put("apps", appArr));
            }
            if (c.hasRunningGame()) {
                obj.put("running_app_id", c.runningGameId);
                if (c.runningGameUuid != null) obj.put("running_app_uuid", c.runningGameUuid);
            }
        }
        return obj;
    }
}
