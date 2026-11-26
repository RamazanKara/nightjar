package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import android.app.Application;
import android.graphics.Bitmap;
import android.graphics.Canvas;
import android.view.View;

import com.limelight.R;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.GraphicsMode;

/**
 * The QR camera's viewfinder: a square in the middle of the picture marked by four corner brackets and nothing between
 * them, in the rim colour until a code is recognised and in the accent after.
 */
@RunWith(RobolectricTestRunner.class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = 34, application = Application.class, qualifiers = "w412dp-h915dp-port-night-mdpi")
public class ViewfinderFrameTest {
    private static final int W = 412;
    private static final int H = 600;

    private static Bitmap drawn(ViewfinderFrame frame) {
        frame.measure(View.MeasureSpec.makeMeasureSpec(W, View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec(H, View.MeasureSpec.EXACTLY));
        frame.layout(0, 0, W, H);
        Bitmap out = Bitmap.createBitmap(W, H, Bitmap.Config.ARGB_8888);
        frame.draw(new Canvas(out));
        return out;
    }

    private static boolean amber(int color) {
        int red = (color >> 16) & 0xFF;
        int blue = color & 0xFF;
        return (color >>> 24) > 200 && red > 200 && blue < 140;
    }

    private static boolean pale(int color) {
        int red = (color >> 16) & 0xFF;
        int blue = color & 0xFF;
        return (color >>> 24) > 200 && red > 200 && blue > 200;
    }

    @Test
    public void theSquareStandsInTheMiddleAndKeepsClearOfTheEdges() {
        float[] wide = ViewfinderFrame.square(1000, 600, 32f, 280f);
        assertEquals(280f, wide[2], 0f);
        assertEquals(360f, wide[0], 0f);
        assertEquals(160f, wide[1], 0f);
        float[] small = ViewfinderFrame.square(300, 500, 32f, 280f);
        assertEquals("the nearer edges decide", 236f, small[2], 0f);
        assertEquals(32f, small[0], 0f);
        assertEquals(132f, small[1], 0f);
        assertEquals("no room, no square", 0f, ViewfinderFrame.square(40, 500, 32f, 280f)[2], 0f);
    }

    @Test
    public void fourCornersAreMarkedAndNothingBetweenThem() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            ViewfinderFrame frame = new ViewfinderFrame(controller.get());
            Bitmap picture = drawn(frame);
            float[] square = ViewfinderFrame.square(W, H, 32f, 280f);
            int left = Math.round(square[0]);
            int top = Math.round(square[1]);
            int right = Math.round(square[0] + square[2]);
            int bottom = Math.round(square[1] + square[2]);
            int along = Math.round(square[2] * ViewfinderFrame.ARM * .8f);
            assertTrue("top left", pale(picture.getPixel(left + along, top)));
            assertTrue(pale(picture.getPixel(left, top + along)));
            assertTrue("top right", pale(picture.getPixel(right - along, top)));
            assertTrue(pale(picture.getPixel(right, top + along)));
            assertTrue("bottom right", pale(picture.getPixel(right - along, bottom)));
            assertTrue(pale(picture.getPixel(right, bottom - along)));
            assertTrue("bottom left", pale(picture.getPixel(left + along, bottom)));
            assertTrue(pale(picture.getPixel(left, bottom - along)));
            assertEquals("no line along the top between the corners", 0, picture.getPixel(W / 2, top) >>> 24);
            assertEquals("nor down a side", 0, picture.getPixel(left, H / 2) >>> 24);
            assertEquals("the picture shows through the middle", 0, picture.getPixel(W / 2, H / 2) >>> 24);
            assertFalse(frame.isRecognised());
        }
    }

    @Test
    public void aRecognisedCodeLightsTheCornersInTheAccent() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            ViewfinderFrame frame = new ViewfinderFrame(controller.get());
            float[] square = ViewfinderFrame.square(W, H, 32f, 280f);
            int x = Math.round(square[0] + square[2] * ViewfinderFrame.ARM * .8f);
            int y = Math.round(square[1]);
            assertFalse(amber(drawn(frame).getPixel(x, y)));
            frame.setRecognised(true);
            assertTrue(frame.isRecognised());
            int lit = drawn(frame).getPixel(x, y);
            assertTrue(Integer.toHexString(lit), amber(lit));
            assertEquals(controller.get().getColor(R.color.nj_color_moon), lit);
            frame.setRecognised(false);
            assertTrue(pale(drawn(frame).getPixel(x, y)));
        }
    }

    @Test
    public void aScreenReaderPassesIt() {
        try (var controller = Robolectric.buildActivity(MotionTest.Screen.class).setup()) {
            assertEquals(View.IMPORTANT_FOR_ACCESSIBILITY_NO, new ViewfinderFrame(controller.get()).getImportantForAccessibility());
        }
    }
}
