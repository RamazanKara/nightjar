package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.view.View;

import android.preference.PreferenceManager;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

/**
 * The stage behind a page without art: light from above the top edge that fades with no edge and no band, the page's own colour
 * at the foot, and each look's own version of it. Nothing on it is a disc, a ring or a point.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class)
public class StageViewTest {
    private static Bitmap drawn(StageView stage, int w, int h) {
        stage.measure(View.MeasureSpec.makeMeasureSpec(w, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(h, View.MeasureSpec.EXACTLY));
        stage.layout(0, 0, w, h);
        Bitmap out = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888);
        stage.draw(new Canvas(out));
        return out;
    }

    private static int luma(int color) {
        return (((color >> 16) & 0xFF) * 299 + ((color >> 8) & 0xFF) * 587 + (color & 0xFF) * 114) / 1000;
    }

    /** The largest step between two neighbours anywhere on the picture, in levels of one channel. */
    private static int largestStep(Bitmap picture) {
        int largest = 0;
        for (int y = 0; y < picture.getHeight(); y++) {
            for (int x = 0; x < picture.getWidth(); x++) {
                int at = picture.getPixel(x, y);
                if (x + 1 < picture.getWidth()) largest = Math.max(largest, step(at, picture.getPixel(x + 1, y)));
                if (y + 1 < picture.getHeight()) largest = Math.max(largest, step(at, picture.getPixel(x, y + 1)));
            }
        }
        return largest;
    }

    private static int step(int a, int b) {
        return Math.max(Math.abs(((a >> 16) & 0xFF) - ((b >> 16) & 0xFF)),
                Math.max(Math.abs(((a >> 8) & 0xFF) - ((b >> 8) & 0xFF)), Math.abs((a & 0xFF) - (b & 0xFF))));
    }

    private static void assertNear(String what, int expected, int actual) {
        assertTrue(what + ": " + Integer.toHexString(actual) + " is not " + Integer.toHexString(expected), step(expected, actual) <= 2);
    }

    @Test
    public void theLightsMiddleIsAboveTheWindowAndItEndsAboveTheFoot() {
        for (int[] size : new int[][] {{412, 915}, {915, 412}, {1920, 1080}, {1280, 800}, {600, 600}}) {
            float[] light = StageView.light(size[0], size[1]);
            assertEquals("from the centre", size[0] / 2f, light[0], 0f);
            assertTrue("the brightest point is never on screen", light[1] < 0f);
            assertTrue("the foot stands on the page's own colour", light[1] + light[3] < size[1] * .75f);
            assertTrue("it is wide: a pool, not a cone", light[2] >= light[3] * .65f);
        }
        assertTrue("held upright it is wider than the phone", StageView.light(412, 915)[2] > 412 / 2f);
    }

    @Test
    public void theLightFallsOffWithoutAnEdge() {
        assertEquals(1f, StageView.falloff(0f), 0f);
        assertEquals(0f, StageView.falloff(1f), 0f);
        assertEquals(0f, StageView.falloff(1.5f), 0f);
        float before = 1f;
        for (int i = 1; i <= 256; i++) {
            float at = StageView.falloff(i / 256f);
            assertTrue("it only gets darker", at <= before);
            assertTrue("no step: " + (before - at) + " at " + i, before - at < .012f);
            before = at;
        }
        assertTrue("it is nearly gone before the rim, so the rim cannot show", StageView.falloff(.9f) < .03f);
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-night-mdpi")
    public void atNightTheLightComesFromTheTopCentreAndThePageTakesOverBelow() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            StageView stage = new StageView(controller.get());
            Bitmap page = drawn(stage, 412, 915);
            int top = luma(page.getPixel(206, 0));
            assertTrue("lit at the top centre", top > luma(page.getPixel(206, 300)) + 8);
            assertTrue("less at the corner", top > luma(page.getPixel(2, 0)));
            assertNear("the foot is the page's colour", controller.get().getColor(R.color.nj_sky_bottom), page.getPixel(206, 914));
            assertNear("from edge to edge", controller.get().getColor(R.color.nj_sky_bottom), page.getPixel(2, 914));
            // Down the middle it only gets darker: the brightest point is the top edge, so there is no disc.
            int before = top;
            for (int y = 1; y < 915; y++) {
                int at = luma(page.getPixel(206, y));
                assertTrue("brighter again at " + y, at <= before + 1);
                before = Math.min(before, at);
            }
            assertTrue("no edge and no band anywhere: " + largestStep(page), largestStep(page) <= 2);
            assertEquals("opaque: one draw covers the window", 0xFF, page.getPixel(100, 500) >>> 24);
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-night-mdpi")
    public void heldSidewaysTheLightStaysInTheMiddleOfTheWidth() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            Bitmap page = drawn(new StageView(controller.get()), 960, 540);
            int middle = luma(page.getPixel(480, 40));
            assertTrue(middle > luma(page.getPixel(40, 40)) + 6);
            assertEquals("the same on both sides", luma(page.getPixel(40, 40)), luma(page.getPixel(919, 40)), 1);
            assertTrue("no edge and no band: " + largestStep(page), largestStep(page) <= 2);
        }
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-notnight-mdpi")
    public void byDayThePageIsLightUnderAFaintCoolLight() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            assertEquals(AppTheme.Look.DAY, AppTheme.look(controller.get()));
            Bitmap page = drawn(new StageView(controller.get()), 412, 915);
            assertNear("the day page at the foot", controller.get().getColor(R.color.nj_day_sky_bottom), page.getPixel(206, 914));
            assertTrue("a light page", luma(page.getPixel(206, 0)) > 240);
            assertTrue(largestStep(page) <= 2);
        }
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-night-mdpi")
    public void onOledThePageIsBlackUnderAFainterLight() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putString(AppTheme.THEME, AppTheme.THEME_OLED_BLACK).commit();
        int night;
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            assertEquals(AppTheme.Look.OLED, AppTheme.look(controller.get()));
            Bitmap page = drawn(new StageView(controller.get()), 412, 915);
            assertEquals("pixels that are off", 0xFF000000, page.getPixel(206, 914));
            assertEquals(0xFF000000, page.getPixel(206, 500));
            night = luma(page.getPixel(206, 0));
            assertTrue("still a stage: the top is lit", night > 6);
            assertTrue(largestStep(page) <= 2);
        }
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication()).edit().remove(AppTheme.THEME).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            Bitmap page = drawn(new StageView(controller.get()), 412, 915);
            assertTrue("fainter than the night page's", night < luma(page.getPixel(206, 0)));
        }
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-night-mdpi")
    public void inHighContrastItIsSolidBlack() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            Bitmap page = drawn(new StageView(controller.get()), 412, 915);
            for (int y = 0; y < 915; y += 5) {
                for (int x = 0; x < 412; x += 5) assertEquals(0xFF000000, page.getPixel(x, y));
            }
        }
    }

    @Test
    @Config(qualifiers = "w960dp-h540dp-land-television-night-mdpi")
    public void aTelevisionHasItsDarkerCanvasUnderAFainterLight() {
        int tv;
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            assertEquals(GlassForm.TELEVISION, GlassForm.of(controller.get()));
            Bitmap page = drawn(new StageView(controller.get()), 960, 540);
            assertNear("the TV canvas at the foot", controller.get().getColor(R.color.nj_color_canvas_tv), page.getPixel(480, 539));
            tv = luma(page.getPixel(480, 0));
            assertTrue("lit from the top", tv > luma(page.getPixel(480, 539)) + 6);
            assertTrue(largestStep(page) <= 2);
        }
        assertTrue("under the night page's light", tv < 40);
    }

    @Test
    @Config(qualifiers = "w412dp-h915dp-port-night-mdpi")
    public void theBakedGlassGetsASmallPictureOfTheSameStage() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            StageView stage = new StageView(controller.get());
            Bitmap baked = stage.bake(412, 915);
            assertNotNull(baked);
            assertNotEquals(412, baked.getWidth());
            assertTrue("lit at the top as the page is", luma(baked.getPixel(baked.getWidth() / 2, 0))
                    > luma(baked.getPixel(baked.getWidth() / 2, baked.getHeight() - 1)) + 8);
            assertEquals(null, stage.bake(0, 915));
        }
    }
}
