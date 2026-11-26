package com.limelight.ui;

import android.app.Activity;
import android.app.Application;
import android.content.Intent;
import android.graphics.drawable.Drawable;
import android.os.Looper;
import android.os.PowerManager;
import android.preference.PreferenceManager;
import android.view.View;
import android.view.inspector.WindowInspector;

import androidx.core.content.ContextCompat;

import com.limelight.R;
import com.limelight.nvstream.http.NvApp;
import com.limelight.nvstream.http.TestHosts;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.Shadows;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.shadows.ShadowLooper;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.*;

/**
 * Nightjar #61, what the Control Center costs the stream. Closed it holds no window, no listener and no view. Open it
 * stands on one blur window only where the tier and the system allow it at that moment; the system's blur switch,
 * battery saver or heat drop it to the baked panel at once, the opaque surface first and the window after; closing
 * removes the window and every listener.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class, qualifiers = "w960dp-h540dp-land-xhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
public class ControlCenterSurfaceTest {
    /** The device's signals as a registry the test reads: what each says, and who listens. */
    static final class Registry implements ControlCenterSurface.Signals {
        boolean blur = true, saver, hot;
        Runnable listener;
        int listens, unlistens;

        @Override public boolean blurEnabled() { return blur; }
        @Override public boolean powerSave() { return saver; }
        @Override public boolean thermalSevere() { return hot; }

        @Override public void listen(Runnable changed) {
            listener = changed;
            listens++;
        }

        @Override public void unlisten() {
            listener = null;
            unlistens++;
        }
    }

    private final List<Registry> made = new ArrayList<>();
    private Registry next = new Registry();

    @Before
    public void registry() {
        ControlCenterSurface.signalsForTest = owner -> {
            made.add(next);
            return next;
        };
        MenuBlurWindow.enabledForTest = true;
    }

    @After
    public void forget() {
        ControlCenterSurface.signalsForTest = null;
        MenuBlurWindow.enabledForTest = null;
        highContrast(false);
    }

    private static void highContrast(boolean on) {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit()
                .putBoolean(AppTheme.HIGH_CONTRAST, on).commit();
    }

    private static StreamMenu build(Activity activity) {
        activity.setTheme(R.style.StreamTheme);
        StreamMenu.Options state = new StreamMenu.Options(NvApp.Role.STREAM, TestHosts.details("rubylight-serverinfo.xml"));
        state.appName = "Skybound";
        state.canSendInput = true;
        return new StreamMenu(activity, () -> state, item -> { }, null, null);
    }

    private static void settle() {
        ShadowLooper.idleMainLooper(600, TimeUnit.MILLISECONDS);
    }

    /** Every window of the process, as the window manager counts them. */
    private static int windows() {
        return WindowInspector.getGlobalWindowViews().size();
    }

    private static int views(View root) {
        int count = 1;
        if (root instanceof android.view.ViewGroup) {
            android.view.ViewGroup group = (android.view.ViewGroup) root;
            for (int i = 0; i < group.getChildCount(); i++) count += views(group.getChildAt(i));
        }
        return count;
    }

    @Test
    public void closedItHoldsNoWindowNoListenerAndNoViewAndOpenOneBlurWindowAtItsOwnBounds() {
        try (var controller = Robolectric.buildActivity(StreamGlassTest.Stream.class).setup().visible()) {
            Activity activity = controller.get();
            GlassTier.force(activity, GlassTier.LENS);
            try {
                settle();
                int windows = windows();
                int views = views(activity.getWindow().getDecorView());
                int flags = activity.getWindow().getAttributes().flags;
                boolean quiet = Shadows.shadowOf(Looper.getMainLooper()).isIdle();

                // Made but not shown, as it is before every opening: nothing is asked, added or heard.
                StreamMenu menu = build(activity);
                ControlCenterSurface surface = menu.sheet().surface();
                assertEquals("no window", windows, windows());
                assertTrue("nobody asked the device anything", made.isEmpty());
                assertNull(surface.kind());
                assertNull(surface.blurWindow());
                assertFalse(surface.listening());
                assertEquals("no view on the stream's window", views, views(activity.getWindow().getDecorView()));

                menu.show();
                settle();
                assertEquals(ControlCenterModel.BLURRED, surface.kind());
                MenuBlurWindow blur = surface.blurWindow();
                assertNotNull(blur);
                assertTrue(blur.isShowing());
                assertEquals("the Control Center's window and one blur window", windows + 2, windows());
                assertEquals(1, made.size());
                assertEquals(1, next.listens);
                assertNotNull(next.listener);
                assertEquals("the see-through fill only over a blur",
                        ContextCompat.getColor(activity, R.color.nj_surface_control_center_fill), surface.glass().fillColor());
                assertEquals("the stream's own window gets nothing", views, views(activity.getWindow().getDecorView()));
                // Never the whole screen: the blur lies under the panel, at its bounds.
                View window = menu.sheet().getWindow().getDecorView();
                window.measure(View.MeasureSpec.makeMeasureSpec(1920, View.MeasureSpec.EXACTLY),
                        View.MeasureSpec.makeMeasureSpec(1080, View.MeasureSpec.EXACTLY));
                window.layout(0, 0, 1920, 1080);
                View panel = menu.sheet().panel();
                assertTrue(panel.getWidth() > 0 && panel.getWidth() < 1920 / 2);
                android.view.WindowManager.LayoutParams at = blur.attributes();
                assertEquals(panel.getWidth(), at.width);
                assertEquals(panel.getHeight(), at.height);
                int[] where = new int[2];
                panel.getLocationOnScreen(where);
                assertEquals(where[0], at.x);
                assertEquals(where[1], at.y);
                assertNotEquals("it takes no focus", 0, at.flags & android.view.WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE);
                assertNotEquals("and no touch", 0, at.flags & android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE);

                menu.sheet().dismiss();
                settle();
                assertFalse(menu.sheet().isShowing());
                assertFalse("removed, not hidden", blur.isShowing());
                assertEquals("no window left", windows, windows());
                assertNull(surface.blurWindow());
                assertNull(surface.kind());
                assertFalse(surface.listening());
                assertEquals("every listener is gone", 1, next.unlistens);
                assertNull(next.listener);
                assertEquals("no view left on the stream's window", views, views(activity.getWindow().getDecorView()));
                assertEquals("no flag left on the stream's window", flags, activity.getWindow().getAttributes().flags);
                assertEquals("nothing left running", quiet, Shadows.shadowOf(Looper.getMainLooper()).isIdle());

                // Opened again it asks again, and closes as clean.
                next = new Registry();
                StreamMenu again = build(activity);
                again.show();
                settle();
                assertEquals(windows + 2, windows());
                again.sheet().onBackPressed();
                settle();
                assertEquals(windows, windows());
                assertEquals(1, next.listens);
                assertEquals(1, next.unlistens);
            } finally {
                GlassTier.force(activity, null);
            }
        }
    }

    @Test
    public void eachFallbackShowsTheBakedPanelFirstThenRemovesTheWindowAndStaysBaked() {
        for (String event : new String[] {"blur", "saver", "heat"}) {
            try (var controller = Robolectric.buildActivity(StreamGlassTest.Stream.class).setup().visible()) {
                Activity activity = controller.get();
                GlassTier.force(activity, GlassTier.LENS);
                try {
                    next = new Registry();
                    int windows = windows();
                    StreamMenu menu = build(activity);
                    menu.show();
                    settle();
                    ControlCenterSurface surface = menu.sheet().surface();
                    MenuBlurWindow blur = surface.blurWindow();
                    assertNotNull(event, blur);
                    assertEquals(event, ControlCenterModel.BLURRED, surface.glass().kind());
                    // What the panel is drawn on at each repaint it asks for, and whether the blur still stands under it.
                    List<String> order = new ArrayList<>();
                    surface.glass().setCallback(new Drawable.Callback() {
                        @Override public void invalidateDrawable(Drawable who) {
                            order.add(surface.glass().kind() + (blur.isShowing() ? " over the blur" : " alone"));
                        }

                        @Override public void scheduleDrawable(Drawable who, Runnable what, long when) { }
                        @Override public void unscheduleDrawable(Drawable who, Runnable what) { }
                    });

                    Registry registry = next;
                    if (event.equals("blur")) registry.blur = false;
                    else if (event.equals("saver")) registry.saver = true;
                    else registry.hot = true;
                    registry.listener.run();

                    assertEquals(event, ControlCenterModel.BAKED, surface.kind());
                    assertEquals(event, ControlCenterModel.BAKED, surface.glass().kind());
                    assertEquals(event, ContextCompat.getColor(activity, R.color.nj_surface_control_center_baked_fill),
                            surface.glass().fillColor());
                    assertFalse(event + ": the window is removed", blur.isShowing());
                    assertNull(event, surface.blurWindow());
                    assertFalse(event + ": no flash, the opaque panel came first", order.isEmpty());
                    assertEquals(event, "baked over the blur", order.get(0));
                    assertFalse(event + ": nothing left to listen for", surface.listening());
                    assertNull(event, registry.listener);
                    assertEquals(event, 1, registry.unlistens);
                    assertTrue(event + ": the Control Center stays", menu.sheet().isShowing());
                    assertEquals(event, windows + 1, windows());

                    // The signal coming back changes nothing until it is opened again.
                    registry.blur = true;
                    registry.saver = registry.hot = false;
                    settle();
                    assertEquals(event, ControlCenterModel.BAKED, surface.kind());
                    assertNull(event, surface.blurWindow());

                    menu.sheet().dismiss();
                    settle();
                    assertEquals(event, windows, windows());
                    assertEquals(event + ": closing asks nobody twice", 1, registry.unlistens);
                } finally {
                    GlassTier.force(activity, null);
                }
            }
        }
    }

    @Test
    public void aBakedOrSolidPanelNeverOpensABlurWindowNorListens() {
        String[] cases = {"tier", "switch", "saver", "heat", "contrast"};
        for (String why : cases) {
            try (var controller = Robolectric.buildActivity(StreamGlassTest.Stream.class).setup().visible()) {
                Activity activity = controller.get();
                GlassTier.force(activity, why.equals("tier") ? GlassTier.BAKED : GlassTier.LENS);
                highContrast(why.equals("contrast"));
                try {
                    next = new Registry();
                    next.blur = !why.equals("switch");
                    next.saver = why.equals("saver");
                    next.hot = why.equals("heat");
                    int windows = windows();
                    StreamMenu menu = build(activity);
                    menu.show();
                    settle();
                    ControlCenterSurface surface = menu.sheet().surface();
                    boolean solid = why.equals("contrast");
                    assertEquals(why, solid ? ControlCenterModel.SOLID : ControlCenterModel.BAKED, surface.kind());
                    assertEquals(why, surface.kind(), surface.glass().kind());
                    assertNull(why, surface.blurWindow());
                    assertEquals(why + ": only the Control Center's own window", windows + 1, windows());
                    assertEquals(why + ": no listener", 0, next.listens);
                    assertFalse(why, surface.listening());
                    if (solid) {
                        float density = activity.getResources().getDisplayMetrics().density;
                        assertEquals("solid black", 0xFF000000, surface.glass().fillColor());
                        assertEquals("inside a 2 dp rim", 2 * density, surface.glass().rimWidth(), 0.01f);
                        assertEquals(ContextCompat.getColor(activity, R.color.nj_hc_color_rim), surface.glass().rimTopColor());
                    } else {
                        assertTrue(why + ": opaque enough to read over any game",
                                android.graphics.Color.alpha(surface.glass().fillColor()) >= 0xE0);
                    }
                    menu.sheet().dismiss();
                    settle();
                    assertEquals(why, windows, windows());
                    assertEquals(why, 0, next.unlistens);
                } finally {
                    GlassTier.force(activity, null);
                    highContrast(false);
                }
            }
        }
    }

    /** A window the system refuses, as when the stream is finishing: the blur opened for it goes with it. */
    @Test
    public void aStreamThatIsGoingLeavesNoBlurBehind() {
        try (var controller = Robolectric.buildActivity(StreamGlassTest.Stream.class).setup().visible()) {
            Activity activity = controller.get();
            GlassTier.force(activity, GlassTier.LENS);
            try {
                int windows = windows();
                StreamMenu menu = build(activity);
                activity.finish();
                menu.show();
                settle();
                assertNull("a finishing stream takes no blur window", menu.sheet().surface().blurWindow());
                assertEquals(0, next.listens);
                menu.sheet().dismiss();
                settle();
                assertEquals(windows, windows());
            } finally {
                GlassTier.force(activity, null);
            }
        }
    }

    /** The system's own signals: nothing is registered before listen, and nothing after unlisten. */
    @Test
    public void theSystemsSignalsAreHeardOnlyBetweenListenAndUnlisten() {
        try (var controller = Robolectric.buildActivity(StreamGlassTest.Stream.class).setup().visible()) {
            Activity activity = controller.get();
            Application app = RuntimeEnvironment.getApplication();
            PowerManager power = (PowerManager) app.getSystemService(android.content.Context.POWER_SERVICE);
            int receivers = Shadows.shadowOf(app).getRegisteredReceivers().size();
            ControlCenterSurface.SystemSignals signals = new ControlCenterSurface.SystemSignals(activity);
            assertEquals("asking registers nothing", receivers, Shadows.shadowOf(app).getRegisteredReceivers().size());
            assertTrue(signals.blurEnabled());
            assertFalse(signals.powerSave());
            assertFalse(signals.thermalSevere());

            int[] changes = new int[1];
            signals.listen(() -> changes[0]++);
            assertEquals(receivers + 1, Shadows.shadowOf(app).getRegisteredReceivers().size());
            // The power manager announces the change itself, as the system does.
            Shadows.shadowOf(power).setIsPowerSaveMode(true);
            settle();
            assertTrue(signals.powerSave());
            assertEquals("battery saver is heard", 1, changes[0]);
            Shadows.shadowOf(power).setCurrentThermalStatus(PowerManager.THERMAL_STATUS_SEVERE);
            settle();
            assertTrue(signals.thermalSevere());
            assertEquals("heat is heard", 2, changes[0]);
            Shadows.shadowOf(power).setCurrentThermalStatus(PowerManager.THERMAL_STATUS_MODERATE);
            settle();
            assertFalse("warm is not severe", signals.thermalSevere());

            int heard = changes[0];
            signals.unlisten();
            assertEquals("the receiver is gone", receivers, Shadows.shadowOf(app).getRegisteredReceivers().size());
            activity.sendBroadcast(new Intent(PowerManager.ACTION_POWER_SAVE_MODE_CHANGED));
            Shadows.shadowOf(power).setCurrentThermalStatus(PowerManager.THERMAL_STATUS_CRITICAL);
            settle();
            assertEquals("and nothing is heard any more", heard, changes[0]);
            // Unlistening twice, as a close after a fallback does, is harmless.
            signals.unlisten();
        }
    }
}
