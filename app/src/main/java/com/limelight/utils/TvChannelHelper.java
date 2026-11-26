package com.limelight.utils;

import android.annotation.TargetApi;
import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.ContentResolver;
import android.content.ContentUris;
import android.content.ContentValues;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.database.sqlite.SQLiteException;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.graphics.drawable.Drawable;
import android.media.tv.TvContract;
import android.net.Uri;
import android.os.Build;

import com.limelight.LimeLog;
import com.limelight.PosterContentProvider;
import com.limelight.R;
import com.limelight.computers.HostCapabilityCache;
import com.limelight.grid.assets.DiskAssetLoader;
import com.limelight.grid.assets.PlaceholderArt;
import com.limelight.nativeapi.ConsoleRepository;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.NvHTTP;

import org.xmlpull.v1.XmlPullParserException;

import java.io.File;
import java.io.IOException;
import java.io.OutputStream;
import java.io.StringReader;
import java.util.List;

public class TvChannelHelper {

    private static final int ASPECT_RATIO_MOVIE_POSTER = 5;
    private static final int TYPE_GAME = 12;
    private static final int INTERNAL_PROVIDER_ID_INDEX = 1;
    private static final int PROGRAM_BROWSABLE_INDEX = 2;
    private static final int POSTER_ART_URI_INDEX = 3;
    private static final int ID_INDEX = 0;
    private Activity context;

    public TvChannelHelper(Activity context) {
        this.context = context;
    }

    void requestChannelOnHomeScreen(ComputerDetails computer) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (!isAndroidTV()) {
                return;
            }

            Long channelId = getChannelId(computer.uuid);
            if (channelId == null) {
                return;
            }

