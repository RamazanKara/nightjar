package com.limelight.binding.video;

import java.io.BufferedWriter;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicLong;

import android.app.ActivityManager;
import android.content.Context;
import android.media.MediaFormat;
import android.os.Handler;
import android.os.HandlerThread;
import android.os.Process;
import android.os.SystemClock;

import com.limelight.LimeLog;
import com.limelight.R;
import com.limelight.nvstream.av.video.HostTimingReport;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;
import com.limelight.utils.ProblemReport;

/**
 * Manages latency CSV capture, stream health sampling, decoder diagnostics,
 * statistics window flips, and failure reporting for {@link MediaCodecDecoderRenderer}.
 */
final class MediaCodecTelemetry {
    private static final long MAX_LATENCY_CSV_BYTES = 8 * 1024 * 1024;

    private final MediaCodecDecoderRenderer r;

    // Guards decoderFailure; never held while waiting for codecRecoveryMonitor
    private final Object decoderFailureLock = new Object();
    volatile boolean decoderFailedWithVulkan;

    VideoStats lastWindowVideoStats;
    volatile VideoStatsFps overlayFps;
    final InputStallLine inputStalls = new InputStallLine();
    volatile int keyframesReceived;
    volatile int largestKeyframeBytes;
    volatile int inputBufferBytes;
    volatile float networkFrameLossPercent;
    final AtomicLong receivedVideoBytes = new AtomicLong();
    private long bitrateWindowStartNs;
    private long bitrateWindowStartBytes;
    volatile float receivedVideoMbps = -1;
    // Every video and audio datagram received, FEC parity included, over the same window.
    private long wireWindowStartBytes = -1;
    volatile float wireMbps = -1;

    float pyroWaveLossTotal;
    int pyroWaveLossSamples;
    float pyroWaveQueueDelayTotal;
    int pyroWaveQueueSamples;
    int pyroWaveSkippedFrames;
    int pyroWaveDecodedFrames;
    volatile float pyroWaveSkippedPercent = -1;
    volatile float pyroWaveLossPercent = -1;
    volatile float pyroWaveQueueDelayMs = -1;
    final QueueDelayTracker queueDelay = new QueueDelayTracker();
    volatile long lateFramesClockOffsetUs;
    private volatile float queueDelayMs = -1;
    private volatile float decodeTimeMs = -1;
    volatile float shownDecodeTimeMs = -1;
    volatile float shownHostProcessingMs = -1;
    final PyroWaveSubmissionTiming pyroWaveSubmissionTiming = new PyroWaveSubmissionTiming();
    ThermalMonitor thermalMonitor;
    private volatile boolean clocksEasedByHeat;
    private volatile long videoReceiveBuffer = -1;

    private HandlerThread latencyThread;
    final AtomicBoolean phaseFlushQueued = new AtomicBoolean();
    final Runnable flushPhaseLockReport = new Runnable() {
        @Override
        public void run() {
            phaseFlushQueued.set(false);
            if (!r.stopping) MoonBridge.flushPhaseLockReport();
        }
    };
    volatile String latencyOverlay = "";
    volatile double[][] latencySummary = new double[10][5];
    volatile String decoderDiagnostics = "";
    volatile String phaseLockOff;

    MediaCodecTelemetry(MediaCodecDecoderRenderer renderer) {
        this.r = renderer;
    }

