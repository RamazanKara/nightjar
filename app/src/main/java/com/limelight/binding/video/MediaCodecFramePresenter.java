package com.limelight.binding.video;

import android.media.MediaCodec;
import android.media.MediaCodec.BufferInfo;
import android.os.Build;
import android.os.SystemClock;

import com.limelight.LimeLog;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;
import com.limelight.preferences.PreferenceConfiguration;

/**
 * Handles decoded MediaCodec output format checks, catch-up draining, decoder
 * residency and debt recovery, display-queue drops, and frame release/queuing
 * for {@link MediaCodecDecoderRenderer}.
 */
final class MediaCodecFramePresenter {
    private final MediaCodecDecoderRenderer r;

    // Lowest latency and Balanced: a buffer too many in the display's queue (nightjar#7).
    final DisplayQueueWatch displayQueue = new DisplayQueueWatch();
    // Why the catch-up loop wants to drop the output it holds; see wantsCatchUp.
    private String catchUpReason;
    private final FrameLatencyStats.OutputTimes debtTimes = new FrameLatencyStats.OutputTimes();

    MediaCodecFramePresenter(MediaCodecDecoderRenderer renderer) {
        this.r = renderer;
    }

    int handleDecodedOutput(int outIndex, long workStartNs, BufferInfo info, long outputDrainStartNs) {
        noteDecoderOutput(outIndex, info.presentationTimeUs, workStartNs);
        long ptsUs = info.presentationTimeUs;

        r.numFramesOut++;
        MediaCodecHdrControl.checkRunningHdrStaticInfo(r);

        if (r.vulkanRenderer != null) {
            r.vulkanOutputs++;
            r.vulkanHelper.handOutputToVulkan(outIndex, ptsUs);
        } else if (r.prefs.framePacing != PreferenceConfiguration.FRAME_PACING_BALANCED ||
                !r.vsyncPacer.balancedPacesOnVsync()) {
            if (r.prefs.framePacing == PreferenceConfiguration.FRAME_PACING_BALANCED) {
                r.balancedOnOutput++;
                dropOutputsQueuedForDisplay();
            }
            // A drain that ends on a format change returns it for the check below.
            outIndex = releaseNewestOutput(outIndex, ptsUs, info, outputDrainStartNs);
        } else {
            r.balancedOnVsync++;
            queueOutputForDisplay(outIndex, ptsUs);
        }
        return outIndex;
    }

    void onOutputFormatChanged() {
        LimeLog.info("Output format changed");
        r.outputFormat = r.videoDecoder.getOutputFormat();
        LimeLog.info("New output format: " + r.outputFormat);
        r.vulkanHelper.checkUpscalingOutputFormat();
        checkFullRangeOutputFormat();
        r.reconfigure.updateShownSize();
    }

