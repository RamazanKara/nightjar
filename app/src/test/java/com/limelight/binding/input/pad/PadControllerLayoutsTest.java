package com.limelight.binding.input.pad;

import android.app.Application;
import android.content.Context;
import android.content.SharedPreferences;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

import static org.junit.Assert.*;

/**
 * On-screen layouts per physical controller (Rubylight #154): a controller's own layout shows while it is connected,
 * the layout for this game or all games comes back when it goes, and without any controller layout nothing changes.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 30, application = Application.class, qualifiers = "w800dp-h360dp-land-xhdpi")
public class PadControllerLayoutsTest extends ModernControllerHarness {
    private static final String[] XBOX = {"usb:045e:0b13", "Xbox Wireless Controller"};
    private static final String[] DUALSENSE = {"usb:054c:0ce6", "DualSense Wireless Controller"};

    private static List<String[]> connected(String[]... pads) {
        return new ArrayList<>(Arrays.asList(pads));
    }

    private SharedPreferences store() {
        return RuntimeEnvironment.getApplication().getSharedPreferences(ModernController.PREFS, Context.MODE_PRIVATE);
    }

    @Test public void theLastConnectedControllerWithALayoutIsChosen() {
        assertNull(PadControllerLayouts.choose(Collections.emptyList(), m -> true));
        assertNull(PadControllerLayouts.choose(Arrays.asList("a", "b"), m -> false));
        assertEquals("b", PadControllerLayouts.choose(Arrays.asList("a", "b"), m -> true));
        assertEquals("a", PadControllerLayouts.choose(Arrays.asList("a", "b"), "a"::equals));
    }

    @Test public void withoutControllerLayoutsAConnectedPadChangesNothing() {
        ModernController controller = controller(GAME);
        PadLayout shared = controller.layout().copy();
        shared.setScale(PadLayout.Control.FACE, .8f);
        controller.save(shared, false);
        controller.setControllers(connected(XBOX));
        assertEquals(.8f, controller.layout().spec(PadLayout.Control.FACE).scale(), 1e-3);
        assertFalse(ModernController.controllerBringsLayout(RuntimeEnvironment.getApplication(), connected(XBOX)));
        // Nothing of the feature is stored until a controller's layout is saved.
        for (String key : store().getAll().keySet()) {
            assertFalse(key, key.startsWith(PadControllerLayouts.ROOT) || key.startsWith(PadControllerLayouts.NAME));
        }
    }

    @Test public void aControllersLayoutComesAndGoesWithIt() {
        ModernController controller = controller(GAME);
        controller.setControllers(connected(XBOX));
        PadLayout own = controller.layout().copy();
        own.spec(PadLayout.Control.LB).hidden = true;
        controller.save(own, false, XBOX[0]);
        assertEquals("Xbox Wireless Controller", store().getString(PadControllerLayouts.nameKey(XBOX[0]), null));
        assertTrue(ModernController.controllerBringsLayout(RuntimeEnvironment.getApplication(), connected(XBOX)));

        controller.setControllers(connected());
        assertFalse("The shared layout is back", controller.layout().spec(PadLayout.Control.LB).hidden);
        controller.setControllers(connected(XBOX));
        assertTrue("Its own layout returns with it", controller.layout().spec(PadLayout.Control.LB).hidden);
        // A second pad without a layout keeps the first one's; the shared layout was never touched.
        controller.setControllers(connected(XBOX, DUALSENSE));
        assertTrue(controller.layout().spec(PadLayout.Control.LB).hidden);
        controller.destroy();
        assertFalse(controller(GAME).layout().spec(PadLayout.Control.LB).hidden);
    }

    @Test public void aNewControllerWithItsOwnLayoutTakesOver() {
        ModernController controller = controller(GAME);
        controller.setControllers(connected(XBOX));
        PadLayout xbox = controller.layout().copy();
        xbox.setScale(PadLayout.Control.FACE, 1.4f);
        controller.save(xbox, false, XBOX[0]);
        controller.setControllers(connected(DUALSENSE));
        PadLayout dualSense = controller.layout().copy();
        dualSense.setScale(PadLayout.Control.FACE, .6f);
        controller.save(dualSense, false, DUALSENSE[0]);

        controller.setControllers(connected(XBOX, DUALSENSE));
        assertEquals(.6f, controller.layout().spec(PadLayout.Control.FACE).scale(), 1e-3);
        controller.setControllers(connected(DUALSENSE, XBOX));
        assertEquals(1.4f, controller.layout().spec(PadLayout.Control.FACE).scale(), 1e-3);
        assertEquals(2, PadControllerLayouts.saved(store()).size());
    }

    @Test public void savingForAllGamesTakesTheControllersLayoutAway() {
        ModernController controller = controller(GAME);
        controller.setControllers(connected(XBOX));
        PadLayout own = controller.layout().copy();
        own.spec(PadLayout.Control.LB).hidden = true;
        controller.save(own, false, XBOX[0]);
        PadLayout shared = controller.layout().copy();
        controller.save(shared, false);
        assertFalse(PadControllerLayouts.has(store(), XBOX[0]));
        assertNull(store().getString(PadControllerLayouts.nameKey(XBOX[0]), null));
        controller.setControllers(connected());
        assertTrue("The draft went to all games", controller.layout().spec(PadLayout.Control.LB).hidden);
    }

    @Test public void theOtherOrientationIsTheFallback() {
        ModernController controller = controller(GAME);
        controller.setControllers(connected(XBOX));
        PadLayout own = controller.layout().copy();
        own.spec(PadLayout.Control.RB).hidden = true;
        controller.save(own, false, XBOX[0]);
        assertNotNull(PadControllerLayouts.read(store(), false, XBOX[0]));
        assertEquals(store().getString(PadControllerLayouts.key(true, XBOX[0]), null),
                PadControllerLayouts.read(store(), false, XBOX[0]));
    }

    @Test public void resetForgetsControllerLayouts() {
        ModernController controller = controller(GAME);
        controller.setControllers(connected(XBOX));
        controller.save(controller.layout().copy(), false, XBOX[0]);
        ModernController.clearLayouts(RuntimeEnvironment.getApplication());
        assertTrue(PadControllerLayouts.saved(store()).isEmpty());
    }
}
