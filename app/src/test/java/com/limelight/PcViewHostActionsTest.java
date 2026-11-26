package com.limelight;

import android.app.Application;
import android.os.Looper;
import android.widget.GridView;
import android.widget.TextView;

import androidx.appcompat.app.AlertDialog;

import com.limelight.PcViewLifecycleTest.Library;
import com.limelight.PcViewTestSupport.Input;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.PairingManager;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.ShadowDialog;
import org.robolectric.util.ReflectionHelpers;

import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CountDownLatch;

import static com.limelight.PcViewTestSupport.*;
import static org.junit.Assert.*;

/**
 * Tests {@link PcView} host action sheets, pairing/unpairing, renaming, removing, and Add-PC entry
 * points.
 */
@RunWith(org.robolectric.RobolectricTestRunner.class)
@Config(sdk = 28, application = Application.class, shadows = PcViewLifecycleTest.NoNativeMoonBridge.class,
        instrumentedPackages = "com.limelight.nvstream.jni")
public class PcViewHostActionsTest {
    private final PcViewTestSupport support = new PcViewTestSupport();

    @Test
    public void eachPcStateOffersItsOwnShortMenu() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails running = pc("running", "Den", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            running.runningGameId = 7;
            support.report(running);
            ComputerDetails ready = pc("ready", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            support.report(ready);
            ComputerDetails apollo = pc("apollo", "Apollo", ComputerDetails.State.ONLINE, PairingManager.PairState.NOT_PAIRED);
            apollo.caps = com.limelight.nvstream.http.TestHosts.fixture("apollo-http-serverinfo.xml");
            support.report(apollo);
            ComputerDetails asleep = pc("asleep", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            asleep.macAddress = "02:00:00:00:00:01";
            support.report(asleep);

            assertEquals(labels(screen, R.string.library_open, R.string.applist_menu_resume, R.string.applist_menu_quit,
                    R.string.pc_menu_settings, R.string.host_menu_rename, R.string.pcview_menu_unpair_pc,
                    R.string.pcview_menu_delete_pc), sheetFor(screen, "Den"));
            assertEquals(labels(screen, R.string.library_open, R.string.pc_menu_settings, R.string.host_menu_rename,
                    R.string.pcview_menu_unpair_pc, R.string.pcview_menu_delete_pc), sheetFor(screen, "Office"));
            assertEquals(labels(screen, R.string.pcview_menu_pair_pc, R.string.pair_otp_title, R.string.pc_menu_settings,
                    R.string.host_menu_rename, R.string.discovery_help, R.string.pcview_menu_delete_pc),
                    sheetFor(screen, "Apollo"));
            // A PC this device has never read offers no one-time PIN.
            assertEquals(labels(screen, R.string.pcview_menu_pair_pc, R.string.pc_menu_settings, R.string.host_menu_rename,
                    R.string.discovery_help, R.string.pcview_menu_delete_pc), sheetFor(screen, "PC"));
            assertEquals(labels(screen, R.string.pcview_menu_send_wol, R.string.pc_menu_settings, R.string.host_menu_rename,
                    R.string.pcview_menu_delete_pc), sheetFor(screen, "Attic"));
            // Without a MAC address Wake is hidden and the sheet says how to get one.
            support.report(pc("lost", "Loft", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED));
            assertEquals(labels(screen, R.string.pc_menu_settings, R.string.host_menu_rename,
                    R.string.pcview_menu_delete_pc), sheetFor(screen, "Loft"));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void sleepTakesWakesPlaceWhileThePcIsOnAndSaysWhyWhenWithheld() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            String power = "<root status_code=\"200\"><hostname>VP</hostname><appversion>7.1.431.-1</appversion>"
                    + "<uniqueid>vp</uniqueid><HttpsPort>47984</HttpsPort><PairStatus>1</PairStatus>"
                    + "<currentgame>0</currentgame><state>SUNSHINE_SERVER_FREE</state>"
                    + "<PowerActionVersion>1</PowerActionVersion><SleepSupported>1</SleepSupported>";
            ComputerDetails allowed = pc("allowed", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            allowed.caps = com.limelight.nvstream.http.TestHosts.caps(power + "<SleepAllowed>1</SleepAllowed></root>");
            support.report(allowed);
            ComputerDetails withheld = pc("withheld", "Den", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            withheld.caps = com.limelight.nvstream.http.TestHosts.caps(power + "<SleepAllowed>0</SleepAllowed></root>");
            support.report(withheld);

            java.util.List<String> sleep = labels(screen, R.string.library_open, R.string.host_menu_sleep,
                    R.string.pc_menu_settings, R.string.host_menu_rename, R.string.pcview_menu_unpair_pc,
                    R.string.pcview_menu_delete_pc);
            // Allowed, Sleep PC is a plain row: it shipped in 3.0.0-rc.3 and carries no Experimental note.
            assertEquals(sleep, sheetFor(screen, "Office"));
            sleep.set(1, screen.getString(R.string.host_menu_sleep) + "\n"
                    + screen.getString(R.string.host_feature_reason_sleep_permission) + " (dimmed)");
            assertEquals(sleep, sheetFor(screen, "Den"));
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void tappingUnpairedPcShowsPinBeforePollingStopsAndCanBeCancelled() throws Exception {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(1), 47989);
        Thread worker = null;
        try {
            Library screen = controller.get();
            GridView grid = screen.findViewById(R.id.fragmentView);
            grid.performItemClick(null, 0, 0);
            worker = ReflectionHelpers.getField(screen.hostActions(), "pairingThread");
            AlertDialog dialog = ReflectionHelpers.getField(screen.hostActions(), "pairingDialog");
            assertNotNull(dialog);
            assertTrue(dialog.isShowing());
            // The PC, its PIN in four cells, one instruction.
            assertEquals("PC", ((TextView) dialog.findViewById(R.id.pairing_cinema_pc)).getText().toString());
            com.limelight.ui.PinSlots cells = dialog.findViewById(R.id.pairing_cinema_pin);
            assertTrue(cells.shown().toString(), cells.shown().toString().matches("[0-9]{4}"));
            assertEquals(4, cells.getChildCount());
            assertEquals(screen.getString(R.string.pair_pairing_help),
                    ((TextView) dialog.findViewById(R.id.pairing_cinema_where)).getText().toString());
            assertFalse(ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));

            dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            worker.join(5000);
            assertFalse(worker.isAlive());
            assertFalse(dialog.isShowing());
            assertNull(ReflectionHelpers.getField(screen.hostActions(), "pairingDialog"));
            assertNull(ReflectionHelpers.getField(screen.hostActions(), "pairingRequest"));
            assertTrue(ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));
        } finally {
            controller.pause().stop().destroy();
            if (worker != null) worker.join(5000);
        }
    }

    /** The menu closes after Pair has stopped the polling; closing it must not start it again mid-pairing. */
    @Test
    public void pairFromTheMenuKeepsPollingStoppedUntilPairingEnds() throws Exception {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(1), 47989);
        Thread worker = null;
        try {
            Library screen = controller.get();
            GridView grid = screen.findViewById(R.id.fragmentView);
            screen.showComputerMenu((PcView.ComputerObject) grid.getItemAtPosition(0));
            com.limelight.ui.ActionSheet sheet = (com.limelight.ui.ActionSheet) ShadowDialog.getLatestDialog();
            android.view.View pair = button(sheet.findViewById(android.R.id.content),
                    screen.getString(R.string.pcview_menu_pair_pc));
            assertNotNull(pair);
            pair.performClick();
            // The sheet's dismiss listener runs from the main looper, after the pairing has started.
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            worker = ReflectionHelpers.getField(screen.hostActions(), "pairingThread");
            assertTrue(screen.hostActions().isPairing());
            assertNull(ReflectionHelpers.getField(screen, "pcMenu"));
            assertFalse("polling restarted during pairing", ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));

            AlertDialog dialog = ReflectionHelpers.getField(screen.hostActions(), "pairingDialog");
            dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            worker.join(5000);
            assertTrue(ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));
        } finally {
            controller.pause().stop().destroy();
            if (worker != null) worker.join(5000);
        }
    }

    /** A rotation into another form recreated the page and cancelled the pairing; now the page waits for it. */
    @Test
    public void aRunningPairingKeepsThePageThroughAFormChangeThenThePageFollows() throws Exception {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(1), 47989);
        Thread worker = null;
        try {
            Library screen = controller.get();
            assertEquals(com.limelight.ui.GlassForm.PHONE, com.limelight.ui.GlassForm.of(screen));
            GridView grid = screen.findViewById(R.id.fragmentView);
            screen.showComputerMenu((PcView.ComputerObject) grid.getItemAtPosition(0));
            com.limelight.ui.ActionSheet sheet = (com.limelight.ui.ActionSheet) ShadowDialog.getLatestDialog();
            button(sheet.findViewById(android.R.id.content), screen.getString(R.string.pcview_menu_pair_pc)).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            worker = ReflectionHelpers.getField(screen.hostActions(), "pairingThread");
            AlertDialog dialog = ReflectionHelpers.getField(screen.hostActions(), "pairingDialog");
            assertTrue(dialog.isShowing());

            org.robolectric.RuntimeEnvironment.setQualifiers("w900dp-h700dp-land");
            screen.onConfigurationChanged(screen.getResources().getConfiguration());
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertEquals(com.limelight.ui.GlassForm.HANDHELD, com.limelight.ui.GlassForm.of(screen));
            assertFalse("the page waits for the pairing", ReflectionHelpers.<Boolean>getField(screen, "recreatingForForm"));
            assertFalse(screen.isDestroyed());
            assertTrue("the pairing still runs", screen.hostActions().isPairing());
            assertTrue("its PIN still shows", dialog.isShowing());

            dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            worker.join(5000);
            assertFalse(screen.hostActions().isPairing());
            screen.onWindowFocusChanged(true);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertTrue("done: the page follows the form", ReflectionHelpers.<Boolean>getField(screen, "recreatingForForm"));
        } finally {
            controller.pause().stop().destroy();
            if (worker != null) worker.join(5000);
        }
    }

    @Test
    public void pairingShowsPinDuringHostCheckAndSupportsRetryAfterFailure() throws Exception {
        try (ServerSocket host = new ServerSocket(0, 1, InetAddress.getByName("127.0.0.1"))) {
            host.setSoTimeout(5000);
            ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), host.getLocalPort());
            Thread worker = null;
            try {
                Library screen = controller.get();
                GridView grid = screen.findViewById(R.id.fragmentView);
                grid.performItemClick(null, 0, 0);
                worker = ReflectionHelpers.getField(screen.hostActions(), "pairingThread");
                AlertDialog pinDialog;
                try (Socket connection = host.accept()) {
                    Shadows.shadowOf(Looper.getMainLooper()).idle();
                    pinDialog = ReflectionHelpers.getField(screen.hostActions(), "pairingDialog");
                    assertNotNull(pinDialog);
                    assertTrue(pinDialog.isShowing());
                    connection.getOutputStream().write(("HTTP/1.1 503 Service Unavailable\r\n" +
                            "Content-Length: 0\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
                    worker.join(5000);
                    assertFalse(worker.isAlive());
                }
                Shadows.shadowOf(Looper.getMainLooper()).idle();
                assertFalse(pinDialog.isShowing());
                AlertDialog failure = (AlertDialog) ShadowDialog.getLatestDialog();
                assertTrue(failure.isShowing());
                assertEquals(screen.getString(R.string.pair_connection_fix),
                        ((TextView) failure.findViewById(android.R.id.message)).getText().toString());
                assertTrue(ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));

                failure.getButton(AlertDialog.BUTTON_POSITIVE).performClick();
                Shadows.shadowOf(Looper.getMainLooper()).idle();
                worker = ReflectionHelpers.getField(screen.hostActions(), "pairingThread");
                try (Socket connection = host.accept()) {
                    AlertDialog retry = ReflectionHelpers.getField(screen.hostActions(), "pairingDialog");
                    assertNotNull(retry);
                    assertTrue(retry.isShowing());
                    // Leaving to type the PIN into the host's web page on this phone keeps pairing going.
                    controller.pause();
                    assertTrue(worker.isAlive());
                    assertSame(retry, ReflectionHelpers.getField(screen.hostActions(), "pairingDialog"));
                    assertNotNull(ReflectionHelpers.getField(screen.hostActions(), "pairingRequest"));
                    connection.getOutputStream().write(("HTTP/1.1 503 Service Unavailable\r\n" +
                            "Content-Length: 0\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.US_ASCII));
                    worker.join(5000);
                    assertFalse(worker.isAlive());
                }
                // The result waits for the return to the PC list.
                Shadows.shadowOf(Looper.getMainLooper()).idle();
                AlertDialog retry = ReflectionHelpers.getField(screen.hostActions(), "pairingDialog");
                assertTrue(retry.isShowing());
                assertFalse(ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));
                controller.resume();
                Shadows.shadowOf(Looper.getMainLooper()).idle();
                assertFalse(retry.isShowing());
                AlertDialog again = (AlertDialog) ShadowDialog.getLatestDialog();
                assertNotSame(failure, again);
                assertTrue(again.isShowing());
                assertEquals(screen.getString(R.string.pair_connection_fix),
                        ((TextView) again.findViewById(android.R.id.message)).getText().toString());
                assertNull(ReflectionHelpers.getField(screen.hostActions(), "pairingRequest"));
                assertTrue(ReflectionHelpers.<Boolean>getField(screen, "runningPolling"));
            } finally {
                if (controller != null) controller.pause().stop().destroy();
                if (worker != null) worker.join(5000);
            }
        }
    }

    @Test
    public void aRenamedPcTakesItsNewPlaceInTheList() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            support.report(pc("a", "Alpha", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED));
            support.report(pc("b", "Beta", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED));
            GridView grid = screen.findViewById(R.id.fragmentView);
            assertEquals("Alpha", ((PcView.ComputerObject) grid.getItemAtPosition(0)).details.displayName());
            PcView.ComputerObject alpha = (PcView.ComputerObject) grid.getItemAtPosition(0);
            saveName(screen, alpha.details, "Zulu");
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertEquals("Beta", ((PcView.ComputerObject) grid.getItemAtPosition(0)).details.displayName());
            assertEquals("Zulu", ((PcView.ComputerObject) grid.getItemAtPosition(1)).details.displayName());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    /** + offers both ways in where there is a camera; with none there is one way, and + takes it. */
    @Test
    public void plusOffersAddAndScanWithACamera() {
        org.robolectric.Shadows.shadowOf(RuntimeEnvironment.getApplication().getPackageManager())
                .setSystemFeature(android.content.pm.PackageManager.FEATURE_CAMERA_ANY, true);
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            screen.findViewById(R.id.manuallyAddPc).performClick();
            com.limelight.ui.ActionSheet sheet = ReflectionHelpers.getField(screen, "addPcSheet");
            assertNotNull(sheet);
            assertTrue(sheet.isShowing());
            sheet.dismiss();
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void plusGoesStraightToAddWithNoCamera() {
        org.robolectric.Shadows.shadowOf(RuntimeEnvironment.getApplication().getPackageManager())
                .setSystemFeature(android.content.pm.PackageManager.FEATURE_CAMERA_ANY, false);
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            screen.findViewById(R.id.manuallyAddPc).performClick();
            assertNull(ReflectionHelpers.getField(screen, "addPcSheet"));
            assertEquals(com.limelight.preferences.AddComputerManually.class.getName(),
                    org.robolectric.Shadows.shadowOf(screen).getNextStartedActivity().getComponent().getClassName());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void aSuccessfulUnpairDropsTheCachedStateAndTellsThePerson() throws Exception {
        ServerSocket host = new ServerSocket(0, 5, InetAddress.getByName("127.0.0.1"));
        java.util.concurrent.atomic.AtomicBoolean unpaired = new java.util.concurrent.atomic.AtomicBoolean();
        Thread server = new Thread(() -> {
            try {
                while (true) {
                    try (Socket client = host.accept()) {
                        java.io.BufferedReader in = new java.io.BufferedReader(
                                new java.io.InputStreamReader(client.getInputStream(), StandardCharsets.UTF_8));
                        String request = in.readLine();
                        for (String line = in.readLine(); line != null && !line.isEmpty(); line = in.readLine()) { }
                        if (request != null && request.contains("/unpair")) unpaired.set(true);
                        byte[] body = ("<root status_code=\"200\"><uniqueid>ready</uniqueid><state>SUNSHINE_SERVER_FREE</state><appversion>7.1.431.-1</appversion><PairStatus>"
                                + (unpaired.get() ? "0" : "1") + "</PairStatus></root>").getBytes(StandardCharsets.UTF_8);
                        client.getOutputStream().write(("HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: "
                                + body.length + "\r\nConnection: close\r\n\r\n").getBytes(StandardCharsets.UTF_8));
                        client.getOutputStream().write(body);
                    }
                }
            } catch (java.io.IOException closed) {
                // The test closed the socket.
            }
        }, "fake-host");
        server.start();
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails ready = pc("ready", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            ready.activeAddress = new ComputerDetails.AddressTuple("127.0.0.1", host.getLocalPort());
            ready.httpsPort = host.getLocalPort();
            support.report(ready);
            ReflectionHelpers.callInstanceMethod(screen.hostActions(), "unpair",
                    ReflectionHelpers.ClassParameter.from(ComputerDetails.class, ready));
            long deadline = System.currentTimeMillis() + 10000;
            while (org.robolectric.shadows.ShadowToast.getTextOfLatestToast() == null
                    || !org.robolectric.shadows.ShadowToast.getTextOfLatestToast().equals(screen.getString(R.string.unpair_success))) {
                assertTrue("Unpair did not finish", System.currentTimeMillis() < deadline);
                Thread.sleep(20);
                Shadows.shadowOf(Looper.getMainLooper()).idle();
            }
            assertEquals(java.util.List.of("ready"), support.invalidated);
        } finally {
            controller.pause().stop().destroy();
            host.close();
        }
    }

    @Test
    public void everyInputOpensTheOptionsOfEveryPcState() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            ComputerDetails running = pc("running", "Den", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED);
            running.runningGameId = 7;
            support.report(running);
            support.report(pc("ready", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED));
            ComputerDetails asleep = pc("asleep", "Attic", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED);
            asleep.macAddress = "02:00:00:00:00:01";
            support.report(asleep);
            support.report(pc("lost", "Loft", ComputerDetails.State.OFFLINE, PairingManager.PairState.PAIRED));
            GridView grid = screen.findViewById(R.id.fragmentView);
            assertEquals(5, grid.getCount());
            for (int row = 0; row < grid.getCount(); row++) {
                for (Input input : Input.values()) {
                    String where = ((PcView.ComputerObject) grid.getItemAtPosition(row)).details.name + " by " + input;
                    com.limelight.ui.ActionSheet sheet = openOptionsWith(screen, row, input);
                    assertNotNull(where, sheet);
                    java.util.List<String> actions = new java.util.ArrayList<>();
                    collectActions(sheet.findViewById(android.R.id.content), actions);
                    assertTrue(where + ": " + actions, actions.contains(screen.getString(R.string.host_menu_rename)));
                    assertTrue(where + ": " + actions, actions.contains(screen.getString(R.string.pcview_menu_delete_pc)));
                    sheet.dismiss();
                    Shadows.shadowOf(Looper.getMainLooper()).idle();
                    assertFalse(where + " must not also act on the card", screen.hostActions().isPairing());
                }
            }
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void renamingShowsTheNameEverywhereAndEmptyBringsBackThePcsOwn() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            support.report(pc("ready", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED));
            GridView grid = screen.findViewById(R.id.fragmentView);
            PcView.ComputerObject computer = (PcView.ComputerObject) grid.getItemAtPosition(0);
            assertEquals("Office", computer.details.name);

            com.limelight.ui.ActionSheet sheet = openOptionsWith(screen, 0, Input.LONG_PRESS);
            java.util.List<android.view.View> buttons = new java.util.ArrayList<>();
            findByText(sheet.findViewById(android.R.id.content), screen.getString(R.string.host_menu_rename), buttons);
            assertEquals(1, buttons.size());
            buttons.get(0).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            AlertDialog dialog = (AlertDialog) ShadowDialog.getLatestDialog();
            android.widget.EditText field = dialog.findViewById(R.id.rename_name);
            assertEquals("Office", field.getText().toString());
            field.setText("  Living room PC  ");
            dialog.getButton(AlertDialog.BUTTON_POSITIVE).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();

            assertEquals("Living room PC", support.renamed.get("ready"));
            assertEquals("Office", computer.details.name);
            assertEquals("Living room PC", computer.details.displayName());
            java.util.List<android.view.View> labels = new java.util.ArrayList<>();
            findByText(grid.getChildAt(0), "Living room PC", labels);
            assertFalse("The row shows the new name", labels.isEmpty());

            // The empty name, and the PC's own name, both mean no alias.
            saveName(screen, computer.details, "   ");
            assertTrue(support.renamed.containsKey("ready"));
            assertNull(support.renamed.get("ready"));
            assertEquals("Office", computer.details.displayName());
            saveName(screen, computer.details, "Den");
            assertEquals("Den", support.renamed.get("ready"));
            saveName(screen, computer.details, " Office ");
            assertNull(support.renamed.get("ready"));
            assertEquals("Office", computer.details.displayName());
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void removeAsksFirstAndForgetsThePcOnlyAfterTheAnswer() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            support.report(pc("ready", "Office", ComputerDetails.State.ONLINE, PairingManager.PairState.PAIRED));
            GridView grid = screen.findViewById(R.id.fragmentView);
            PcView.ComputerObject computer = (PcView.ComputerObject) grid.getItemAtPosition(0);
            computer.details.alias = "Den";

            screen.hostActions().confirmRemove(computer.details);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            AlertDialog dialog = (AlertDialog) ShadowDialog.getLatestDialog();
            assertEquals(screen.getString(R.string.delete_pc_msg),
                    ((TextView) dialog.findViewById(android.R.id.message)).getText().toString());
            assertEquals(screen.getString(R.string.pcview_menu_delete_pc),
                    dialog.getButton(AlertDialog.BUTTON_POSITIVE).getText().toString());
            dialog.getButton(AlertDialog.BUTTON_NEGATIVE).performClick();
            assertTrue(support.removed.isEmpty());
            assertEquals(2, grid.getCount());

            screen.hostActions().confirmRemove(computer.details);
            ((AlertDialog) ShadowDialog.getLatestDialog()).getButton(AlertDialog.BUTTON_POSITIVE).performClick();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertEquals(java.util.List.of(computer.details), support.removed);
            assertEquals(1, grid.getCount());
            assertEquals("PC", ((PcView.ComputerObject) grid.getItemAtPosition(0)).details.name);
        } finally {
            controller.pause().stop().destroy();
        }
    }

    @Test
    public void aCancelledPairingNeverShowsItsResult() throws Exception {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(1), 47989);
        Thread worker = null;
        try {
            Library screen = controller.get();
            com.limelight.computers.HostActions actions = screen.hostActions();
            GridView grid = screen.findViewById(R.id.fragmentView);
            ComputerDetails computer = ((PcView.ComputerObject) grid.getItemAtPosition(0)).details;
            grid.performItemClick(null, 0, 0);
            worker = ReflectionHelpers.getField(actions, "pairingThread");
            int generation = ReflectionHelpers.getField(actions, "pairingGeneration");
            assertTrue(actions.isPairing());

            actions.cancelPairing();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            worker.join(5000);
            assertFalse(actions.isPairing());
            android.app.Dialog before = ShadowDialog.getLatestDialog();

            // The cancelled attempt reports late, on screen and while the screen is paused.
            showResult(actions, generation, true, null, computer);
            showResult(actions, generation, false, "late failure", computer);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertSame(before, ShadowDialog.getLatestDialog());
            controller.pause();
            showResult(actions, generation, false, "late failure", computer);
            assertFalse(actions.deliverPendingResult());
            controller.resume();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertSame(before, ShadowDialog.getLatestDialog());
            assertNull(ReflectionHelpers.getField(actions, "pendingPairingResult"));
        } finally {
            controller.pause().stop().destroy();
            if (worker != null) worker.join(5000);
        }
    }

    @Test
    public void destroyDismissesTrackedHostActionDialogsAndErrorDialog() {
        ActivityController<Library> controller = support.pairingScreen(new CountDownLatch(0), 47989);
        try {
            Library screen = controller.get();
            com.limelight.computers.HostActions actions = screen.hostActions();
            GridView grid = screen.findViewById(R.id.fragmentView);
            ComputerDetails computer = ((PcView.ComputerObject) grid.getItemAtPosition(0)).details;

            actions.confirmUnpair(computer);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            AlertDialog unpairDialog = (AlertDialog) ShadowDialog.getLatestDialog();
            assertNotNull(unpairDialog);
            assertTrue(unpairDialog.isShowing());

            actions.showError("Title", "Boom", null);
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            AlertDialog errDialog = (AlertDialog) ShadowDialog.getLatestDialog();
            assertNotNull(errDialog);
            assertTrue(errDialog.isShowing());

            actions.destroy();
            Shadows.shadowOf(Looper.getMainLooper()).idle();
            assertFalse(unpairDialog.isShowing());
            assertFalse(errDialog.isShowing());
        } finally {
            controller.pause().stop().destroy();
        }
    }
}