    private void checkFullRangeOutputFormat() {
        if (r.fullRangeFallbackAttempted || r.vulkanHandedBack || r.vulkanRenderer != null || r.wantVulkan) return;
        boolean hdr = r.currentHdrMode != null ? r.currentHdrMode : r.codecSelector.assumesHdr();
        String reason = FullRangePolicy.outputFallbackReason(r.getPreferredColorRange() == MoonBridge.COLOR_RANGE_FULL,
                hdr, r.dolbyVisionActive, r.packing != null, (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0,
                r.activeDecoderName, r.outputFormat);
        if (reason == null) return;
        r.fullRangeFallbackAttempted = true;
        if (!FullRangePolicy.canUseVulkan(Build.VERSION.SDK_INT, VulkanRendererBridge::isSupported)) {
            LimeLog.warning("Decoder " + r.telemetry.decoderName() + " " + reason + "; Vulkan renderer is unavailable, keeping direct output");
            return;
        }
        synchronized (r.codecRecoveryMonitor) {
            if (r.stopping) return;
            r.wantVulkan = r.rebuildForVulkan = true;
            r.codecRecoveryAttempts = 0;
            LimeLog.warning("Decoder " + r.telemetry.decoderName() + " " + reason + "; switching this stream to the Vulkan renderer");
            r.session.requestRestart("full color range on Vulkan");
        }
    }

    /** The output at index just left the decoder: its timing, and the climb watch of erratum 23. */
    void noteDecoderOutput(int index, long ptsUs, long outputNs) {
        r.vulkanHelper.recordFrameWarpPts(index, ptsUs);
        r.frameLatencyStats.onDecoderOutput(index, ptsUs, outputNs);
        KeyframeSafetyNet safetyNet = r.keyframeSafetyNet;
        if (safetyNet == null) return;
        safetyNet.onDecodeLatency(SystemClock.uptimeMillis(), r.frameLatencyStats.getInputToOutputNs(index) / 1_000_000f,
                r.streamFrameRateMilliHz);
        String event = safetyNet.takeEvent();
        if (event != null) LimeLog.info(event);
    }

    /** Feeds the residency governor with the output at index and logs the queue it saw. */
    void noteResidency(int index) {
        long excessNs = r.residency.onOutput(r.frameLatencyStats.getInputToOutputNs(index), System.nanoTime());
        r.residency.track(excessNs, r.vsyncPacer.contentIntervalNs());
        r.frameLatencyStats.onQueueState(index, DecoderQueuePolicy.depth(r.numFramesIn, r.numFramesOut, r.decoderDepthBase), excessNs);
    }

    /** Feeds the debt policy with the output at index, logs the queue it saw and an ended recovery. */
    private void noteDebt(int index) {
        int depth = DecoderQueuePolicy.depth(r.numFramesIn, r.numFramesOut, r.decoderDepthBase);
        long excessNs;
        if (r.debt.watchesDisplay()) {
            // Decoder output thread only: one lock for the three timings.
            r.frameLatencyStats.readOutputTimes(index, debtTimes);
            excessNs = r.debt.onOutput(debtTimes.inputToOutputNs, debtTimes.submitToOutputNs,
                    debtTimes.outputNs, r.vsyncPacer.contentIntervalNs(), depth);
        } else {
            excessNs = r.debt.onOutput(r.frameLatencyStats.getInputToOutputNs(index), System.nanoTime(),
                    r.vsyncPacer.contentIntervalNs());
        }
        r.frameLatencyStats.onQueueState(index, depth, excessNs);
        logDebtEvent();
    }

    private void logDebtEvent() {
        String event = r.debt.takeEvent();
        if (event != null) LimeLog.info(event);
    }

    private boolean wantsCatchUp(int index, int drained, boolean scheduled) {
        if (drained >= FramePacingPolicy.MAX_OUTPUT_DRAIN) return false;
        int effectiveMode = r.hostFrameTimes ? PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS : r.prefs.framePacing;
        if (FramePacingPolicy.dropQueuedFrames(effectiveMode)) {
            if (effectiveMode == PreferenceConfiguration.FRAME_PACING_BALANCED &&
                    r.vsyncPacer.keepsBurstForRefreshSlot(System.nanoTime())) {
                return false;
            }
            catchUpReason = "catch_up";
            return true;
        }
        int reason = NightjarPolicyJni.decoderWantsCatchUp(r.residency.handle(),
                effectiveMode,
                r.frameLatencyStats.getEnqueueTimeNsForOutput(index), System.nanoTime(),
                r.streamFrameRateMilliHz, drained, FramePacingPolicy.MAX_OUTPUT_DRAIN,
                scheduled, r.numFramesIn, r.numFramesOut, r.decoderDepthBase);
        if (reason == 0) return false;
        catchUpReason = reason == 1 ? "catch_up" : "governor";
        return true;
    }

    int releaseNewestOutput(int outIndex, long ptsUs, BufferInfo info, long outputDrainStartNs) {
        int lastIndex = outIndex;
        long lastPtsUs = ptsUs;
        // Latency mode takes the newest output. Smooth modes only catch up
        // after a backlog exceeds the age budget or a sustained debt, then
        // resume normal order.
        // All frames have already been decoded, preserving codec references.
        int drained = 0;
        boolean shown = true;
        boolean scheduled = FramePacingPolicy.schedulesFrames(r.hostFrameTimes, r.prefs.framePacing);
        boolean smooth = FramePacingPolicy.recoversDecoderDebt(r.hostFrameTimes, r.prefs.framePacing);
        if (scheduled) noteResidency(lastIndex);
        else if (smooth) noteDebt(lastIndex);
        while (!r.stopping && r.codecRecoveryType.get() == MediaCodecDecoderRenderer.CR_RECOVERY_TYPE_NONE &&
                FramePacingPolicy.hasOutputDrainTimeRemaining(
                        SystemClock.elapsedRealtimeNanos() - outputDrainStartNs,
                        r.streamFrameRateMilliHz) &&
                wantsCatchUp(lastIndex, drained, scheduled) &&
                (outIndex = r.videoDecoder.dequeueOutputBuffer(info, 0)) >= 0) {
            noteDecoderOutput(outIndex, info.presentationTimeUs, System.nanoTime());
            r.frameLatencyStats.onOutputDropped(lastIndex, catchUpReason, System.nanoTime(), reportsRenderTimes());
            r.videoDecoder.releaseOutputBuffer(lastIndex, false);
            if (scheduled && "governor".equals(catchUpReason)) {
                r.residency.onDropped();
                r.presentScheduler.onGovernorDropped();
            }

            r.numFramesOut++;

            lastIndex = outIndex;
            lastPtsUs = info.presentationTimeUs;
            drained++;
            if (scheduled) noteResidency(lastIndex);
            else if (smooth) noteDebt(lastIndex);
        }

        if (smooth && drained == 0 && outIndex != MediaCodec.INFO_OUTPUT_FORMAT_CHANGED && !r.stopping &&
                r.codecRecoveryType.get() == MediaCodecDecoderRenderer.CR_RECOVERY_TYPE_NONE && r.debt.shouldDrop() &&
                DecoderQueuePolicy.depth(r.numFramesIn, r.numFramesOut, r.decoderDepthBase) == 0) {
            r.frameLatencyStats.onOutputDropped(lastIndex, "debt", System.nanoTime(), reportsRenderTimes());
            r.videoDecoder.releaseOutputBuffer(lastIndex, false);
            r.debt.onDropped();
            return outIndex;
        }
        if (smooth && outIndex != MediaCodec.INFO_OUTPUT_FORMAT_CHANGED && !r.stopping &&
                r.codecRecoveryType.get() == MediaCodecDecoderRenderer.CR_RECOVERY_TYPE_NONE && r.debt.shouldDrop() &&
                DecoderQueuePolicy.depth(r.numFramesIn, r.numFramesOut, r.decoderDepthBase) >= 1) {
            int newer = r.videoDecoder.dequeueOutputBuffer(info, DecoderQueuePolicy.Debt.WAIT_US);
            if (newer >= 0) {
                long nowNs = System.nanoTime();
                noteDecoderOutput(newer, info.presentationTimeUs, nowNs);
                r.frameLatencyStats.onOutputDropped(lastIndex, "debt", nowNs, reportsRenderTimes());
                r.videoDecoder.releaseOutputBuffer(lastIndex, false);
                r.debt.onDropped();
                r.numFramesOut++;
                lastIndex = newer;
                lastPtsUs = info.presentationTimeUs;
                outIndex = newer;
                noteDebt(lastIndex);
            } else if (newer == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                outIndex = newer;
            } else {
                r.debt.onNoNewer();
                logDebtEvent();
            }
        }

        if (scheduled) {
            lastPtsUs = r.vsyncPacer.releaseScheduled(lastIndex, lastPtsUs, drained > 0);
        } else {
            long releaseNs = System.nanoTime();
            boolean displayQueueDrop = displayQueueDropDue(releaseNs);
            long displayAtNs;
            long handoffAtNs;
            if (r.prefs.framePacing == PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS ||
                    r.prefs.framePacing == PreferenceConfiguration.FRAME_PACING_CAP_FPS) {
                displayAtNs = r.vsyncPacer.smoothDisplayNs(releaseNs);
                handoffAtNs = displayAtNs;
            } else if (r.hostFrameTimes) {
                displayAtNs = r.hostFrameClock.presentNs(lastPtsUs, releaseNs);
                handoffAtNs = displayAtNs;
            } else if (r.prefs.framePacing == PreferenceConfiguration.FRAME_PACING_BALANCED) {
                displayAtNs = displayQueueDrop ? 0 : r.vsyncPacer.planRefreshSlot(releaseNs);
                handoffAtNs = displayQueueDrop ? 0 : r.vsyncPacer.refreshSlotHandoffNs(releaseNs, displayAtNs);
            } else {
                displayAtNs = r.vsyncPacer.heldDisplayNs(releaseNs);
                handoffAtNs = displayAtNs;
            }
            if (displayQueueDrop) {
                dropForDisplayQueue(lastIndex, releaseNs);
                shown = false;
            } else {
                countRenderRelease(lastIndex, releaseNs, handoffAtNs);
                releaseOutputFrame(lastIndex, displayAtNs);
            }
        }
        // Artemis counts the frame it presents. Its decode time ended when the
        // decoder returned it; release and pacing waits are not decode time.
        r.frameLatencyStats.onArtemisDecodeCounted(lastPtsUs);
        // A scheduled frame is only stamped with its due time, never waited for (ScheduledRelease):
        // now is when it was ready in either mode, not when it is shown.
        r.vsyncPacer.reportPhaseSlack(System.nanoTime());

        if (shown) r.activeWindowVideoStats.totalFramesRendered++;
        return outIndex;
    }

    boolean displayQueueDropDue(long nowNs) {
        if (!r.prefs.displayQueueRecovery) return false;
        long refreshNs;
        boolean arr;
        synchronized (r.vsyncMonitor) {
            refreshNs = r.vsyncIntervalNs;
            arr = r.vsyncPacer.adaptiveInUse();
        }
        boolean watched = DisplayQueueWatch.applies(r.prefs.displayQueueRecovery,
                r.prefs.framePacing == PreferenceConfiguration.FRAME_PACING_MIN_LATENCY ||
                        r.prefs.framePacing == PreferenceConfiguration.FRAME_PACING_BALANCED,
                r.hostFrameTimes || FramePacingPolicy.schedulesFrames(r.hostFrameTimes, r.prefs.framePacing),
                r.session.frameRenderedListenerSet && r.upscaler == null && r.vulkanRenderer == null,
                r.prefs.vrr || arr, r.streamFrameRateMilliHz, refreshNs);
        displayQueue.configure(watched, refreshNs);
        boolean drop = watched && displayQueue.takeDrop(nowNs);
        String event = displayQueue.takeEvent();
        if (event != null) LimeLog.info(event);
        return drop;
    }

    /** Releases the output without showing it: the screen keeps the previous frame one refresh longer. */
    void dropForDisplayQueue(int index, long nowNs) {
        r.frameLatencyStats.onOutputDropped(index, "display_queue", nowNs, reportsRenderTimes());
        r.videoDecoder.releaseOutputBuffer(index, false);
    }

    void dropOutputsQueuedForDisplay() {
        while (true) {
            int stale;
            synchronized (r.outputBufferQueue) {
                stale = r.outputBufferQueue.poll();
            }
            if (stale == IntRing.EMPTY) return;
            r.frameLatencyStats.onOutputDropped(stale, "pace_switch", System.nanoTime(), reportsRenderTimes());
            r.videoDecoder.releaseOutputBuffer(stale, false);
        }
    }

    void queueOutputForDisplay(int lastIndex, long lastPtsUs) {
        // Discard the oldest buffer if we've exceeded our limit (producer side, since consumer may lag).
        int queueLimit = r.prefs.dropLateFrames ? 1 : r.outputBufferQueueLimit;
        int droppedIndex;
        synchronized (r.outputBufferQueue) {
            droppedIndex = r.outputBufferQueue.size() >= queueLimit ?
                    r.outputBufferQueue.poll() : IntRing.EMPTY;
            r.outputBufferQueue.add(lastIndex);
        }
        // Publish replacement before releasing old output so the display callback finds it immediately.
        if (droppedIndex != IntRing.EMPTY) {
            r.frameLatencyStats.onOutputDropped(droppedIndex, "replaced", System.nanoTime(), reportsRenderTimes());
            r.videoDecoder.releaseOutputBuffer(droppedIndex, false);
        } else {
            // Artemis counts a queued frame unless queuing it dropped an older one.
            r.frameLatencyStats.onArtemisDecodeCounted(lastPtsUs);
        }
    }

    boolean reportsRenderTimes() {
        return r.upscaler == null;
    }

    private boolean rendersOnRelease() {
        return r.upscaler == null || r.upscalingPolicy.getReason() == UpscalingPolicy.Reason.NONE;
    }

    void countRenderRelease(int index, long releaseNs, long displayAtNs) {
        if (r.upscaler == null) {
            r.frameLatencyStats.onOutputHandedOff(index, releaseNs, displayAtNs, reportsRenderTimes());
        } else {
            r.frameLatencyStats.onOutputReleased(index, releaseNs, rendersOnRelease(), reportsRenderTimes());
        }
    }

    void releaseOutputFrame(int index, long presentationTimeNs) {
        GlesUpscaler current = r.upscaler;
        if (current != null && r.upscalingPolicy.getReason() != UpscalingPolicy.Reason.NONE) {
            r.videoDecoder.releaseOutputBuffer(index, false);
        } else {
            r.videoDecoder.releaseOutputBuffer(index, current == null ? presentationTimeNs :
                    current.drawsFrameWarp() ? current.releaseFrame(presentationTimeNs, r.vulkanHelper.frameWarpPtsUs(index)) :
                            current.releaseFrame(presentationTimeNs));
        }
    }
}
