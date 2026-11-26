package com.limelight;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import com.limelight.binding.crypto.AndroidCryptoProvider;
import com.limelight.binding.video.DecoderBenchmark;
import com.limelight.computers.ComputerManagerListener;
import com.limelight.computers.ComputerManagerService;
import com.limelight.computers.HostActions;
import com.limelight.computers.HostMenu;
import com.limelight.console.HomeSettings;
import com.limelight.grid.PcGridAdapter;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.PairingManager.PairState;
import com.limelight.preferences.GlPreferences;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.ui.ActionSheet;
import com.limelight.ui.AdapterFragment;
import com.limelight.ui.AdapterFragmentCallbacks;
import com.limelight.ui.GlassForm;
import com.limelight.ui.NightjarActivity;
import com.limelight.ui.ShellBar;
import com.limelight.ui.TopAppBar;
import com.limelight.update.UpdateFlow;
import com.limelight.utils.Dialog;
import com.limelight.utils.ShortcutHelper;
import com.limelight.utils.UiHelper;

import android.app.Service;
import android.content.ComponentName;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.res.Configuration;
import android.opengl.GLSurfaceView;
import android.os.Build;
import android.os.Bundle;
import android.os.IBinder;
import android.os.SystemClock;
import android.preference.PreferenceManager;
import android.view.View;
import android.widget.AbsListView;

import androidx.annotation.VisibleForTesting;

import javax.microedition.khronos.egl.EGL10;
import javax.microedition.khronos.egl.EGLConfig;
import javax.microedition.khronos.egl.EGLDisplay;
import javax.microedition.khronos.opengles.GL10;

public class PcView extends NightjarActivity implements AdapterFragmentCallbacks {
    private final PcViewChrome viewChrome = new PcViewChrome(this);
    private final PcViewActions viewActions = new PcViewActions(this);

    ActionSheet pcMenu;
    ActionSheet addPcSheet;
    PcGridAdapter pcGridAdapter;
    private ShortcutHelper shortcutHelper;
    // The replay when polling starts runs on the UI thread; the shortcut service is a binder call, so
    // publishing is moved off it.
    private final java.util.concurrent.ExecutorService shortcutPublisher =
            java.util.concurrent.Executors.newSingleThreadExecutor(task -> {
                Thread thread = new Thread(task, "PC shortcuts");
                thread.setDaemon(true);
                return thread;
            });
    final HostActions hostActions = new HostActions(this, new HostActions.Callbacks() {
        @Override public ComputerManagerService.ComputerManagerBinder binder() { return managerBinder; }
        @Override public boolean isForeground() { return inForeground; }
        @Override public void onPaired(ComputerDetails computer) {
            if (HomeSettings.landsOnHome(PcView.this, computer)) {
                // Home is where its games are; the PCs screen steps aside unless it is the first screen.
                ShellBar.open(PcView.this, ShellBar.Destination.PCS, ShellBar.Destination.HOME);
            } else {
                viewActions.doAppList(computer, true, false);
            }
        }
        @Override public void onRemoved(ComputerDetails computer) { removeFromList(computer); }
        @Override public void onRenamed(ComputerDetails computer) { pcGridAdapter.notifyDataSetChanged(); }
        @Override public void onWakeSent(ComputerDetails computer) {
            ComputerObject row = findComputer(computer.uuid);
            if (row != null) {
                // The row says "Waking…" until the PC answers or the wake window passes.
                row.wakeSentAt = SystemClock.elapsedRealtime();
                pcGridAdapter.notifyDataSetChanged();
            }
        }
        @Override public void onWakeEnded(ComputerDetails computer) {
            ComputerObject row = findComputer(computer.uuid);
            // An answer, a timeout or a failed send: the row no longer says "Waking…".
            if (row != null && row.wakeSentAt != 0) {
                row.wakeSentAt = 0;
                pcGridAdapter.notifyDataSetChanged();
            }
            // A form change waited for the wake. Not at once: a wake moved to another PC ends this one first, and a
            // timeout's error dialog comes right after (its dismissal gives the window focus back, which asks again).
            followFormSoon();
        }
        @Override public void pausePolling() { stopComputerUpdates(false); }
        @Override public void resumePolling() { startComputerUpdates(); }
    });
    volatile ComputerManagerService.ComputerManagerBinder managerBinder;
    private boolean managerServiceBound;
    private Thread serviceWaitThread;
    private boolean freezeUpdates, runningPolling, inForeground, completeOnCreateCalled;
    private final ServiceConnection serviceConnection = new ServiceConnection() {
        public void onServiceConnected(ComponentName className, IBinder binder) {
            // No binder (as under Robolectric) leaves nothing to wait for.
            if (!(binder instanceof ComputerManagerService.ComputerManagerBinder)) return;
            final ComputerManagerService.ComputerManagerBinder localBinder =
                    ((ComputerManagerService.ComputerManagerBinder)binder);

            // Wait in a separate thread to avoid stalling the UI
            serviceWaitThread = new Thread() {
                @Override
                public void run() {
                    // Wait for the binder to be ready
                    if (!localBinder.waitForReady() || isFinishing() || isDestroyed()) {
                        return;
                    }

                    // Now make the binder visible
                    runOnUiThread(() -> {
                        if (!managerServiceBound || isFinishing() || isDestroyed()) {
                            return;
                        }
                        managerBinder = localBinder;
                        startComputerUpdates();
                    });

                    // Force a keypair to be generated early to avoid discovery delays
                    new AndroidCryptoProvider(getApplicationContext()).getClientCertificate();
                }
            };
            serviceWaitThread.start();
        }

        public void onServiceDisconnected(ComponentName className) {
            managerBinder = null;
            runningPolling = false;
            viewChrome.setDiscovering(false);
        }
    };

