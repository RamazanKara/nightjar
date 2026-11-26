package com.limelight.ui;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

import android.content.res.Configuration;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

import org.junit.Test;

/** The interface's size comes from viewport_scale; the Java mirror matches it where the library is not loaded. */
public class ViewportScalerTest {
    private static Configuration screen(int widthDp, int heightDp) {
        Configuration config = new Configuration();
        config.screenWidthDp = widthDp;
        config.screenHeightDp = heightDp;
        return config;
    }

    @Test
    public void theMirrorMatchesThePolicy() {
        float[][] sizes = {{360, 640}, {393, 852}, {640, 360}, {853, 480}, {1280, 720}, {1280, 800}, {960, 540}, {1920, 1080},
                {961.5f, 540.8f}, {3840, 2160}, {5120, 1440}, {673, 841}, {0, 0}, {Float.NaN, 500}};
        for (String form : new String[] {"phone", "handheld", "tv", "desktop", "watch"}) {
            for (int pct : new int[] {0, 50, 85, 100, 115, 130, 150, 300}) {
                for (boolean compact : new boolean[] {false, true}) {
                    for (float[] size : sizes) {
                        float[] policy = NightjarUiPolicyJni.viewportScale(size[0], size[1], 0f, form, pct, compact);
                        float[] mirror = ViewportScaler.mirror(size[0], size[1], form, pct, compact);
                        assertArrayEquals(form + " " + pct + " " + compact + " " + size[0] + "x" + size[1], policy, mirror, 1e-4f);
                    }
                }
            }
        }
    }

    @Test
    public void aTvIsNormalisedToItsCanvasAndKeepsTheUsersSize() {
        assertEquals(1f, ViewportScaler.factor(screen(960, 540), true, true, 100, false), 0f);
        assertEquals("a TV reporting 1920x1080 dp", 2f, ViewportScaler.factor(screen(1920, 1080), true, true, 100, false), 1e-5f);
        assertEquals(0.85f, ViewportScaler.factor(screen(960, 540), true, true, 100, true), 1e-5f);
        assertEquals(1.15f * 0.85f, ViewportScaler.factor(screen(960, 540), true, true, 115, true), 1e-5f);
        assertEquals(1.5f, ViewportScaler.factor(screen(960, 540), true, true, 150, false), 1e-5f);
        assertEquals("never under 72 lines of 1080", 0.75f, ViewportScaler.factor(screen(960, 540), true, true, 85, true), 1e-5f);
        assertEquals(1f, ViewportScaler.factor(new Configuration(), true, true, 100, false), 0f);
    }

    @Test
    public void aPhoneOrHandheldTakesOnlyTheUsersSize() {
        assertEquals(1f, ViewportScaler.factor(screen(393, 852), false, false, 100, false), 0f);
        assertEquals(1f, ViewportScaler.factor(screen(640, 360), false, false, 100, true), 0f);
        assertEquals(0.85f, ViewportScaler.factor(screen(640, 360), false, false, 85, false), 0f);
        assertEquals("the TV layout on a handheld keeps the old compaction", 0.85f,
                ViewportScaler.factor(screen(853, 480), false, true, 100, true), 1e-6f);
        assertEquals("a TV laid out as a handheld", 1.3f, ViewportScaler.factor(screen(960, 540), true, false, 130, true), 1e-6f);
    }

    @Test
    public void aTargetIsNeverUnder48SystemDpUnderAHand() {
        for (String form : new String[] {"phone", "handheld"}) {
            for (int pct : AppTheme.UI_SCALES) {
                ViewportScaler.Scale s = ViewportScaler.scale(640, 360, 0, form, pct, false);
                assertTrue(form + " " + pct, s.touchMinDp * s.ui >= ViewportScaler.TOUCH_MIN_DP - 0.01f);
            }
        }
        ViewportScaler.Scale handheld = ViewportScaler.scale(640, 360, 0, "handheld", 100, false);
        assertEquals("spacing compaction is for layouts, the density stays", 1f, handheld.ui, 0f);
        assertEquals(0.75f, handheld.density, 0f);
    }

    @Test
    public void theMirrorAnswersWithoutTheLibrary() {
        NightjarJni.forceUnavailableForTest(new UnsatisfiedLinkError("test"));
        try {
            assertEquals(2f, ViewportScaler.factor(screen(1920, 1080), true, true, 100, false), 1e-5f);
            assertEquals(7, ViewportScaler.scale(960, 540, 0, "tv", 100, false).columns);
        } finally {
            NightjarJni.forceUnavailableForTest(null);
        }
    }
}
