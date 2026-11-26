package com.limelight.nvstream.jni;

import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

/**
 * Every export of {@code protocol/android/src/policy_ui_70.rs} links and answers what
 * {@code nightjar-policy} decides, on the host library the unit tests load.
 */
public class NightjarUiPolicyJniTest {
    @Test
    public void tiersLandAndStep() {
        assertEquals("lens", NightjarUiPolicyJni.tierLand(true, true, false, false, false, false, false, null));
        assertEquals("live", NightjarUiPolicyJni.tierLand(true, true, false, false, false, false, false, "LIVE"));
        assertEquals("baked", NightjarUiPolicyJni.tierLand(true, true, false, false, false, false, true, null));
        assertEquals("solid", NightjarUiPolicyJni.tierLand(true, true, false, true, false, true, false, null));
        assertEquals("live", NightjarUiPolicyJni.tierStepDown("lens"));
        assertEquals("baked", NightjarUiPolicyJni.tierStepDown("LIVE"));
    }

    @Test
    public void probeAndGuardAnswer() {
        assertEquals(16_666_667L, NightjarUiPolicyJni.probeBudgetNs(60f));
        assertEquals(8_333_333L, NightjarUiPolicyJni.probeBudgetNs(144f));
        assertTrue(NightjarUiPolicyJni.probeSlow(120f, 6_500_000L, 11_000_000L));
        assertFalse(NightjarUiPolicyJni.probeSlow(120f, 6_500_000L, 8_400_000L));
        assertTrue(NightjarUiPolicyJni.probeSlow(120f, 6_500_000L, -1L));
        assertEquals(7, NightjarUiPolicyJni.probeSlowFramesToFail(60));
        assertTrue(NightjarUiPolicyJni.probeSettled(2000));
        assertFalse(NightjarUiPolicyJni.probeSettled(1999));
        assertArrayEquals(new long[] {60, 2000, 8_333_333L}, NightjarUiPolicyJni.probeConstants());
        assertArrayEquals(new float[] {0.70f, 0.25f, 120f}, NightjarUiPolicyJni.probeFractions(), 0f);
        assertTrue(NightjarUiPolicyJni.guardInFlight(true, "v1"));
        assertFalse(NightjarUiPolicyJni.guardInFlight(false, "v1"));
        assertTrue(NightjarUiPolicyJni.guardTripped(true, "v1", "v1"));
        assertFalse(NightjarUiPolicyJni.guardTripped(true, "v1", "v2"));
        assertEquals("Previous live-surface probe did not finish; using baked surfaces", NightjarUiPolicyJni.guardWarning());
    }

    @Test
    public void viewportAndResolutionAnswer() {
        float[] tv = NightjarUiPolicyJni.viewportScale(1920f, 1080f, 0f, "tv", 100, false);
        assertEquals(9, tv.length);
        assertEquals(2f, tv[0], 1e-5f);
        float[] phone = NightjarUiPolicyJni.viewportScale(393f, 852f, 420f, "phone", 100, false);
        assertEquals(1f, phone[0], 0f);
        assertEquals(48f, phone[6], 0f);
        assertArrayEquals(new int[] {100, 1920, 1080, 85, 1632, 918, 75, 1440, 810, 67, 1280, 720, 50, 960, 540},
                NightjarUiPolicyJni.resolutionChoices(1920, 1080));
        assertArrayEquals(new int[] {2560, 1440}, NightjarUiPolicyJni.resolutionScaled(3840, 2160, 67));
        assertEquals(67, NightjarUiPolicyJni.resolutionStepOf(1920, 1080, 1280, 720));
        assertEquals(0, NightjarUiPolicyJni.resolutionStepOf(1920, 1080, 1600, 900));
    }

    @Test
    public void glyphsAnswer() {
        assertEquals("button:B", NightjarUiPolicyJni.inputGlyph("pad", 0x057E, "go", false));
        assertEquals("button:×", NightjarUiPolicyJni.inputGlyph("pad", 0x054C, "go", false));
        assertEquals("remote_hold_ok:Hold OK", NightjarUiPolicyJni.inputGlyph("remote", 0, "more", false));
        assertNull(NightjarUiPolicyJni.inputGlyph("touch", 0, "go", false));
        assertEquals("nintendo", NightjarUiPolicyJni.inputStyleOfVendor(0x057E));
        assertTrue(NightjarUiPolicyJni.figtreeCovers("pt-BR"));
        assertFalse(NightjarUiPolicyJni.figtreeCovers("ja"));
    }

    @Test
    public void feedbackAnswersAndIsSilentInAStream() {
        assertEquals("press|", NightjarUiPolicyJni.feedback("confirm", false, true, false));
        assertEquals("|", NightjarUiPolicyJni.feedback("confirm", true, true, true));
        long limiter = NightjarUiPolicyJni.feedbackLimiterCreate();
        try {
            assertEquals("tick|", NightjarUiPolicyJni.feedbackLimited(limiter, "focus_tick", false, true, false, 1000));
            assertEquals("|", NightjarUiPolicyJni.feedbackLimited(limiter, "focus_tick", false, true, false, 1020));
        } finally {
            NightjarUiPolicyJni.feedbackLimiterDestroy(limiter);
        }
    }

    @Test
    public void controlCenterCallAnswersJson() {
        assertEquals("{\"ok\":\"sheet\"}", NightjarUiPolicyJni.controlCenterCall(
                "{\"op\":\"placement\",\"form\":\"phone\",\"width_dp\":393,\"height_dp\":852}"));
        assertTrue(NightjarUiPolicyJni.controlCenterCall("nope").contains("\"error\":\"invalid\""));
        assertTrue(NightjarUiPolicyJni.controlCenterCall(null).contains("\"error\":\"invalid\""));
    }
}
