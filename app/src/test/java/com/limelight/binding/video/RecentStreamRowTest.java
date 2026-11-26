package com.limelight.binding.video;

import android.app.Activity;
import android.app.Application;
import android.graphics.SurfaceTexture;
import android.media.MediaCodecInfo;
import android.media.MediaFormat;
import android.view.Surface;

import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.preferences.PreferenceConfiguration;
import com.limelight.utils.RecentStreams;
import com.limelight.utils.WiredLink;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.annotation.Implementation;
import org.robolectric.annotation.Implements;
import org.robolectric.shadows.MediaCodecInfoBuilder;
import org.robolectric.shadows.ShadowBuild;
import org.robolectric.shadows.ShadowSystemClock;
import org.robolectric.util.ReflectionHelpers;
import org.robolectric.util.ReflectionHelpers.ClassParameter;

import java.time.Duration;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;

/** A finished stream leaves one row in the report, with what the renderer saw and no address. */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class,
        shadows = {KeyframeSafetyNetWiringTest.QueueingCodec.class, RecentStreamRowTest.BufferBridge.class},
        instrumentedPackages = "com.limelight.nvstream.jni")
public class RecentStreamRowTest {
    /** No native library, and what the engine says about the video socket. */
    @Implements(value = MoonBridge.class, isInAndroidSdk = false)
    public static class BufferBridge {
        static long info = -1;

        @Implementation protected static void __staticInitializer__() { }
        @Implementation protected static long getVideoReceiveBufferInfo() {
            return info;
        }
        @Implementation protected static long getDecoderDrops() {
            return 0;
        }
    }

    private ActivityController<Activity> activity;
    private SurfaceTexture texture;
    private Surface surface;
    private MediaCodecDecoderRenderer renderer;
    private int frame;

    @Before
    public void setUp() {
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        RecentStreams.clear();
        BufferBridge.info = -1;
        activity = Robolectric.buildActivity(Activity.class).create();
        texture = new SurfaceTexture(0);
        surface = new Surface(texture);
    }

    @After
    public void tearDown() {
        surface.release();
        texture.release();
        activity.close();
    }

    private void stream(String model, String decoder, int bytes) {
        ShadowBuild.setModel(model);
        PreferenceConfiguration prefs = new PreferenceConfiguration();
        prefs.videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
        prefs.codecLowLatency = true;
        prefs.vendorLowLatency = true;
        renderer = new MediaCodecDecoderRenderer(activity.get(), prefs, null, 0, false, false, "", null);
        ReflectionHelpers.setField(renderer, "renderTarget", surface);
        ReflectionHelpers.setField(renderer, "videoFormat", MoonBridge.VIDEO_FORMAT_H265);
        ReflectionHelpers.setField(renderer, "initialWidth", 3840);
        ReflectionHelpers.setField(renderer, "initialHeight", 2160);
        ReflectionHelpers.setField(renderer, "refreshRate", 60);
        ReflectionHelpers.setField(renderer, "streamStartMs", 0L);
        ReflectionHelpers.setField(renderer, "streamStartUptimeMs", android.os.SystemClock.uptimeMillis());
        configure(decoder, true);
        submit(bytes, MoonBridge.FRAME_TYPE_IDR);
        submit(64, MoonBridge.FRAME_TYPE_PFRAME);
        submit(bytes / 2, MoonBridge.FRAME_TYPE_IDR);
        ShadowSystemClock.advanceBy(Duration.ofMillis(97_000));
    }