    /** "When Nightjar opens": the game's page, opened once this screen is set up. */
    private Intent openPlayPage;
    // Takes the button of transfers a closed Files page left running away again (RunningTransfers.showOn).
    private Runnable transfersButton;

    /** How long a row says "Waking…" after a wake signal while the PC has not answered. */
    static final long WAKE_WINDOW_MS = 2 * 60 * 1000;

    @Override
    public void onConfigurationChanged(Configuration newConfig) {
        super.onConfigurationChanged(newConfig);
        if (recreatingForForm()) return;

        if (completeOnCreateCalled) {
            UiHelper.notifyNewRootView(this);
        }
    }

    private void initializeViews() {
        setContentView(R.layout.activity_pc_view);

        UiHelper.notifyNewRootView(this);

        // Allow floating expanded PiP overlays while browsing PCs
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            setShouldDockBigOverlays(false);
        }

        // Set default preferences if we've never been run
        PreferenceManager.setDefaultValues(this, R.xml.preferences, false);

        TopAppBar.configure(this, R.id.home_toolbar, getString(R.string.library_computers), 0);
        ShellBar.install(this, ShellBar.Destination.PCS);
        ShellBar.installBack(this, ShellBar.Destination.PCS);
        findViewById(R.id.manuallyAddPc).setOnClickListener(v -> viewActions.offerAddPc());
        viewChrome.setUpGlass();

        getFragmentManager().beginTransaction()
            .replace(R.id.pcFragmentContainer, new AdapterFragment())
            .commitAllowingStateLoss();

