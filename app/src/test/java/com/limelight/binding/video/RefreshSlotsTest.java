package com.limelight.binding.video;

import org.junit.Test;

import static org.junit.Assert.*;

/**
 * Balanced's one-frame-per-refresh slot assignment at the panel rate (RefreshSlots): consecutive
 * refreshes for two outputs in one refresh, a late output on the next refresh it can make, a burst
 * sharing the refresh already claimed so its newest frame replaces the older buffer, a new grid
 * after a refresh change, a slower stream taking only the refresh it could have reached anyway, and
 * the rate gate that keeps a stream faster than the panel off the grid.
 */
public class RefreshSlotsTest {
    /** The Retroid's 120 Hz panel: its latch lead (11.33 ms) is longer than one refresh. */
    private static final long INTERVAL_120 = 8_333_333L;
    private static final long DEADLINE_120 = 11_330_000L;
    /** A 60 Hz panel, with the usual lead of a few milliseconds. */
    private static final long INTERVAL_60 = 16_666_667L;
    private static final long DEADLINE_60 = 4_000_000L;
    /** The last vsync the display's callback reported. */
    private static final long VSYNC = 1_000_000_000L;

    private final RefreshSlots slots = new RefreshSlots();
    private final RefreshSlots.Slot slot = new RefreshSlots.Slot();

    private long plan(long releaseNs) {
        slots.plan(releaseNs, VSYNC, INTERVAL_120, DEADLINE_120, slot);
        return slot.vsyncNs;
    }

    @Test public void twoOutputsInOneRefreshGetConsecutiveRefreshes() {
        // Both come out after the same vsync and before the latch of the first refresh they can
        // make (the Retroid needs 11.33 ms of lead, so that is two refreshes on): releasing either
        // "now" would land it on that refresh, and the second would replace the first there.
        long first = plan(VSYNC + 1_000_000L);
        assertEquals(0L, slot.holdNs);
        long second = plan(VSYNC + 3_000_000L);
        assertEquals(VSYNC + 2 * INTERVAL_120, first);
        assertEquals(first + INTERVAL_120, second);
        // The stamp is half a refresh before its refresh, so the compositor rounds to it.
        assertEquals(second - INTERVAL_120 / 2, slot.targetNs);
        assertFalse(slot.shared);
        assertEquals(INTERVAL_120, slot.holdNs);
    }

    @Test public void anOutputPastTheLatchTakesTheNextRefreshItCanMake() {
        long first = plan(VSYNC + 1_000_000L);
        assertEquals(VSYNC + 2 * INTERVAL_120, first);
        // Released after the latch of the refresh after the first's (VSYNC + 25 ms minus the
        // 11.33 ms lead): nothing claimed that refresh, and this output cannot make it either, so
        // it takes the one after, not the one after the previous claim.
        long late = plan(VSYNC + 14_500_000L);
        assertEquals(VSYNC + 4 * INTERVAL_120, late);
        assertEquals(2 * INTERVAL_120, late - first);
        assertEquals(late - INTERVAL_120 / 2, slot.targetNs);
    }

    @Test public void aBurstSharesTheClaimedRefreshSoItsNewestFrameReplacesTheOlder() {
        // Three outputs inside one refresh; the panel can show one of them per refresh.
        long first = plan(VSYNC + 1_000_000L);
        long second = plan(VSYNC + 2_000_000L);
        assertEquals(VSYNC + 2 * INTERVAL_120, first);
        assertEquals(first + INTERVAL_120, second);
        assertEquals(2, slots.planned());
        assertEquals(0, slots.shared());
        // The third cannot have a refresh of its own without waiting two refreshes for it, so it is
        // stamped with the refresh already claimed: the compositor replaces the second output there,
        // and the newest frame is the one that shows.
        assertEquals(second, plan(VSYNC + 3_000_000L));
        assertTrue(slot.shared);
        assertEquals(second - INTERVAL_120 / 2, slot.targetNs);
        assertEquals(INTERVAL_120, slot.holdNs);
        assertEquals(2, slots.planned());
        assertEquals(1, slots.shared());
        assertEquals(second, plan(VSYNC + 4_000_000L));
        assertTrue(slot.shared);
        assertEquals(INTERVAL_120, slot.holdNs);
        assertEquals(2, slots.shared());
        // Once the panel has passed the claimed refreshes, the next output gets one of its own.
        assertEquals(VSYNC + 4 * INTERVAL_120, plan(VSYNC + 12_000_000L));
        assertFalse(slot.shared);
        // Its earliest refresh is the one the burst already claimed, so it waits one more.
        assertEquals(INTERVAL_120, slot.holdNs);
    }

