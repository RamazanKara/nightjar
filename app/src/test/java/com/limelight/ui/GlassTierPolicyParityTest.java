package com.limelight.ui;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import com.limelight.nvstream.jni.NightjarJni;
import com.limelight.nvstream.jni.NightjarUiPolicyJni;

import org.junit.Test;

/**
 * The Java mirrors in {@link GlassTier} and {@link GlassProbe}, used only where nightjar_core does not load, decide
 * exactly as nightjar-policy's glass_probe does, over every case of a table.
 */
public class GlassTierPolicyParityTest {
    private static final GlassTier[] STORED = {null, GlassTier.LENS, GlassTier.LIVE, GlassTier.BAKED, GlassTier.SOLID};

    @Test
    public void landingMatchesThePolicyForEveryDevice() {
        for (int bits = 0; bits < 128; bits++) {
            boolean lens = (bits & 1) != 0, live = (bits & 2) != 0, lowRam = (bits & 4) != 0, saver = (bits & 8) != 0;
            boolean hot = (bits & 16) != 0, contrast = (bits & 32) != 0, guard = (bits & 64) != 0;
            if (lens && !live) continue; // Lens needs API 33, which has live blur.
            for (GlassTier stored : STORED) {
                String policy = NightjarUiPolicyJni.tierLand(lens, live, lowRam, saver, hot, contrast, guard,
                        stored == null ? null : stored.name());
                GlassTier mirror = GlassTier.decideMirror(lens, live, lowRam, saver, hot, contrast, guard, stored);
                assertEquals(bits + "/" + stored, GlassTier.parse(policy), mirror);
            }
        }
    }

    @Test
    public void steppingDownMatchesThePolicy() {
        for (GlassTier tier : GlassTier.values()) {
            assertEquals(tier.toString(), GlassTier.parse(NightjarUiPolicyJni.tierStepDown(tier.key())), tier.stepDownMirror());
            assertEquals(tier.stepDownMirror(), tier.stepDown());
        }
    }

    @Test
    public void theProbesNumbersMatchThePolicy() {
        for (float hz : new float[] {0f, 0.5f, 24f, 50f, 59.94f, 60f, 90f, 120f, 144f, 165f, 240f, Float.NaN}) {
            assertEquals("budget at " + hz, NightjarUiPolicyJni.probeBudgetNs(hz), GlassProbe.budgetNsMirror(hz));
            long budget = GlassProbe.budgetNsMirror(hz);
            long allowed = (long) (budget * (double) 0.7f);
            for (long work = 0; work <= 30_000_000L; work += 250_000L) {
                for (long total : new long[] {1_000_000L, 8_400_000L, 10_416_666L, 10_416_667L, 11_000_000L, 21_000_000L, 40_000_000L}) {
                    assertEquals(hz + " " + work + " " + total, NightjarUiPolicyJni.probeSlow(hz, work, total),
                            GlassProbe.slow(work, total, allowed, budget));
                }
            }
        }
        for (int frames = 0; frames <= 120; frames++) {
            assertEquals(NightjarUiPolicyJni.probeSlowFramesToFail(frames), GlassProbe.slowFramesToFailMirror(frames));
        }
        long[] constants = NightjarUiPolicyJni.probeConstants();
        assertEquals(GlassProbe.SETTLE_MILLIS, constants[1]);
        float[] fractions = NightjarUiPolicyJni.probeFractions();
        assertEquals(GlassProbe.LATE_FRACTION, 1f + fractions[1], 0f);
        assertEquals(GlassProbe.MIN_BUDGET_HZ, fractions[2], 0f);
        assertEquals(GlassTier.GUARD_WARNING, NightjarUiPolicyJni.guardWarning());
    }

    @Test
    public void theBudgetFloorsAtA120HzFrame() {
        assertEquals(16_666_667L, GlassProbe.budgetNs(60f));
        assertEquals(8_333_333L, GlassProbe.budgetNs(120f));
        assertEquals(8_333_333L, GlassProbe.budgetNs(144f));
        assertEquals(8_333_333L, GlassProbe.budgetNs(240f));
    }

    @Test
    public void theMirrorsAnswerWhereTheLibraryIsNotLoaded() {
        NightjarJni.forceUnavailableForTest(new UnsatisfiedLinkError("test"));
        try {
            assertEquals(GlassTier.BAKED, GlassTier.decide(34, false, false, false, false, true, null));
            assertEquals(GlassTier.LENS, GlassTier.decide(34, false, false, false, false, false, GlassTier.SOLID));
            assertEquals(GlassTier.SOLID, GlassTier.SOLID.stepDown());
            assertEquals(8_333_333L, GlassProbe.budgetNs(240f));
            assertEquals(7, GlassProbe.slowFramesToFail(60));
            assertTrue(GlassTier.guardTripped(true, "v1", "v1"));
            assertFalse(GlassTier.guardTripped(true, "v1", "v2"));
        } finally {
            NightjarJni.forceUnavailableForTest(null);
        }
    }
}
