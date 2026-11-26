package com.limelight.nvstream.jni;

/**
 * Pure-JDK JNI bridge to Rust's cross-host console repository (`nightjar_core::console`),
 * persistent console stores (`console-library/`, `hidden_apps.json`, `console_recents.json`,
 * `console_favourites.json`), Home filter/sort/search projections, Game detail state, and
 * pure native API JSON/policy helpers.
 */
public final class NightjarConsoleJni {
    static {
        NightjarJni.ensureLibraryLoaded();
    }

    /** Transport callback invoked by Rust worker threads for tests / demo transports. */
    public interface TransportCallback {
        String call(String hostUuid, String op, String arg);
    }

    private NightjarConsoleJni() {
    }

    // Repository lifecycle & one-time legacy SharedPreferences migration
    public static native long repoCreate(String baseDir);
    public static native void repoDestroy(long handle);
    public static native void repoClearCache(long handle);
    public static native void repoResetForTests(long handle, String baseDir);
    public static native String repoImportLegacy(
            long handle,
            String recentsJson,
            String favouritesJson,
            String hiddenEntriesJson);

    // Snapshot, refresh & library actions
    public static native String repoCached(long handle, String hostsJson);
    public static native String repoRefresh(
            long handle,
            String hostsJson,
            byte[] certDer,
            byte[] keyPkcs8,
            String uniqueId,
            long nowMs,
            TransportCallback callback);
    public static native String repoAddToLibrary(
            long handle,
            String hostJson,
            String gameId,
            byte[] certDer,
            byte[] keyPkcs8,
            String uniqueId,
            TransportCallback callback);

    // Favourites, recents, hidden state & cleanup
    public static native boolean repoToggleFavourite(long handle, String entryJson);
    public static native boolean repoSetFavourite(long handle, String entryJson, boolean starred);
    public static native boolean repoIsFavourite(
            long handle,
            String hostId,
            String installId,
            int appId);
    public static native boolean repoSetHidden(long handle, String keysJson, boolean hidden);
    public static native boolean repoHideGame(
            long handle,
            String hostsJson,
            String hostId,
            int appId,
            boolean hidden);
    public static native void repoRecordLaunch(long handle, String recentJson);
    public static native String repoRecents(long handle);
    public static native void repoForgetHost(long handle, String hostId);
    public static native String repoHiddenListings(long handle, String hostsJson);
    public static native boolean repoIsHidden(
            long handle,
            String hostId,
            String installId,
            int appId);
    public static native String repoHiddenOf(long handle, String hostId);
    public static native String repoHiddenIds(long handle, String hostId);
    public static native boolean repoHiddenSetApp(
            long handle,
            String hostId,
            int appId,
            String stableId,
            boolean hidden);
    public static native String repoHiddenFollowing(
            long handle,
            String hostId,
            String gamesJson);
    public static native String repoHiddenChanged(
            long handle,
            String hostId,
            String shownJson,
            String hiddenJson);
    public static native boolean repoHiddenSnapshotChanged(long handle, boolean recordOnly);
    public static native boolean repoHasAnyLibrary(String baseDir);
    public static native boolean repoHasLibrary(String baseDir, String hostId);
    public static native boolean repoHasClassicAppList(String appListDir, String hostId);
    public static native String repoFindCachedLibraryCover(
            String baseDir,
            String hostId,
            int appId,
            String installId);

    // Home & GameDetail projections
    public static native String repoProjectHome(
            long handle,
            String hostsJson,
            String filterJson,
            String query,
            String sortBy,
            String sortOrder);
    public static native String repoGameDetail(
            long handle,
            String hostsJson,
            String hostId,
            int appId,
            boolean nativeApi);

    // Pure JSON / policy helpers
    public static native String parseAdded(String rawJson, String gameId);
    public static native String parseCapabilitiesLenient(String rawJson);
    public static native String coopProblemKind(int status, String code, String message);
    public static native boolean guestsEnded(String reason);
}
