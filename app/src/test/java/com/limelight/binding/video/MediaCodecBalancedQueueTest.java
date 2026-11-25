package com.limelight.binding.video;

import android.app.Activity;
import android.app.Application;
import android.media.MediaCodec;
import android.view.Choreographer;

import com.limelight.preferences.PreferenceConfiguration;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import org.robolectric.shadows.ShadowMediaCodec;
import org.robolectric.util.ReflectionHelpers;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

import static com.limelight.preferences.PreferenceConfiguration.FRAME_PACING_BALANCED;
import static com.limelight.preferences.PreferenceConfiguration.FRAME_PACING_MIN_LATENCY;
import static org.junit.Assert.*;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 29, application = Application.class, shadows = MediaCodecBalancedQueueTest.QueuedCodec.class)
public class MediaCodecBalancedQueueTest {
    private static final int OLD = 10, SECOND = 11, NEW = 12;
    private static final long FRAME_NS = 1_000_000_000L / 120;

    @Test public void replacementIsReadyWhileDiscardingTheOldOutputIsBlocked() throws Exception {
        try (Fixture fixture = new Fixture(1, new IntRing())) {
            fixture.seed(OLD);
            fixture.start();
            assertTrue("Producer did not begin returning the old buffer", fixture.state.discardEntered.await(5, TimeUnit.SECONDS));

            // The real Choreographer branch runs while the codec's old-buffer release is paused.
            // Previously remove -> codec release -> add left this callback with an empty queue.
            fixture.frame(0);
            assertEquals(Collections.singletonList(NEW), fixture.state.rendered);
            assertEquals(Collections.singletonList(OLD), fixture.state.dropped);
            assertTrue(fixture.queue.isEmpty());
            assertFalse("Codec release held the handoff monitor", fixture.state.codecCallHeldQueueLock);
        }
    }

