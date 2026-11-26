package com.limelight.ui;

import static org.junit.Assert.assertEquals;
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

/**
 * A PC's display is drawn in one ink: a face that is lit from its top corner, a rim, and no colour of its own; fainter and flat
 * while the PC is away, and only the solid rim in high contrast.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w412dp-h915dp-port-night-mdpi")
public class PcScreenTest {
    private static final int W = 200;

    private static Bitmap drawn(PcScreen screen) {
        screen.measure(View.MeasureSpec.makeMeasureSpec(W, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED));
        screen.layout(0, 0, screen.getMeasuredWidth(), screen.getMeasuredHeight());
        Bitmap out = Bitmap.createBitmap(screen.getMeasuredWidth(), screen.getMeasuredHeight(), Bitmap.Config.ARGB_8888);
        screen.draw(new Canvas(out));
        return out;
    }

    /** The spread between a pixel's colour channels: an ink has next to none. */
    private static int hue(int color) {
        int red = (color >> 16) & 0xFF;
        int green = (color >> 8) & 0xFF;
        int blue = color & 0xFF;
        return Math.max(red, Math.max(green, blue)) - Math.min(red, Math.min(green, blue));
    }

    @Test
    public void theInkKeepsItsColourAtAShareOfItsStrength() {
        assertEquals(0x80F5F7FA, PcScreen.ink(0xFFF5F7FA, .5f));
        assertEquals(0x00123456, PcScreen.ink(0xFF123456, 0f));
    }

    @Test
    public void theFaceIsOneInkLitFromItsTopCorner() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            PcScreen screen = new PcScreen(controller.get());
            Bitmap picture = drawn(screen);
            int h = screen.faceHeight();
            int lit = picture.getPixel(W / 6, h / 6);
            int far = picture.getPixel(W * 5 / 6, h * 5 / 6);
            assertTrue("the face is drawn", (lit >>> 24) > 0 && (far >>> 24) > 0);
            assertTrue("stronger where the light lands", (lit >>> 24) > (far >>> 24));
            // A faint pixel reads back coarsely, so the look is taken where the face is strongest.
            for (int x = W / 8; x <= W / 2; x += W / 8) {
                int pixel = picture.getPixel(x, h / 6);
                assertTrue("no colour of its own at " + x + ": " + Integer.toHexString(pixel), hue(pixel) <= 24);
            }
            assertTrue("the rim is stronger than the face", (picture.getPixel(W / 2, 0) >>> 24) > (lit >>> 24));
            assertTrue("the stand is under the face", (picture.getPixel(W / 2, picture.getHeight() - 2) >>> 24) > 0);
        }
    }

    @Test
    public void anAwayPcIsFainterAndFlat() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            PcScreen screen = new PcScreen(controller.get());
            int h = Math.round(W * .62f);
            int here = drawn(screen).getPixel(W / 6, h / 6);
            screen.setAway(true);
            Bitmap away = drawn(screen);
            assertTrue((away.getPixel(W / 6, h / 6) >>> 24) < (here >>> 24));
            assertEquals("flat", away.getPixel(W / 6, h / 6) >>> 24, away.getPixel(W * 5 / 6, h * 5 / 6) >>> 24);
        }
    }

    @Test
    public void highContrastDrawsTheRimAndNoFace() {
        PreferenceManager.getDefaultSharedPreferences(RuntimeEnvironment.getApplication())
                .edit().putBoolean(AppTheme.HIGH_CONTRAST, true).commit();
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            PcScreen screen = new PcScreen(controller.get());
            Bitmap picture = drawn(screen);
            assertEquals("no fill", 0, picture.getPixel(W / 2, screen.faceHeight() / 2) >>> 24);
            assertTrue("a solid rim", (picture.getPixel(W / 2, 1) >>> 24) > 200);
        }
    }
}
