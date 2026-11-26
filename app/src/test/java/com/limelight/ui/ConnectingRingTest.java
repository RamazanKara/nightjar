package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.view.View;

import android.preference.PreferenceManager;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

/** The ring around a connecting cover: one thin track, one arc of light with its head at the top, and nothing when it is bare. */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w412dp-h915dp-port-night-mdpi")
public class ConnectingRingTest {
    private static final int SIZE = 280;

    private static Bitmap drawn(ConnectingRing ring) {
        ring.measure(View.MeasureSpec.makeMeasureSpec(SIZE, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(SIZE, View.MeasureSpec.EXACTLY));
        ring.layout(0, 0, SIZE, SIZE);
        Bitmap out = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888);
        ring.draw(new Canvas(out));
        return out;
    }

    /** The strongest alpha across the ring's stroke on the line from the middle at {@code degrees} (0 is the top, clockwise). */
    private static int inkAt(Bitmap picture, float radius, float degrees) {
        int most = 0;
        for (float r = radius - 2f; r <= radius + 2f; r += .5f) {
            int x = Math.round(SIZE / 2f + r * (float) Math.sin(Math.toRadians(degrees)));
            int y = Math.round(SIZE / 2f - r * (float) Math.cos(Math.toRadians(degrees)));
            most = Math.max(most, picture.getPixel(x, y) >>> 24);
        }
        return most;
    }

    @Test
    public void theRingKeepsItsStrokeInsideTheView() {
        assertEquals(136f, ConnectingRing.radius(280, 280, 4f), 0f);
        assertEquals("the shorter side decides", 96f, ConnectingRing.radius(280, 200, 4f), 0f);
        assertEquals(0f, ConnectingRing.radius(0, 0, 4f), 0f);
    }

    @Test
    public void aFaintTrackAndAnArcThatEndsBrightestAtTheTop() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            ConnectingRing ring = new ConnectingRing(controller.get());
            Bitmap picture = drawn(ring);
            float radius = ConnectingRing.radius(SIZE, SIZE, 4f);
            int track = inkAt(picture, radius, 180f);
            assertTrue("a whole ring is there, faint", track > 0 && track < 60);
            assertEquals("the same all round, away from the arc", track, inkAt(picture, radius, 90f), 2);
            int head = inkAt(picture, radius, -2f);
            assertTrue("the arc's head is ink", head > 200);
            int tail = inkAt(picture, radius, -ConnectingRing.ARC_DEGREES + 6f);
            assertTrue("its tail fades into the track", tail < head / 3);
            assertTrue(inkAt(picture, radius, -ConnectingRing.ARC_DEGREES / 2f) > tail);
            assertEquals("nothing in the middle, where the cover is", 0, picture.getPixel(SIZE / 2, SIZE / 2) >>> 24);
        }
    }

    @Test
    public void bareItDrawsNothing() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            ConnectingRing ring = new ConnectingRing(controller.get());
            ring.setBare(true);
            Bitmap picture = drawn(ring);
            float radius = ConnectingRing.radius(SIZE, SIZE, 4f);
            for (float at = 0f; at < 360f; at += 15f) assertEquals(0, inkAt(picture, radius, at));
        }
    }

    @Test
    public void inHighContrastTheArcIsSolid() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            Bitmap picture = drawn(new ConnectingRing(controller.get()));
            float radius = ConnectingRing.radius(SIZE, SIZE, 4f);
            assertTrue(inkAt(picture, radius, -ConnectingRing.ARC_DEGREES + 6f) > 200);
            assertTrue("and the track is the rim, plain to see", inkAt(picture, radius, 180f) > 100);
        }
    }

    @Test
    public void itTurnsOnlyWhileItIsShownAndMotionIsOn() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            ConnectingRing ring = new ConnectingRing(controller.get());
            assertFalse("not before it is asked to", ring.isTurning());
            ring.setMoving(true);
            assertFalse("not off screen", ring.isTurning());
            controller.get().setContentView(ring);
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertTrue(ring.isTurning());
            ring.setVisibility(View.INVISIBLE);
            assertFalse("a hidden ring costs nothing", ring.isTurning());
            ring.setVisibility(View.VISIBLE);
            ring.setMoving(false);
            assertFalse(ring.isTurning());
        }
    }

    @Test
    public void withReducedMotionItRests() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putString(Motion.REDUCE_MOTION, Motion.REDUCE).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            ConnectingRing ring = new ConnectingRing(controller.get());
            controller.get().setContentView(ring);
            ring.setMoving(true);
            org.robolectric.shadows.ShadowLooper.idleMainLooper();
            assertFalse(ring.isTurning());
        }
    }
}