    @Test public void aNewRefreshRateStartsANewGrid() {
        assertEquals(VSYNC + 2 * INTERVAL_120, plan(VSYNC + 1_000_000L));
        assertEquals(VSYNC + 3 * INTERVAL_120, plan(VSYNC + 2_000_000L));
        // The screen drops to 60 Hz: the claim on the 120 Hz grid must not hold this output back
        // on the new grid, where the first refresh it can make comes a whole 16.7 ms later.
        slots.plan(VSYNC + 3_000_000L, VSYNC, INTERVAL_60, DEADLINE_60, slot);
        assertEquals(VSYNC + INTERVAL_60, slot.vsyncNs);
        slots.plan(VSYNC + 4_000_000L, VSYNC, INTERVAL_60, DEADLINE_60, slot);
        assertEquals(VSYNC + 2 * INTERVAL_60, slot.vsyncNs);
        // And back up to 120 Hz, on the 120 Hz grid again.
        slots.plan(VSYNC + 40_000_000L, VSYNC, INTERVAL_120, DEADLINE_120, slot);
        assertEquals(VSYNC + 7 * INTERVAL_120, slot.vsyncNs);
    }

    @Test public void aStreamSlowerThanThePanelKeepsItsOwnCadence() {
        // 60 fps on 120 Hz: each output comes out after the previous refresh, so it takes the first
        // refresh it can make, which is where releasing it "now" would land: no extra hold.
        long first = plan(VSYNC + 1_000_000L);
        long second = plan(VSYNC + 1_000_000L + 2 * INTERVAL_120);
        long third = plan(VSYNC + 1_000_000L + 4 * INTERVAL_120);
        assertEquals(VSYNC + 2 * INTERVAL_120, first);
        assertEquals(first + 2 * INTERVAL_120, second);
        assertEquals(second + 2 * INTERVAL_120, third);
        // Each stamp is the refresh that follows it, less half a refresh; none was moved a refresh
        // later by the previous claim.
        assertEquals(third - INTERVAL_120 / 2, slot.targetNs);
        assertEquals(slot.vsyncNs, RefreshSlots.firstVsyncNs(
                VSYNC + 1_000_000L + 4 * INTERVAL_120, VSYNC, INTERVAL_120, DEADLINE_120));
        assertFalse(slot.shared);
        // 30 fps on 60 Hz leaves two refreshes between outputs, still with no extra hold.
        long slow = plan2(VSYNC + 1_000_000L, INTERVAL_60, DEADLINE_60);
        long slower = plan2(VSYNC + 1_000_000L + 2 * INTERVAL_60, INTERVAL_60, DEADLINE_60);
        assertEquals(VSYNC + INTERVAL_60, slow);
        assertEquals(VSYNC + 3 * INTERVAL_60, slower);
        assertFalse(slot.shared);
    }

    @Test public void onlyAStreamAtThePanelRateIsPlanned() {
        // 120 fps on a 120 Hz panel, including the panel's and the host's small rate error.
        assertTrue(RefreshSlots.applies(8_332_800L, 8_333_333L));
        assertTrue(RefreshSlots.applies(8_333_333L, 8_332_800L));
        // A faster stream has surplus frames at every refresh: 144 or 240 fps on 120 Hz, or 120 fps
        // on a panel that dropped to 90 Hz. A slower one never reaches the slots anyway.
        assertFalse(RefreshSlots.applies(1_000_000_000L / 144, 8_333_333L));
        assertFalse(RefreshSlots.applies(1_000_000_000L / 240, 8_333_333L));
        assertFalse(RefreshSlots.applies(8_333_333L, 1_000_000_000L / 90));
        assertFalse(RefreshSlots.applies(8_333_333L, INTERVAL_60));
        assertFalse(RefreshSlots.applies(16_666_667L, INTERVAL_120));
        // Just inside and just outside the 2% band.
        assertTrue(RefreshSlots.applies(8_333_333L + 166_666L, 8_333_333L));
        assertFalse(RefreshSlots.applies(8_333_333L + 170_000L, 8_333_333L));
        assertFalse(RefreshSlots.applies(0, INTERVAL_120));
        assertFalse(RefreshSlots.applies(INTERVAL_120, 0));
    }

    @Test public void withoutAGridTheOutputIsReleasedNow() {
        slot.holdNs = INTERVAL_120;
        slots.plan(VSYNC, 0, INTERVAL_120, DEADLINE_120, slot);
        assertEquals(0, slot.vsyncNs);
        assertEquals(0, slot.targetNs);
        assertFalse(slot.shared);
        assertEquals(0L, slot.holdNs);
        slots.plan(VSYNC, VSYNC, 0, DEADLINE_120, slot);
        assertEquals(0, slot.vsyncNs);
        assertEquals(0L, slot.holdNs);
        assertEquals(0, slots.planned());
        assertEquals(0, slots.shared());
    }

