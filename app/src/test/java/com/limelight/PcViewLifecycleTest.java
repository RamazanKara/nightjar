package com.limelight;

import android.app.Application;
import android.content.Intent;
import android.content.ServiceConnection;
import android.os.Build;
import android.os.Looper;
import android.widget.GridView;

import androidx.appcompat.app.AlertDialog;

import com.limelight.computers.ComputerManagerListener;
import com.limelight.computers.ComputerManagerService;
import com.limelight.nativeapi.NativeApiFixtures;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.PairingManager;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.wol.WakeTargets;
import com.limelight.preferences.GlPreferences;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import org.robolectric.shadows.ShadowDialog;
import org.robolectric.util.ReflectionHelpers;

import java.util.concurrent.CountDownLatch;

import static com.limelight.PcViewTestSupport.*;
import static org.junit.Assert.*;

/**
 * Tests {@link PcView} lifecycle, polling refreshes, Wake-on-LAN state, form-change/rotation
 * resilience, first-run layout, and hosts the shared {@link NoNativeMoonBridge} shadow and
 * {@link Library} activity subclass used across the Android test suite.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class PcViewLifecycleTest {
    @Implements(value = MoonBridge.class, isInAndroidSdk = false)
    public static class NoNativeMoonBridge {
        @Implementation protected static void __staticInitializer__() { }
    }

    public static class Library extends PcView {
        final java.util.List<ComputerDetails> woken = new java.util.ArrayList<>();

        // The update check would reach for the network.
        @Override protected void checkForUpdates() { }

        @Override public boolean bindService(Intent intent, ServiceConnection connection, int flags) {
            return false;
        }

        // Tests record the wake instead of broadcasting a magic packet.
        @Override protected void doWakeOnLan(ComputerDetails computer) {
            woken.add(computer);
        }
    }

    private final PcViewTestSupport support = new PcViewTestSupport();

    @Test
    public void theShortcutReplayWhenPollingStartsRunsOffTheUiThread() throws Exception {
        ActivityController<Library> controller = support.emptyScreen(new CountDownLatch(0));
        try {
            Library screen = controller.get();
            java.util.concurrent.atomic.AtomicReference<Thread> publishedOn =
                    new java.util.concurrent.atomic.AtomicReference<>();
            final CountDownLatch published = new CountDownLatch(1);
            ReflectionHelpers.setField(screen, "shortcutHelper",
                    new com.limelight.utils.ShortcutHelper(screen) {
                        @Override public void createAppViewShortcutForOnlineHost(ComputerDetails details) {
                            publishedOn.set(Thread.currentThread());
                            published.countDown();
                        }
                    });
            ComputerManagerService manager = new ComputerManagerService();
            ReflectionHelpers.setField(screen, "managerBinder", manager.new ComputerManagerBinder() {
                @Override public boolean isPersistent() { return true; }
                @Override public void startPolling(ComputerManagerListener listener) { support.polling[0] = listener; }
                @Override public void stopPolling() { }
                @Override public String getUniqueId() { return "client"; }
            });
            ReflectionHelpers.setField(screen, "runningPolling", false);
            ReflectionHelpers.callInstanceMethod(screen, "startComputerUpdates");

            // The service replays its PCs on the caller's thread, the UI thread here.
            ComputerDetails online = pc("shortcut-host", "PC", ComputerDetails.State.ONLINE,
                    PairingManager.PairState.PAIRED);
            online.caps = com.limelight.nvstream.http.HostCapabilities.UNKNOWN;
            support.polling[0].notifyComputerUpdated(online);

            assertTrue(published.await(5, java.util.concurrent.TimeUnit.SECONDS));
            assertNotSame("the shortcut service is asked off the UI thread",
                    Looper.getMainLooper().getThread(), publishedOn.get());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void rowsListRunningThenReadyThenUnpairedThenOfflinePcs() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            support.report(pc("b-off", "Basement", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED));
            support.report(pc("a-off", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED));
            support.report(pc("ready", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED));
            ComputerDetails running = pc("running", "Zen", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            running.runningGameUuid = "apollo-input-only";
            support.report(running);
            GridView grid = screen.findViewById(R.id.fragmentView);
            java.util.List<String> order = new java.util.ArrayList<>();
            java.util.List<String> status = new java.util.ArrayList<>();
            for (int i = 0; i < grid.getCount(); i++) {
                PcView.ComputerObject computer = (PcView.ComputerObject) grid.getItemAtPosition(i);
                order.add(computer.details.name);
                status.add(com.limelight.grid.PcGridAdapter.statusText(screen, computer));
            }
            assertEquals(java.util.List.of("Zen", "Office", "PC", "Attic", "Basement"), order);
            assertEquals(java.util.List.of(screen.getString(R.string.pc_status_playing_unknown),
                    screen.getString(R.string.pc_status_online), screen.getString(R.string.pc_status_not_paired),
                    screen.getString(R.string.pc_offline), screen.getString(R.string.pc_offline)), status);

            // The running game is named once the PC's app list has it.
            running.rawAppList = "<root status_code=\"200\"><App><AppTitle>Racing Game</AppTitle><ID>2</ID>" +
                    "<UUID>apollo-input-only</UUID></App></root>";
            running.runningGameUuid = "apollo-input-only";
            ComputerDetails update = new ComputerDetails(running);
            update.runningGameId = 2;
            support.report(update);
            for (int i = 0; i < 50 && ((PcView.ComputerObject) grid.getItemAtPosition(0)).runningGame == null; i++) {
                Shadows.shadowOf(Looper.getMainLooper()).idleFor(java.time.Duration.ofMillis(20));
                try { Thread.sleep(20); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
            }
            assertEquals(screen.getString(R.string.pc_status_playing, "Racing Game"),
                    com.limelight.grid.PcGridAdapter.statusText(screen, (PcView.ComputerObject) grid.getItemAtPosition(0)));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void tappingAnOfflinePcWithOnlyTheMacsItsWakeListedWakesIt() throws Exception {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            new WakeTargets(screen).store("asleep", NativeApiFixtures.wake("02:e0:4c:68:00:01"));
            ComputerDetails asleep = pc("asleep", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            asleep.macAddress = null;
            support.report(asleep);
            GridView grid = screen.findViewById(R.id.fragmentView);
            int row = grid.getCount() - 1;
            PcView.ComputerObject computer = (PcView.ComputerObject) grid.getItemAtPosition(row);
            assertSame(asleep, computer.details);
            assertEquals(screen.getString(R.string.pcview_menu_send_wol),
                    screen.getString(com.limelight.grid.PcGridAdapter.mainActionLabel(screen, computer)));
            grid.performItemClick(null, row, row);
            assertEquals(java.util.List.of(asleep), screen.woken);
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void tappingAnOfflinePcWithAMacWakesItAndTheRowSaysSo() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails asleep = pc("asleep", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            asleep.macAddress = "02:00:00:00:00:01";
            support.report(asleep);
            GridView grid = screen.findViewById(R.id.fragmentView);
            int row = grid.getCount() - 1;
            PcView.ComputerObject computer = (PcView.ComputerObject) grid.getItemAtPosition(row);
            assertSame(asleep, computer.details);
            assertEquals(screen.getString(R.string.pcview_menu_send_wol),
                    screen.getString(com.limelight.grid.PcGridAdapter.mainActionLabel(screen, computer)));
            grid.performItemClick(null, row, row);
            assertEquals(java.util.List.of(asleep), screen.woken);
            assertNull("Waking is the tap's action, not a menu", ShadowDialog.getLatestDialog());

            computer.wakeSentAt = android.os.SystemClock.elapsedRealtime();
            assertEquals(screen.getString(R.string.pc_status_waking),
                    com.limelight.grid.PcGridAdapter.statusText(screen, computer));
            // The PC answers, then sleeps again: it is offline, no longer waking.
            ComputerDetails awake = new ComputerDetails(asleep);
            awake.state = ComputerDetails.State.ONLINE;
            support.report(awake);
            assertEquals(screen.getString(R.string.pc_status_online),
                    com.limelight.grid.PcGridAdapter.statusText(screen, computer));
            support.report(asleep);
            assertEquals(screen.getString(R.string.pc_offline),
                    com.limelight.grid.PcGridAdapter.statusText(screen, computer));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /**
     * Waking a second PC ended the first one's wait, and the page, no longer held, was laid out again for a form change
     * that had waited: its destruction cancelled the second wake without a word. The page waits for the second wake now.
     */
    @Test
    public void wakingASecondPcKeepsThePageThroughAFormChangeAndItsWake() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails attic = support.asleep("attic", "Attic");
            ComputerDetails cellar = support.asleep("cellar", "Cellar");
            support.startWake(screen, attic, host -> { });
            assertTrue(screen.hostActions().isWaking());
            toHandheld(screen);
            assertFalse("the page waits for the wake", ReflectionHelpers.<Boolean>getField(screen, "recreatingForForm"));

            support.startWake(screen, cellar, host -> { });
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertFalse("the second wake holds the page", ReflectionHelpers.<Boolean>getField(screen, "recreatingForForm"));
            assertFalse(screen.isDestroyed());
            assertTrue("and still runs", screen.hostActions().isWaking());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /**
     * A wake whose packet could not go out ended, the page followed the waiting form at once and its Try again dialog came
     * up on a page being laid out again. The page now waits for the dialog, then follows.
     */
    @Test
    public void aWakeFailuresDialogKeepsThePageThenThePageFollows() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails attic = support.asleep("attic", "Attic");
            int[] sends = {0};
            support.wakeClock = 0;
            support.startWake(screen, attic, host -> {
                if (++sends[0] > 1) throw new java.io.IOException("no route");
            });
            toHandheld(screen);
            for (int i = 0; i < 20 && screen.hostActions().isWaking(); i++) {
                support.wakeClock += 1_000;
                Shadows.shadowOf(Looper.getMainLooper()).idleFor(1, java.util.concurrent.TimeUnit.SECONDS);
            }
            assertFalse("the resend failed", screen.hostActions().isWaking());
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            AlertDialog dialog = (AlertDialog) ShadowDialog.getLatestDialog();
            assertTrue("Try again shows", dialog.isShowing());
            assertFalse("the page waits under it", ReflectionHelpers.<Boolean>getField(screen, "recreatingForForm"));

            dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            screen.onWindowFocusChanged(true);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertTrue("done: the page follows the form", ReflectionHelpers.<Boolean>getField(screen, "recreatingForForm"));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void aPollWithNoNewsLeavesTheRowsAloneAndAnyNewsDrawsThem() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails den = pc("den", "Den", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            ComputerDetails attic = pc("attic", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            support.report(den);
            support.report(attic);
            GridView grid = screen.findViewById(R.id.fragmentView);
            int[] refreshes = refreshesOf(grid);

            // Every poll reports every PC, a second or two apart.
            support.report(den);
            support.report(attic);
            support.report(den);
            assertEquals(0, refreshes[0]);

            // The manager reports the PC it holds and changes it in place, so the row compares values it took.
            den.pairState = PairingManager.PairState.NOT_PAIRED;
            support.report(den);
            assertEquals(1, refreshes[0]);
            assertEquals(screen.getString(R.string.pcview_menu_pair_pc), screen.getString(
                    com.limelight.grid.PcGridAdapter.mainActionLabel(screen, rowOf(grid, "den"))));
            support.report(den);
            assertEquals(1, refreshes[0]);

            // A field no row shows today is still news: nothing has to remember to add it when one starts to.
            den.httpsPort = 47984;
            support.report(den);
            assertEquals(2, refreshes[0]);

            // A new PC is news once.
            ComputerDetails loft = pc("loft", "Loft", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            support.report(loft);
            int withLoft = refreshes[0];
            assertTrue(withLoft > 2);
            support.report(loft);
            assertEquals(withLoft, refreshes[0]);
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void aWakingRowIsDrawnWhenItsWaitStartsAndWhenTheClockEndsIt() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails attic = pc("attic", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            attic.macAddress = "02:00:00:00:00:01";
            support.report(attic);
            GridView grid = screen.findViewById(R.id.fragmentView);
            PcView.ComputerObject row = rowOf(grid, "attic");
            int[] refreshes = refreshesOf(grid);

            row.wakeSentAt = android.os.SystemClock.elapsedRealtime();
            support.report(attic);
            assertEquals(1, refreshes[0]);
            assertEquals(screen.getString(R.string.pc_status_waking),
                    com.limelight.grid.PcGridAdapter.statusText(screen, row));
            support.report(attic);
            assertEquals(1, refreshes[0]);

            // The PC never answered. No poll says the wait is over; the clock does.
            org.robolectric.shadows.ShadowSystemClock.advanceBy(java.time.Duration.ofMillis(PcView.WAKE_WINDOW_MS));
            support.report(attic);
            assertEquals(2, refreshes[0]);
            assertEquals(screen.getString(R.string.pc_offline),
                    com.limelight.grid.PcGridAdapter.statusText(screen, row));
            support.report(attic);
            assertEquals(2, refreshes[0]);
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /** First run's last action keeps its ring clear of the gesture bar and the screen's edge. */
    @Test
    public void firstRunLeavesRoomUnderItsLastAction() {
        Application application = RuntimeEnvironment.getApplication();
        GlPreferences gl = GlPreferences.readPreferences(application);
        gl.savedFingerprint = Build.FINGERPRINT;
        gl.glRenderer = "Test renderer";
        gl.writePreferences();
        application.getSharedPreferences("FirstRun", 0).edit().putBoolean("pairing_guide_shown", true).commit();
        ActivityController<Library> controller = Robolectric.buildActivity(Library.class).setup();
        try {
            Library screen = controller.get();
            org.robolectric.util.ReflectionHelpers.callInstanceMethod(com.limelight.PcView.class, screen, "layOutUnderChrome",
                    org.robolectric.util.ReflectionHelpers.ClassParameter.from(int.class, 0),
                    org.robolectric.util.ReflectionHelpers.ClassParameter.from(int.class, 0),
                    org.robolectric.util.ReflectionHelpers.ClassParameter.from(int.class, 0),
                    org.robolectric.util.ReflectionHelpers.ClassParameter.from(int.class, 48));
            assertTrue(screen.findViewById(R.id.first_run).getPaddingBottom()
                    >= 48 + screen.getResources().getDimensionPixelSize(R.dimen.space_l));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /**
     * A real TV's first run: the large title was absent on Add PC and "PCs" on How to connect, because a focus change wrote it.
     * With no PC there is no selection to name and the selected tab names the page, wherever focus is. The first PC brings it.
     */
    @Test
    @Config(qualifiers = "w960dp-h540dp-land-television-xhdpi")
    public void aTvsFirstRunHasNoLargeTitleWhereverFocusIs() {
        Application application = RuntimeEnvironment.getApplication();
        GlPreferences gl = GlPreferences.readPreferences(application);
        gl.savedFingerprint = Build.FINGERPRINT;
        gl.glRenderer = "Test renderer";
        gl.writePreferences();
        application.getSharedPreferences("FirstRun", 0).edit().putBoolean("pairing_guide_shown", true).commit();
        try (var empty = Robolectric.buildActivity(Library.class).setup().visible()) {
            android.view.View title = empty.get().findViewById(R.id.pc_focus_title);
            assertEquals(android.view.View.GONE, title.getVisibility());
            empty.get().findViewById(R.id.discovery_help).requestFocus();
            org.robolectric.shadows.ShadowLooper.shadowMainLooper().idle();
            assertEquals(android.view.View.GONE, title.getVisibility());
        }
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            assertEquals(android.view.View.VISIBLE, controller.get().findViewById(R.id.pc_focus_title).getVisibility());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /** First run's one action offers what the page's + offers: the address, or the code a host shows. */
    @Test
    public void theFirstRunActionOffersWhatPlusOffers() {
        Application application = RuntimeEnvironment.getApplication();
        GlPreferences gl = GlPreferences.readPreferences(application);
        gl.savedFingerprint = Build.FINGERPRINT;
        gl.glRenderer = "Test renderer";
        gl.writePreferences();
        application.getSharedPreferences("FirstRun", 0).edit().putBoolean("pairing_guide_shown", true).commit();
        Shadows.shadowOf(application.getPackageManager()).setSystemFeature(android.content.pm.PackageManager.FEATURE_CAMERA_ANY, true);
        ActivityController<Library> controller = Robolectric.buildActivity(Library.class).setup();
        try {
            Library screen = controller.get();
            assertEquals(android.view.View.VISIBLE, screen.findViewById(R.id.no_pc_found_layout).getVisibility());
            screen.findViewById(R.id.discovery_add).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            android.app.Dialog sheet = ShadowDialog.getLatestDialog();
            assertTrue("a sheet, not straight to the address page", sheet instanceof com.limelight.ui.ActionSheet);
            assertTrue(sheet.isShowing());
            assertNull(Shadows.shadowOf(screen).getNextStartedActivity());
            sheet.dismiss();
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void resizingKeepsTheLibraryViewsAndKeyboardFocus() {
        Application application = RuntimeEnvironment.getApplication();
        GlPreferences gl = GlPreferences.readPreferences(application);
        gl.savedFingerprint = Build.FINGERPRINT;
        gl.glRenderer = "Test renderer";
        gl.writePreferences();
        application.getSharedPreferences("FirstRun", 0).edit().putBoolean("pairing_guide_shown", true).commit();
        ActivityController<Library> controller = Robolectric.buildActivity(Library.class).setup();
        try {
            android.view.View add = controller.get().findViewById(R.id.discovery_add);
            add.requestFocus();
            android.content.res.Configuration config = new android.content.res.Configuration(application.getResources().getConfiguration());
            config.orientation = android.content.res.Configuration.ORIENTATION_LANDSCAPE;
            controller.get().onConfigurationChanged(config);
            assertSame(add, controller.get().findViewById(R.id.discovery_add));
            assertTrue(add.hasFocus());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    @Config(qualifiers = "w700dp-h440dp-land-xhdpi")
    public void rotatingFromHandheldToPhoneKeepTheBuiltFormLayoutParamsWithoutClassCast() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(1), 47989);
        Thread worker = null;
        try {
            Library screen = controller.get();
            // Start pairing so holdsForm() is true while the configuration changes to portrait phone.
            GridView grid = screen.findViewById(R.id.fragmentView);
            grid.performItemClick(null, 0, 0);
            worker = ReflectionHelpers.getField(screen.hostActions(), "pairingThread");
            assertTrue(screen.hostActions().isPairing());

            RuntimeEnvironment.setQualifiers("w400dp-h700dp-port-xhdpi");
            android.content.res.Configuration portrait = new android.content.res.Configuration(
                    screen.getResources().getConfiguration());
            screen.onConfigurationChanged(portrait);
            assertFalse(screen.isFinishing());
            assertTrue(screen.findViewById(R.id.pc_options_hint).getLayoutParams()
                    instanceof android.widget.LinearLayout.LayoutParams);

            screen.hostActions().cancelPairing();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
        } finally {
            controller.pause().stop().destroy();
            if (worker != null) {
                try {
                    worker.join(5000);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }
        }
    }
}