            Intent intent = new Intent(TvContract.ACTION_REQUEST_CHANNEL_BROWSABLE);
            intent.putExtra(TvContract.EXTRA_CHANNEL_ID, getChannelId(computer.uuid));
            try {
                context.startActivityForResult(intent, 0);
            } catch (Exception ignored) {
                // ActivityNotFoundException is the only officially documented
                // exception that can result from this call. However some buggy
                // devices throw others.
                // See https://github.com/moonlight-stream/moonlight-android/issues/1302
            }
        }
    }

    void createTvChannel(ComputerDetails computer) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (!isAndroidTV()) {
                return;
            }

            ChannelBuilder builder = new ChannelBuilder()
                    .setType(TvContract.Channels.TYPE_PREVIEW)
                    .setDisplayName(computer.displayName())
                    .setInternalProviderId(computer.uuid)
                    .setAppLinkIntent(ServerHelper.createPcShortcutIntent(context, computer));

            Long channelId = getChannelId(computer.uuid);
            if (channelId != null) {
                try {
                    context.getContentResolver().update(TvContract.buildChannelUri(channelId),
                            builder.toContentValues(), null, null);
                } catch (RuntimeException e) {
                    LimeLog.warning("TV channel update failed: " + e.getMessage());
                }
                return;
            }

            Uri channelUri;

            try {
                channelUri = context.getContentResolver().insert(
                        TvContract.Channels.CONTENT_URI, builder.toContentValues());
                if (channelUri != null) {
                    long id = ContentUris.parseId(channelUri);
                    updateChannelIcon(id);
                }
            } catch (RuntimeException e) {
                // Broken OEM / HarmonyOS devices may reject or lack Leanback provider URIs.
                LimeLog.warning("TV channel insert failed: " + e.getMessage());
            }
        }
    }

    @TargetApi(Build.VERSION_CODES.O)
    private void updateChannelIcon(long channelId) {
        Bitmap logo = drawableToBitmap(context.getResources().getDrawable(R.drawable.ic_channel));
        try {
            Uri localUri = TvContract.buildChannelLogoUri(channelId);
            try (OutputStream outputStream = context.getContentResolver().openOutputStream(localUri)) {
                logo.compress(Bitmap.CompressFormat.PNG, 100, outputStream);
                outputStream.flush();
            } catch (SQLiteException | IOException e) {
                LimeLog.warning("Failed to store the logo to the system content provider.");
                e.printStackTrace();
            }
        } finally {
            logo.recycle();
        }
    }

    private Bitmap drawableToBitmap(Drawable drawable) {
        int width = context.getResources().getDimensionPixelSize(R.dimen.tv_channel_logo_width);
        int height = context.getResources().getDimensionPixelSize(R.dimen.tv_channel_logo_width);

        Bitmap bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888);
        Canvas canvas = new Canvas(bitmap);
        drawable.setBounds(0, 0, canvas.getWidth(), canvas.getHeight());
        drawable.draw(canvas);
        return bitmap;
    }

    /**
     * The key of the game's cached cover: the one the launch was started with, unless only the
     * app's own key names a cached file. A stream's app is made again from the launch's extras
     * without the host's art version, so its own key names a file that was never cached and the
     * channel's card stayed grey (nightjar#130).
     */
    static String posterArtKey(String launchArtKey, String appKey, boolean launchCached, boolean appCached) {
        boolean launch = launchArtKey != null && !launchArtKey.isEmpty();
        if (launch && launchCached) return launchArtKey;
        if (appCached) return appKey;
        return launch ? launchArtKey : appKey;
    }

    private static boolean isCover(DiskAssetLoader disk, String uuid, String key) {
        return DiskAssetLoader.isValidCacheKey(key) && PlaceholderArt.isCover(disk.getFile(uuid, key));
    }

    private static boolean isPlaceholder(DiskAssetLoader disk, String uuid, String key) {
        if (!DiskAssetLoader.isValidCacheKey(key)) return false;
        File file = disk.getFile(uuid, key);
        return file.isFile() && PlaceholderArt.matches(file);
    }

    private static String pickNewerCover(DiskAssetLoader disk, String uuid, String first, String second) {
        boolean firstCover = isCover(disk, uuid, first);
        boolean secondCover = isCover(disk, uuid, second);
        if (firstCover && secondCover) {
            File f1 = disk.getFile(uuid, first);
            File f2 = disk.getFile(uuid, second);
            return f2.lastModified() > f1.lastModified() ? second : first;
        }
        if (firstCover) return first;
        if (secondCover) return second;
        return null;
    }

    private static String cachedAppListArtKey(Context context, String computerUuid, int appId, String appUuid) {
        if (context == null || computerUuid == null || computerUuid.isEmpty()) return null;
        if (!CacheHelper.cacheFileExists(context.getCacheDir(), "applist", computerUuid)) return null;
        try {
            String raw = CacheHelper.readInputStreamToString(
                    CacheHelper.openCacheFileForInput(context.getCacheDir(), "applist", computerUuid));
            if (raw.isEmpty()) return null;
            NvApp idMatch = null;
            for (NvApp candidate : NvHTTP.getAppListByReader(new StringReader(raw),
                    HostCapabilityCache.load(context, computerUuid))) {
                if (appUuid != null && !appUuid.isEmpty()
                        && appUuid.equalsIgnoreCase(candidate.getAppUuid())) {
                    return candidate.getAssetCacheKey();
                }
                if (idMatch == null && appId != 0 && candidate.getAppId() == appId) {
                    idMatch = candidate;
                }
            }
            return idMatch == null ? null : idMatch.getAssetCacheKey();
        } catch (IOException | XmlPullParserException e) {
            // No readable cached applist.
        }
        return null;
    }

    /** Extracts the cover key from one of this app's own PosterContentProvider URIs for {@code uuid}, or null. */
    static String posterArtKeyFromUri(String uriString, String uuid) {
        if (uriString == null || uriString.isEmpty() || uuid == null || uuid.isEmpty()) return null;
        Uri uri = Uri.parse(uriString);
        if (!ContentResolver.SCHEME_CONTENT.equals(uri.getScheme())
                || !PosterContentProvider.AUTHORITY.equals(uri.getAuthority())) {
            return null;
        }
        List<String> segments = uri.getPathSegments();
        if (segments.size() != 3 || !"boxart".equals(segments.get(0))) return null;
        if (!uuid.equals(segments.get(1))) return null;
        String key = segments.get(2);
        return DiskAssetLoader.isValidCacheKey(key) ? key : null;
    }

    /**
     * Resolves the cover cache key when launching a game through a shortcut, TV channel card, or
     * frontend entry ({@code ShortcutTrampoline}). A shortcut's {@link NvApp} is rebuilt without the
     * host's cover version, so without this a subsequent launch from a TV channel card passes an
     * unversioned key and overwrites the card's cover art with the title-only poster (nightjar#130).
     */
    public static String resolveShortcutArtKey(Context context, String computerUuid, NvApp app,
                                               int fallbackAppId, String incomingArtKey) {
        String validIncoming = DiskAssetLoader.isValidCacheKey(incomingArtKey) ? incomingArtKey : null;
        String appKey = app.getAssetCacheKey();
        if (context == null || computerUuid == null || computerUuid.isEmpty()) {
            return validIncoming != null ? validIncoming : appKey;
        }
        int lookupAppId = app.getAppId() != 0 ? app.getAppId() : fallbackAppId;
        DiskAssetLoader disk = new DiskAssetLoader(context);
        ConsoleRepository.CachedLibraryCover libraryCover =
                ConsoleRepository.findCachedLibraryCover(context, computerUuid, lookupAppId, app.getAppUuid());
        String libraryKey = libraryCover == null ? null : libraryCover.artKey;
        String appListKey = cachedAppListArtKey(context, computerUuid, lookupAppId, app.getAppUuid());

        // Prefer the current library/applist cover when cached on disk so cover updates on the host
        // flow into existing shortcuts as well.
        String cachedFromMeta = pickNewerCover(disk, computerUuid, libraryKey, appListKey);
        if (cachedFromMeta != null) return cachedFromMeta;

        if (isCover(disk, computerUuid, validIncoming)) return validIncoming;
        if (isCover(disk, computerUuid, appKey)) return appKey;

        boolean explicitlyNoCover = (libraryCover != null && libraryCover.explicitlyNoCover)
                || isPlaceholder(disk, computerUuid, validIncoming);
        if (!explicitlyNoCover) {
            String diskKey = disk.findCachedCoverKey(computerUuid, lookupAppId);
            if (diskKey != null) return diskKey;
        }

        if (DiskAssetLoader.isValidCacheKey(libraryKey)) return libraryKey;
        if (DiskAssetLoader.isValidCacheKey(appListKey)) return appListKey;
        return validIncoming != null ? validIncoming : appKey;
    }

    /**
     * Resolves the cover key to store on a TV channel card, keeping the fast path when
     * {@code launchArtKey} already names a cached cover, and otherwise recovering from saved
     * libraries, the card's existing poster URI, or the on-disk cover cache.
     */
    static String resolveChannelArtKey(Context context, String computerUuid, NvApp app,
                                       String launchArtKey, String existingPosterArtKey) {
        String validLaunch = DiskAssetLoader.isValidCacheKey(launchArtKey) ? launchArtKey : null;
        String validExisting = DiskAssetLoader.isValidCacheKey(existingPosterArtKey) ? existingPosterArtKey : null;
        String appKey = app.getAssetCacheKey();
        DiskAssetLoader disk = new DiskAssetLoader(context);

        if (isCover(disk, computerUuid, validLaunch)) return validLaunch;
        if (isCover(disk, computerUuid, appKey)) return appKey;

        ConsoleRepository.CachedLibraryCover libraryCover =
                ConsoleRepository.findCachedLibraryCover(context, computerUuid, app.getAppId(), app.getAppUuid());
        String libraryKey = libraryCover == null ? null : libraryCover.artKey;
        String appListKey = cachedAppListArtKey(context, computerUuid, app.getAppId(), app.getAppUuid());
        String cachedFromMeta = pickNewerCover(disk, computerUuid, libraryKey, appListKey);
        if (cachedFromMeta != null) return cachedFromMeta;

        boolean explicitlyNoCover = (libraryCover != null && libraryCover.explicitlyNoCover)
                || isPlaceholder(disk, computerUuid, validLaunch);
        if (!explicitlyNoCover) {
            if (isCover(disk, computerUuid, validExisting)) return validExisting;
            String diskKey = disk.findCachedCoverKey(computerUuid, app.getAppId());
            if (diskKey != null) return diskKey;
        }

        if (DiskAssetLoader.isValidCacheKey(libraryKey)) return libraryKey;
        if (DiskAssetLoader.isValidCacheKey(appListKey)) return appListKey;
        return posterArtKey(validLaunch, appKey, false, false);
    }

    private static String stableProgramId(NvApp app) {
        String uuid = app.getAppUuid();
        return (uuid == null || uuid.isEmpty()) ? String.valueOf(app.getAppId()) : uuid;
    }

    String addGameToChannel(ComputerDetails computer, NvApp app, String launchArtKey) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (!isAndroidTV()) {
                return null;
            }

            Long channelId = getChannelId(computer.uuid);
            if (channelId == null) {
                return null;
            }

            ExistingProgram existing = getExistingProgram(channelId, app);
            String existingArtKey = existing == null ? null : posterArtKeyFromUri(existing.posterArtUri, computer.uuid);
            String artKey = resolveChannelArtKey(context, computer.uuid, app, launchArtKey, existingArtKey);
            // A game without a cover of its own gets a poster with its title: the TV home screen
            // draws a poster it cannot open as an empty grey card.
            if (!PlaceholderArt.isCover(new DiskAssetLoader(context).getFile(computer.uuid, artKey))) {
                try {
                    PosterContentProvider.writeTitlePoster(context, computer.uuid, artKey, app.getAppName());
                } catch (IOException | RuntimeException e) {
                    LimeLog.warning("TV channel: no title poster for " + app.getAppName() + ": " + e);
                }
            }

            PreviewProgramBuilder builder = new PreviewProgramBuilder()
                    .setChannelId(channelId)
                    .setType(TYPE_GAME)
                    .setTitle(app.getAppName())
                    .setPosterArtAspectRatio(ASPECT_RATIO_MOVIE_POSTER)
                    .setPosterArtUri(PosterContentProvider.createBoxArtUri(computer.uuid, artKey))
                    .setIntent(ServerHelper.createAppShortcutIntent(context, computer, app, artKey))
                    .setInternalProviderId(stableProgramId(app))
                    // Weight should increase each time we run the game
                    .setWeight((int)((System.currentTimeMillis() - 1500000000000L) / 1000));

            if (existing != null) {
                try {
                    context.getContentResolver().update(TvContract.buildPreviewProgramUri(existing.id),
                            builder.toContentValues(), null, null);
                } catch (RuntimeException e) {
                    LimeLog.warning("TV program update failed: " + e.getMessage());
                    return null;
                }
                return artKey;
            }

            try {
                context.getContentResolver().insert(TvContract.PreviewPrograms.CONTENT_URI,
                        builder.toContentValues());
                TvContract.requestChannelBrowsable(context, channelId);
            } catch (RuntimeException e) {
                // Broken OEM / HarmonyOS devices may reject or lack Leanback provider URIs.
                LimeLog.warning("TV program insert failed: " + e.getMessage());
                return null;
            }

            return artKey;
        }
        return null;
    }

    void deleteChannel(ComputerDetails computer) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (!isAndroidTV()) {
                return;
            }

            Long channelId = getChannelId(computer.uuid);
            if (channelId == null) {
                return;
            }

            try {
                context.getContentResolver().delete(TvContract.buildChannelUri(channelId), null, null);
            } catch (RuntimeException e) {
                LimeLog.warning("TV channel delete failed: " + e.getMessage());
            }
        }
    }

    void deleteProgram(ComputerDetails computer, NvApp app) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            if (!isAndroidTV()) {
                return;
            }

            Long channelId = getChannelId(computer.uuid);
            if (channelId == null) {
                return;
            }

            ExistingProgram existing = getExistingProgram(channelId, app);
            if (existing == null) {
                return;
            }

            try {
                context.getContentResolver().delete(TvContract.buildPreviewProgramUri(existing.id), null, null);
            } catch (RuntimeException e) {
                LimeLog.warning("TV program delete failed: " + e.getMessage());
            }
        }
    }

    private static final class ExistingProgram {
        final long id;
        final String posterArtUri;

        ExistingProgram(long id, String posterArtUri) {
            this.id = id;
            this.posterArtUri = posterArtUri;
        }
    }

    @TargetApi(Build.VERSION_CODES.O)
    private Long getChannelId(String computerUuid) {
        try (Cursor cursor = context.getContentResolver().query(
                TvContract.Channels.CONTENT_URI,
                new String[] {TvContract.Channels._ID, TvContract.Channels.COLUMN_INTERNAL_PROVIDER_ID},
                null,
                null,
                null)) {
            if (cursor == null || cursor.getCount() == 0) {
                return null;
            }
            while (cursor.moveToNext()) {
                String internalProviderId = cursor.getString(INTERNAL_PROVIDER_ID_INDEX);
                if (computerUuid.equals(internalProviderId)) {
                    return cursor.getLong(ID_INDEX);
                }
            }

            return null;
        } catch (RuntimeException e) {
            LimeLog.warning("TV channel query failed: " + e.getMessage());
            return null;
        }
    }

    @TargetApi(Build.VERSION_CODES.O)
    private ExistingProgram getExistingProgram(long channelId, NvApp app) {
        String primaryId = stableProgramId(app);
        String legacyId = String.valueOf(app.getAppId());
        try (Cursor cursor = context.getContentResolver().query(
                TvContract.buildPreviewProgramsUriForChannel(channelId),
                new String[] {
                        TvContract.PreviewPrograms._ID,
                        TvContract.PreviewPrograms.COLUMN_INTERNAL_PROVIDER_ID,
                        TvContract.PreviewPrograms.COLUMN_BROWSABLE,
                        TvContract.PreviewPrograms.COLUMN_POSTER_ART_URI
                },
                null,
                null,
                null)) {
            if (cursor == null || cursor.getCount() == 0) {
                return null;
            }
            ExistingProgram legacyMatch = null;
            while (cursor.moveToNext()) {
                String internalProviderId = cursor.getString(INTERNAL_PROVIDER_ID_INDEX);
                boolean isPrimary = primaryId.equals(internalProviderId);
                boolean isLegacy = !isPrimary && legacyId.equals(internalProviderId);
                if (isPrimary || isLegacy) {
                    long id = cursor.getLong(ID_INDEX);
                    int browsable = cursor.getInt(PROGRAM_BROWSABLE_INDEX);
                    if (browsable != 0) {
                        String posterUri = cursor.getColumnCount() > POSTER_ART_URI_INDEX && !cursor.isNull(POSTER_ART_URI_INDEX)
                                ? cursor.getString(POSTER_ART_URI_INDEX) : null;
                        ExistingProgram match = new ExistingProgram(id, posterUri);
                        if (isPrimary) {
                            return match;
                        }
                        if (legacyMatch == null) {
                            legacyMatch = match;
                        }
                    } else {
                        int countDeleted = context.getContentResolver().delete(TvContract.buildPreviewProgramUri(id), null, null);
                        if (countDeleted > 0) {
                            LimeLog.info("Preview program has been deleted");
                        } else {
                            LimeLog.warning("Preview program has not been deleted");
                        }
                    }
                }
            }

            return legacyMatch;
        } catch (RuntimeException e) {
            LimeLog.warning("TV program query failed: " + e.getMessage());
            return null;
        }
    }

    private static <T> String toValueString(T value) {
        return value == null ? null : value.toString();
    }

    private static String toUriString(Intent intent) {
        return intent == null ? null : intent.toUri(Intent.URI_INTENT_SCHEME);
    }

    @TargetApi(Build.VERSION_CODES.O)
    private boolean isAndroidTV() {
        return context.getPackageManager().hasSystemFeature(PackageManager.FEATURE_LEANBACK);
    }

    @TargetApi(Build.VERSION_CODES.O)
    private static class PreviewProgramBuilder {

        private ContentValues mValues = new ContentValues();


        public PreviewProgramBuilder setChannelId(Long channelId) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_CHANNEL_ID, channelId);
            return this;
        }

        public PreviewProgramBuilder setType(int type) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_TYPE, type);
            return this;
        }

        public PreviewProgramBuilder setTitle(String title) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_TITLE, title);
            return this;
        }

        public PreviewProgramBuilder setPosterArtAspectRatio(int aspectRatio) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_POSTER_ART_ASPECT_RATIO, aspectRatio);
            return this;
        }

        public PreviewProgramBuilder setIntent(Intent intent) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_INTENT_URI, toUriString(intent));
            return this;
        }

        public PreviewProgramBuilder setIntentUri(Uri uri) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_INTENT_URI, toValueString(uri));
            return this;
        }

        public PreviewProgramBuilder setInternalProviderId(String id) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_INTERNAL_PROVIDER_ID, id);
            return this;
        }

        public PreviewProgramBuilder setPosterArtUri(Uri uri) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_POSTER_ART_URI, toValueString(uri));
            return this;
        }

        public PreviewProgramBuilder setWeight(int weight) {
            mValues.put(TvContract.PreviewPrograms.COLUMN_WEIGHT, weight);
            return this;
        }

        public ContentValues toContentValues() {
            return new ContentValues(mValues);
        }

    }

    @TargetApi(Build.VERSION_CODES.O)
    private static class ChannelBuilder {

        private ContentValues mValues = new ContentValues();

        public ChannelBuilder setType(String type) {
            mValues.put(TvContract.Channels.COLUMN_TYPE, type);
            return this;
        }

        public ChannelBuilder setDisplayName(String displayName) {
            mValues.put(TvContract.Channels.COLUMN_DISPLAY_NAME, displayName);
            return this;
        }

        public ChannelBuilder setInternalProviderId(String internalProviderId) {
            mValues.put(TvContract.Channels.COLUMN_INTERNAL_PROVIDER_ID, internalProviderId);
            return this;
        }

        public ChannelBuilder setAppLinkIntent(Intent intent) {
            mValues.put(TvContract.Channels.COLUMN_APP_LINK_INTENT_URI, toUriString(intent));
            return this;
        }

        public ContentValues toContentValues() {
            return new ContentValues(mValues);
        }

    }
}