    @Test public void earliestClaimedTracksBurstClaimsAndLiveGridDrift() {
        assertFalse(slots.earliestClaimed(VSYNC + 1_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));
        assertFalse(slots.earliestClaimed(VSYNC + 1_000_000L, 0, INTERVAL_120, DEADLINE_120));
        assertFalse(slots.earliestClaimed(VSYNC + 1_000_000L, VSYNC, 0, DEADLINE_120));

        plan(VSYNC + 1_000_000L);
        assertEquals(0L, slot.holdNs);
        assertTrue(slots.earliestClaimed(VSYNC + 2_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));
        assertFalse(slots.earliestClaimed(VSYNC + INTERVAL_120 + 1_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));
        assertFalse(slots.earliestClaimed(VSYNC + 2_000_000L, VSYNC, INTERVAL_60, DEADLINE_60));

        plan(VSYNC + 2_000_000L);
        assertEquals(INTERVAL_120, slot.holdNs);
        assertTrue(slots.earliestClaimed(VSYNC + 3_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));
        assertTrue(slots.earliestClaimed(VSYNC + INTERVAL_120 + 1_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));
        assertFalse(slots.earliestClaimed(VSYNC + 2 * INTERVAL_120 + 1_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));

        slots.reset();
        assertFalse(slots.earliestClaimed(VSYNC + 2_000_000L, VSYNC, INTERVAL_120, DEADLINE_120));

        long actualPeriodNs = INTERVAL_120 - 50_000L;
        long[] jitterNs = {0L, 400_000L, -350_000L, 250_000L, -400_000L, 150_000L};
        for (int frame = 0; frame < 60; frame++) {
            long liveVsyncNs = VSYNC + frame * actualPeriodNs + jitterNs[frame % jitterNs.length];
            long releaseNs = liveVsyncNs + 1_000_000L;
            assertFalse("steady frame " + frame, slots.earliestClaimed(releaseNs, liveVsyncNs, INTERVAL_120, DEADLINE_120));
            slots.plan(releaseNs, liveVsyncNs, INTERVAL_120, DEADLINE_120, slot);
            assertEquals(0L, slot.holdNs);
            long shiftedVsyncNs = liveVsyncNs + 300_000L;
            assertTrue("same-refresh burst frame " + frame,
                    slots.earliestClaimed(shiftedVsyncNs + 1_500_000L, shiftedVsyncNs, INTERVAL_120, DEADLINE_120));
        }
    }

    @Test public void panelClockDriftAndChoreographerJitterDoNotRatchetOffTheVsyncGrid() {
        // Real panel period is 50 us shorter than the nominal 120 Hz interval, with +-400 us
        // Choreographer jitter on reported vsync timestamps.
        long actualPeriodNs = INTERVAL_120 - 50_000L;
        long[] jitterNs = {0L, 400_000L, -350_000L, 250_000L, -400_000L, 150_000L};
        for (int frame = 0; frame < 600; frame++) {
            long liveVsyncNs = VSYNC + frame * actualPeriodNs + jitterNs[frame % jitterNs.length];
            long releaseNs = liveVsyncNs + 1_000_000L;
            slots.plan(releaseNs, liveVsyncNs, INTERVAL_120, DEADLINE_120, slot);
            assertFalse("frame " + frame + " should not be forced into sharing", slot.shared);
            // Every planned slot stays locked onto the live vsyncNs grid (within 0 ns of a multiple).
            long gridRemainder = Math.floorMod(slot.vsyncNs - liveVsyncNs, INTERVAL_120);
            assertEquals("frame " + frame + " drifted off grid", 0L, gridRemainder);
            long earliestNs = RefreshSlots.firstVsyncNs(releaseNs, liveVsyncNs, INTERVAL_120, DEADLINE_120);
            assertEquals("frame " + frame + " accumulated drift ahead of earliest", earliestNs, slot.vsyncNs);
        }
    }

    @Test public void sharedBurstSlotStaysSnappedToLiveDriftingVsyncGrid() {
        long liveVsync = VSYNC;
        slots.plan(liveVsync + 1_000_000L, liveVsync, INTERVAL_120, DEADLINE_120, slot);
        slots.plan(liveVsync + 2_000_000L, liveVsync, INTERVAL_120, DEADLINE_120, slot);
        // Live vsync shifts by +300 us when a burst third output shares the claimed slot.
        long shiftedVsync = liveVsync + 300_000L;
        slots.plan(shiftedVsync + 2_500_000L, shiftedVsync, INTERVAL_120, DEADLINE_120, slot);
        assertTrue(slot.shared);
        assertEquals(0L, Math.floorMod(slot.vsyncNs - shiftedVsync, INTERVAL_120));
        assertEquals(slot.vsyncNs - INTERVAL_120 / 2, slot.targetNs);
    }

    @Test public void firstVsyncHandlesReadyTimeSlightlyBeforeLatestVsyncAnchor() {
        // When onVsync advances vsyncNs slightly past a releaseNs captured just before the lock,
        // floorDiv keeps the first reachable refresh one interval after vsyncNs.
        assertEquals(VSYNC + INTERVAL_120, RefreshSlots.firstVsyncNs(VSYNC - 500_000L, VSYNC, INTERVAL_120, 0L));
    }

    private long plan2(long releaseNs, long intervalNs, long deadlineNs) {
        slots.plan(releaseNs, VSYNC, intervalNs, deadlineNs, slot);
        return slot.vsyncNs;
    }
}
