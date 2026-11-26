package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.preference.PreferenceManager;
import android.view.View;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.RuntimeEnvironment;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;
import org.robolectric.shadows.ShadowLooper;

/**
 * First run's scan pulse: three rings a third of a loop apart, each out of nothing and gone at the rim; still rings when it
 * does not run, one plain ring in high contrast, and no motion unless it is asked for, shown and allowed.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w412dp-h915dp-port-night-mdpi")
public class ScanPulseViewTest {
    private static final int SIZE = 240;

    private static Bitmap drawn(ScanPulseView pulse) {
        pulse.measure(View.MeasureSpec.makeMeasureSpec(SIZE, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(SIZE, View.MeasureSpec.EXACTLY));
        pulse.layout(0, 0, SIZE, SIZE);
        Bitmap out = Bitmap.createBitmap(SIZE, SIZE, Bitmap.Config.ARGB_8888);
        pulse.draw(new Canvas(out));
        return out;
    }

    /** How many separate rings a line from the middle to the right edge crosses, and the strongest alpha on it. */
    private static int[] crossed(Bitmap picture) {
        int rings = 0;
        int most = 0;
        boolean inside = false;
        for (int x = SIZE / 2; x < SIZE; x++) {
            int alpha = picture.getPixel(x, SIZE / 2) >>> 24;
            most = Math.max(most, alpha);
            if (alpha > 8 && !inside) rings++;
            inside = alpha > 8;
        }
        return new int[] {rings, most};
    }

    @Test
    public void theRingsLeaveAThirdOfALoopApart() {
        assertEquals(0f, ScanPulseView.phase(0f, 0), 0f);
        assertEquals(1f / 3f, ScanPulseView.phase(0f, 1), 1e-6f);
        assertEquals(2f / 3f, ScanPulseView.phase(0f, 2), 1e-6f);
        assertEquals("the loop wraps", .1f, ScanPulseView.phase(.767f, 1), 1e-3f);
        assertEquals("slow: one ring's way takes 2.4 s", 2400, ScanPulseView.LOOP_MS);
        for (float time = 0f; time < 1f; time += .01f) {
            float first = ScanPulseView.phase(time, 0);
            float second = ScanPulseView.phase(time, 1);
            float apart = Math.abs(first - second);
            assertEquals(1f / 3f, Math.min(apart, 1f - apart), 1e-4f);
        }
    }

    @Test
    public void aRingGrowsFromThePointToTheRimAndNeverPastIt() {
        assertEquals(0f, ScanPulseView.radius(0f, 100f), 0f);
        assertEquals(100f, ScanPulseView.radius(1f, 100f), 0f);
        float before = 0f;
        for (int i = 1; i <= 100; i++) {
            float at = ScanPulseView.radius(i / 100f, 100f);
            assertTrue("only outward", at > before);
            assertTrue(at <= 100f);
            before = at;
        }
        assertTrue("faster out of the point than toward the rim",
                ScanPulseView.radius(.2f, 100f) - ScanPulseView.radius(.1f, 100f) > ScanPulseView.radius(.9f, 100f) - ScanPulseView.radius(.8f, 100f));
        assertEquals("the stroke stays inside the view", 118f, ScanPulseView.rim(240, 300, 2f), 0f);
        assertEquals(0f, ScanPulseView.rim(0, 0, 2f), 0f);
    }

    @Test
    public void aRingComesOutOfNothingAndIsGoneAtTheRim() {
        assertEquals(0f, ScanPulseView.strength(0f), 0f);
        assertEquals(0f, ScanPulseView.strength(1f), 0f);
        float most = 0f;
        float before = 0f;
        for (int i = 1; i < 100; i++) {
            float at = ScanPulseView.strength(i / 100f);
            assertTrue("no jump: a ring neither pops in nor out", Math.abs(at - before) < .03f);
            most = Math.max(most, at);
            before = at;
        }
        assertEquals(ScanPulseView.PEAK, most, .005f);
        assertTrue("the last step to nothing is small too", before < .03f);
    }

    @Test
    public void atRestThreeRingsStandStill() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            ScanPulseView pulse = new ScanPulseView(controller.get());
            int[] crossed = crossed(drawn(pulse));
            assertEquals(3, crossed[0]);
            assertTrue("thin and faint: " + crossed[1], crossed[1] > 30 && crossed[1] <= Math.round(255 * ScanPulseView.PEAK) + 2);
            assertFalse(pulse.isPulsing());
        }
    }

    @Test
    public void inHighContrastItIsOnePlainRing() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            ScanPulseView pulse = new ScanPulseView(controller.get());
            controller.get().setContentView(pulse);
            pulse.setScanning(true);
            ShadowLooper.idleMainLooper();
            assertFalse("and it does not move", pulse.isPulsing());
            int[] crossed = crossed(drawn(pulse));
            assertEquals(1, crossed[0]);
            assertTrue(crossed[1] > 200);
        }
    }

    @Test
    public void itMovesOnlyWhileItScansAndIsShown() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            ScanPulseView pulse = new ScanPulseView(controller.get());
            pulse.setScanning(true);
            assertFalse("not off screen", pulse.isPulsing());
            controller.get().setContentView(pulse);
            ShadowLooper.idleMainLooper();
            assertTrue(pulse.isPulsing());
            pulse.setVisibility(View.GONE);
            assertFalse("a PC was found: the page with the pulse is gone", pulse.isPulsing());
            pulse.setVisibility(View.VISIBLE);
            assertTrue(pulse.isPulsing());
            pulse.setScanning(false);
            assertFalse("discovery stopped", pulse.isPulsing());
        }
    }

    @Test
    public void withReducedMotionItStandsStill() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putString(Motion.REDUCE_MOTION, Motion.REDUCE).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup().visible()) {
            ScanPulseView pulse = new ScanPulseView(controller.get());
            controller.get().setContentView(pulse);
            pulse.setScanning(true);
            ShadowLooper.idleMainLooper();
            assertFalse(pulse.isPulsing());
            assertEquals("the three rings are there, at rest", 3, crossed(drawn(pulse))[0]);
        }
    }

    @Test
    public void thePulseTakesTheRoomItIsGivenUpToItsSizeOrNone() {
        assertEquals("room to spare", 464, ScanPulseView.side(760, 900, 464, 192));
        assertEquals("the height decides", 300, ScanPulseView.side(760, 300, 464, 192));
        assertEquals("the width decides", 250, ScanPulseView.side(250, 900, 464, 192));
        assertEquals("too small to read as a search", 0, ScanPulseView.side(760, 191, 464, 192));
        assertEquals(0, ScanPulseView.side(760, -40, 464, 192));
    }
}
