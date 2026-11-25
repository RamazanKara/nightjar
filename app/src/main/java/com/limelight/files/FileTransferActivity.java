package com.limelight.files;

import android.app.Application;
import android.content.ActivityNotFoundException;
import android.content.ComponentName;
import android.content.ContentResolver;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.net.Uri;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.provider.DocumentsContract;
import android.view.View;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.Toast;

import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import androidx.lifecycle.ViewModel;
import androidx.lifecycle.ViewModelProvider;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.binding.PlatformBinding;
import com.limelight.computers.ComputerManagerListener;
import com.limelight.computers.ComputerManagerService;
import com.limelight.nativeapi.HostEndpoint;
import com.limelight.nativeapi.JniNativeApiTransport;
import com.limelight.nativeapi.NativeApiClient;
import com.limelight.nativeapi.NativeApiException;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.HostCapabilities;
import com.limelight.nvstream.jni.Json;
import com.limelight.ui.NightjarActivity;
import com.limelight.ui.TopAppBar;
import com.limelight.utils.ProblemReport;
import com.limelight.utils.UiHelper;

import java.io.FileNotFoundException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * File transfer (Experimental) with one PC: send files (from a picker or another app's share sheet), see the
 * PC's file transfer folder and download from it, with progress, cancel and try again. Opened from the PC
 * menu and the stream menu with {@link #EXTRA_PC_UUID}; a share always asks which PC first, and sends only
 * another app's files.
 *
 * <p>Transfers run while the screen exists, also in the background, and through a configuration change (dark
 * theme, font or display size, language: {@link TransferWork}). Leaving it keeps unfinished ones going while the
 * app runs ({@link RunningTransfers}), one PC at a time, and so does one that fails until the page shows it; with
 * nothing unfinished or failed unseen the work ends with the screen.
 */
public class FileTransferActivity extends NightjarActivity implements FileTransferController.Ui {
    public static final String EXTRA_PC_UUID = "files_pc_uuid";
    /**
     * The not exported alias of this screen that Report a problem opens with its report: only a share through
     * it may send one of Nightjar's own files, the report (see {@link #sendable(Uri, String, String, Uri)}).
     */
    public static final String REPORT_ALIAS = "com.limelight.files.SendReportToPc";

    private final Handler handler = new Handler(Looper.getMainLooper());

    TransferWork work;
    private ComputerManagerService.ComputerManagerBinder manager;
    private boolean bound;
    private boolean foreground;
    private boolean polling;
    private FileProblems.Access uploadAccess = FileProblems.Access.CHECKING;
    private FileProblems.Access downloadAccess = FileProblems.Access.CHECKING;
    private int generation;

    private FileTransferViews views;
    private Button refresh;
    private LinearLayout files;
    /** A PC was just chosen: the first of its buttons to show takes the focus (see {@link #render}). */
    private boolean focusAfterPick;

    private final ActivityResultLauncher<String[]> picker = registerForActivityResult(
            new ActivityResultContracts.OpenMultipleDocuments(), this::onPicked);
    private final ActivityResultLauncher<String> creator = registerForActivityResult(
            new ActivityResultContracts.CreateDocument("application/octet-stream"), this::onCreated);

    private final ServiceConnection service = new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name, IBinder binder) {
            // No binder (as under Robolectric) leaves nothing to wait for.
            if (!(binder instanceof ComputerManagerService.ComputerManagerBinder)) return;
            ComputerManagerService.ComputerManagerBinder candidate = (ComputerManagerService.ComputerManagerBinder) binder;
            work.workers.execute(() -> {
                if (!candidate.waitForReady()) return;
                handler.post(() -> {
                    if (isFinishing() || isDestroyed()) return;
                    manager = candidate;
                    if (work.uuid != null) {
                        ComputerDetails known = manager.getComputer(work.uuid);
                        if (known != null) onComputer(new ComputerDetails(known));
                    }
                    startPolling();
                    render();
                });
            });
        }

        @Override public void onServiceDisconnected(ComponentName name) {
            manager = null;
            polling = false;
        }
    };

    @Override
    protected void onCreate(Bundle state) {
        UiHelper.setLocale(this);
        super.onCreate(state);
        FrameLayout content = TopAppBar.setUpPage(this, R.string.files_title, this::finish);
        views = new FileTransferViews(this);
        content.addView(views.buildPage(
                this::launchPicker,
                this::loadAccess,
                () -> {
                    work.leftovers.clear();
                    if (work.controller != null) work.controller.clearFinished();
                    render();
                }));
        refresh = views.refresh;
        files = views.files;

        Keeper keeper = new ViewModelProvider(this).get(Keeper.class);
        if (keeper.work == null) {
            String pc = chosenPc(getIntent());
            TransferWork held = RunningTransfers.held();
            if (held != null && pc != null && pc.equalsIgnoreCase(held.uuid) && sharedUris(getIntent()).isEmpty()) {
                // Back to the PC whose transfers went on: the page shows them again.
                keeper.work = RunningTransfers.take();
            } else if (held != null && held.running()) {
                if (pc != null) {
                    Toast.makeText(this, R.string.files_busy_other_pc, Toast.LENGTH_LONG).show();
                    finish();
                    return;
                }
                // A share names no PC: the held transfers go on, and the PC picked decides (joinHeld).
            } else if (held != null && pc == null && held.kept()) {
                // Nor does a share end failed transfers the page has not shown: picked for their PC, it shows them.
            } else if (held != null) {
                held.end();
            }
            if (keeper.work == null) {
                FileShareIntents.releaseStaleGrants(this, null);
                keeper.work = new TransferWork(getApplication());
            }
        }
        work = keeper.work;
        // Recreated after a configuration change, the screen goes on with what it had: a share is taken once.
        if (!work.opened) {
            work.opened = true;
            work.shareConsumed = state != null && state.getBoolean("files_share_consumed", false);
            work.uuid = chosenPc(getIntent());
            int refused = 0;
            Uri report = forReport(getIntent()) ? ProblemReport.reportUri() : null;
            if (!work.shareConsumed) {
                for (Uri uri : sharedUris(getIntent())) {
                    if (sendable(uri, getPackageName(), FileShareIntents.providerPackage(this, uri), report)) {
                        work.waiting.add(uri);
                    } else {
                        refused++;
                    }
                }
            }
            if (refused > 0) {
                LimeLog.warning("File transfer: refused " + refused + " shared items that are not another app's files");
                Toast.makeText(this, getResources().getQuantityString(R.plurals.files_share_refused, refused, refused),
                        Toast.LENGTH_LONG).show();
            }
            work.engine = FileTransferEngine.jni();
        }
        work.screen = this;
        if (work.engine != null) {
            bound = bindService(new Intent(this, ComputerManagerService.class), service, BIND_AUTO_CREATE);
        }
        if (work.controller != null) loadAccess();
        render();
        UiHelper.notifyNewRootView(this);
    }

    /** Opens the screen for one PC, from the PC menu or the stream menu. */
    public static Intent forPc(Context context, String pcUuid) {
        return new Intent(context, FileTransferActivity.class).putExtra(EXTRA_PC_UUID, pcUuid);
    }

    static String chosenPc(Intent intent) {
        return FileShareIntents.chosenPc(intent);
    }

    static boolean sendable(Uri uri, String ownPackage, String providerPackage) {
        return FileShareIntents.sendable(uri, ownPackage, providerPackage);
    }

    static boolean sendable(Uri uri, String ownPackage, String providerPackage, Uri report) {
        return FileShareIntents.sendable(uri, ownPackage, providerPackage, report);
    }

    static boolean forReport(Intent intent) {
        return FileShareIntents.forReport(intent);
    }

    static List<Uri> sharedUris(Intent intent) {
        return FileShareIntents.sharedUris(intent);
    }

    private void startPolling() {
        if (manager == null || !foreground || polling) return;
        polling = true;
        manager.startPolling(new ComputerManagerListener() {
            @Override public void notifyComputerUpdated(ComputerDetails details) {
                ComputerDetails copy = new ComputerDetails(details);
                handler.post(() -> onComputer(copy));
            }
        });
    }

    private void stopPolling() {
        if (manager != null && polling) manager.stopPolling();
        polling = false;
    }

    private void onComputer(ComputerDetails details) {
        if (isFinishing() || isDestroyed() || details.uuid == null) return;
        work.computers.put(details.uuid, details);
        if (work.uuid != null && work.uuid.equalsIgnoreCase(details.uuid)) {
            ComputerDetails previous = work.computer;
            boolean first = previous == null;
            work.computer = details;
            // The first time the PC is reachable (or its endpoint changes while idle): refresh its controller.
            if (details.state == ComputerDetails.State.ONLINE) {
                HostEndpoint endpoint = manager == null ? null : HostEndpoint.of(details, manager.getUniqueId(),
                        PlatformBinding.getCryptoProvider(getApplicationContext()));
                if (endpoint != null && (work.controller == null
                        || (!work.running() && !work.controller.host().sameAddress(endpoint)))) {
                    if (work.controller != null) work.controller.close();
                    work.controller = new FileTransferController(work.engine, endpoint, work.workers,
                            work.handler::post, work);
                    loadAccess();
                } else if (first) {
                    render();
                }
            } else if (first) {
                render();
            }
        }
        if (work.uuid == null) render();
    }

    /** Asks the PC what this device may do, then reads its folder. */
    private void loadAccess() {
        FileTransferController current = work.controller;
        if (current == null || work.computer == null) {
            render();
            return;
        }
        int asked = ++generation;
        uploadAccess = FileProblems.Access.CHECKING;
        downloadAccess = FileProblems.Access.CHECKING;
        views.status.setText(R.string.files_checking);
        views.status.setVisibility(View.VISIBLE);
        HostCapabilities known = work.computer.caps;
        work.workers.execute(() -> {
            String failure = null;
            HostCapabilities caps = known;
            try {
                caps = known.withNativeCapabilities(
                        new NativeApiClient(new JniNativeApiTransport(), current.host()).fileCapabilities());
            } catch (NativeApiException | Json.SyntaxException | org.json.JSONException e) {
                failure = e.getMessage();
            }
            RemoteFiles list = null;
            if (failure == null && (FileProblems.access(caps, true) == FileProblems.Access.READY
                    || FileProblems.access(caps, false) == FileProblems.Access.READY)) {
                list = RemoteFiles.parse(work.engine.list(current.host()));
            }
            HostCapabilities read = caps;
            String problem = failure;
            RemoteFiles folderList = list;
            handler.post(() -> {
                if (asked != generation || isFinishing() || isDestroyed()) return;
                if (problem != null) {
                    uploadAccess = FileProblems.Access.CHECKING;
                    downloadAccess = FileProblems.Access.CHECKING;
                    render();
                    views.status.setText(getString(R.string.files_check_failed, problem));
                    return;
                }
                uploadAccess = FileProblems.access(read, true);
                downloadAccess = FileProblems.access(read, false);
                render();
                showFiles(folderList);
                sendWaiting();
            });
        });
    }

    private void refreshFiles() {
        FileTransferController current = work.controller;
        if (current == null) return;
        int asked = generation;
        work.workers.execute(() -> {
            RemoteFiles list = RemoteFiles.parse(work.engine.list(current.host()));
            handler.post(() -> {
                if (asked == generation && !isFinishing() && !isDestroyed()) showFiles(list);
            });
        });
    }

    private void sendWaiting() {
        if (uploadAccess != FileProblems.Access.READY || work.controller == null) return;
        List<Uri> now = new ArrayList<>(work.waiting);
        work.waiting.clear();
        if (!now.isEmpty()) {
            work.shareConsumed = true;
            queueUploads(now);
        }
        render();
    }

    private void onPicked(List<Uri> uris) {
        if (uris == null || uris.isEmpty()) return;
        for (Uri uri : uris) work.persistPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
        work.waiting.addAll(uris);
        sendWaiting();
    }

    private void launchPicker() {
        try {
            picker.launch(new String[] {"*/*"});
        } catch (ActivityNotFoundException e) {
            Toast.makeText(this, R.string.folders_no_picker, Toast.LENGTH_LONG).show();
        }
    }

    private void launchCreator(String name) {
        try {
            creator.launch(name);
        } catch (ActivityNotFoundException e) {
            work.pendingDownload = null;
            work.pendingRetry = -1;
            Toast.makeText(this, R.string.folders_no_picker, Toast.LENGTH_LONG).show();
        }
    }

    /**
     * Reads each file's name and size on a worker, since a cloud provider may fetch the file first, then queues the
     * uploads in their order on the main thread. The uploads' openers outlive this screen in the ViewModel: nothing
     * here holds the Activity.
     */
    private void queueUploads(List<Uri> uris) {
        TransferWork owner = work;
        FileTransferController controller = owner.controller;
        ContentResolver resolver = getApplicationContext().getContentResolver();
        owner.pendingDescribes++;
        try {
            owner.workers.execute(() -> {
                List<SharedFile> queued = new ArrayList<>();
                List<android.os.ParcelFileDescriptor> preopened = new ArrayList<>();
                try {
                    for (Uri uri : uris) {
                        SharedFile info = describe(resolver, uri);
                        queued.add(info);
                        if (!owner.hasPersistedPermission(uri)) {
                            owner.persistPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                        }
                        // Non-persistable URIs (e.g. external shares) lose Activity-scoped grants on finish();
                        // capture a descriptor while the Activity is still alive so background uploads succeed.
                        preopened.add(info.size >= 0 && !owner.hasPersistedPermission(uri)
                                ? FileShareIntents.tryOpenRead(resolver, uri) : null);
                    }
                } finally {
                    owner.handler.post(() -> {
                        try {
                            for (int i = 0; i < queued.size(); i++) {
                                SharedFile file = queued.get(i);
                                android.os.ParcelFileDescriptor pfd = preopened.get(i);
                                if (owner.controller != controller) {
                                    closeQuietly(pfd);
                                    // Another PC's transfers meanwhile: the file waits for one that may receive it.
                                    owner.waiting.add(file.uri);
                                } else if (file.size < 0) {
                                    closeQuietly(pfd);
                                    // The host needs the length before the first byte.
                                    FileTransferActivity screen = owner.screen;
                                    if (screen != null) {
                                        screen.views.status.setText(screen.getString(R.string.files_size_unknown, file.name));
                                        screen.views.status.setVisibility(View.VISIBLE);
                                    }
                                } else {
                                    controller.send(file.name, file.size,
                                            FileShareIntents.uploadOpener(resolver, file.uri, pfd));
                                }
                            }
                        } finally {
                            owner.pendingDescribes = Math.max(0, owner.pendingDescribes - 1);
                            if (owner.screen == null && !owner.kept()) owner.end();
                        }
                    });
                }
            });
        } catch (java.util.concurrent.RejectedExecutionException e) {
            owner.pendingDescribes = Math.max(0, owner.pendingDescribes - 1);
        }
    }

    private static void closeQuietly(android.os.ParcelFileDescriptor pfd) {
        if (pfd != null) {
            try { pfd.close(); } catch (java.io.IOException ignored) { }
        }
    }

    /** A shared file's name and size, as its provider gives them; size is -1 when it gives none. */
    static final class SharedFile {
        final Uri uri;
        final String name;
        final long size;

        SharedFile(Uri uri, String name, long size) {
            this.uri = uri;
            this.name = name;
            this.size = size;
        }
    }

    static SharedFile describe(ContentResolver resolver, Uri uri) {
        return FileShareIntents.describe(resolver, uri);
    }

    private void download(RemoteFiles.Entry entry) {
        work.pendingDownload = entry;
        work.pendingRetry = -1;
        launchCreator(entry.name);
    }

    /** The save dialog's document for the download it was opened for; package-private for tests. */
    void onCreated(Uri uri) {
        RemoteFiles.Entry entry = work.pendingDownload;
        long retry = work.pendingRetry;
        work.pendingDownload = null;
        work.pendingRetry = -1;
        if (uri == null) return;
        String name = FileShareIntents.displayName(getContentResolver(), uri,
                entry != null ? entry.name : String.valueOf(uri.getLastPathSegment()));
        ContentResolver resolver = getApplicationContext().getContentResolver();
        boolean existedBefore = FileShareIntents.documentHasExistingContent(resolver, uri);
        Runnable discard = work.discard(uri, name, !existedBefore);
        FileTransferController current = work.controller;
        if (current == null || (entry == null && retry < 0)) {
            // Android closed Nightjar while the save dialog was open: the download it was for is gone, and the
            // document the dialog made would stay behind empty unless it held pre-existing content.
            Toast.makeText(this, R.string.files_save_lost, Toast.LENGTH_LONG).show();
            work.workers.execute(discard);
            return;
        }
        if (!work.persistPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION)) {
            work.persistPermission(uri, Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
        }
        FileTransferController.Opener opener = () -> FileShareIntents.detach(resolver.openFileDescriptor(uri, "rwt"));
        if (retry >= 0) {
            current.retryDownload(retry, opener, discard);
        } else {
            current.download(entry.name, name, entry.size, opener, discard);
        }
    }

    @Override
    public void onTransfersChanged(List<FileRow> rows) {
        if (isFinishing() || isDestroyed()) return;
        showTransfers(rows);
    }

    @Override
    public void onUploaded() {
        refreshFiles();
    }

    private void render() {
        if (views == null || views.status == null) return;
        boolean picking = work.uuid == null;
        if (!picking) views.clearPcs();
        int note;
        if (work.engine == null) {
            note = R.string.files_no_library;
        } else if (picking) {
            note = views.showPcs(work.computers, this::pick) ? R.string.files_pick_pc : R.string.files_no_pc;
        } else if (work.computer == null || work.controller == null) {
            note = work.computer == null ? R.string.files_checking : R.string.files_offline;
        } else if (uploadAccess != FileProblems.Access.READY) {
            note = FileProblems.accessText(uploadAccess, true);
        } else {
            note = FileProblems.accessText(downloadAccess, false);
        }
        views.status.setText(note == 0 ? "" : getString(note));
        views.status.setVisibility(note == 0 && work.waiting.isEmpty() ? View.GONE : View.VISIBLE);
        if (!work.waiting.isEmpty() && !picking) {
            views.status.append((note == 0 ? "" : "\n\n") + getString(R.string.files_waiting_to_send, work.waiting.size()));
        }
        boolean open = work.controller != null;
        views.send.setVisibility(open ? View.VISIBLE : View.GONE);
        views.send.setEnabled(uploadAccess == FileProblems.Access.READY);
        views.refresh.setVisibility(open ? View.VISIBLE : View.GONE);
        boolean listed = open && (uploadAccess == FileProblems.Access.READY || downloadAccess == FileProblems.Access.READY);
        views.filesHeading.setVisibility(listed ? View.VISIBLE : View.GONE);
        views.folder.setVisibility(listed ? View.VISIBLE : View.GONE);
        views.files.setVisibility(listed ? View.VISIBLE : View.GONE);
        showTransfers(open ? work.controller.rows() : Collections.emptyList());
        // The chosen PC's button is gone: Send files, or Check again while sending is not allowed, takes the
        // focus once shown, instead of the Up button.
        if (focusAfterPick && open) {
            focusAfterPick = !(FileTransferViews.takesFocus(views.send) || FileTransferViews.takesFocus(views.refresh));
        }
    }

    /** A share's PC was picked; package-private for tests. */
    void pick(String uuid, ComputerDetails details) {
        if (joinHeld(uuid)) return;
        ComputerDetails latest = work.computers.get(uuid);
        work.uuid = uuid;
        work.computer = null;
        focusAfterPick = true;
        onComputer(latest != null ? latest : details);
        render();
    }

    /**
     * A share picked for a PC while a closed page's transfers are held: for their PC, the page takes them back and
     * the shared files wait to be sent with them, as a share to that PC's page; for another PC, the share is refused
     * while they run. Held work with nothing running ends, as opening another page ends it. True when handled here.
     */
    private boolean joinHeld(String uuid) {
        TransferWork held = RunningTransfers.held();
        if (held == null || held == work) return false;
        if (!uuid.equalsIgnoreCase(held.uuid)) {
            if (!held.running()) {
                held.end();
                return false;
            }
            Toast.makeText(this, R.string.files_busy_other_pc, Toast.LENGTH_LONG).show();
            finish();
            return true;
        }
        TransferWork share = work;
        RunningTransfers.take();
        held.waiting.addAll(share.waiting);
        share.waiting.clear();
        share.screen = null;
        share.end();
        new ViewModelProvider(this).get(Keeper.class).work = held;
        work = held;
        held.screen = this;
        focusAfterPick = true;
        // Asks the PC again, then sends what waits (sendWaiting).
        loadAccess();
        return true;
    }

    private void showTransfers(List<FileRow> rows) {
        views.showTransfers(work, rows, id -> work.controller.cancel(id), row -> {
            if (work.controller.needsNewDocument(row.id)) {
                work.pendingDownload = null;
                work.pendingRetry = row.id;
                launchCreator(row.localName);
            } else {
                work.controller.retry(row.id);
            }
        });
    }

    static String rowKey(FileRow row) {
        return FileTransferViews.rowKey(row);
    }

    /** Where the focus was in a group about to be made again: the focused view's tag and its row's index. */
    static final class Focus {
        final Object tag;
        final int index;

        Focus(Object tag, int index) {
            this.tag = tag;
            this.index = index;
        }
    }

    static Object otherButton(Focus focus) {
        return FileTransferViews.otherButton(focus);
    }

    /** A download's document that could not be deleted is named under the transfers. */
    void onLeftoversChanged() {
        if (isFinishing() || isDestroyed()) return;
        render();
    }

    static String describe(Context context, FileRow row) {
        return FileTransferViews.describe(context, row);
    }

    private void showFiles(RemoteFiles list) {
        views.showFiles(list, downloadAccess, this::download);
    }

    @Override
    protected void onResume() {
        super.onResume();
        foreground = true;
        startPolling();
    }

    @Override
    protected void onPause() {
        foreground = false;
        stopPolling();
        super.onPause();
    }

    @Override protected void onSaveInstanceState(Bundle outState) { super.onSaveInstanceState(outState); outState.putBoolean("files_share_consumed", work != null && work.shareConsumed); }
    @Override
    protected void onDestroy() {
        generation++;
        // The transfers stay with the work: it closes them once the screen is gone for good and nothing runs.
        if (work != null && work.screen == this) work.screen = null;
        stopPolling();
        if (bound) unbindService(service);
        super.onDestroy();
    }

    /**
     * The screen's transfers and what they are for, kept across a configuration change, which recreates the
     * screen: cancelling there would delete the documents of unfinished downloads, and a share taken again would
     * be sent twice. Read and written on the main thread. When the screen goes for good, unfinished transfers go
     * on in {@link RunningTransfers} until they finish or are cancelled on the page opened again; otherwise the
     * work ends.
     */
    public static final class TransferWork implements FileTransferController.Ui {
        private final Application application;
        final Handler handler = new Handler(Looper.getMainLooper());
        final ExecutorService workers = Executors.newCachedThreadPool(r -> new Thread(r, "File transfer"));
        final Map<String, ComputerDetails> computers = new LinkedHashMap<>();
        /** Shared with this screen and waiting for a PC that may receive them. */
        final List<Uri> waiting = new ArrayList<>();
        /** Unfinished downloads whose documents could not be deleted, by name, until Clear finished. */
        final List<String> leftovers = new ArrayList<>();
        /** The failed transfers the page last showed ({@link #seen}). */
        final Set<Long> seenFailed = new HashSet<>();
        /** Persistable SAF grants held for background transfers, released when this work ends. */
        final Map<Uri, Integer> persistedPermissions = java.util.Collections.synchronizedMap(new LinkedHashMap<>());
        /** In-flight upload describe batches that have not queued their controller jobs yet. */
        int pendingDescribes;
        /** The intent was read: its share is in {@link #waiting} or sent. */
        boolean opened;
        boolean shareConsumed;
        String uuid;
        ComputerDetails computer;
        FileTransferEngine engine;
        FileTransferController controller;
        RemoteFiles.Entry pendingDownload;
        long pendingRetry = -1;
        /** The screen showing the work; null between the old screen's end and the new one's start. */
        FileTransferActivity screen;

        public TransferWork(Application application) { this.application = application; }

        Application getApplication() { return application; }

        @Override public void onTransfersChanged(List<FileRow> rows) {
            if (screen != null) {
                screen.onTransfersChanged(rows);
            } else if (RunningTransfers.held() == this) {
                RunningTransfers.changed();
                // The last one finished with no page open and no failure to show: nothing is left to keep.
                if (!kept()) handler.post(() -> { if (screen == null && !kept()) end(); });
            }
        }

        /** Whether a transfer has not finished yet. */
        boolean running() {
            return controller != null && controller.isActive();
        }

        /** The page shows {@code rows}: the failed ones among them count as seen. */
        void seen(List<FileRow> rows) {
            seenFailed.clear();
            for (FileRow row : rows) {
                if (row.state == FileRow.State.FAILED) seenFailed.add(row.id);
            }
        }

        /** Transfers that failed since the page last showed them. */
        int unseenFailures() {
            if (controller == null) return 0;
            int count = 0;
            for (FileRow row : controller.rows()) {
                if (row.state == FileRow.State.FAILED && !seenFailed.contains(row.id)) count++;
            }
            return count;
        }

        boolean persistPermission(Uri uri, int flags) {
            if (FileShareIntents.takePersistable(application, application.getContentResolver(), uri, flags)) {
                persistedPermissions.put(uri, flags);
                return true;
            }
            return false;
        }

        boolean hasPersistedPermission(Uri uri) {
            return persistedPermissions.containsKey(uri);
        }

        /**
         * Whether the work outlives its page: a transfer runs, an upload batch is still being described, or one failed
         * that the page has not shown, which stays until that PC's page is opened again.
         */
        boolean kept() {
            return running() || unseenFailures() > 0 || pendingDescribes > 0;
        }

        /**
         * The screen went for good: unfinished transfers, and failed ones not shown yet, stay in
         * {@link RunningTransfers}; otherwise all ends.
         */
        void screenGone() {
            screen = null;
            if (kept()) RunningTransfers.hold(this);
            else end();
        }

        /** Cancels what has not finished and lets the workers end. */
        void end() {
            RunningTransfers.release(this);
            if (controller != null) controller.close();
            synchronized (persistedPermissions) {
                ContentResolver resolver = application.getContentResolver();
                for (Map.Entry<Uri, Integer> entry : persistedPermissions.entrySet()) {
                    FileShareIntents.releasePersistable(application, resolver, entry.getKey(), entry.getValue());
                }
                persistedPermissions.clear();
            }
            workers.shutdown();
        }

        @Override public void onUploaded() {
            if (screen != null) screen.onUploaded();
        }

        /** Deletes a newly created download's document on a worker unless it overwrote a pre-existing document. */
        Runnable discard(Uri uri, String name, boolean deleteOnFailure) {
            ContentResolver resolver = getApplication().getContentResolver();
            return () -> {
                if (!deleteOnFailure) return;
                if (deleteDocument(resolver, uri)) return;
                handler.post(() -> {
                    leftovers.add(name);
                    if (screen != null) screen.onLeftoversChanged();
                });
            };
        }

        /** Deletes a download's document on a worker; one that stays is named on the screen. */
        Runnable discard(Uri uri, String name) {
            return discard(uri, name, true);
        }

        static boolean deleteDocument(ContentResolver resolver, Uri uri) {
            try {
                if (DocumentsContract.deleteDocument(resolver, uri)) return true;
                LimeLog.warning("File transfer: an unfinished download was not deleted: " + uri);
            } catch (FileNotFoundException | RuntimeException e) {
                LimeLog.warning("File transfer: could not delete an unfinished download: " + e.getMessage());
            }
            return false;
        }
    }

    /** Holds the screen's {@link TransferWork} through a configuration change; cleared when the screen goes for good. */
    public static final class Keeper extends ViewModel {
        TransferWork work;

        @Override protected void onCleared() {
            if (work != null) work.screenGone();
        }
    }
}