    private void configure(String decoder, boolean lowLatencyFlag) {
        MediaFormat format = MediaFormat.createVideoFormat("video/hevc", 3840, 2160);
        if (lowLatencyFlag) format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1);
        ReflectionHelpers.callInstanceMethod(renderer, "tryConfigureDecoder",
                ClassParameter.from(MediaCodecInfo.class, MediaCodecInfoBuilder.newBuilder().setName(decoder).build()),
                ClassParameter.from(MediaFormat.class, format),
                ClassParameter.from(boolean.class, true));
    }

    private void submit(int bytes, int frameType) {
        frame++;
        renderer.submitDecodeUnit(new byte[bytes], bytes, MoonBridge.BUFFER_TYPE_PICDATA, frame, frameType, (char) 0,
                frame * 16_000L, frame * 16_000L, frame * 16_000_000L, frame * 16_000L);
    }

    @Test
    public void theRowSaysWhatTheStreamWas() {
        RecentStreams.noteLink(new WiredLink.Hints(1000, false));
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 512);
        renderer.cleanup();
        String section = RecentStreams.section();
        for (String part : new String[] {"1970-01-01T00:00:00Z 97 s", "c2.mtk.hevc.decoder video/hevc", "3840x2160 60 fps",
                "HDR off", "engine Rust", "low latency on", "vendor on", "RFI off", "keyframe net on",
                "frames in 3 ", "keyframes 2, largest 512 B", "climbs 0, settled 0, repairs climb 0 end 0 quiet 0 limit 0",
                "gap frames 0", "held 0 frames 0 ms", "input waits 0", "input buffer 4096 B", "receive buffer unknown",
                "hottest normal, thermal protection off", "link wired 1000 Mbps"}) {
            assertTrue(part + " in " + section, section.contains(part));
        }
    }

    // The two things the first report from that TV has to judge the input hold by.
    @Test
    public void theRowSaysHowLongFramesWereHeldAndWhatTheSystemGranted() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 512);
        KeyframeSafetyNet.Counts counts = ReflectionHelpers.getField(renderer, "repairCounts");
        counts.heldFrames = 310;
        counts.heldNs = 2_500_000_000L;
        ReflectionHelpers.callInstanceMethod(renderer, "noteVideoReceiveBuffer",
                ClassParameter.from(long.class, (16_384L * 1024 << 32) | 208 * 1024));
        // The session is gone before the row is written: what it said stays.
        ReflectionHelpers.callInstanceMethod(renderer, "noteVideoReceiveBuffer", ClassParameter.from(long.class, -1L));
        renderer.cleanup();
        String section = RecentStreams.section();
        assertTrue(section, section.contains("held 310 frames 2500 ms"));
        assertTrue(section, section.contains("receive buffer granted 208 of 16384 KiB"));
    }

    @Test
    public void theOnceASecondTickAsksTheEngine() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 512);
        Runnable tick = ReflectionHelpers.getField(renderer, "updateLatencyStats");
        ReflectionHelpers.setField(renderer, "latencyHandler", new android.os.Handler(android.os.Looper.getMainLooper()));
        tick.run();
        BufferBridge.info = (456L * 1024 << 32) | 77 * 1024;
        tick.run();
        BufferBridge.info = -1;
        tick.run();
        // No latency thread of its own to stop.
        ReflectionHelpers.setField(renderer, "latencyHandler", null);
        renderer.cleanup();
        String section = RecentStreams.section();
        assertTrue(section, section.contains("receive buffer granted 77 of 456 KiB"));
    }

    @Test
    public void whatTheSystemGrantedReachesTheReportOnce() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 512);
        // Sizes of its own: the report's log outlives a test.
        long info = (789L * 1024 << 32) | 123 * 1024;
        for (int i = 0; i < 3; i++) {
            ReflectionHelpers.callInstanceMethod(renderer, "noteVideoReceiveBuffer", ClassParameter.from(long.class, info));
        }
        String log = com.limelight.LimeLog.getRedactedLog();
        String line = "Video receive buffer: the system granted 123 KiB of the 789 KiB asked";
        assertTrue(log, log.contains(line));
        assertEquals(log, log.indexOf(line), log.lastIndexOf(line));
    }

    // nightjar#119: eight judder rows, none saying its pacing or why 2-3% of the frames were not shown.
    @Test
    public void theRowSaysHowFramesWerePacedAndWhyTheyWereDropped() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 512);
        PreferenceConfiguration prefs = ReflectionHelpers.getField(renderer, "prefs");
        prefs.framePacing = PreferenceConfiguration.FRAME_PACING_BALANCED;
        prefs.dropLateFrames = true;
        ReflectionHelpers.setField(renderer, "hostPhaseLock", true);
        ReflectionHelpers.setField(renderer, "balancedOnOutput", 2700L);
        ReflectionHelpers.setField(renderer, "balancedOnVsync", 3L);
        FrameLatencyStats stats = ReflectionHelpers.getField(renderer, "frameLatencyStats");
        stats.onDropReason(101, "catch_up");
        stats.onDropReason(102, "replaced");
        stats.onDropReason(103, "catch_up");
        // Ten out, four shown: three with a reason, three without one.
        ReflectionHelpers.setField(renderer, "numFramesOut", 10);
        VideoStats global = ReflectionHelpers.getField(renderer, "globalVideoStats");
        VideoStats window = ReflectionHelpers.getField(renderer, "activeWindowVideoStats");
        ReflectionHelpers.setField(global, "totalFramesRendered", 0);
        ReflectionHelpers.setField(window, "totalFramesRendered", 4);
        ReflectionHelpers.setField(renderer, "phaseReports", 2650L);
        ReflectionHelpers.setField(renderer, "outputHasArrSupport", true);
        renderer.cleanup();
        String section = RecentStreams.section();
        for (String part : new String[] {"out 10 rendered 4", "| pacing balanced (release passes as outputs came out 2700, " +
                "outputs queued for vsync 3), PC frame times off, timed to your screen on, report batches 2650, ARR screen on, " +
                "fixed timing off, VRR off, newest only on, queue take-back off, vsync ",
                "| dropped catch_up 2, replaced 1, other 3 |"}) {
            assertTrue(part + " in " + section, section.contains(part));
        }
        String log = com.limelight.LimeLog.getRedactedLog();
        assertTrue(log, log.contains("Stream summary: "));
        assertTrue(log, log.contains("| dropped catch_up 2, replaced 1, other 3 |"));
    }

    @Test
    public void pacingDropsOverlayLineFormatsDropsAndSharedSlots() {
        assertNull(MediaCodecOverlayReport.formatPacingDropsLine("none", 0));
        assertNull(MediaCodecOverlayReport.formatPacingDropsLine(null, 0));
        assertNull(MediaCodecOverlayReport.formatPacingDropsLine("", 0));
        assertNull(MediaCodecOverlayReport.formatPacingDropsLine("none", -1));
        assertEquals("Discarded before display: catch_up 4",
                MediaCodecOverlayReport.formatPacingDropsLine("catch_up 4", 0));
        assertEquals("Discarded before display: catch_up 4, replaced 1 | shared refresh 2",
                MediaCodecOverlayReport.formatPacingDropsLine("catch_up 4, replaced 1", 2));
        assertEquals("Shared refresh slots: 3",
                MediaCodecOverlayReport.formatPacingDropsLine("none", 3));
        assertEquals("Shared refresh slots: 1",
                MediaCodecOverlayReport.formatPacingDropsLine(null, 1));
    }

    @Test
    public void lowestLatencyWithNothingDroppedSaysSo() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 512);
        ReflectionHelpers.setField(renderer, "numFramesOut", 0);
        renderer.cleanup();
        String section = RecentStreams.section();
        assertTrue(section, section.contains("| pacing lowest latency, PC frame times off, timed to your screen off (other PC), report batches 0"));
        assertFalse("no Vulkan renderer", section.contains("Vulkan renderer"));
        assertTrue(section, section.contains("| dropped none |"));
    }

    @Test
    public void anotherDecoderShowsItsNetOffAndCleaningUpTwiceAddsOneRow() {
        stream("AFTMM", "c2.mtk.hevc.decoder", 128);
        renderer.cleanup();
        renderer.cleanup();
        String section = RecentStreams.section();
        assertTrue(section, section.contains("keyframe net off"));
        assertFalse("no repairs to tell of", section.contains("climbs"));
        assertTrue(section, section.indexOf("c2.mtk.hevc.decoder") == section.lastIndexOf("c2.mtk.hevc.decoder"));
    }

    @Test
    public void theRowSaysTheModeTheDecoderGotNotTheSwitch() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 128);
        // The retry after a rejected configure has no flag, with the switch still on.
        configure("c2.mtk.hevc.decoder", false);
        renderer.cleanup();
        String section = RecentStreams.section();
        assertTrue(section, section.contains("low latency off"));
        assertTrue(section, section.contains("keyframe net off"));
    }

    @Test
    public void theCountsLastThroughADecoderRestartAndEndWithTheRow() {
        stream("AFTNE4E43F", "c2.mtk.hevc.decoder", 128);
        KeyframeSafetyNet.Counts counts = ReflectionHelpers.getField(renderer, "repairCounts");
        counts.limitRepairs = 2;
        configure("c2.mtk.hevc.decoder", true);
        assertSame(counts, ReflectionHelpers.getField(renderer, "repairCounts"));
        renderer.cleanup();
        assertTrue(RecentStreams.section(), RecentStreams.section().contains("limit 2"));
        assertNull("the next stream starts over", ReflectionHelpers.getField(renderer, "repairCounts"));
    }

    @Test
    public void theRowSaysTheHottestLevelAndWhetherProtectionWasOn() {
        stream("AFTMM", "c2.mtk.hevc.decoder", 128);
        ReflectionHelpers.setField(renderer, "hottestThermalLevel", ThermalMonitor.HOT);
        ReflectionHelpers.<PreferenceConfiguration>getField(renderer, "prefs").thermalProtection = true;
        renderer.cleanup();
        assertTrue(RecentStreams.section(), RecentStreams.section().contains("hottest hot, thermal protection on"));
        assertEquals(ThermalMonitor.NORMAL, (int) ReflectionHelpers.<Integer>getField(renderer, "hottestThermalLevel"));
    }


    @Test
    public void theRowSaysFullRangeAndTheOutputsColorRange() {
        stream("TB375FC", "c2.mtk.hevc.decoder.lowlatency", 128);
        ReflectionHelpers.<PreferenceConfiguration>getField(renderer, "prefs").fullRange = true;
        MediaFormat output = MediaFormat.createVideoFormat("video/raw", 2944, 1840);
        output.setInteger(MediaFormat.KEY_COLOR_FORMAT, 2130708361);
        output.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
        output.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
        output.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED);
        ReflectionHelpers.setField(renderer, "outputFormat", output);
        renderer.cleanup();
        String section = RecentStreams.section();
        assertTrue(section, section.contains("HDR off full range"));
        assertTrue(section, section.contains("output 2944x1840 color 2130708361 standard 1 transfer 3 range 2"));
    }

    @Test
    public void aStreamThatNeverStartedLeavesNoRow() {
        ShadowBuild.setModel("AFTMM");
        PreferenceConfiguration prefs = new PreferenceConfiguration();
        prefs.videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
        new MediaCodecDecoderRenderer(activity.get(), prefs, null, 0, false, false, "", null).cleanup();
        assertTrue(RecentStreams.section(), RecentStreams.section().contains("None finished"));
        assertFalse(RecentStreams.section().contains("engine"));
    }
}
