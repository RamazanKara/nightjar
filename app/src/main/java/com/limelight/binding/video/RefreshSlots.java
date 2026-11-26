package com.limelight.binding.video;

import com.limelight.nvstream.jni.HandleReaper;
import com.limelight.nvstream.jni.NightjarPolicyJni;

/**
 * Balanced with a stream about as fast as the panel, which releases each decoded output as it
 * comes out: the refresh of the display's vsync grid that the output is shown at
 * ({@code nightjar_policy::video_pacing::RefreshSlots}).
 */
final class RefreshSlots implements AutoCloseable {
    /** How far the stream's frame interval may be from the refresh interval and still be planned. */
    static final int RATE_TOLERANCE_PERCENT = 2;

    /** One output's place on the grid. */
    static final class Slot {
        /** The refresh the output is shown at; 0 when there is no grid to plan on. */
        long vsyncNs;
        /** The time to release it with, the refresh less half a refresh; 0 without a grid. */
        long targetNs;
        /** The output takes a refresh already claimed: the compositor replaces the buffer there. */
        boolean shared;
        /** Extra refresh hold beyond the earliest reachable refresh; 0 when shown at earliest or without a grid. */
        long holdNs;
        /** Reusable JNI output buffer ({vsyncNs, targetNs, shared, holdNs}) for zero-alloc planning. */
        final long[] jniOut = new long[4];
    }

    private final long handle;
    private final HandleReaper.Token reaper;

    RefreshSlots() {
        this.handle = NightjarPolicyJni.pacingSlabCreate(NightjarPolicyJni.PACING_SLAB_REFRESH_SLOTS);
        this.reaper = HandleReaper.register(
                this,
                handle,
                h -> NightjarPolicyJni.pacingSlabDestroy(NightjarPolicyJni.PACING_SLAB_REFRESH_SLOTS, h));
    }

    @Override
    public void close() {
        reaper.close();
    }

    /** Forgets the grid, for a new display mode or stream. */
    void reset() {
        NightjarPolicyJni.refreshSlotsOp(0, handle, 0, 0);
    }

    static boolean applies(long frameIntervalNs, long refreshIntervalNs) {
        return NightjarPolicyJni.refreshSlotsOp(1, 0, frameIntervalNs, refreshIntervalNs) != 0;
    }

    static long firstVsyncNs(long readyNs, long vsyncNs, long intervalNs, long leadNs) {
        return NightjarPolicyJni.vsyncTiming(0, readyNs, vsyncNs, intervalNs, leadNs, 0, false);
    }

    void plan(long releaseNs, long vsyncNs, long intervalNs, long leadNs, Slot out) {
        out.vsyncNs = 0;
        out.targetNs = 0;
        out.shared = false;
        out.holdNs = 0;
        if (NightjarPolicyJni.refreshSlotsPlan(handle, releaseNs, vsyncNs, intervalNs, leadNs, out.jniOut)) {
            out.vsyncNs = out.jniOut[0];
            out.targetNs = out.jniOut[1];
            out.shared = out.jniOut[2] != 0;
            out.holdNs = out.jniOut[3];
        }
    }

    boolean earliestClaimed(long releaseNs, long vsyncNs, long intervalNs, long leadNs) {
        return NightjarPolicyJni.refreshSlotsEarliestClaimed(handle, releaseNs, vsyncNs, intervalNs, leadNs);
    }

    /** Outputs given a refresh of their own and outputs sharing the previous one, for the report row. */
    long planned() {
        return NightjarPolicyJni.refreshSlotsOp(2, handle, 0, 0);
    }

    long shared() {
        return NightjarPolicyJni.refreshSlotsOp(3, handle, 0, 0);
    }
}