    void runLatencyTick(Runnable self) {
        long nowNs = System.nanoTime();
        long bytes = receivedVideoBytes.get();
        long wireBytes = MoonBridge.getWireBytesReceived();
        if (bitrateWindowStartNs != 0) {
            receivedVideoMbps = PerformanceOverlay.videoMbps(bytes - bitrateWindowStartBytes, nowNs - bitrateWindowStartNs);
            wireMbps = wireWindowStartBytes >= 0 && wireBytes >= wireWindowStartBytes
                    ? PerformanceOverlay.videoMbps(wireBytes - wireWindowStartBytes, nowNs - bitrateWindowStartNs) : -1;
        }
        bitrateWindowStartNs = nowNs;
        bitrateWindowStartBytes = bytes;
        wireWindowStartBytes = wireBytes;
        noteVideoReceiveBuffer(MoonBridge.getVideoReceiveBufferInfo());
        r.thermalLevel = thermalMonitor.level();
        r.hottestThermalLevel = Math.max(r.hottestThermalLevel, r.thermalLevel);
        // Forced max clocks on a hot phone only bring on harder throttling (with Thermal protection on).
        if (ThermalMonitor.forAdaptation(r.prefs.thermalProtection, r.thermalLevel) == ThermalMonitor.HOT &&
                r.pyroWaveRenderer.hasMaxClocks()) {
            r.pyroWaveRenderer.restoreClocks();
            clocksEasedByHeat = true;
            updateDecoderDiagnostics();
        }
        r.pyroWaveRenderer.pollRenderedFrames(r.frameLatencyStats);
        r.vulkanHelper.pollVulkan();
        r.recovery.recordVendorKeyHang();
        r.vulkanHelper.logScanoutLatency(System.nanoTime());
        r.frameLatencyStats.expire(System.nanoTime());
        flushLatencyCsv();
        // The overlay can be toggled mid-stream; present intervals are recorded only while it shows.
        r.frameLatencyStats.setPresentJitterEnabled(r.prefs.enablePerfOverlay);
        if (r.prefs.enablePerfOverlay) {
            latencySummary = r.frameLatencyStats.summarizeWithMedian();
            StringBuilder text = new StringBuilder(formatLatencyOverlay(r.context, latencySummary, shownDecodeTimeMs,
                    (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0));
            if (r.latencyCsv == null) {
                text.append('\n').append(r.context.getString(R.string.latency_csv_failed));
            } else if (r.frameLatencyStats.getCsvRowsLost() != 0) {
                text.append('\n').append(r.context.getString(R.string.latency_csv_lost, r.frameLatencyStats.getCsvRowsLost()));
            }
            latencyOverlay = text.toString();
            MediaCodecOverlayReport.updatePerformanceOverlay(r);
        }
        r.latencyHandler.postDelayed(self, 1000);
    }

    void startLatencyThread() {
        // Default, not background: the decode thread shares the timing lock with this one.
        latencyThread = new HandlerThread("Video - Latency", Process.THREAD_PRIORITY_DEFAULT);
        latencyThread.start();
        r.latencyHandler = new Handler(latencyThread.getLooper());
        r.latencyHandler.post(new Runnable() {
            @Override
            public void run() {
                try {
                    openLatencyCsv(true);
                } catch (IOException e) {
                    LimeLog.warning("Unable to open latency CSV: " + e);
                    closeLatencyCsv();
                }
                r.updateLatencyStats.run();
            }
        });
    }

    void stopLatencyThread() {
        if (r.latencyHandler != null) {
            r.latencyHandler.removeCallbacks(flushPhaseLockReport);
            phaseFlushQueued.set(false);
            r.latencyHandler.post(new Runnable() {
                @Override
                public void run() {
                    r.latencyHandler.removeCallbacks(r.updateLatencyStats);
                    r.frameLatencyStats.discardPending("stream_ended");
                    flushLatencyCsv();
                    File dir = r.context.getFilesDir();
                    File tmp = new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME + ".tmp");
                    File prev = new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME + ".prev");
                    File out = new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME);
                    if (closeLatencyCsv()) {
                        File source = r.latencyCsvBytes <= FrameLatencyStats.CSV_HEADER.length() && prev.isFile() ? prev : tmp;
                        if (!source.renameTo(out)) {
                            LimeLog.warning("Unable to save completed latency CSV");
                        }
                    }
                    tmp.delete();
                    prev.delete();
                    latencyThread.quitSafely();
                }
            });
            boolean interrupted = false;
            while (latencyThread.isAlive()) {
                try {
                    latencyThread.join();
                } catch (InterruptedException e) {
                    interrupted = true;
                }
            }
            r.latencyHandler = null;
            latencyThread = null;
            if (interrupted) {
                Thread.currentThread().interrupt();
            }
        }
    }

    void openLatencyCsv() throws IOException {
        openLatencyCsv(false);
    }

    private void openLatencyCsv(boolean resetPrev) throws IOException {
        File dir = r.context.getFilesDir();
        if (resetPrev) {
            new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME + ".prev").delete();
        }
        r.latencyCsv = new BufferedWriter(new OutputStreamWriter(new FileOutputStream(
                new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME + ".tmp")), StandardCharsets.UTF_8));
        r.latencyCsv.write(FrameLatencyStats.CSV_HEADER);
        r.latencyCsvBytes = FrameLatencyStats.CSV_HEADER.length();
    }

    void flushLatencyCsv() {
        if (r.latencyCsv != null) {
            try {
                if (r.frameLatencyStats.hasCompletedFrames() && r.latencyCsvBytes >= MAX_LATENCY_CSV_BYTES) {
                    closeLatencyCsv();
                    File dir = r.context.getFilesDir();
                    File tmp = new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME + ".tmp");
                    File prev = new File(dir, MediaCodecDecoderRenderer.LATENCY_CSV_NAME + ".prev");
                    if (prev.exists()) prev.delete();
                    if (!tmp.renameTo(prev)) {
                        LimeLog.warning("Unable to rotate previous latency CSV");
                    }
                    openLatencyCsv(false);
                }
                r.latencyCsvBytes += r.frameLatencyStats.writeCsv(r.latencyCsv, r.hostFrameClock::clockOffset);
                r.latencyCsv.flush();
            } catch (IOException e) {
                LimeLog.warning("Unable to write latency CSV: " + e);
                closeLatencyCsv();
            }
        }
    }

    boolean closeLatencyCsv() {
        if (r.latencyCsv != null) {
            try {
                r.latencyCsv.close();
            } catch (IOException e) {
                LimeLog.warning("Unable to close latency CSV: " + e);
                return false;
            } finally {
                r.latencyCsv = null;
            }
            return true;
        }
        return false;
    }

    void failDecoder(String reason, Exception cause) {
        synchronized (r.codecRecoveryMonitor) {
            reason = markDecoderFailed(reason);
            if (reason == null) return;
            r.codecRecoveryThreadQuiescedFlags = 0;
            r.codecRecoveryMonitor.notifyAll();
        }
        reportDecoderFailure(reason, cause);
    }

    String markDecoderFailed(String reason) {
        synchronized (decoderFailureLock) {
            if (r.decoderFailure != null) return null;
            ActivityManager.MemoryInfo memory = lowMemory();
            if (memory != null) {
                reason += "; this device is low on memory (" + memory.availMem / (1024 * 1024) + " MB free)";
            }
            r.decoderFailure = reason;
            decoderFailedWithVulkan = r.vulkanRenderer != null;
            r.stopping = true;
            r.codecRecoveryType.set(MediaCodecDecoderRenderer.CR_RECOVERY_TYPE_NONE);
            return reason;
        }
    }

    void reportDecoderFailure(String reason, Exception cause) {
        LimeLog.severe("Decoder failed, ending the stream: " + reason);
        if (r.crashListener != null) {
            r.crashListener.notifyCrash(new MediaCodecDecoderRenderer.RendererException(r,
                    cause != null ? cause : new IllegalStateException(reason)));
        }
        MoonBridge.bridgeClConnectionTerminated(MoonBridge.CLIENT_ERROR_DECODER);
    }

    private ActivityManager.MemoryInfo lowMemory() {
        ActivityManager am = r.context != null ? (ActivityManager) r.context.getSystemService(Context.ACTIVITY_SERVICE) : null;
        if (am == null) return null;
        ActivityManager.MemoryInfo memory = new ActivityManager.MemoryInfo();
        am.getMemoryInfo(memory);
        return memory.lowMemory ? memory : null;
    }

    String decoderName() {
        String name = r.activeDecoderName;
        return name != null ? name : "The decoder";
    }

    void onHostTiming(HostTimingReport report) {
        r.frameLatencyStats.onHostTiming(report, System.nanoTime());
        for (int i = 0; i < report.size(); i++) {
            HostTimingReport.Record record = report.get(i);
            r.lateFrames.report(record.frame, record.hostUs, record.sendUs);
        }
    }

    static String formatLatencyOverlay(Context context, double[][] summary, float decodeAverageMs, boolean pyroWave) {
        boolean hostReports = summary.length > FrameLatencyStats.HOST_SENT_STAGE && summary[FrameLatencyStats.HOST_SENT_STAGE][0] > 0;
        int[] labels = { R.string.latency_receive_render, R.string.latency_queue_wait,
                pyroWave ? R.string.latency_pyrowave_completed_decode : R.string.latency_input_output, R.string.latency_output_render,
                R.string.latency_host_processing, R.string.latency_host_processing_send };
        int[] stages = {3, 0, 1, 2, 4, FrameLatencyStats.HOST_SENT_STAGE};
        StringBuilder text = new StringBuilder(context.getString(R.string.latency_header, FrameLatencyStats.WINDOW_SIZE));
        for (int line = 0; line < labels.length; line++) {
            int stage = stages[line];
            if (stage == FrameLatencyStats.HOST_SENT_STAGE && !hostReports) {
                continue;
            }
            if (stage == 4) {
                text.append("\n\n").append(context.getString(R.string.overlay_host));
            }
            text.append('\n').append(context.getString(labels[line])).append(": ");
            if (summary[stage][0] == 0) {
                text.append(context.getString(R.string.latency_unavailable));
            } else {
                double average = stage == 1 && !pyroWave && decodeAverageMs >= 0 ? decodeAverageMs : summary[stage][1];
                text.append(context.getString(R.string.latency_values, average,
                        summary[stage][4], summary[stage][2], (int) summary[stage][0]));
            }
        }
        if (summary.length > 7 && summary[5][0] > 0 && summary[6][0] > 0 && summary[7][0] > 0) {
            text.append('\n').append(context.getString(R.string.latency_host_split,
                    summary[5][1], summary[6][1], summary[7][1]));
        }
        return text.toString();
    }

    float getPyroWaveLossPercent() { return Math.max(networkFrameLossPercent, pyroWaveLossPercent); }
    float getPyroWaveQueueDelayMs() { return pyroWaveQueueDelayMs; }
    float getDecodeTimeMs() { return decodeTimeMs; }
    float getQueueDelayMs() { return queueDelayMs; }

    com.limelight.utils.StreamHealth.Sample healthSample(long nowMs, int fps, float jitterMs) {
        r.vulkanHelper.drainVulkanPresentReports(r.vulkanRenderer);
        long nowNs = System.nanoTime();
        double[][] timing = r.frameLatencyStats.summarizeRecent(nowNs, 2_000_000_000L);
        float[] rates = r.frameLatencyStats.getFrameRates(nowNs);
        boolean hostReports = timing[FrameLatencyStats.HOST_SENT_STAGE][0] > 0;
        int hostStage = hostReports ? FrameLatencyStats.HOST_SENT_STAGE : 4;
        float hostP95Ms = timing[hostStage][0] > 0 ? (float) timing[hostStage][2] : -1;
        return new com.limelight.utils.StreamHealth.Sample(nowMs, r.hasRecentVideoFrames(nowMs), fps,
                (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0 ? r.getPyroWaveLossPercent() : r.getNetworkFrameLossPercent(),
                jitterMs, r.getDecodeTimeMs(), hostP95Ms,
                timing[2][0] > 0 ? (float) timing[2][2] : -1, rates[0], rates[1], rates[2], r.getThermalLevel(),
                hostReports, r.lateFrames.tick(lateFramesNowUs(nowNs)),
                timing[FrameLatencyStats.SCREEN_STAGE][0] > 0 ? (float) timing[FrameLatencyStats.SCREEN_STAGE][2] : -1);
    }

    void skipHealthSample() {
        r.lateFrames.tick(lateFramesNowUs(System.nanoTime()));
    }

    private long lateFramesNowUs(long nowNs) {
        return nowNs / 1000 + lateFramesClockOffsetUs;
    }

    void noteVideoReceiveBuffer(long info) {
        if (info == -1 || info == videoReceiveBuffer) return;
        videoReceiveBuffer = info;
        LimeLog.info("Video receive buffer: the system granted " + (info & 0xffffffffL) / 1024 +
                " KiB of the " + (info >>> 32) / 1024 + " KiB asked");
    }

    String receiveBufferStats() {
        long info = videoReceiveBuffer;
        return info == -1 ? "receive buffer unknown" :
                "receive buffer granted " + (info & 0xffffffffL) / 1024 + " of " + (info >>> 32) / 1024 + " KiB";
    }

    String thermalStats() {
        float headroom = thermalMonitor.headroom();
        int state = thermalState(r.thermalLevel, r.heatHoldsBitrate, r.heatEasesLoad || clocksEasedByHeat);
        return r.context.getString(R.string.perf_overlay_thermal, r.context.getString(state)) +
                (headroom >= 0 ? String.format(Locale.US, " (%.2f)", headroom) : "");
    }

    static int thermalState(int level, boolean holdsBitrate, boolean easesLoad) {
        int normalized = level == ThermalMonitor.HOT ? 2 : (level == ThermalMonitor.WARM ? 1 : 0);
        switch (NightjarPolicyJni.videoThermalState(normalized, holdsBitrate, easesLoad)) {
            case NightjarPolicyJni.THERMAL_STATE_HOT_EASING: return R.string.perf_overlay_thermal_hot_easing;
            case NightjarPolicyJni.THERMAL_STATE_HOT: return R.string.perf_overlay_thermal_hot;
            case NightjarPolicyJni.THERMAL_STATE_WARM_HOLDING: return R.string.perf_overlay_thermal_warm_holding;
            case NightjarPolicyJni.THERMAL_STATE_WARM: return R.string.perf_overlay_thermal_warm;
            default: return R.string.perf_overlay_thermal_normal;
        }
    }

    static String formatPyroWaveStats(float recordLoss, float queueMs, int gpuUs) {
        return "PyroWave records missing: " + metric(recordLoss, "%") +
                "\nQueue: " + metric(queueMs, " ms") +
                (gpuUs > 0 ? " | GPU decode (last): " + metric(gpuUs / 1000.0f, " ms") : "");
    }

    static String metric(float value, String unit) {
        return value >= 0 && Float.isFinite(value) ?
                String.format(Locale.getDefault(), "%.2f%s", value, unit) : "unavailable";
    }

    void updateDecoderDiagnostics() {
        if ((r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0) {
            decoderDiagnostics = r.context.getString(R.string.perf_overlay_decoder,
                    "PyroWave (Vulkan, " + r.pyroWaveRenderer.getPresentMode() + ")") + '\n' +
                    r.context.getString(R.string.perf_overlay_gpu_driver, r.pyroWaveRenderer.getDriver()) +
                    (r.pyroWaveRenderer.hasMaxClocks() ? '\n' + r.context.getString(R.string.title_gpu_max_clocks) + ": " +
                            r.context.getString(R.string.stream_enabled) : "");
        } else {
            String mimeType = r.configuredFormat.getString(MediaFormat.KEY_MIME);
            String[] options = MediaCodecHelper.getDecoderLowLatencyOptions(r.configuredFormat, r.session.inputFormat,
                    MediaCodecHelper.getIntegerVendorParameters(r.videoDecoder.getName()),
                    MediaCodecHelper.decoderSupportsAndroidRLowLatency(r.videoDecoder.getCodecInfo(), mimeType));
            int mode;
            switch (options[2]) {
                case "android+vendor": mode = R.string.perf_overlay_low_latency_android_vendor; break;
                case "android": mode = R.string.perf_overlay_low_latency_android; break;
                case "vendor": mode = R.string.perf_overlay_low_latency_vendor; break;
                case "requested": mode = R.string.perf_overlay_low_latency_requested; break;
                default: mode = R.string.perf_overlay_low_latency_off; break;
            }
            decoderDiagnostics = r.context.getString(R.string.perf_overlay_decoder,
                    r.videoDecoder.getName() + " (" + mimeType + ")") + '\n' +
                    r.context.getString(R.string.perf_overlay_low_latency, r.context.getString(mode)) + '\n' +
                    r.context.getString(R.string.perf_overlay_low_latency_keys, options[0]) + '\n' +
                    r.context.getString(R.string.perf_overlay_low_latency_unconfirmed, options[1]);
        }
        decoderDiagnostics += '\n' + r.context.getString(R.string.perf_overlay_phone_hints,
                r.context.getString(r.prefs.phonePerformanceHints ? R.string.stream_enabled : R.string.stream_disabled));
        LimeLog.info(decoderDiagnostics);
        MediaCodecDecoderRenderer.HungKeys hung = r.session.hungKeysOfConfigured();
        String keyReport = (r.videoDecoder == null || (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0) ? "" :
                "\nTuning keys applied: " + describeKeys(r.session.lowLatencyKeysApplied) +
                "\nTuning keys dropped after rejected configures: " + describeKeys(r.session.lowLatencyKeysDropped) +
                        (hung != null ? "\nTuning keys dropped after a hang" +
                                (hung.earlierStream ? " in an earlier stream" : "") + ": " +
                                describeKeys(hung.keys) : "") +
                        (r.session.lowLatencyKeysExperimental
                                ? "\nVendor keys not run on this hardware" : "");
        ProblemReport.setDecoderDiagnostics(r.initialWidth + "x" + r.initialHeight + " @ " + r.refreshRate + " FPS\n" +
                decoderDiagnostics + keyReport);
    }

    static String describeKeys(List<String> keys) {
        return keys.isEmpty() ? "none" : String.join(", ", keys);
    }

    static long recordFrameGap(VideoStats window, int gap, long newDecoderDrops) {
        long packed = NightjarPolicyJni.videoRecordFrameGap(gap, newDecoderDrops);
        int decoder = (int) packed;
        int lost = (int) (packed >>> 32);
        window.totalFrames += gap;
        window.decoderDrops += decoder;
        if (lost > 0) {
            window.framesLost += lost;
            window.frameLossEvents++;
        }
        return decoder;
    }

    static void recordHostProcessing(VideoStats window, char frameHostProcessingLatency) {
        if (frameHostProcessingLatency != 0) {
            if (window.minHostProcessingLatency != 0) {
                window.minHostProcessingLatency = (char) Math.min(window.minHostProcessingLatency, frameHostProcessingLatency);
            } else {
                window.minHostProcessingLatency = frameHostProcessingLatency;
            }
            window.framesWithHostProcessingLatency += 1;
        }
        window.maxHostProcessingLatency = (char) Math.max(window.maxHostProcessingLatency, frameHostProcessingLatency);
        window.totalHostProcessingLatency += frameHostProcessingLatency;
    }

    void flipStatsWindow() {
        pyroWaveLossPercent = pyroWaveLossSamples == 0 ? -1 : pyroWaveLossTotal / pyroWaveLossSamples;
        pyroWaveLossTotal = 0;
        pyroWaveLossSamples = 0;
        if (pyroWaveQueueSamples != 0) {
            pyroWaveQueueDelayMs = pyroWaveQueueDelayTotal / pyroWaveQueueSamples;
            pyroWaveQueueDelayTotal = 0;
            pyroWaveQueueSamples = 0;
        }
        if (pyroWaveSkippedFrames + pyroWaveDecodedFrames != 0) {
            pyroWaveSkippedPercent = 100f * pyroWaveSkippedFrames / (pyroWaveSkippedFrames + pyroWaveDecodedFrames);
            pyroWaveSkippedFrames = pyroWaveDecodedFrames = 0;
        }
        r.vulkanHelper.onFlipStatsWindow();
        decodeTimeMs = r.frameLatencyStats.takeDecodeTimeMs();
        boolean pyroWave = (r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0;
        if (!pyroWave) {
            r.activeWindowVideoStats.decoderTimeMs += r.frameLatencyStats.takeArtemisDecodeMs();
        }
        queueDelayMs = queueDelay.takeWindowMs();
        VideoStats lastTwo = new VideoStats();
        lastTwo.add(lastWindowVideoStats);
        lastTwo.add(r.activeWindowVideoStats);
        shownDecodeTimeMs = lastTwo.getWindowDecoderLatencyMs(pyroWave || decodeTimeMs >= 0);
        shownHostProcessingMs = lastTwo.getHostProcessingMs();
        networkFrameLossPercent = r.activeWindowVideoStats.totalFrames == 0 ? 0 :
                (float) r.activeWindowVideoStats.framesLost / r.activeWindowVideoStats.totalFrames * 100;
        overlayFps = lastTwo.getFps();

        r.globalVideoStats.add(r.activeWindowVideoStats);
        lastWindowVideoStats.copy(r.activeWindowVideoStats);
        r.activeWindowVideoStats.clear();
        r.activeWindowVideoStats.measurementStartTimestamp = SystemClock.uptimeMillis();
    }

    int getAverageDecoderLatency() {
        if ((r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0) {
            return r.globalVideoStats.getAverageDecoderLatencyMs();
        }
        return r.globalVideoStats.getAverageDecoderLatencyMs(r.activeWindowVideoStats,
                r.frameLatencyStats.getArtemisDecodeMs());
    }
}