        viewChrome.bindViews();
        findViewById(R.id.discovery_add).setOnClickListener(v -> viewActions.offerAddPc());
        findViewById(R.id.discovery_help).setOnClickListener(v -> viewActions.showPairingGuide());
        findViewById(R.id.pc_list_help).setOnClickListener(v -> viewActions.showPairingGuide());
        viewChrome.updateEmptyState();
        pcGridAdapter.notifyDataSetChanged();
    }

    /** Its chrome follows the form: a rotation into another form lays it out again. */
    @Override
    protected boolean followsForm() {
        return true;
    }

    /** A pairing, a wake or a failure's dialog keeps this screen until it is done; it then follows the form. */
    @Override
    protected boolean holdsForm() {
        return hostActions.isPairing() || hostActions.isWaking() || hostActions.isShowingError();
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        if (savedInstanceState == null) {
            // Reclaim any orphaned SAF grants from prior crashed/killed file transfers while keeping unrelated or live held grants.
            com.limelight.files.FileShareIntents.releaseStaleGrants(this, com.limelight.files.RunningTransfers.activeGrantUris());
        }

        // Once per phone and app version: lets Automatic skip codecs that decode slower here. Before Home takes over the
        // start, which would otherwise leave it unmeasured until the PC list is opened.
        DecoderBenchmark.runIfNeeded(this);

        // Home is the launch screen once a host has shown its games; the list of PCs stays one tap away. It then looks for
        // app updates, which this screen does when it stays.
        Intent openPlay = com.limelight.console.OpenPlay.atEntry(this, savedInstanceState);
        if (savedInstanceState == null && opensHome()) {
            Intent home = new Intent(this, com.limelight.console.ConsoleHomeActivity.class);
            if (openPlay != null) {
                // The game's page over Home: Cancel, a failed start and the stream's end all land there.
                startActivities(new Intent[] {home, openPlay});
            } else {
                startActivity(home);
            }
            finish();
            return;
        }
        openPlayPage = openPlay;

        // Assume we're in the foreground when created to avoid a race
        // between binding to CMS and onResume()
        inForeground = true;

        // Create a GLSurfaceView to fetch GLRenderer unless we have
        // a cached result already.
        final GlPreferences glPrefs = GlPreferences.readPreferences(this);
        if (!glPrefs.savedFingerprint.equals(Build.FINGERPRINT) || glPrefs.glRenderer.isEmpty()) {
            GLSurfaceView surfaceView = new GLSurfaceView(this);
            surfaceView.setEGLConfigChooser(new ProbeConfigChooser());
            surfaceView.setRenderer(new GLSurfaceView.Renderer() {
                @Override
                public void onSurfaceCreated(GL10 gl10, EGLConfig eglConfig) {
                    // Save the GLRenderer string so we don't need to do this next time
                    glPrefs.glRenderer = gl10.glGetString(GL10.GL_RENDERER);
                    glPrefs.savedFingerprint = Build.FINGERPRINT;
                    glPrefs.writePreferences();

                    LimeLog.info("Fetched GL Renderer: " + glPrefs.glRenderer);

                    runOnUiThread(PcView.this::completeOnCreate);
                }

                @Override public void onSurfaceChanged(GL10 gl10, int i, int i1) {}
                @Override public void onDrawFrame(GL10 gl10) {}
            });
            setContentView(surfaceView);
        }
        else {
            LimeLog.info("Cached GL Renderer: " + glPrefs.glRenderer);
            completeOnCreate();
        }
    }

    /**
     * The renderer probe reads only GL_RENDERER. GLSurfaceView's default chooser insists on
     * RGB888 with a depth buffer and throws on devices that have none, ending the app at
     * start; any window config will do. A class of its own, so PcView's methods name no EGL
     * type. After Moonlight V+ (qiin2333/moonlight-vplus, GPL-3.0, PcView.kt, pull request 848).
     */
    private static final class ProbeConfigChooser implements GLSurfaceView.EGLConfigChooser {
        @Override
        public EGLConfig chooseConfig(EGL10 egl, EGLDisplay display) {
            int[] attributes = {
                    EGL10.EGL_RED_SIZE, 4,
                    EGL10.EGL_GREEN_SIZE, 4,
                    EGL10.EGL_BLUE_SIZE, 4,
                    EGL10.EGL_SURFACE_TYPE, EGL10.EGL_WINDOW_BIT,
                    EGL10.EGL_NONE
            };
            EGLConfig[] configs = new EGLConfig[1];
            int[] count = new int[1];
            if (!egl.eglChooseConfig(display, attributes, configs, configs.length, count)
                    || count[0] <= 0 || configs[0] == null) {
                throw new IllegalArgumentException("No EGL window config for the GL renderer probe");
            }
            return configs[0];
        }
    }

    private boolean opensHome() {
        Intent intent = getIntent();
        if (intent == null || !Intent.ACTION_MAIN.equals(intent.getAction())
                || intent.getBooleanExtra(HomeSettings.EXTRA_STAY, false)) {
            return false;
        }
        GlPreferences glPrefs = GlPreferences.readPreferences(this);
        // Before the first GL renderer is read this screen has work to do; Home waits for the next start.
        return glPrefs.savedFingerprint.equals(Build.FINGERPRINT) && !glPrefs.glRenderer.isEmpty()
                && HomeSettings.opensOnHome(this);
    }

    private void completeOnCreate() {
        if (completeOnCreateCalled || isFinishing() || isDestroyed()) {
            return;
        }
        completeOnCreateCalled = true;

        shortcutHelper = new ShortcutHelper(this);

        UiHelper.setLocale(this);

        // Bind to the computer manager service
        managerServiceBound = bindService(new Intent(PcView.this, ComputerManagerService.class), serviceConnection,
                Service.BIND_AUTO_CREATE);

        pcGridAdapter = new PcGridAdapter(this, PreferenceConfiguration.readPreferences(this));

        initializeViews();

        // The first onResume can come before the GL renderer was read and this ran; look again now.
        if (inForeground) {
            checkForUpdates();
        }

        // The discovery screen explains setup inline; Help remains available from the toolbar.

        if (openPlayPage != null) {
            startActivity(openPlayPage);
            openPlayPage = null;
        }
    }

    private void startComputerUpdates() {
        // Only allow polling to start if we're bound to CMS, polling is not already running,
        // and our activity is in the foreground.
        if (managerBinder != null && !runningPolling && inForeground) {
            freezeUpdates = false;
            // Another screen may have removed or disabled a shortcut since: the first poll publishes each again.
            shortcutHelper.forgetPublished();
            managerBinder.startPolling(new ComputerManagerListener() {
                @Override
                public void notifyComputerUpdated(final ComputerDetails details) {
                    if (!freezeUpdates) {
                        PcView.this.runOnUiThread(() -> {
                            if (inForeground && !freezeUpdates && !isFinishing() && !isDestroyed()) {
                                updateComputer(details);
                            }
                        });

                        // Add a launcher shortcut for this PC (off the main thread to prevent ANRs)
                        ComputerManagerService.ComputerManagerBinder binder = managerBinder;
                        if (binder != null && binder.isPersistent() && details.pairState == PairState.PAIRED) {
                            // startPolling replays each PC on the caller's thread, which is the UI thread here.
                            if (android.os.Looper.myLooper() == android.os.Looper.getMainLooper()) {
                                ComputerDetails snapshot = new ComputerDetails(details);
                                shortcutPublisher.execute(
                                        () -> shortcutHelper.createAppViewShortcutForOnlineHost(snapshot));
                            } else {
                                shortcutHelper.createAppViewShortcutForOnlineHost(details);
                            }
                        }
                        // Home needs a library to open on; read it once for a host that has the native API.
                        if (binder != null) {
                            com.limelight.console.ConsoleSeed.seedOnce(PcView.this, binder.getUniqueId(), details,
                                    () -> runOnUiThread(PcView.this::onLibrarySeeded));
                        }
                    }
                }
            });
            runningPolling = true;
            viewChrome.setDiscovering(true);
        }
    }

    private void stopComputerUpdates(boolean wait) {
        if (managerBinder != null) {
            if (!runningPolling) {
                return;
            }

            freezeUpdates = true;

            managerBinder.stopPolling();

            if (wait) {
                managerBinder.waitForPollingStopped();
            }

            runningPolling = false;
            viewChrome.setDiscovering(false);
        }
    }

    @Override
    public void onDestroy() {
        super.onDestroy();

        viewChrome.onDestroy();
        hostActions.destroy();
        shortcutPublisher.shutdownNow();
        if (serviceWaitThread != null) {
            serviceWaitThread.interrupt();
        }
        if (managerServiceBound) {
            unbindService(serviceConnection);
            managerServiceBound = false;
        }
        managerBinder = null;
    }

    @Override
    protected void onResume() {
        super.onResume();

        // Display a decoder crash notification if we've returned after a crash
        UiHelper.showDecoderCrashDialog(this);

        inForeground = true;
        viewChrome.onResume();
        if (completeOnCreateCalled) {
            // A host's library can arrive while this screen is away, which makes Home available.
            ShellBar.refresh(this);
        }
        // A PC removed from its settings page is gone from the manager but still in this list.
        dropRemovedComputers();
        if (!hostActions.deliverPendingResult() && !hostActions.isPairing()) {
            startComputerUpdates();
        }
        if (completeOnCreateCalled) {
            checkForUpdates();
        }
        transfersButton = com.limelight.files.RunningTransfers.showOn(this);
    }

    /** At most every 12 hours; the PC list is never in front during a stream. */
    protected void checkForUpdates() {
        UpdateFlow.autoCheck(this, () -> inForeground);
    }

    @Override
    protected void onPause() {
        super.onPause();

        inForeground = false;
        if (transfersButton != null) { transfersButton.run(); transfersButton = null; }
        if (pcMenu != null) pcMenu.dismiss();
        // Pairing keeps running: people leave to type the PIN into the host's web page on this
        // phone, or the screen turns off on the way to the PC.
        stopComputerUpdates(false);
    }

    @Override
    protected void onStop() {
        super.onStop();

        Dialog.closeDialogs(PcView.this);
    }

    @Override
    public boolean dispatchKeyEvent(android.view.KeyEvent event) {
        if (viewChrome.onKeyEvent(event)) return true;
        return super.dispatchKeyEvent(event);
    }

    @Override
    public boolean dispatchTouchEvent(android.view.MotionEvent event) {
        viewChrome.onTouchEvent(event);
        return super.dispatchTouchEvent(event);
    }

    /** The first library was just written: Home joins the shell bar, and with it becomes the root for Back. */
    void onLibrarySeeded() {
        if (isFinishing() || isDestroyed()) return;
        ShellBar.refresh(this);
    }

    @VisibleForTesting
    void placeListFooter() {
        viewChrome.placeListFooter();
    }

    private void layOutUnderChrome(int left, int top, int right, int bottom) {
        viewChrome.layOutUnderChrome(left, top, right, bottom);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        viewChrome.onWindowFocusChanged(hasFocus);
    }

    /**
     * The PC's menu, in this order: Open library, Resume and Quit, Join co-op, Shared folders, Pair, one-time PIN, Wake
     * or Sleep, Files, PC settings, Rename, Unpair or Pairing help, then Remove PC last. Touch reaches it by long press
     * or the three dots, a mouse by right-click, a D-pad or keyboard by Menu or Shift+F10, a controller by X.
     */
    void showComputerMenu(ComputerObject computer) {
        if (pcMenu != null) return;
        stopComputerUpdates(false);
        String game = computer.runningGame != null ? computer.runningGame.getAppName() : null;
        ActionSheet sheet = HostMenu.build(this, computer.details, computer.rowState(), game,
                action -> viewActions.onComputerAction(computer, action));
        pcMenu = sheet;
        sheet.setOnDismissListener(dialog -> {
            pcMenu = null;
            // Pair from the menu stopped the polling, which would race the pairing; it restarts once that ends.
            if (!hostActions.isPairing()) startComputerUpdates();
        });
        sheet.show();
    }

    /** Sends the wake signal; tests record it instead of broadcasting a magic packet. */
    protected void doWakeOnLan(final ComputerDetails computer) {
        hostActions.wake(computer);
    }

    private ComputerObject findComputer(String uuid) {
        for (int i = 0; i < pcGridAdapter.getCount(); i++) {
            ComputerObject computer = (ComputerObject) pcGridAdapter.getItem(i);
            if (computer.details.uuid.equals(uuid)) {
                return computer;
            }
        }
        return null;
    }

    HostActions hostActions() {
        return hostActions;
    }

    /** Takes the rows of PCs the manager no longer knows off the list. */
    void dropRemovedComputers() {
        ComputerManagerService.ComputerManagerBinder binder = managerBinder;
        if (binder == null) {
            return;
        }
        List<ComputerDetails> gone = new ArrayList<>();
        for (int i = 0; i < pcGridAdapter.getCount(); i++) {
            ComputerDetails details = ((ComputerObject) pcGridAdapter.getItem(i)).details;
            if (binder.getComputer(details.uuid) == null) {
                gone.add(details);
            }
        }
        for (ComputerDetails details : gone) {
            removeFromList(details);
        }
    }

    /** The manager has removed the PC; take its row, its shortcuts and its alias off this screen. */
    private void removeFromList(ComputerDetails details) {
        for (int i = 0; i < pcGridAdapter.getCount(); i++) {
            ComputerObject computer = (ComputerObject) pcGridAdapter.getItem(i);

            if (details.uuid.equals(computer.details.uuid)) {
                // Disable or delete shortcuts referencing this PC
                shortcutHelper.disableComputerShortcut(details,
                        getResources().getString(R.string.scut_deleted_pc));

                pcGridAdapter.removeComputer(computer);
                pcGridAdapter.notifyDataSetChanged();

                if (pcGridAdapter.getCount() == 0) {
                    // Show the "Discovery in progress" view
                    viewChrome.updateEmptyState();
                }

                break;
            }
        }
    }

    private void updateComputer(ComputerDetails details) {
        ComputerObject existingEntry = null;

        for (int i = 0; i < pcGridAdapter.getCount(); i++) {
            ComputerObject computer = (ComputerObject) pcGridAdapter.getItem(i);

            // Check if this is the same computer
            if (details.uuid.equals(computer.details.uuid)) {
                existingEntry = computer;
                break;
            }
        }

        boolean added = existingEntry == null;
        if (existingEntry != null) {
            // Replace the information in the existing entry
            existingEntry.details = details;
        }
        else {
            // Add a new entry
            existingEntry = new ComputerObject(details);
            pcGridAdapter.addComputer(existingEntry);
        }
        if (details.state == ComputerDetails.State.ONLINE) {
            existingEntry.wakeSentAt = 0;
            com.limelight.update.HostUpdateChecker.check(this, details.caps, pcGridAdapter::notifyDataSetChanged);
        }
        viewActions.findRunningGame(existingEntry);
        // The first saved app list of a PC without the native API makes Home available.
        View home = details.rawAppList != null ? findViewById(R.id.shell_home) : null;
        if (home != null && home.getVisibility() != View.VISIBLE) ShellBar.refresh(this);

        // Notify the view that the data has changed. Every poll reports its PC, changed or not, and a refresh sorts
        // and binds every row, so only news asks for one. A running game's cover that was not on disk is looked for
        // again by a later bind, so the grid keeps refreshing while one is missing.
        if (existingEntry.changedSinceDrawn() || pcGridAdapter.awaitsCover()) {
            pcGridAdapter.notifyDataSetChanged();
        }
        if (added) {
            // Remove the "Discovery in progress" view only now that the grid has been told of the new PC (a new PC is always
            // news): a grid given an empty adapter takes no focus until the adapter reports a change, and the first PC's
            // tile shows its ring only if the grid holds focus when it is laid out.
            viewChrome.updateEmptyState();
        }
    }

    @Override
    public int getAdapterFragmentLayoutId() {
        return GlassForm.of(this) == GlassForm.PHONE ? R.layout.pc_grid_view : R.layout.pc_grid_cards;
    }

    @Override
    public void receiveAbsListView(AbsListView listView) {
        viewChrome.bindListView(listView, viewActions);
    }

    public static class ComputerObject {
        public ComputerDetails details;
        /** The running game from the PC's app list, once it has been found there. */
        public NvApp runningGame;
        String runningGameKey;
        /** When this device last sent the PC a wake signal, in elapsed realtime; 0 for never. */
        public long wakeSentAt;
        /** What a poll last had the grid draw this row from: every field of the PC, and whether it was waking. */
        private Object[] drawnFields;
        private boolean drawnWaking;

        public ComputerObject(ComputerDetails details) {
            if (details == null) {
                throw new IllegalArgumentException("details must not be null");
            }
            this.details = details;
        }

        public boolean isWaking() {
            return wakeSentAt != 0 && SystemClock.elapsedRealtime() - wakeSentAt < WAKE_WINDOW_MS;
        }

        public PcGridAdapter.RowState rowState() {
            return PcGridAdapter.rowState(details, isWaking());
        }

        /**
         * Whether the row has news since a poll last had the grid draw it, which counts as drawn from now on. It
         * compares every field of the PC ({@link ComputerDetails#fields}), not only the ones the row is known to
         * read, and the waking state, which ends by the clock with no poll saying so.
         */
        boolean changedSinceDrawn() {
            Object[] fields = details.fields();
            boolean waking = isWaking();
            if (waking == drawnWaking && Arrays.deepEquals(fields, drawnFields)) {
                return false;
            }
            drawnFields = fields;
            drawnWaking = waking;
            return true;
        }

        @Override
        public String toString() {
            return details.displayName();
        }
    }
}
