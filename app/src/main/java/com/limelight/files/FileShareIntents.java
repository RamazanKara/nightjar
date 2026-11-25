package com.limelight.files;

import android.content.ComponentName;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ProviderInfo;
import android.database.Cursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import android.text.TextUtils;

import com.limelight.LimeLog;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Share-intent parsing, URI validation, persisted SAF grant ownership, and {@link ContentResolver} file
 * inspection helpers for {@link FileTransferActivity}. Enforces that external shares only send another app's
 * content URIs (except Nightjar's own problem report via {@link FileTransferActivity#REPORT_ALIAS}).
 */
public final class FileShareIntents {
    private static final String GRANTS_PREFS = "file_transfer_grants";

    private FileShareIntents() {}

    /**
     * The PC the screen is for, or null to ask. Only Nightjar's menus name one: a share always asks, so another
     * app cannot have files sent to a PC without the user choosing it.
     */
    static String chosenPc(Intent intent) {
        String action = intent.getAction();
        boolean share = Intent.ACTION_SEND.equals(action) || Intent.ACTION_SEND_MULTIPLE.equals(action);
        return share ? null : intent.getStringExtra(FileTransferActivity.EXTRA_PC_UUID);
    }

    /**
     * Whether a shared file may be sent: a content URI that another app's provider serves. A file path or one of
     * Nightjar's providers would let any app have Nightjar send its private files (keys, settings). An authority
     * with a user prefix ({@code 0@...}) resolves to no provider here but opens the unprefixed one, Nightjar's
     * own included, so it is refused, and so is an authority no provider serves.
     */
    static boolean sendable(Uri uri, String ownPackage, String providerPackage) {
        String authority = uri.getAuthority();
        return ContentResolver.SCHEME_CONTENT.equals(uri.getScheme()) && !TextUtils.isEmpty(authority)
                && authority.indexOf('@') < 0 && providerPackage != null && !ownPackage.equals(providerPackage);
    }

    /**
     * As above, except that a share through {@link FileTransferActivity#REPORT_ALIAS} may also send Nightjar's
     * problem report: {@code report} is its URI then, null otherwise. Only that one file, not the rest of the
     * provider that serves it (the clipboard image), and only Nightjar can open that alias, so no other app gets
     * a private file sent.
     */
    static boolean sendable(Uri uri, String ownPackage, String providerPackage, Uri report) {
        if (report != null && report.equals(uri)) return true;
        return sendable(uri, ownPackage, providerPackage);
    }

    /** Whether Report a problem opened the screen, through {@link FileTransferActivity#REPORT_ALIAS}. */
    static boolean forReport(Intent intent) {
        ComponentName component = intent.getComponent();
        return component != null && FileTransferActivity.REPORT_ALIAS.equals(component.getClassName());
    }

    /** The app whose provider serves {@code uri}, or null when none does (then the share is refused). */
    static String providerPackage(Context context, Uri uri) {
        if (TextUtils.isEmpty(uri.getAuthority())) return null;
        ProviderInfo provider = context.getPackageManager().resolveContentProvider(uri.getAuthority(), 0);
        return provider == null ? null : provider.packageName;
    }

    /** The files of an ACTION_SEND or ACTION_SEND_MULTIPLE, in the order shared. */
    static List<Uri> sharedUris(Intent intent) {
        List<Uri> uris = new ArrayList<>();
        if (Intent.ACTION_SEND.equals(intent.getAction())) {
            Uri uri = intent.getParcelableExtra(Intent.EXTRA_STREAM);
            if (uri != null) uris.add(uri);
        } else if (Intent.ACTION_SEND_MULTIPLE.equals(intent.getAction())) {
            List<Uri> shared = intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM);
            if (shared != null) {
                for (Uri uri : shared) if (uri != null) uris.add(uri);
            }
        }
        return uris;
    }

    /** Asks the provider for a file's name and size, opening the file when the size is not listed. Off the main thread. */
    static FileTransferActivity.SharedFile describe(ContentResolver resolver, Uri uri) {
        String name = null;
        long size = -1;
        try (Cursor cursor = resolver.query(uri, new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE},
                null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                if (!cursor.isNull(0)) name = cursor.getString(0);
                if (!cursor.isNull(1)) size = cursor.getLong(1);
            }
        } catch (RuntimeException e) {
            LimeLog.warning("File transfer: no details for a shared file: " + e.getMessage());
        }
        if (name == null) name = uri.getLastPathSegment() != null ? uri.getLastPathSegment() : "file";
        if (size < 0) {
            try (ParcelFileDescriptor fd = resolver.openFileDescriptor(uri, "r")) {
                if (fd != null) size = fd.getStatSize();
            } catch (IOException | RuntimeException e) {
                LimeLog.warning("File transfer: could not open a shared file: " + e.getMessage());
            }
        }
        return new FileTransferActivity.SharedFile(uri, name, size);
    }

    static int detach(ParcelFileDescriptor fd) throws IOException {
        if (fd == null) throw new FileNotFoundException("no descriptor");
        return fd.detachFd();
    }

    /** Attempts to persist a SAF URI grant so background transfers survive Activity finish; returns true on success. */
    static boolean takePersistable(ContentResolver resolver, Uri uri, int flags) {
        return takePersistable(null, resolver, uri, flags);
    }

    static boolean takePersistable(Context context, ContentResolver resolver, Uri uri, int flags) {
        if (resolver == null || uri == null) return false;
        try {
            resolver.takePersistableUriPermission(uri, flags);
            if (context != null) {
                SharedPreferences ledger = grantPrefs(context);
                int combined = ledger.getInt(uri.toString(), 0) | flags;
                ledger.edit().putInt(uri.toString(), combined).apply();
            }
            return true;
        } catch (SecurityException | UnsupportedOperationException | IllegalArgumentException e) {
            return false;
        }
    }

    static void releasePersistable(ContentResolver resolver, Uri uri, int flags) {
        releasePersistable(null, resolver, uri, flags);
    }

    static void releasePersistable(Context context, ContentResolver resolver, Uri uri, int flags) {
        if (resolver == null || uri == null) return;
        try {
            resolver.releasePersistableUriPermission(uri, flags);
        } catch (SecurityException | UnsupportedOperationException | IllegalArgumentException ignored) {
        } finally {
            if (context != null) {
                grantPrefs(context).edit().remove(uri.toString()).apply();
            }
        }
    }

    /** Releases all file-transfer SAF grants recorded across process kills except URIs currently held. */
    public static void releaseStaleGrants(Context context, Collection<Uri> activeUris) {
        if (context == null) return;
        SharedPreferences ledger = grantPrefs(context);
        Map<String, ?> entries = ledger.getAll();
        if (entries.isEmpty()) return;
        Set<String> keep = new HashSet<>();
        if (activeUris != null) {
            for (Uri uri : activeUris) {
                if (uri != null) keep.add(uri.toString());
            }
        }
        ContentResolver resolver = context.getContentResolver();
        SharedPreferences.Editor editor = ledger.edit();
        for (Map.Entry<String, ?> entry : entries.entrySet()) {
            String key = entry.getKey();
            if (keep.contains(key)) continue;
            int flags = entry.getValue() instanceof Integer ? (Integer) entry.getValue()
                    : (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
            if (resolver != null && flags != 0) {
                try {
                    resolver.releasePersistableUriPermission(Uri.parse(key), flags);
                } catch (SecurityException | UnsupportedOperationException | IllegalArgumentException ignored) {
                }
            }
            editor.remove(key);
        }
        editor.apply();
    }

    /** Whether {@code uri} already holds content before opening {@code "rwt"} for download. */
    static boolean documentHasExistingContent(ContentResolver resolver, Uri uri) {
        if (resolver == null || uri == null) return false;
        try (Cursor cursor = resolver.query(uri, new String[] {OpenableColumns.SIZE}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) {
                return cursor.getLong(0) > 0;
            }
        } catch (RuntimeException ignored) {
        }
        try (ParcelFileDescriptor pfd = resolver.openFileDescriptor(uri, "r")) {
            return pfd != null && pfd.getStatSize() > 0;
        } catch (IOException | RuntimeException ignored) {
            return false;
        }
    }

    private static SharedPreferences grantPrefs(Context context) {
        return context.getApplicationContext().getSharedPreferences(GRANTS_PREFS, Context.MODE_PRIVATE);
    }

    /**
     * Opens an upload URI. When a non-persistable temporary URI grant (such as an external share) is queued,
     * callers may pass a pre-opened {@link ParcelFileDescriptor} captured while the Activity still held the grant
     * so queued uploads succeed in the background after the Activity finishes; unused descriptors are closed on
     * cancel/clear/close.
     */
    static FileTransferController.Opener uploadOpener(ContentResolver resolver, Uri uri, ParcelFileDescriptor preopened) {
        AtomicReference<ParcelFileDescriptor> held = new AtomicReference<>(preopened);
        return new FileTransferController.Opener() {
            @Override public int open() throws IOException {
                ParcelFileDescriptor pfd = held.getAndSet(null);
                if (pfd != null) return detach(pfd);
                return detach(resolver.openFileDescriptor(uri, "r"));
            }

            @Override public void close() {
                ParcelFileDescriptor pfd = held.getAndSet(null);
                if (pfd != null) {
                    try {
                        pfd.close();
                    } catch (IOException ignored) {
                    }
                }
            }
        };
    }

    static ParcelFileDescriptor tryOpenRead(ContentResolver resolver, Uri uri) {
        try {
            return resolver.openFileDescriptor(uri, "r");
        } catch (IOException | RuntimeException e) {
            return null;
        }
    }

    static String displayName(ContentResolver resolver, Uri uri, String fallback) {
        try (Cursor cursor = resolver.query(uri, new String[] {OpenableColumns.DISPLAY_NAME},
                null, null, null)) {
            if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) return cursor.getString(0);
        } catch (RuntimeException ignored) {
            // The name the PC has is as good.
        }
        return fallback;
    }
}