    @Test public void aStreamAsFastAsThePanelIsReleasedAtOnceAndDropsWhatWaitedForVsync() throws Exception {
        // Issue 7: at 120 fps on 120 Hz, waiting for the vsync callback dropped 6% of the frames.
        try (Fixture fixture = new Fixture(1, new IntRing())) {
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "hostPhaseLock", true);
            fixture.dropLateFrames();
            fixture.state.allowDiscard.countDown();
            fixture.seed(OLD);
            fixture.start();
            fixture.join();
            assertEquals("queued before the switch, older than the new output", Collections.singletonList(OLD), fixture.state.dropped);
            assertEquals(Collections.singletonList(NEW), fixture.state.rendered);
            assertTrue(fixture.queue.isEmpty());
            // The problem report's row tells this path and its drop apart (nightjar#119).
            assertEquals("pace_switch 1", fixture.state.stats.describeDrops(1));
            assertEquals(1L, (long) ReflectionHelpers.getField(fixture.renderer, "balancedOnOutput"));
            assertEquals(0L, (long) ReflectionHelpers.getField(fixture.renderer, "balancedOnVsync"));
        }
    }

    @Test public void withoutPhaseLockAStreamAsFastAsThePanelStillWaitsForVsync() throws Exception {
        // A host without phase lock reports cannot move its capture off the latch: keep the queue.
        try (Fixture fixture = new Fixture(2, new IntRing())) {
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "hostPhaseLock", false);
            fixture.dropLateFrames();
            fixture.state.allowDiscard.countDown();
            fixture.seed(OLD);
            fixture.start();
            fixture.join();
            // The old Balanced queue: the newest output waits for the callback, a late one goes.
            assertTrue(fixture.state.rendered.isEmpty());
            assertEquals(Collections.singletonList(OLD), fixture.state.dropped);
            assertEquals(Collections.singletonList(NEW), fixture.queue.toList());
            assertEquals("replaced 1", fixture.state.stats.describeDrops(1));
            assertEquals(1L, (long) ReflectionHelpers.getField(fixture.renderer, "balancedOnVsync"));
        }
    }

    @Test public void fullQueueDecisionCannotBecomeStaleAfterTheConsumerRemovesAFrame() throws Exception {
        SizeBarrierQueue queue = new SizeBarrierQueue();
        try (Fixture fixture = new Fixture(2, queue)) {
            fixture.seed(OLD);
            fixture.seed(SECOND);
            fixture.start();
            assertTrue("Producer did not inspect the full queue", queue.sizeRead.await(5, TimeUnit.SECONDS));
            try {
                fixture.frame(0);
            } finally {
                queue.consumerFinished.countDown();
            }
            assertTrue(fixture.state.discardEntered.await(5, TimeUnit.SECONDS));

            // The producer entered its atomic replacement first: it evicts OLD, then the
            // consumer gets SECOND and NEW remains. A stale size check instead consumes OLD
            // and needlessly evicts SECOND, even though the consumer already freed a slot.
            assertEquals(Collections.singletonList(OLD), fixture.state.dropped);
            assertEquals(Collections.singletonList(SECOND), fixture.state.rendered);
            assertEquals(Collections.singletonList(NEW), fixture.queue.toList());
        } finally {
            queue.consumerFinished.countDown();
        }
    }

    @Test public void consumerThatFreesASlotBeforeEnqueuePreservesBothRemainingFrames() throws Exception {
        try (Fixture fixture = new Fixture(2, new IntRing())) {
            fixture.seed(OLD);
            fixture.seed(SECOND);
            fixture.frame(0);
            fixture.start();
            fixture.join();
            assertTrue(fixture.state.dropped.isEmpty());
            assertEquals(Arrays.asList(SECOND, NEW), fixture.queue.toList());

            // The fake decoder stopped after its single output; resume just the frame callbacks.
            ReflectionHelpers.setField(fixture.renderer, "stopping", false);
            fixture.frame(1);
            fixture.frame(2);
            assertEquals(Arrays.asList(OLD, SECOND, NEW), fixture.state.rendered);
            assertTrue(fixture.queue.isEmpty());
        }
    }

    @Test public void frameTimelineBoundaryJitterDoesNotSkipVsyncsInBalancedEveryFrame() throws Exception {
        // At 120 Hz, truncated vsyncIntervalNs is 8,333,333 ns while 2 true refreshes are
        // 16,666,667 ns. Crossing 2 * intervalNs caused VsyncTimeline.anchorNs to jump back by one
        // full refresh; Balanced's Choreographer cadence must stay monotonic and render every tick.
        try (Fixture fixture = new Fixture(2, new IntRing())) {
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "refreshSlotsEnabled", true);
            long base = System.nanoTime();
            long lead = 10_000_000L;
            long[] aheadJitter = {2 * FRAME_NS, 2 * FRAME_NS + 1, 2 * FRAME_NS - 500, 2 * FRAME_NS + 500,
                    2 * FRAME_NS, 2 * FRAME_NS + 1};
            List<Integer> expected = new ArrayList<>();
            long[] anchors = new long[aheadJitter.length];
            for (int tick = 0; tick < aheadJitter.length; tick++) {
                int id = 100 + tick;
                expected.add(id);
                fixture.seed(id);
                long frameTimeNs = base + tick * FRAME_NS;
                long expectedPresentNs = frameTimeNs + aheadJitter[tick];
                anchors[tick] = VsyncTimeline.anchorNs(expectedPresentNs, frameTimeNs, FRAME_NS);
                fixture.renderer.onFrameTimeline(frameTimeNs, expectedPresentNs, expectedPresentNs - lead);
            }
            assertEquals("every vsync must render without skipping across anchorNs steps", expected,
                    fixture.state.rendered);
            assertTrue(fixture.queue.isEmpty());
            // Every panel-rate release is stamped half a refresh ahead of a refresh on the live
            // vsync grid (via RefreshSlots), and consecutive outputs advance by one refresh.
            assertEquals(expected.size(), fixture.state.releaseTimestamps.size());
            for (int i = 0; i < fixture.state.releaseTimestamps.size(); i++) {
                long slotNs = fixture.state.releaseTimestamps.get(i) + FRAME_NS / 2;
                assertEquals("release " + i + " should lie on the live vsync grid", 0L,
                        Math.floorMod(slotNs - anchors[i], FRAME_NS));
            }
            long firstStep = fixture.state.releaseTimestamps.get(1) - fixture.state.releaseTimestamps.get(0);
            assertTrue("consecutive outputs should claim consecutive refreshes, got " + firstStep,
                    firstStep >= FRAME_NS - 10_000L && firstStep <= FRAME_NS + 10_000L);
        }
    }

    @Test public void balancedDirectPathKeepsTwoFrameBurstAcrossConsecutiveRefreshesWithoutLatchLeadWait() throws Exception {
        try (Fixture fixture = new Fixture(1, new IntRing())) {
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "hostPhaseLock", true);
            ReflectionHelpers.setField(fixture.renderer, "refreshSlotsEnabled", true);
            ReflectionHelpers.setField(fixture.renderer, "vsyncPresentationDeadlineNs", 11_330_000L);
            // An ARR screen with VRR off still plans refresh slots; it sends no phase reports,
            // which would need the native bridge here.
            ReflectionHelpers.setField(fixture.renderer, "outputHasArrSupport", true);
            fixture.dropLateFrames();
            fixture.state.outputs = new int[] {20, 21};
            fixture.state.anchorVsyncOnFirstOutput = true;
            fixture.renderer.testClockNs = fixture.firstVsync;
            fixture.state.allowDiscard.countDown();

            fixture.start();
            fixture.join();

            assertTrue("two-frame burst must not drop either frame", fixture.state.dropped.isEmpty());
            assertEquals(Arrays.asList(20, 21), fixture.state.rendered);
            assertEquals(2, fixture.state.releaseTimestamps.size());
            assertEquals("both outputs should land on consecutive refreshes", FRAME_NS,
                    fixture.state.releaseTimestamps.get(1) - fixture.state.releaseTimestamps.get(0));
            assertEquals(2.0, fixture.state.stats.summarize()[2][0], 0.0);
            FrameLatencyMetrics metrics = ReflectionHelpers.getField(fixture.state.stats, "metrics");
            long[][] samples = ReflectionHelpers.getField(metrics, "samples");
            double wait1Ms = samples[2][0] / 1_000_000.0;
            double wait2Ms = samples[2][1] / 1_000_000.0;
            assertTrue("Frame 1 Display wait should exclude latch lead (< 1 ms), got " + wait1Ms, wait1Ms < 1.0);
            assertEquals("Frame 2 Display wait should match one-refresh hold", FRAME_NS / 1_000_000.0, wait2Ms, 1.0);
        }
    }

    @Test public void balancedDirectPathDropsMiddleFrameOfThreeOutputBurstAsCatchUp() throws Exception {
        try (Fixture fixture = new Fixture(1, new IntRing())) {
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "hostPhaseLock", true);
            ReflectionHelpers.setField(fixture.renderer, "refreshSlotsEnabled", true);
            ReflectionHelpers.setField(fixture.renderer, "vsyncPresentationDeadlineNs", 11_330_000L);
            // An ARR screen with VRR off still plans refresh slots; it sends no phase reports,
            // which would need the native bridge here.
            ReflectionHelpers.setField(fixture.renderer, "outputHasArrSupport", true);
            fixture.dropLateFrames();
            fixture.state.outputs = new int[] {20, 21, 22};
            fixture.state.anchorVsyncOnFirstOutput = true;
            fixture.renderer.testClockNs = fixture.firstVsync;
            fixture.state.allowDiscard.countDown();

            fixture.start();
            fixture.join();

            assertEquals(Collections.singletonList(21), fixture.state.dropped);
            assertEquals(Arrays.asList(20, 22), fixture.state.rendered);
            assertEquals("catch_up 1", fixture.state.stats.describeDrops(1));
            assertEquals(2, fixture.state.releaseTimestamps.size());
            assertEquals(FRAME_NS,
                    fixture.state.releaseTimestamps.get(1) - fixture.state.releaseTimestamps.get(0));
        }
    }

    @Test public void balancedEveryFrameOnVsyncExcludesLatchLeadFromDisplayWait() throws Exception {
        try (Fixture fixture = new Fixture(2, new IntRing())) {
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "refreshSlotsEnabled", true);
            long base = System.nanoTime();
            long lead = 10_000_000L;
            fixture.seed(100);
            long expectedPresentNs = base + 2 * FRAME_NS;
            long anchor = VsyncTimeline.anchorNs(expectedPresentNs, base, FRAME_NS);
            fixture.renderer.onFrameTimeline(base, expectedPresentNs, expectedPresentNs - lead);

            assertEquals(Collections.singletonList(100), fixture.state.rendered);
            assertEquals(1, fixture.state.releaseTimestamps.size());
            assertEquals(anchor + 2 * FRAME_NS - FRAME_NS / 2, (long) fixture.state.releaseTimestamps.get(0));
            double[][] summary = fixture.state.stats.summarize();
            assertEquals(1.0, summary[2][0], 0.0);
            assertTrue("Display wait should exclude latch lead (< 1 ms), got " + summary[2][1], summary[2][1] < 1.0);
        }
    }

    @Test public void lowestLatencyStillDropsOlderOutputOfTwoFrameBurstAsCatchUp() throws Exception {
        try (Fixture fixture = new Fixture(1, new IntRing())) {
            PreferenceConfiguration prefs = ReflectionHelpers.getField(fixture.renderer, "prefs");
            prefs.framePacing = FRAME_PACING_MIN_LATENCY;
            fixture.renderer.setStreamFrameRateMilliHz(120_000);
            ReflectionHelpers.setField(fixture.renderer, "hostPhaseLock", true);
            ReflectionHelpers.setField(fixture.renderer, "refreshSlotsEnabled", true);
            ReflectionHelpers.setField(fixture.renderer, "vsyncPresentationDeadlineNs", 11_330_000L);
            // An ARR screen with VRR off still plans refresh slots; it sends no phase reports,
            // which would need the native bridge here.
            ReflectionHelpers.setField(fixture.renderer, "outputHasArrSupport", true);
            fixture.state.outputs = new int[] {20, 21};
            fixture.state.anchorVsyncOnFirstOutput = true;
            fixture.renderer.testClockNs = fixture.firstVsync;
            fixture.state.allowDiscard.countDown();

            fixture.start();
            fixture.join();

            assertEquals(Collections.singletonList(20), fixture.state.dropped);
            assertEquals(Collections.singletonList(21), fixture.state.rendered);
            assertEquals("catch_up 1", fixture.state.stats.describeDrops(1));
        }
    }

    /** Force the old size/poll race, but never wait for a consumer while holding its monitor. */
    private static final class SizeBarrierQueue extends IntRing {
        final CountDownLatch sizeRead = new CountDownLatch(1);
        final CountDownLatch consumerFinished = new CountDownLatch(1);
        boolean observed;

        @Override int size() {
            int size = super.size();
            if (!observed && size == 2) {
                observed = true;
                sizeRead.countDown();
                if (!Thread.holdsLock(this)) await(consumerFinished);
            }
            return size;
        }
    }

    private static final class State {
        MediaCodecDecoderRenderer renderer;
        FrameLatencyStats stats;
        IntRing queue;
        final CountDownLatch discardEntered = new CountDownLatch(1);
        final CountDownLatch allowDiscard = new CountDownLatch(1);
        final List<Integer> dropped = new ArrayList<>();
        final List<Integer> rendered = new ArrayList<>();
        final List<Long> releaseTimestamps = new ArrayList<>();
        int[] outputs = {NEW};
        int nextOutput;
        boolean anchorVsyncOnFirstOutput, codecCallHeldQueueLock;
    }

    private static final class Fixture implements AutoCloseable {
        final ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create();
        final MediaCodecDecoderRenderer renderer;
        final IntRing queue;
        final State state = new State();
        final MediaCodec codec;
        final long firstVsync = System.nanoTime();

        Fixture(int capacity, IntRing queue) throws Exception {
            ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
            PreferenceConfiguration prefs = new PreferenceConfiguration();
            prefs.videoFormat = PreferenceConfiguration.FormatOption.AUTO;
            prefs.framePacing = FRAME_PACING_BALANCED;
            renderer = new MediaCodecDecoderRenderer(activity.get(), prefs, null, 0, false, false, "", null);
            // Slower than the panel, so Balanced paces on vsync (FramePacingPolicy.balancedPacesOnVsync).
            renderer.setStreamFrameRateMilliHz(60_000);
            this.queue = queue;
            ReflectionHelpers.setField(renderer, "outputBufferQueue", queue);
            ReflectionHelpers.setField(renderer, "outputBufferQueueLimit", capacity);
            ReflectionHelpers.setField(renderer, "outputVsyncAvailable", true);
            ReflectionHelpers.setField(renderer, "vsyncIntervalNs", FRAME_NS);
            ReflectionHelpers.setField(renderer, "appVsyncOffsetNs", 0L);
            state.renderer = renderer;
            state.queue = queue;
            state.stats = ReflectionHelpers.getField(renderer, "frameLatencyStats");
            QueuedCodec.state = state;
            codec = MediaCodec.createDecoderByType("video/hevc");
            ReflectionHelpers.setField(renderer, "videoDecoder", codec);
        }

        /** The setting's default; the fixture's queue tests run with it off for their capacity. */
        void dropLateFrames() {
            PreferenceConfiguration prefs = ReflectionHelpers.getField(renderer, "prefs");
            prefs.dropLateFrames = true;
        }

        void seed(int index) {
            long now = System.nanoTime();
            state.stats.onDecoderInput(index, index, now, now, now, (char) 0);
            state.stats.onDecoderOutput(index, index, now);
            queue.add(index);
        }

        /** The vsync callback once per stream frame (two refreshes at 60 fps on 120 Hz). */
        void frame(int tick) {
            renderer.doFrame(firstVsync + tick * 2 * FRAME_NS);
        }

        void start() {
            ReflectionHelpers.callInstanceMethod(renderer, "startRendererThread");
        }

        void join() throws InterruptedException {
            Thread thread = ReflectionHelpers.getField(renderer, "rendererThread");
            if (thread != null) {
                thread.join(5000);
                assertFalse("Renderer thread did not finish", thread.isAlive());
            }
        }

        @Override public void close() throws Exception {
            state.allowDiscard.countDown();
            if (queue instanceof SizeBarrierQueue) ((SizeBarrierQueue) queue).consumerFinished.countDown();
            ReflectionHelpers.setField(renderer, "stopping", true);
            join();
            Choreographer.getInstance().removeFrameCallback(renderer);
            renderer.stop();
            codec.release();
            activity.close();
        }
    }

    private static void await(CountDownLatch latch) {
        try {
            if (!latch.await(5, TimeUnit.SECONDS)) throw new AssertionError("Timed out waiting for the controlled interleaving");
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new AssertionError(e);
        }
    }

    @Implements(MediaCodec.class)
    public static class QueuedCodec extends ShadowMediaCodec {
        static State state;

        @Implementation public int dequeueOutputBuffer(MediaCodec.BufferInfo info, long timeoutUs) {
            if (state.nextOutput >= state.outputs.length) {
                if (timeoutUs > 0) {
                    ReflectionHelpers.setField(state.renderer, "stopping", true);
                }
                return MediaCodec.INFO_TRY_AGAIN_LATER;
            }
            int id = state.outputs[state.nextOutput++];
            long now = state.renderer.nowNs();
            if (state.anchorVsyncOnFirstOutput && state.nextOutput == 1) {
                synchronized (state.renderer.vsyncMonitor) {
                    // A latch just passed: the burst has almost a whole refresh before the next one.
                    state.renderer.vsyncTimeNs = now + state.renderer.vsyncPresentationDeadlineNs - 100_000L;
                }
            }
            info.presentationTimeUs = id;
            state.stats.onDecoderInput(id, id, now, now, now, (char) 0);
            return id;
        }

        @Implementation public void releaseOutputBuffer(int index, boolean render) {
            assertFalse(render);
            state.dropped.add(index);
            state.codecCallHeldQueueLock |= Thread.holdsLock(state.queue);
            state.discardEntered.countDown();
            await(state.allowDiscard);
        }

        @Implementation public void releaseOutputBuffer(int index, long timestampNs) {
            state.codecCallHeldQueueLock |= Thread.holdsLock(state.queue);
            state.rendered.add(index);
            state.releaseTimestamps.add(timestampNs);
        }
    }
}
