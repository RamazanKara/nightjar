package com.limelight.ui;

import android.app.Activity;
import android.app.Application;
import android.view.View;

import com.limelight.R;
import com.limelight.binding.input.KeyboardShortcuts;
import com.limelight.nvstream.http.ComputerDetails;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.TestHosts;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.shadows.ShadowLooper;
import org.robolectric.util.ReflectionHelpers;

import java.io.File;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static com.limelight.ui.StreamMenu.Item.*;
import static org.junit.Assert.*;

/**
 * The Control Center on a 1080p TV (nightjar#52, nightjar#63, redesigned as a dock for 7.0): Back opens it with focus on
 * its first card, Down walks the cards and rows to Return to stream and Disconnect, the arrow toward the rail reaches
 * the lit module, and Back again closes it without leaving. Set NIGHTJAR_PREVIEW_OUTPUT to capture the dock, on a TV
 * and on a handheld held sideways.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class, qualifiers = "w960dp-h540dp-land-television-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
public class StreamMenuTvFocusTest {
    /** Each label key of the shared model resolves to a non-empty Android string. */
    // keep class body clean

    private static StreamMenu open(Activity activity, List<StreamMenu.Item> chosen) {
        activity.setTheme(R.style.StreamTheme);
        ComputerDetails host = TestHosts.details("rubylight-serverinfo.xml");
        StreamMenu.Options state = new StreamMenu.Options(NvApp.Role.STREAM, host);
        state.appName = "Open World";
        state.streamWidth = 2560;
        state.streamHeight = 1440;
        state.streamFps = 144;
        state.videoFormat = 0x200;
        state.hdr = true;
        state.webUiAddress = true;
        state.hostMenu = true;
        state.overlayShown = true;
        state.controllerMouseAvailable = true;
        state.microphoneOffered = true;
        List<KeyboardShortcuts.Shortcut> saved = new ArrayList<>();
        saved.add(new KeyboardShortcuts.Shortcut("a", "Task view", KeyboardShortcuts.parseChord("WIN+TAB")));
        saved.add(new KeyboardShortcuts.Shortcut("b", "Snip", KeyboardShortcuts.parseChord("WIN+SHIFT+S")));
        StreamMenu menu = new StreamMenu(activity, () -> state, chosen::add, null, new StreamMenu.Shortcuts() {
            @Override public List<KeyboardShortcuts.Shortcut> saved() { return new ArrayList<>(saved); }
            @Override public void save(List<KeyboardShortcuts.Shortcut> shortcuts) { }
            @Override public void send(int[] keys) { }
        });
        menu.show();
        ShadowLooper.idleMainLooper(100, TimeUnit.MILLISECONDS);
        return menu;
    }

    private static void capture(Activity activity, StreamMenu menu, String name) throws Exception {
        String output = System.getenv("NIGHTJAR_PREVIEW_OUTPUT");
        if (output == null || output.isBlank()) return;
        File directory = new File(output, "day");
        assertTrue(directory.isDirectory() || directory.mkdirs());
        TvScreensPreviewTest.captureDialog(activity, menu.sheet(), new File(directory, name));
    }

    @Test
    public void focusStartsOnTheFirstTileAndDownWalksTheGridToTheWaysOut() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            List<StreamMenu.Item> chosen = new ArrayList<>();
            StreamMenu menu = open(activity, chosen);
            // A remote's press leaves the sheet's own window in focus mode before its first focus lands.
            Object root = ReflectionHelpers.callInstanceMethod(menu.sheet().getWindow().getDecorView(), "getViewRootImpl");
            ReflectionHelpers.callInstanceMethod(root, "ensureTouchMode", ReflectionHelpers.ClassParameter.from(boolean.class, false));
            layOut(menu);
            // What the sheet does once it is attached, or a page is shown, outside touch mode.
            menu.sheet().focusFirstAction();
            View focused = menu.sheet().getWindow().getDecorView().findFocus();
            assertEquals(ControlCenterModel.DOCK, menu.sheet().placement());
            assertSame("focus starts on the first card, not on Disconnect", menu.row(KEYBOARD), focused);
            assertSame(menu.grid().getChildAt(0), focused);
            assertEquals("the header says the stream's mode, as desktop Nightjar's", "1440p · 144 FPS · HEVC HDR",
                    menu.sheet().subtitle().getText().toString());
            assertSame("Right is the next card", menu.grid().getChildAt(1), focused.focusSearch(View.FOCUS_RIGHT));
            assertSame("Left from the first column is the rail, on the module that shows",
                    menu.sheet().railEntry(ControlCenterModel.QUICK), focused.focusSearch(View.FOCUS_LEFT));

            View next = focused;
            boolean passedReturn = false;
            for (int row = 0; row < 16 && next != null && next != menu.row(DISCONNECT); row++) {
                // As a remote's press does, the page scrolls the focused card into view before the next move.
                assertTrue(next.requestFocus());
                passedReturn |= next == menu.row(CLOSE);
                next = next.focusSearch(View.FOCUS_DOWN);
            }
            assertSame("Down through the cards and rows reaches Disconnect", menu.row(DISCONNECT), next);
            assertTrue("past Return to stream, the first control under the page", passedReturn);
            assertTrue(focused.requestFocus());
            ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
            menu.row(KEYBOARD).jumpDrawablesToCurrentState();
            capture(activity, menu, "tv-stream-menu.png");

            menu.sheet().onBackPressed();
            assertFalse(menu.sheet().isShowing());
            assertTrue("Back closes the menu and nothing else", chosen.isEmpty());
        }
    }

    /**
     * A handheld such as the Retroid Pocket held sideways: the dock at its compact width against the end edge, its cards
     * two to a row at the design's height, the ways out in sight under them; then the shortcuts page.
     */
    @Test @Config(qualifiers = "w640dp-h360dp-land-xxhdpi")
    public void aHandheldShowsTheCompactDockWithItsCardsTwoToARow() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            StreamMenu menu = open(activity, new ArrayList<>());
            layOut(menu);
            android.content.res.Resources res = activity.getResources();
            assertEquals(ControlCenterModel.DOCK, menu.sheet().placement());
            View panel = menu.sheet().panel();
            assertEquals(res.getDimensionPixelSize(R.dimen.nj_size_control_center_width_compact), panel.getWidth());
            assertEquals("against the end edge, its margin clear of it",
                    1920 - res.getDimensionPixelSize(R.dimen.control_center_margin), panel.getRight());
            assertTrue("the picture stays in sight beside it", panel.getLeft() >= 1920 / 3);
            assertTrue("inside the window", panel.getTop() >= 0 && panel.getBottom() <= 1080);
            assertEquals(2, menu.grid().columns());
            float density = res.getDisplayMetrics().density;
            View tile = menu.row(KEYBOARD);
            int card = res.getDimensionPixelSize(R.dimen.nj_size_control_center_card);
            assertTrue("a card keeps the design's height: " + tile.getHeight() / density + "dp",
                    tile.getHeight() >= card && tile.getHeight() <= card + 24 * density);
            // Six modules on a dock 336 dp tall: the rail shows them all, each still a touch target, before it would scroll.
            View lastModule = menu.sheet().railEntry(menu.modules().get(menu.modules().size() - 1));
            assertTrue("the rail shows every module: " + lastModule.getBottom() / density + "dp",
                    lastModule.getBottom() <= ((View) lastModule.getParent().getParent()).getHeight());
            assertTrue("a module is a touch target", lastModule.getHeight() >= 48 * density);
            assertTrue("Return to stream is in sight", menu.sheet().primary().isShown()
                    && menu.sheet().primary().getHeight() >= 48 * density);
            assertEquals("Keyboard", ((android.widget.TextView) tile).getText().toString());
            assertNoLabelCut(menu);
            capture(activity, menu, "handheld-stream-menu.png");
            menu.reveal(SHORTCUTS).performClick();
            layOut(menu);
            capture(activity, menu, "handheld-stream-shortcuts.png");
            menu.sheet().dismiss();
        }
    }

    /**
     * A card, the lit module and Return to stream paint their own colours. A MaterialButton hands a background that is not
     * its own the tint of its style, clear at rest: that once painted every card and the amber capsule away.
     */
    @Test
    public void theCardsTheLitModuleAndReturnToStreamPaintTheirOwnColours() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            StreamMenu menu = open(activity, new ArrayList<>());
            layOut(menu);
            int moon = androidx.core.content.ContextCompat.getColor(activity, R.color.nj_color_moon);
            assertEquals("Return to stream is the amber capsule", moon, middle(menu.sheet().primary()));
            assertNotEquals("a card at rest has its tint", 0, android.graphics.Color.alpha(middle(menu.row(KEYBOARD))));
            assertNotEquals("the lit module has its container", 0,
                    android.graphics.Color.alpha(middle(menu.sheet().railEntry(ControlCenterModel.QUICK))));
            menu.sheet().dismiss();
        }
    }

    // What a view's background paints at its middle, in the bounds the view gives it when it draws.
    private static int middle(View view) {
        android.graphics.Bitmap bitmap = android.graphics.Bitmap.createBitmap(view.getWidth(), view.getHeight(),
                android.graphics.Bitmap.Config.ARGB_8888);
        android.graphics.drawable.Drawable background = view.getBackground();
        background.setBounds(0, 0, view.getWidth(), view.getHeight());
        background.draw(new android.graphics.Canvas(bitmap));
        return bitmap.getPixel(view.getWidth() / 2, view.getHeight() / 2);
    }

    /** German words run longest: every tile and the ways out keep their whole label. */
    @Test @Config(qualifiers = "de-w640dp-h360dp-land-xxhdpi")
    public void germanLabelsFitTheirTiles() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup().visible()) {
            Activity activity = controller.get();
            StreamMenu menu = open(activity, new ArrayList<>());
            layOut(menu);
            assertEquals("Tastatur", ((android.widget.TextView) menu.row(KEYBOARD)).getText().toString());
            assertNoLabelCut(menu);
            capture(activity, menu, "handheld-stream-menu-de.png");
            menu.sheet().dismiss();
        }
    }

    /** Each label key of the shared model resolves to a non-empty Android string. */
    @Test
    public void everyModelLabelKeyHasItsEnglishString() throws Exception {
        try (var controller = Robolectric.buildActivity(Activity.class).setup()) {
            java.util.Map<String, String> labels = StreamMenuModel.labels();
            assertTrue(labels.size() > 20);
            for (String key : new java.util.LinkedHashSet<>(labels.values())) {
                int id = StreamMenu.tileLabel(key);
                assertNotEquals("no string for " + key, 0, id);
                assertFalse(key, controller.get().getString(id).trim().isEmpty());
            }
        }
    }

    private static void assertNoLabelCut(StreamMenu menu) {
        List<android.widget.TextView> labels = new ArrayList<>();
        for (int i = 0; i < menu.grid().getChildCount(); i++) labels.add((android.widget.TextView) menu.grid().getChildAt(i));
        View footer = (View) menu.row(DISCONNECT).getParent();
        for (int i = 0; i < ((android.view.ViewGroup) footer).getChildCount(); i++) {
            labels.add((android.widget.TextView) ((android.view.ViewGroup) footer).getChildAt(i));
        }
        labels.add(menu.sheet().primary());
        for (String module : menu.modules()) labels.add(menu.sheet().railEntry(module));
        for (android.widget.TextView label : labels) {
            android.text.Layout layout = label.getLayout();
            assertNotNull(label.getText().toString(), layout);
            for (int line = 0; line < layout.getLineCount(); line++) {
                assertEquals("\"" + label.getText() + "\" is cut", 0, layout.getEllipsisCount(line));
            }
            assertTrue("\"" + label.getText() + "\" runs past its lines", layout.getLineCount() <= label.getMaxLines());
        }
    }

    // The sheet's window draws no frames here: measure and lay it out at 1080p so focus search sees the rows.
    private static void layOut(StreamMenu menu) {
        ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
        View sheet = menu.sheet().getWindow().getDecorView();
        sheet.measure(View.MeasureSpec.makeMeasureSpec(1920, View.MeasureSpec.AT_MOST),
                View.MeasureSpec.makeMeasureSpec(1080, View.MeasureSpec.AT_MOST));
        sheet.layout(0, 0, sheet.getMeasuredWidth(), sheet.getMeasuredHeight());
        ShadowLooper.idleMainLooper(400, TimeUnit.MILLISECONDS);
    }
}
