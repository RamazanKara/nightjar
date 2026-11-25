package com.limelight.binding.video;

import java.time.Instant;
import java.time.temporal.ChronoUnit;
import java.util.Locale;

import android.media.MediaCodec.CodecException;
import android.media.MediaCodecInfo;
import android.media.MediaFormat;
import android.os.Build;
import android.os.SystemClock;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.StreamEngine;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.utils.RecentStreams;

/**
 * Formats the live performance overlay, recent-stream problem-report rows,
 * pacing summaries, and {@link MediaCodecDecoderRenderer.RendererException}
 * diagnostics for {@link MediaCodecDecoderRenderer}.
 */
final class MediaCodecOverlayReport {
    private MediaCodecOverlayReport() {}

    /** This stream's line in the problem report: only device names and counters, no address. */
    static void addRecentStreamRow(MediaCodecDecoderRenderer r) {
        boolean pyroWave = (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0;
        if (r.streamStartMs < 0 || (!pyroWave && r.activeDecoderName == null)) return;
        long startMs = r.streamStartMs;
        r.streamStartMs = -1;
        VulkanRendererBridge vulkan = r.vulkanRenderer;
        // The last stats window's presents, which flipStatsWindow has not taken yet.
        if (vulkan != null) r.activeWindowVideoStats.totalFramesRendered += vulkan.takePresentedFrames();
        VideoStats total = new VideoStats();
        total.add(r.globalVideoStats);
        total.add(r.activeWindowVideoStats);
        KeyframeSafetyNet safetyNet = r.keyframeSafetyNet;
        KeyframeSafetyNet.Counts repairs = r.repairCounts;
        MediaFormat output = r.outputFormat;
        boolean rfi = r.refFrameInvalidationActive;
        boolean lowLatency = r.configuredFormat != null && MediaCodecHelper.asksForAndroidLowLatency(r.configuredFormat);
        String vulkanLine = vulkan != null ? vulkan.getDiagnosticsText() : null;
        if (vulkanLine == null) vulkanLine = r.vulkanHelper.vulkanDiagnostics;
        r.vulkanHelper.vulkanDiagnostics = null;
        String row = Instant.ofEpochMilli(startMs).truncatedTo(ChronoUnit.SECONDS) + " " +
                (SystemClock.uptimeMillis() - r.streamStartUptimeMs) / 1000 + " s" +
                " | " + (pyroWave ? "PyroWave" : r.activeDecoderName + " " + configuredMime(r)) +
                " | " + r.initialWidth + "x" + r.initialHeight + " " + r.refreshRate + " fps" +
                " HDR " + onOff(Boolean.TRUE.equals(r.currentHdrMode)) +
                (r.getPreferredColorRange() == MoonBridge.COLOR_RANGE_FULL ? " full range" : "") +
                " | engine " + StreamEngine.label() +
                " | low latency " + onOff(lowLatency) + ", vendor " + onOff(r.prefs.vendorLowLatency) +
                ", hints " + onOff(r.prefs.phonePerformanceHints) + ", RFI " + onOff(rfi) +
                ", keyframe net " + onOff(safetyNet != null) +
                " | frames in " + r.numFramesIn + " out " + r.numFramesOut + " rendered " + total.totalFramesRendered +
                " lost " + total.framesLost + " in " + total.frameLossEvents + " events" +
                " | " + describePacing(r) +
                " | dropped " + r.frameLatencyStats.describeDrops(r.numFramesOut - total.totalFramesRendered) +
                " | decode avg " + r.telemetry.getAverageDecoderLatency() + " ms" +
                " | keyframes " + r.telemetry.keyframesReceived + ", largest " + r.telemetry.largestKeyframeBytes + " B" +
                (repairs != null ? " | " + repairs.describe() : "") +
                " | input waits " + r.telemetry.inputStalls.total() + ", worst " + r.telemetry.inputStalls.totalMaxMs() + " ms" +
                ", input buffer " + r.telemetry.inputBufferBytes + " B, " + r.telemetry.receiveBufferStats() +
                " | output " + (output == null ? "none" : FullRangePolicy.outputDescription(output)) +
                (vulkanLine != null ? " | " + vulkanLine : "") +
                " | hottest " + thermalName(r.hottestThermalLevel) + ", thermal protection " + onOff(r.prefs.thermalProtection);
        RecentStreams.add(row);
        logStreamSummary(row);
        r.repairCounts = null;
        r.hottestThermalLevel = ThermalMonitor.NORMAL;
    }

    /** The report log keeps 300 characters a line: the row goes in lines of whole " | " parts. */
    private static void logStreamSummary(String row) {
        String[] parts = row.split(" \\| ");
        StringBuilder line = new StringBuilder();
        boolean continued = false;
        for (int i = 0; i < parts.length; i++) {
            if (line.length() > 0 && line.length() + 3 + parts[i].length() > 270) {
                LimeLog.info("Stream summary: " + (continued ? "| " : "") + line + " |");
                line.setLength(0);
                continued = true;
            }
            line.append(line.length() > 0 ? " | " : "").append(parts[i]);
        }
        LimeLog.info("Stream summary: " + (continued ? "| " : "") + line);
    }

    private static String describePacing(MediaCodecDecoderRenderer r) {
        long refreshNs;
        synchronized (r.vsyncMonitor) {
            refreshNs = r.outputVsyncAvailable ? r.vsyncIntervalNs : 0;
        }
        int pacing = r.prefs.framePacing;
        return "pacing " + (pacing == PreferenceConfiguration.FRAME_PACING_MIN_LATENCY ? "lowest latency" :
                pacing == PreferenceConfiguration.FRAME_PACING_BALANCED ? "balanced" :
                pacing == PreferenceConfiguration.FRAME_PACING_CAP_FPS ? "FPS limit" :
                pacing == PreferenceConfiguration.FRAME_PACING_MAX_SMOOTHNESS ? "smoothest" : "mode " + pacing) +
                (pacing == PreferenceConfiguration.FRAME_PACING_BALANCED ? " (release passes as outputs came out " +
                        r.balancedOnOutput + ", outputs queued for vsync " + r.balancedOnVsync + ")" : "") +
                (r.refreshSlots.planned() > 0 ? ", refreshes claimed " + r.refreshSlots.planned() +
                        ", shared " + r.refreshSlots.shared() : "") +
                (r.vulkanOutputs > 0 ? ", Vulkan renderer " + r.vulkanOutputs + " outputs" : "") +
                // Fractional V-Sync, or Smoothest and the FPS limit at a whole multiple (smoothDisplayNs).
                (r.heldAnchorRefreshes > 1 ? ", each frame held " + r.heldAnchorRefreshes + " refreshes" : "") +
                ", PC frame times " + onOff(r.hostFrameTimes) +
                ", " + timedToScreen(r) +
                ", ARR screen " + onOff(r.outputHasArrSupport) + ", fixed timing " + onOff(r.prefs.arrFixedTiming) +
                ", VRR " + onOff(r.prefs.vrr) + ", newest only " + onOff(r.prefs.dropLateFrames) +
                ", queue take-back " + onOff(r.prefs.displayQueueRecovery) +
                ", vsync " + (refreshNs > 0 ? String.format(Locale.ROOT, "%.2f ms", refreshNs / 1e6) : "unknown");
    }

    private static String timedToScreen(MediaCodecDecoderRenderer r) {
        String off = !r.hostPhaseLock ? "other PC" : r.phaseReports > 0 ? null :
                r.telemetry.phaseLockOff != null ? r.telemetry.phaseLockOff :
                r.vulkanOutputs > 0 ? "Vulkan" : r.balancedOnVsync > 0 ? "Balanced queue" : "no reports";
        return "timed to your screen " + (off == null ? "on" : "off (" + off + ")") + ", report batches " + r.phaseReports;
    }

    private static String thermalName(int level) {
        return level == ThermalMonitor.HOT ? "hot" : level == ThermalMonitor.WARM ? "warm" : "normal";
    }

    private static String configuredMime(MediaCodecDecoderRenderer r) {
        return r.configuredFormat == null ? "" : r.configuredFormat.getString(MediaFormat.KEY_MIME);
    }

    private static String onOff(boolean on) {
        return on ? "on" : "off";
    }

    static String formatPacingDropsLine(String drops, long sharedSlots) {
        boolean hasDrops = drops != null && !drops.isEmpty() && !"none".equals(drops);
        if (!hasDrops && sharedSlots <= 0) return null;
        if (!hasDrops) return "Shared refresh slots: " + sharedSlots;
        if (sharedSlots > 0) return "Discarded before display: " + drops + " | shared refresh " + sharedSlots;
        return "Discarded before display: " + drops;
    }

    static void updatePerformanceOverlay(MediaCodecDecoderRenderer r) {
        MediaCodecTelemetry t = r.telemetry;
        VideoStatsFps fps = t.overlayFps;
        if (r.perfListener == null || fps == null) return;
        float[] frameRates = r.frameLatencyStats.getFrameRates(System.nanoTime());
        // Rendered FPS counts frames submitted for display, like other Moonlight clients;
        // compositor confirmations (stage 2) only feed the latency diagnostics.
        String shownFps = MediaCodecTelemetry.metric(PerformanceOverlay.renderedFps(frameRates), " FPS");
        float displayDropPercent = r.frameLatencyStats.getDisplayDropPercent(System.nanoTime());
        boolean mediaCodec = (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) == 0;
        long rttInfo = MoonBridge.getEstimatedRttInfo();
        float networkMs = rttInfo == -1 ? -1 : (int) (rttInfo >> 32);
        float varianceMs = rttInfo == -1 ? -1 : (int) rttInfo;
        String codec = (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0 ? "PyroWave" :
                (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_AV1) != 0 ? "AV1" :
                (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_H265) != 0 ? "HEVC" : "H.264";
        String color = Boolean.TRUE.equals(r.currentHdrMode) ?
                HdrFormatPolicy.overlayWord(r.announcedHdrFormat, r.dolbyVisionActive, r.vulkanHelper.vulkanPassesHdr10Plus()) :
                Sdr10Policy.overlayColor((r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_10BIT) != 0, r.codecSelector.sdr10, r.currentHdrMode);
        String stream = r.reconfigure.streamWidth + "×" + r.reconfigure.streamHeight + " · " + codec + " · " + color
                + " · " + StreamEngine.label();
        StringBuilder video = new StringBuilder();
        video.append(stream).append('\n');
        video.append(r.context.getString(R.string.perf_overlay_resolution_source, r.context.getString(
                r.prefs.resolutionFollowsScreen ?
                        (r.prefs.fromPcProfile ? R.string.resolution_source_native_profile : R.string.resolution_source_native) :
                        (r.prefs.fromPcProfile ? R.string.resolution_source_fixed_profile : R.string.resolution_source_fixed))))
                .append('\n');
        if (r.prefs.layersText != null) {
            video.append(r.prefs.layersText).append('\n');
        }
        if (r.prefs.hostDisplay != null) {
            video.append(r.context.getString(R.string.perf_overlay_host_display,
                    r.context.getString(r.prefs.hostDisplay.label))).append('\n');
        }
        video.append(r.context.getString(R.string.perf_overlay_incomingfps, frameRates[0])).append('\n');
        video.append(r.context.getString(R.string.perf_overlay_shownfps, shownFps)).append('\n');
        if (frameRates[2] >= 0) {
            video.append(r.context.getString(R.string.perf_overlay_onscreenfps,
                    MediaCodecTelemetry.metric(frameRates[2], " FPS"))).append('\n');
        }
        video.append(PerformanceOverlay.displayDropsText(r.context, displayDropPercent)).append('\n');
        long vsyncNs;
        synchronized (r.vsyncMonitor) {
            vsyncNs = r.vsyncIntervalNs;
        }
        String pacing = PerformanceOverlay.presentJitterText(r.context, r.frameLatencyStats.presentJitter(vsyncNs),
                frameRates[0]);
        if (pacing != null) video.append(pacing).append('\n');
        // Packed full chroma travels as a 4:2:0 stream twice as tall, but the picture shown is 4:4:4.
        video.append(r.packing != null ? "YUV 4:4:4 (packed)" :
                (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_YUV444) != 0 ? "YUV 4:4:4" : "YUV 4:2:0");
        if ((r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_10BIT) != 0) {
            video.append(" 10-bit");
        }
        Locale locale = r.context.getResources().getConfiguration().locale;
        String wire = PerformanceOverlay.wireText(r.context, t.wireMbps);
        String network = r.context.getString(R.string.stats_throughput,
                PerformanceOverlay.measurement(locale, t.receivedVideoMbps, "%.1f"),
                PerformanceOverlay.measurement(locale, fps.lossPercent, "%.2f")) + '\n' +
                (fps.decoderDrops > 0 ? r.context.getString(R.string.perf_overlay_decoder_drops, fps.decoderDrops) + '\n' : "") +
                (wire != null ? wire + '\n' : "") +
                r.context.getString(R.string.stats_network,
                        PerformanceOverlay.measurement(locale, networkMs, "%.0f"),
                        PerformanceOverlay.measurement(locale, varianceMs, "%.0f"));
        StringBuilder decode = new StringBuilder(t.decoderDiagnostics);
        decode.append('\n').append(r.context.getString(R.string.perf_overlay_streaming_engine, StreamEngine.summary()));
        // The timing block below already lists decode time; show it here only without that block.
        if (mediaCodec && t.latencyOverlay.isEmpty()) {
            decode.append('\n').append(t.shownDecodeTimeMs >= 0 ? r.context.getString(R.string.perf_overlay_dectime, t.shownDecodeTimeMs) :
                    r.context.getString(R.string.latency_input_output) + ": " + r.context.getString(R.string.latency_unavailable));
        }
        if (!mediaCodec) {
            decode.append('\n').append(r.context.getString(R.string.perf_overlay_pyrowave_submit,
                    PerformanceOverlay.measurement(locale, t.shownDecodeTimeMs, "%.2f")));
            int gpuUs = r.pyroWaveRenderer.getLastGpuDecodeUs();
            float[] waits = r.pyroWaveRenderer.getWaits();
            decode.append('\n').append(MediaCodecTelemetry.formatPyroWaveStats(t.pyroWaveLossPercent, t.pyroWaveQueueDelayMs, gpuUs))
                    .append("\nWaits per frame: free planes ").append(MediaCodecTelemetry.metric(waits[0], " ms"))
                    .append(" | screen image ").append(MediaCodecTelemetry.metric(waits[1], " ms"))
                    .append("\nReplaced by a newer frame: before decode ").append(MediaCodecTelemetry.metric(t.pyroWaveSkippedPercent, "%"))
                    .append(" | before display ").append(MediaCodecTelemetry.metric(waits[2], "%"));
            String hostTimes = PyroWaveDecoderRenderer.hostTimesLine(waits[3]);
            if (hostTimes != null) decode.append('\n').append(hostTimes);
            if (MoonBridge.isPyroWaveEarlyStream()) {
                decode.append('\n').append(PyroWaveDecoderRenderer.earlyRecordsLine(r.pyroWaveRenderer.getEarlyRecords()));
            }
        }
        int leadUs = MoonBridge.getPhaseLockLeadUs();
        if (leadUs != Integer.MIN_VALUE) {
            decode.append("\nTimed to your screen: ready ").append(MediaCodecTelemetry.metric(leadUs / 1000f, " ms")).append(" before its refresh");
        }
        String hostFrameLine = r.hostFrameTimes && mediaCodec ? r.hostFrameClock.overlayLine() : null;
        if (hostFrameLine != null) decode.append('\n').append(hostFrameLine);
        String schedulerLine = r.hostFrameTimes && mediaCodec ? r.presentScheduler.overlayLine() : null;
        if (schedulerLine != null) decode.append('\n').append(schedulerLine);
        String dropsLine = mediaCodec ?
                formatPacingDropsLine(r.frameLatencyStats.describeDrops(0), r.refreshSlots.shared()) : null;
        if (dropsLine != null) decode.append('\n').append(dropsLine);
        decode.append('\n').append(r.vulkanHelper.upscalingStats());
        decode.append('\n').append(t.thermalStats());
        decode.append('\n').append(t.latencyOverlay);
        double[][] timings = t.latencySummary;
        r.perfListener.onPerfUpdate(video.toString(), network, decode.toString(),
                PerformanceOverlay.withDecoderDrops(r.context,
                        PerformanceOverlay.statisticsText(r.context, stream, PerformanceOverlay.renderedFps(frameRates),
                                frameRates[0], t.receivedVideoMbps, t.wireMbps, fps.lossPercent,
                                networkMs, varianceMs, t.shownHostProcessingMs,
                                PerformanceOverlay.average(timings, FrameLatencyStats.HOST_SENT_STAGE), t.shownDecodeTimeMs,
                                PerformanceOverlay.average(timings, 2), !mediaCodec),
                        fps.decoderDrops),
                PerformanceOverlay.compactText(r.context, PerformanceOverlay.renderedFps(frameRates),
                        PerformanceOverlay.healthFps(frameRates), frameRates[0],
                        PerformanceOverlay.panelHz(vsyncNs, r.outputDisplay == null ? -1 :
                                r.outputDisplay.getMode().getRefreshRate()),
                        networkMs,
                        fps.lossPercent, t.getDecodeTimeMs()));
    }

    static String formatRendererException(MediaCodecDecoderRenderer renderer, Exception originalException, String delimiter) {
        String str;
        if ((renderer.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_AV1) != 0) {
            if (renderer.numFramesIn == 0) {
                str = "PreIFrameError";
            } else if (renderer.outputFormat == null) {
                str = "PreOutputConfigError";
            } else if (renderer.numFramesOut == 0) {
                str = "PreOutputError";
            } else if (renderer.numFramesOut <= renderer.refreshRate * 30) {
                str = "EarlyOutputError";
            } else {
                str = "ErrorWhileStreaming";
            }
        } else if (renderer.numVpsIn == 0 && renderer.numSpsIn == 0 && renderer.numPpsIn == 0) {
            str = "PreSPSError";
        } else if (renderer.numSpsIn > 0 && renderer.numPpsIn == 0) {
            str = "PrePPSError";
        } else if (renderer.numPpsIn > 0 && renderer.numFramesIn == 0) {
            str = "PreIFrameError";
        } else if (renderer.numFramesIn > 0 && renderer.outputFormat == null) {
            str = "PreOutputConfigError";
        } else if (renderer.outputFormat != null && renderer.numFramesOut == 0) {
            str = "PreOutputError";
        } else if (renderer.numFramesOut <= renderer.refreshRate * 30) {
            str = "EarlyOutputError";
        } else {
            str = "ErrorWhileStreaming";
        }

        // The decoders that run the format: 10-bit SDR may use other ones than the 8-bit stream.
        MediaCodecInfo hevcShown = (renderer.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_H265) != 0 ?
                renderer.decoderFor(renderer.videoFormat) : renderer.hevcDecoder;
        MediaCodecInfo av1Shown = (renderer.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_AV1) != 0 ?
                renderer.decoderFor(renderer.videoFormat) : renderer.av1Decoder;
        str += "Format: " + String.format("%x", renderer.videoFormat) + delimiter;
        str += "AVC Decoder: " + ((renderer.avcDecoder != null) ? renderer.avcDecoder.getName() : "(none)") + delimiter;
        str += "HEVC Decoder: " + ((hevcShown != null) ? hevcShown.getName() : "(none)") + delimiter;
        str += "AV1 Decoder: " + ((av1Shown != null) ? av1Shown.getName() : "(none)") + delimiter;
        str += codecCapabilities("AVC", "video/avc", renderer.avcDecoder, renderer.initialWidth, renderer.initialHeight, delimiter);
        str += codecCapabilities("HEVC", "video/hevc", hevcShown, renderer.initialWidth, renderer.initialHeight, delimiter);
        str += codecCapabilities("AV1", "video/av01", av1Shown, renderer.initialWidth, renderer.initialHeight, delimiter);
        str += "Configured format: " + renderer.configuredFormat + delimiter;
        str += "Input format: " + renderer.session.inputFormat + delimiter;
        str += "Output format: " + renderer.outputFormat + delimiter;
        str += "Adaptive playback: " + renderer.adaptivePlayback + delimiter;
        str += "GL Renderer: " + renderer.glRenderer + delimiter;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            str += "SOC: " + Build.SOC_MANUFACTURER + " - " + Build.SOC_MODEL + delimiter;
            str += "Performance class: " + Build.VERSION.MEDIA_PERFORMANCE_CLASS + delimiter;
        }
        str += "Consecutive crashes: " + renderer.consecutiveCrashCount + delimiter;
        str += "RFI active: " + renderer.refFrameInvalidationActive + delimiter;
        str += "Using modern SPS patching: " + (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) + delimiter;
        str += "Fused IDR frames: " + renderer.decodeLoop.fusedIdrFrame + delimiter;
        str += "Video dimensions: " + renderer.initialWidth + "x" + renderer.initialHeight + delimiter;
        str += "FPS target: " + renderer.refreshRate + delimiter;
        str += "Bitrate: " + renderer.prefs.bitrate + " Kbps" + delimiter;
        str += "CSD stats: " + renderer.numVpsIn + ", " + renderer.numSpsIn + ", " + renderer.numPpsIn + delimiter;
        str += "Frames in-out: " + renderer.numFramesIn + ", " + renderer.numFramesOut + delimiter;
        str += "Total frames received: " + renderer.globalVideoStats.totalFramesReceived + delimiter;
        str += "Total frames rendered: " + renderer.globalVideoStats.totalFramesRendered + delimiter;
        str += "Frame losses: " + renderer.globalVideoStats.framesLost + " in " + renderer.globalVideoStats.frameLossEvents + " loss events" + delimiter;
        str += "Average end-to-end client latency: " + renderer.getAverageEndToEndLatency() + "ms" + delimiter;
        str += "Average hardware decoder latency: " + renderer.frameLatencyStats.getAverageDecoderLatency() + "ms" + delimiter;
        str += "Frame pacing mode: " + renderer.prefs.framePacing + delimiter;

        if (originalException instanceof CodecException) {
            CodecException ce = (CodecException) originalException;
            str += "Diagnostic Info: " + ce.getDiagnosticInfo() + delimiter;
            str += "Recoverable: " + ce.isRecoverable() + delimiter;
            str += "Transient: " + ce.isTransient() + delimiter;
            str += "Codec Error Code: " + ce.getErrorCode() + delimiter;
        }

        str += originalException.toString();
        return str;
    }

    private static String codecCapabilities(String label, String mime, MediaCodecInfo codec, int width, int height, String delimiter) {
        if (codec == null) return "";
        MediaCodecInfo.VideoCapabilities caps = codec.getCapabilitiesForType(mime).getVideoCapabilities();
        String s = label + " supported width range: " + caps.getSupportedWidths() + delimiter;
        try {
            return s + label + " achievable FPS range: " + caps.getAchievableFrameRatesFor(width, height) + delimiter;
        } catch (IllegalArgumentException e) {
            return s + label + " achievable FPS range: UNSUPPORTED!" + delimiter;
        }
    }
}
