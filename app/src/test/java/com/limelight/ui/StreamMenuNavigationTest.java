package com.limelight.ui;

import android.app.Activity;
import android.app.Application;
import android.view.KeyEvent;
import android.view.View;

import com.google.android.material.button.MaterialButton;
import com.limelight.R;
import com.limelight.nativeapi.NativeApiFixtures;
import com.limelight.nvstream.http.NvApp;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static com.limelight.ui.StreamMenu.Item.*;
import static com.limelight.ui.StreamMenuTestSupport.*;
import static org.junit.Assert.*;

/**
 * Interactive navigation, header/mode display, bitrate row updates, TV/D-pad focus, font scaling,
 * and live host-audio refresh tests for {@link StreamMenu}.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class)
public class StreamMenuNavigationTest {
    /** As desktop Nightjar's: the game over the stream's mode from the shared model, the PC while the mode is unknown. */
    @Test
    public void theHeaderNamesTheGameOverTheStreamsMode() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options("rubylight-serverinfo.xml", NvApp.Role.STREAM);
            state.appName = "Open World";
            state.host.name = "Gaming PC";
            state.streamWidth = 2560;
            state.streamHeight = 1440;
            state.streamFps = 144;
            state.videoFormat = 0x0200;
            state.hdr = true;
            StreamMenu menu = new StreamMenu(activity, () -> state, item -> { }, null, null);
            assertArrayEquals(new String[] {"Open World", "1440p · 144 FPS · HEVC HDR"}, menu.header(state));
            StreamMenu.Options starting = options("rubylight-serverinfo.xml", NvApp.Role.STREAM);
            starting.appName = "Open World";
            starting.host.name = "Gaming PC";
            assertArrayEquals(new String[] {"Open World", "Gaming PC"}, menu.header(starting));
            menu.show();
            assertEquals("Open World", menu.sheet().heading().toString());
            assertEquals("1440p · 144 FPS · HEVC HDR", menu.sheet().subtitle().getText().toString());
            // A page a level down shows its own title and no mode.
            menu.reveal(OVERLAY_PAGE).performClick();
            assertEquals(View.GONE, menu.sheet().subtitle().getVisibility());
            menu.sheet().dismiss();

            StreamMenu.Options monitor = options("rubylight-serverinfo.xml", NvApp.Role.REMOTE_MONITOR);
            monitor.appName = "Desktop";
            assertEquals(activity.getString(R.string.stream_monitor_title), menu.header(monitor)[0]);
        }
    }

    /** Without the live frame-rate change the row sets the bitrate, so it shows the bitrate, not the frame rate (nightjar #134). */
    @Test
    public void theBitrateRowShowsTheBitrateAndTheRateChangeRowTheFrameRate() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options("rubylight-serverinfo.xml", NvApp.Role.STREAM);
            state.streamFps = 60;
            state.bitrateKbps = 80000;
            state.bitrateChange = kbps -> kbps;
            StreamMenu menu = new StreamMenu(activity, () -> state, item -> { }, null, null);
            menu.show();
            MenuRow bitrate = (MenuRow) menu.reveal(BITRATE);
            assertEquals(BitrateSlider.text(activity, 80000), bitrate.value().toString());
            // Moving the slider moves the row's value with it.
            long now = android.os.SystemClock.uptimeMillis();
            assertTrue(menu.bitrateSlider().onKeyDown(android.view.KeyEvent.KEYCODE_DPAD_LEFT,
                    new android.view.KeyEvent(now, now, android.view.KeyEvent.ACTION_DOWN, android.view.KeyEvent.KEYCODE_DPAD_LEFT, 0)));
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertNotEquals(BitrateSlider.text(activity, 80000), bitrate.value().toString());
            menu.sheet().dismiss();

            state.rateChange = true;
            menu = new StreamMenu(activity, () -> state, item -> { }, null, null);
            menu.show();
            assertEquals("60 FPS", ((MenuRow) menu.reveal(RATE_CHANGE)).value().toString());
            menu.sheet().dismiss();
        }
    }

    @Test
    public void backInsideThePageReturnsToTheMenuAndBackAgainClosesIt() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = new StreamMenu(activity, () -> options("rubylight-serverinfo.xml", NvApp.Role.STREAM), chosen::add);
            menu.show();
            View back = menu.sheet().findViewById(R.id.action_sheet_back);
            assertEquals("A module's page has no back arrow", View.GONE, back.getVisibility());
            menu.reveal(OVERLAY_PAGE).performClick();
            assertEquals(StreamMenu.Page.OVERLAY, menu.page());
            assertNotNull(menu.row(OVERLAY_DEBUG));
            assertNotNull("The ways out stand under every page", menu.row(DISCONNECT));
            assertSame(menu.sheet().exits(), menu.row(DISCONNECT).getParent());
            assertTrue(chosen.isEmpty());
            // Touch has a back arrow on the page; it does what Back does.
            assertEquals(View.VISIBLE, back.getVisibility());
            back.performClick();
            assertEquals(StreamMenu.Page.MAIN, menu.page());
            assertEquals(View.GONE, back.getVisibility());
            menu.row(OVERLAY_PAGE).performClick();
            menu.sheet().onBackPressed();
            assertTrue(menu.sheet().isShowing());
            assertEquals(StreamMenu.Page.MAIN, menu.page());
            assertNotNull(menu.row(DISCONNECT));
            menu.sheet().onBackPressed();
            assertFalse(menu.sheet().isShowing());
        }
    }

    /** The text size of a quick toggle, a page row and an action row, in pixels. */
    private static float[] textSizes(float fontScale) {
        org.robolectric.RuntimeEnvironment.setFontScale(fontScale);
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu menu = new StreamMenu(activity, () -> options("rubylight-serverinfo.xml", NvApp.Role.STREAM), item -> { });
            menu.show();
            return new float[] {
                    ((MaterialButton) menu.row(KEYBOARD)).getTextSize(),
                    ((MaterialButton) menu.reveal(OVERLAY_PAGE)).getTextSize(),
                    ((MaterialButton) menu.row(DISCONNECT)).getTextSize()};
        } finally {
            org.robolectric.RuntimeEnvironment.setFontScale(1f);
        }
    }

    @Test
    public void theMenuTextFollowsTheSystemFontScale() {
        float[] normal = textSizes(1f);
        float[] large = textSizes(1.3f);
        for (int i = 0; i < normal.length; i++) {
            assertTrue("text " + i + " is drawn", normal[i] > 0);
            // A tile's 12sp lands on whole pixels, so its ratio is 16/12 rather than 1.3.
            assertEquals("text " + i + " grows with the font scale", 1.3f, large[i] / normal[i], 0.04f);
        }
    }

    @Test @Config(qualifiers = "television")
    public void tvMenusKeepRemoteActionsAndLeaveOutTouchOnlyControls() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options("rubylight-serverinfo.xml", NvApp.Role.STREAM);
            state.onscreenControls = true;
            state.gameControls = true;
            state.hostMenu = true;
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = new StreamMenu(activity, () -> state, chosen::add);
            menu.show();
            for (StreamMenu.Item item : new StreamMenu.Item[]{TOUCH_MODE, ONSCREEN_CONTROLS, CONTROLS_LAYOUT, GAME_CONTROLS, ZOOM}) {
                assertNull(item.name(), menu.reveal(item));
                assertFalse(items(menu.entries()).contains(item));
            }
            assertEquals(ControlCenterModel.DOCK, menu.sheet().placement());
            assertEquals("Keyboard", menu.reveal(KEYBOARD).getText().toString());
            assertNotNull(menu.row(DISCONNECT));
            assertNotNull(menu.reveal(KEYBOARD_KEYS));
            assertEquals(ControlCenterModel.QUICK, menu.module());
            assertEquals("Open the PC’s menu", menu.reveal(HOST_MENU).getText().toString());
            assertEquals(ControlCenterModel.PC, menu.module());
            menu.row(HOST_MENU).performClick();
            assertEquals(Arrays.asList(HOST_MENU), chosen);
            assertEquals(StreamMenu.Page.MAIN, menu.page());
            menu.reveal(OVERLAY_PAGE).performClick();
            assertEquals(StreamMenu.Page.OVERLAY, menu.page());
            menu.back();
            assertEquals(StreamMenu.Page.MAIN, menu.page());
            menu.sheet().dismiss();
        }
    }

    @Test
    public void controllerBAndEscapeStepBackFromThePageToo() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            for (int key : new int[] {KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_ESCAPE}) {
                StreamMenu menu = new StreamMenu(activity, () -> options("rubylight-serverinfo.xml", NvApp.Role.STREAM), item -> { });
                menu.show();
                menu.reveal(OVERLAY_PAGE).performClick();
                press(menu, key);
                assertEquals(StreamMenu.Page.MAIN, menu.page());
                assertTrue(menu.sheet().isShowing());
                press(menu, key);
                assertFalse(menu.sheet().isShowing());
            }
        }
    }

    @Test
    public void quickTogglesChangeInPlaceAndOtherItemsReportOnce() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options("rubylight-serverinfo.xml", NvApp.Role.STREAM);
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = new StreamMenu(activity, () -> state, item -> {
                chosen.add(item);
                if (item == OVERLAY) state.overlayShown = !state.overlayShown;
            });
            menu.show();
            assertFalse("Keyboard is an action, not a switch", menu.row(KEYBOARD).isCheckable());
            assertEquals("the model's short label, as desktop Nightjar's", activity.getString(R.string.stream_tile_keyboard),
                    menu.row(KEYBOARD).getText().toString());
            assertFalse("The PC keyboard is an action too", menu.row(PC_KEYBOARD).isCheckable());
            assertEquals(activity.getString(R.string.pc_keyboard_title), menu.row(PC_KEYBOARD).getText().toString());
            MaterialButton overlay = menu.reveal(OVERLAY);
            assertTrue(overlay.isCheckable());
            assertFalse(overlay.isChecked());
            assertTrue("A card says its state in words too",
                    overlay.getText().toString().endsWith("\n" + activity.getString(R.string.native_off)));
            overlay.performClick();
            assertTrue(menu.row(OVERLAY).isChecked());
            assertTrue(overlay.getText().toString().endsWith("\n" + activity.getString(R.string.native_on)));
            assertSame("The card is updated, not rebuilt", overlay, menu.row(OVERLAY));
            menu.reveal(KEYBOARD).performClick();
            menu.row(DISCONNECT).performClick();
            assertEquals(Arrays.asList(OVERLAY, KEYBOARD, DISCONNECT), chosen);
        }
    }

    @Test
    public void aFailedAudioReadSaysSoInTheMenu() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options(nativeApiHost(), NvApp.Role.STREAM);
            state.nightModeOffered = true;
            state.headphoneSurroundOffered = true;
            StreamMenu menu = new StreamMenu(activity, () -> state, item -> { });
            menu.show();
            assertNull("Nothing while the host has not answered", menu.audioNote());

            // GET /audio failed: no switches, and a line says why.
            state.hostAudioUnread = true;
            menu.refresh();
            assertNull(menu.row(NIGHT_MODE));
            assertNotNull(menu.audioNote());
            assertEquals(activity.getString(R.string.stream_audio_unread), menu.audioNote().getText().toString());
            assertTrue(menu.audioNote().isShown());

            // The next answer that works replaces the line with the switches, on the modules the shared model gives them.
            state.hostAudioUnread = false;
            state.hostAudio = NativeApiFixtures.audio(NativeApiFixtures.audioJson(false, false));
            menu.refresh();
            assertNull(menu.audioNote());
            assertNotNull(menu.reveal(NIGHT_MODE));
            assertEquals(menu.dock().moduleOf("night_mode"), menu.module());
            assertNotNull(menu.reveal(HEADPHONE_SURROUND));
            assertEquals(ControlCenterModel.AUDIO, menu.module());

            // Remote Monitor changes nothing on the PC, so it has nothing to say about it either.
            StreamMenu.Options monitor = options(nativeApiHost(), NvApp.Role.REMOTE_MONITOR);
            monitor.hostAudioUnread = true;
            StreamMenu watching = new StreamMenu(activity, () -> monitor, item -> { });
            watching.show();
            assertNull(watching.audioNote());
            menu.sheet().dismiss();
            watching.sheet().dismiss();
        }
    }

    @Test
    public void theAudioSwitchesAppearWhenTheHostAnswersAndShowItsNewState() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options(nativeApiHost(), NvApp.Role.STREAM);
            state.nightModeOffered = true;
            state.headphoneSurroundOffered = true;
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = new StreamMenu(activity, () -> state, chosen::add);
            menu.show();
            assertNull(menu.row(NIGHT_MODE));

            // GET /audio answers after the menu opened; headphone surround brings the Audio module, which the rail gains.
            assertFalse(menu.modules().contains(ControlCenterModel.AUDIO));
            state.hostAudio = NativeApiFixtures.audio(NativeApiFixtures.audioJson(false, false));
            menu.refresh();
            assertNull("Quick stays as it was", menu.row(NIGHT_MODE));
            assertTrue(menu.modules().contains(ControlCenterModel.AUDIO));
            MaterialButton surround = menu.reveal(HEADPHONE_SURROUND);
            assertEquals(ControlCenterModel.AUDIO, menu.module());
            assertTrue("a switch is a card, wherever the model has it", surround instanceof TileGrid.Tile && surround.isCheckable());
            MaterialButton night = menu.reveal(NIGHT_MODE);
            assertEquals(menu.dock().moduleOf("night_mode"), menu.module());
            assertEquals(activity.getString(R.string.stream_night_mode), night.getText().toString().split("\n")[0]);
            assertTrue(night.isCheckable());
            assertFalse(night.isChecked());

            // The switch asks; until the host answers it shows the host's state, then the new one in place.
            night.performClick();
            assertEquals(Arrays.asList(NIGHT_MODE), chosen);
            assertTrue(menu.sheet().isShowing());
            assertFalse(menu.row(NIGHT_MODE).isChecked());
            state.hostAudio = NativeApiFixtures.audio(NativeApiFixtures.audioJson(true, false));
            menu.refresh();
            assertTrue(menu.row(NIGHT_MODE).isChecked());
            assertSame("The row is updated, not rebuilt", night, menu.row(NIGHT_MODE));
            menu.sheet().dismiss();
        }
    }

    @Test
    public void everyRowTakesDpadFocusAndTheFirstToggleHasItFirst() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu menu = new StreamMenu(activity, () -> options("rubylight-serverinfo.xml", NvApp.Role.STREAM), item -> { });
            menu.show();
            for (String module : menu.modules()) {
                menu.select(module);
                assertEquals(module, menu.module());
                assertFalse(module + " has something to choose", menu.entries().isEmpty());
                for (StreamMenu.Entry entry : menu.entries()) {
                    View row = menu.row(entry.item);
                    assertTrue(entry.item + " is focusable", row.isFocusable());
                    assertTrue(entry.item + " is clickable", row.isClickable());
                }
                assertTrue(module + " is on the rail", menu.sheet().railEntry(module).isClickable());
            }
            assertTrue(menu.reveal(KEYBOARD).isFocusedByDefault());
            assertFalse(menu.row(DISCONNECT).isFocusedByDefault());
            menu.reveal(OVERLAY_PAGE).performClick();
            assertTrue("The page's first row takes the focus", menu.row(OVERLAY_OFF).isFocusedByDefault());
        }
    }

    @Test
    public void withoutTogglesFocusStartsPastDisconnectAndBackClosesWithoutLeaving() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = new StreamMenu(activity,
                    () -> options("butterpollo-viewer-serverinfo.xml", NvApp.Role.INPUT_ONLY), chosen::add);
            menu.show();
            assertFalse("OK right after opening must not disconnect", menu.row(DISCONNECT).isFocusedByDefault());
            // Nothing to type with: the rail leaves Quick out and opens on the first module that has something.
            assertFalse(menu.modules().isEmpty());
            assertEquals(menu.modules().get(0), menu.module());
            MaterialButton first = menu.sheet().page().firstAction();
            assertNotNull("the page that opens has something to choose", first);
            assertTrue("and the focus starts on it", first.isFocusedByDefault());
            assertNotSame(menu.row(DISCONNECT), first);
            assertNotNull("Reconnect is on Power", menu.reveal(RECONNECT));
            assertEquals(ControlCenterModel.POWER, menu.module());
            // Back opened the menu; Back again only closes it.
            menu.sheet().onBackPressed();
            assertFalse(menu.sheet().isShowing());
            assertTrue(chosen.isEmpty());
        }
    }

    /** The stream menu on a 1080p TV, both pages: no text under the TV caption size, and the sheet inside the safe area. */
    @Test @Config(qualifiers = "w960dp-h540dp-land-television-xhdpi")
    public void tvMenuTextIsReadableAndTheSheetFitsInsideTheOverscan() {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            activity.setTheme(R.style.StreamTheme);
            StreamMenu.Options state = options("rubylight-serverinfo.xml", NvApp.Role.STREAM);
            state.hostMenu = true;
            StreamMenu menu = new StreamMenu(activity, () -> state, item -> { });
            menu.show();
            // Its window is attached once the first frame ran: only then does a view know where on the screen it is.
            org.robolectric.shadows.ShadowLooper.idleMainLooper(600, java.util.concurrent.TimeUnit.MILLISECONDS);
            int width = activity.getResources().getDisplayMetrics().widthPixels;
            int height = activity.getResources().getDisplayMetrics().heightPixels;
            View window = menu.sheet().getWindow().getDecorView();
            for (String module : menu.modules()) {
                menu.select(module);
                layOut(window, width, height);
                assertEquals(module, java.util.Collections.emptyList(), TvAudit.findings(window, width, height));
            }
            menu.reveal(OVERLAY_PAGE).performClick();
            layOut(window, width, height);
            assertEquals(java.util.Collections.emptyList(), TvAudit.findings(window, width, height));
            menu.sheet().dismiss();
        }
    }

    private static void layOut(View window, int width, int height) {
        window.measure(View.MeasureSpec.makeMeasureSpec(width, View.MeasureSpec.EXACTLY),
                View.MeasureSpec.makeMeasureSpec(height, View.MeasureSpec.EXACTLY));
        window.layout(0, 0, width, height);
    }

    /** Disconnect's and End session's confirms say different things: one waits to resume, the other gives the PC back. */
    @Test
    public void endSessionsConfirmSaysWhatSetsItApartFromDisconnect() {
        android.content.Context context = org.robolectric.RuntimeEnvironment.getApplication();
        String disconnect = context.getString(R.string.stream_disconnect_confirm);
        String end = context.getString(R.string.stream_end_session_confirm);
        assertNotEquals(disconnect, end);
        assertTrue(disconnect, disconnect.contains("resume"));
        assertFalse(end, end.contains("resume"));
        assertTrue(end, end.contains("game keeps running"));
    }
}
