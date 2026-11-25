package com.limelight.files;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.content.Context;
import android.content.Intent;
import android.net.Uri;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

/** What the file screen takes from a share and how it words a transfer's row. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class FileTransferScreenTest {
    private final Context context = RuntimeEnvironment.getApplication();

    private static FileRow row(String json) {
        return FileRow.parse("[" + json + "]").get(0);
    }

    @Test
    public void aShareOfOneOrSeveralFilesKeepsTheirOrderAndAnythingElseBringsNone() {
        Uri a = Uri.parse("content://a/1");
        Uri b = Uri.parse("content://b/2");
        Intent one = new Intent(Intent.ACTION_SEND).putExtra(Intent.EXTRA_STREAM, a);
        assertEquals(Collections.singletonList(a), FileTransferActivity.sharedUris(one));
        Intent several = new Intent(Intent.ACTION_SEND_MULTIPLE)
                .putParcelableArrayListExtra(Intent.EXTRA_STREAM, new ArrayList<>(Arrays.asList(b, null, a)));
        assertEquals(Arrays.asList(b, a), FileTransferActivity.sharedUris(several));
        // A shared text has no file; opening from a menu has no share.
        assertTrue(FileTransferActivity.sharedUris(new Intent(Intent.ACTION_SEND)
                .putExtra(Intent.EXTRA_TEXT, "hello")).isEmpty());
        assertTrue(FileTransferActivity.sharedUris(FileTransferActivity.forPc(context, "pc")).isEmpty());
    }

    /** A provider that lists one file's name and size, as a gallery or cloud app does. */
    public static final class Listed extends android.content.ContentProvider {
        @Override public boolean onCreate() { return true; }
        @Override public android.database.Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
            android.database.MatrixCursor cursor = new android.database.MatrixCursor(
                    new String[] {android.provider.OpenableColumns.DISPLAY_NAME, android.provider.OpenableColumns.SIZE});
            cursor.addRow(new Object[] {"clip.mp4", 42L});
            return cursor;
        }
        @Override public String getType(Uri uri) { return null; }
        @Override public Uri insert(Uri uri, android.content.ContentValues values) { return null; }
        @Override public int delete(Uri uri, String selection, String[] args) { return 0; }
        @Override public int update(Uri uri, android.content.ContentValues values, String selection, String[] args) { return 0; }
    }

    @Test
    public void aSharedFilesNameAndSizeComeFromItsProviderAndAnUnknownSizeIsSaidSo() {
        // Read on a worker before the upload is queued; a provider that may fetch the file first never holds the screen.
        org.robolectric.Robolectric.setupContentProvider(Listed.class, "listed");
        FileTransferActivity.SharedFile listed = FileTransferActivity.describe(context.getContentResolver(), Uri.parse("content://listed/7"));
        assertEquals("clip.mp4", listed.name);
        assertEquals(42L, listed.size);
        FileTransferActivity.SharedFile gone = FileTransferActivity.describe(context.getContentResolver(), Uri.parse("content://gone/report.txt"));
        assertEquals("report.txt", gone.name);
        assertEquals(-1L, gone.size);
    }

    @Test
    public void theMenusOpenTheScreenForTheirPc() {
        Intent intent = FileTransferActivity.forPc(context, "tower");
        assertEquals(FileTransferActivity.class.getName(), intent.getComponent().getClassName());
        assertEquals("tower", intent.getStringExtra(FileTransferActivity.EXTRA_PC_UUID));
        assertEquals("tower", FileTransferActivity.chosenPc(intent));
    }

    @Test
    public void aShareAlwaysAsksWhichPcAndSendsOnlyAnotherAppsFiles() {
        // Another app naming the PC would send without the user choosing it.
        for (String action : new String[] {Intent.ACTION_SEND, Intent.ACTION_SEND_MULTIPLE}) {
            Intent share = FileTransferActivity.forPc(context, "tower").setAction(action)
                    .putExtra(Intent.EXTRA_STREAM, Uri.parse("content://media/1"));
            assertNull(action, FileTransferActivity.chosenPc(share));
        }
        String own = "com.butterpollo.client";
        assertTrue(FileTransferActivity.sendable(Uri.parse("content://media/external/1"), own, "com.android.providers.media"));
        assertFalse(FileTransferActivity.sendable(Uri.parse("content://com.butterpollo.client.reports/r/log.txt"), own, own));
        assertFalse(FileTransferActivity.sendable(
                Uri.parse("file:///data/data/com.butterpollo.client/shared_prefs/settings.xml"), own, null));
        assertFalse(FileTransferActivity.sendable(Uri.parse("android.resource://com.butterpollo.client/1"), own, null));
        assertFalse(FileTransferActivity.sendable(Uri.parse("content:///no-authority"), own, null));
    }

    @Test
    public void aShareWithAUserPrefixedAuthorityIsRefused() {
        // 0@<package>.reports resolves to no provider, but the resolver strips the prefix and opens Nightjar's own.
        String own = "com.butterpollo.client";
        assertFalse(FileTransferActivity.sendable(Uri.parse("content://0@com.butterpollo.client.reports/r/log.txt"),
                own, null));
        assertFalse(FileTransferActivity.sendable(Uri.parse("content://10@media/external/1"), own,
                "com.android.providers.media"));
    }

    @Test
    public void aShareNoProviderServesIsRefused() {
        String own = "com.butterpollo.client";
        assertFalse(FileTransferActivity.sendable(Uri.parse("content://gone/1"), own, null));
    }

    @Test
    public void onlyTheReportAliasSendsNightjarsReportAndNothingElseOfItsOwn() {
        String own = "com.butterpollo.client";
        Uri report = Uri.parse("content://com.butterpollo.client.reports/reports/nightjar-problem.txt");
        assertTrue(FileTransferActivity.sendable(report, own, own, report));
        assertFalse(FileTransferActivity.sendable(report, own, own, null));
        // The rest of the provider that serves the report (the clipboard image), another of Nightjar's providers
        // or a user-prefixed authority stays refused through the alias too.
        assertFalse(FileTransferActivity.sendable(
                Uri.parse("content://com.butterpollo.client.reports/clipboard/image.png"), own, own, report));
        assertFalse(FileTransferActivity.sendable(
                Uri.parse("content://com.butterpollo.client.reports/reports/other.txt"), own, own, report));
        assertFalse(FileTransferActivity.sendable(Uri.parse("content://com.butterpollo.client.files/x"), own, own, report));
        assertFalse(FileTransferActivity.sendable(
                Uri.parse("content://0@com.butterpollo.client.reports/reports/nightjar-problem.txt"), own, null, report));
        assertFalse(FileTransferActivity.sendable(Uri.parse("file:///data/data/com.butterpollo.client/x"), own, null, report));
        // Other apps' files go as before.
        assertTrue(FileTransferActivity.sendable(Uri.parse("content://media/external/1"), own,
                "com.android.providers.media", report));
    }

    @Test
    public void eachRowSaysWhereItIs() {
        String head = "\"direction\":\"upload\",\"name\":\"a_.txt\",\"local_name\":\"a?.txt\",\"size\":2048,";
        assertEquals(context.getString(R.string.files_queued),
                FileTransferActivity.describe(context, row("{\"id\":1," + head + "\"state\":\"queued\"}")));
        String sending = FileTransferActivity.describe(context,
                row("{\"id\":1," + head + "\"state\":\"running\",\"done\":1024,\"total\":2048}"));
        assertTrue(sending, sending.contains("50%"));
        assertEquals(context.getString(R.string.files_cancelling), FileTransferActivity.describe(context,
                row("{\"id\":1," + head + "\"state\":\"running\",\"done\":1,\"total\":2048,\"cancelling\":true}")));
        assertEquals(context.getString(R.string.files_sent), FileTransferActivity.describe(context,
                row("{\"id\":1," + head + "\"state\":\"done\",\"saved_name\":\"a_.txt\"}")));
        assertEquals(context.getString(R.string.files_sent_as, "a_ (1).txt"), FileTransferActivity.describe(context,
                row("{\"id\":1," + head + "\"state\":\"done\",\"saved_name\":\"a_ (1).txt\"}")));
        assertEquals(context.getString(R.string.files_problem_no_space), FileTransferActivity.describe(context,
                row("{\"id\":1," + head + "\"state\":\"failed\",\"problem\":\"no_space\",\"message\":\"507\"}")));
        assertEquals(context.getString(R.string.files_cancelled), FileTransferActivity.describe(context,
                row("{\"id\":1," + head + "\"state\":\"cancelled\"}")));
    }

    @Test
    public void aDownloadWithoutALengthCountsBytesAndALandedOneIsSaved() {
        String head = "\"direction\":\"download\",\"name\":\"b\",\"local_name\":\"b\",\"size\":0,";
        String unknown = FileTransferActivity.describe(context,
                row("{\"id\":2," + head + "\"state\":\"running\",\"done\":4096,\"total\":0}"));
        assertTrue(unknown, unknown.startsWith(context.getString(R.string.files_receiving_unknown, "").trim()));
        assertEquals(context.getString(R.string.files_saved), FileTransferActivity.describe(context,
                row("{\"id\":2," + head + "\"state\":\"done\",\"saved_name\":\"b\"}")));
    }

    @Test
    public void everyStringHasItsGermanText() {
        List<String> missing = new ArrayList<>();
        Context german = context.createConfigurationContext(configuration("de"));
        for (int id : new int[] {R.string.files_menu, R.string.files_title, R.string.files_share_label,
                R.string.files_open, R.string.files_stream_leaves, R.string.files_no_upload_permission}) {
            if (german.getString(id).equals(context.getString(id))) missing.add(context.getResources().getResourceEntryName(id));
        }
        assertTrue(missing.toString(), missing.isEmpty());
    }

    private android.content.res.Configuration configuration(String language) {
        android.content.res.Configuration configuration =
                new android.content.res.Configuration(context.getResources().getConfiguration());
        configuration.setLocale(new java.util.Locale(language));
        return configuration;
    }

    @Test
    public void staleTransferGrantsAreReleasedWhileUnrelatedPersistableGrantsStayIntact() {
        Uri transferUri = Uri.parse("content://com.android.externalstorage.documents/document/primary%3Asend.bin");
        Uri unrelatedTreeUri = Uri.parse("content://com.android.externalstorage.documents/tree/primary%3AROMs");
        android.content.ContentResolver resolver = context.getContentResolver();
        resolver.takePersistableUriPermission(unrelatedTreeUri,
                android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION | android.content.Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        assertTrue(FileShareIntents.takePersistable(context, resolver, transferUri,
                android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION));

        FileShareIntents.releaseStaleGrants(context, null);

        java.util.List<android.content.UriPermission> kept = resolver.getPersistedUriPermissions();
        boolean sawUnrelated = false;
        boolean sawTransfer = false;
        for (android.content.UriPermission perm : kept) {
            if (unrelatedTreeUri.equals(perm.getUri())) sawUnrelated = true;
            if (transferUri.equals(perm.getUri())) sawTransfer = true;
        }
        assertTrue("unrelated persistable grant (e.g. FrontendSync ROM tree) must remain intact", sawUnrelated);
        assertFalse("tracked file transfer grant must be released", sawTransfer);
        assertTrue(context.getSharedPreferences("file_transfer_grants", Context.MODE_PRIVATE).getAll().isEmpty());
    }

    @Test
    public void discardSkipsDeletionWhenPreExistingDocumentFlagIsFalse() throws Exception {
        java.io.File preExisting = new java.io.File(context.getFilesDir(), "existing_target.bin");
        try (java.io.FileOutputStream out = new java.io.FileOutputStream(preExisting)) {
            out.write(new byte[] {1, 2, 3, 4});
        }
        Uri fileUri = Uri.fromFile(preExisting);
        assertTrue(FileShareIntents.documentHasExistingContent(context.getContentResolver(), fileUri));

        FileTransferActivity.TransferWork work = new FileTransferActivity.TransferWork(
                (android.app.Application) context.getApplicationContext());
        try {
            work.discard(fileUri, "existing_target.bin", false).run();
            assertTrue("pre-existing document must not be deleted on failed download", preExisting.exists());
        } finally {
            work.end();
            preExisting.delete();
        }
    }
}
