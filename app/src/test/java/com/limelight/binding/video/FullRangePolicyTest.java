package com.limelight.binding.video;

import android.app.Activity;
import android.app.Application;
import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaFormat;
import android.os.Looper;
import android.view.Surface;

import com.limelight.LimeLog;
import com.limelight.PcViewLifecycleTest;
import com.limelight.R;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.preferences.PreferenceConfiguration;

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
import org.robolectric.shadows.ShadowMediaCodec;
import org.robolectric.shadows.ShadowToast;
import org.robolectric.util.ReflectionHelpers;
import org.robolectric.util.ReflectionHelpers.ClassParameter;

import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.Assert.*;
import static org.robolectric.Shadows.shadowOf;

/** Full-range SDR colour policy and its wiring into MediaCodecDecoderRenderer (issue #153). */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class,
        instrumentedPackages = {"com.limelight.nvstream.jni", "com.limelight.binding.video"},
        shadows = {FullRangePolicyTest.FakeVulkanBridge.class, FullRangePolicyTest.FakeCodec.class,
                PcViewLifecycleTest.NoNativeMoonBridge.class})
public class FullRangePolicyTest {
    @Implements(value = VulkanRendererBridge.class, isInAndroidSdk = false)
    public static class FakeVulkanBridge {
        static boolean supported = true;
        static int probes;

        @Implementation public static synchronized boolean isSupported() {
            probes++;
            return supported;
        }

        // The renderer never starts here.
        @Implementation public static VulkanRendererBridge create(Surface output, int streamWidth, int streamHeight,
                int streamFps, int framePacing, int jitterBuffer, int ditherMode, int colorspace, boolean fullRange,
                boolean tenBit, boolean hlg, float displayRefreshHz, String codec, String decoderName,
                String traceDirectory, boolean frontBuffer, long frameWarpApi) {
            return null;
        }

        @Implementation public static String getLastError() {
            return "";
        }
    }

    @Implements(MediaCodec.class)
    public static class FakeCodec extends ShadowMediaCodec {
        static final List<String> calls = new ArrayList<>();
        static MediaFormat output;

        @Implementation public String getName() {
            return "c2.qti.hevc.decoder";
        }

        @Implementation public MediaFormat getOutputFormat() {
            return output;
        }

        @Implementation public void stop() {
            calls.add("stop");
        }

        @Implementation public void reset() {
            calls.add("reset");
        }

        @Implementation public void release() {
            calls.add("release");
        }
    }

    @Before
    public void resetState() {
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        FakeVulkanBridge.supported = true;
        FakeVulkanBridge.probes = 0;
        FakeCodec.calls.clear();
        FakeCodec.output = null;
    }

    @Test
    public void outputFallbackDetectsOnlyADroppedFullRange() {
        // Full range off: never triggers.
        assertNull(FullRangePolicy.outputFallbackReason(false, false, false, false, false,
                MediaFormat.COLOR_RANGE_LIMITED, true));
        // HDR, Dolby Vision, packed 4:4:4 and PyroWave stay on their own paths.
        assertNull(FullRangePolicy.outputFallbackReason(true, true, false, false, false,
                MediaFormat.COLOR_RANGE_LIMITED, true));
        assertNull(FullRangePolicy.outputFallbackReason(true, false, true, false, false,
                MediaFormat.COLOR_RANGE_LIMITED, true));
        assertNull(FullRangePolicy.outputFallbackReason(true, false, false, true, false,
                MediaFormat.COLOR_RANGE_LIMITED, true));
        assertNull(FullRangePolicy.outputFallbackReason(true, false, false, false, true,
                MediaFormat.COLOR_RANGE_LIMITED, true));

        // A decoder honoring full range, or reporting no colour keys at all, stays direct.
        assertNull(FullRangePolicy.outputFallbackReason(true, false, false, false, false,
                MediaFormat.COLOR_RANGE_FULL, true));
        assertNull(FullRangePolicy.outputFallbackReason(true, false, false, false, false,
                (Integer) null, false));

        // Limited range for full-range SDR triggers the fallback.
        String limited = FullRangePolicy.outputFallbackReason(true, false, false, false, false,
                MediaFormat.COLOR_RANGE_LIMITED, true);
        assertNotNull(limited);
        assertTrue(limited, limited.contains("limited"));

        // So does standard/transfer with the range omitted (Codec2 then takes limited).
        String omitted = FullRangePolicy.outputFallbackReason(true, false, false, false, false,
                (Integer) null, true);
        assertNotNull(omitted);
        assertTrue(omitted, omitted.contains("omitted"));
    }

    @Test
    public void canUseVulkanDoesNotProbeBelowApi29() {
        boolean[] probed = {false};
        assertFalse(FullRangePolicy.canUseVulkan(28, () -> probed[0] = true));
        assertFalse(probed[0]);
        assertTrue(FullRangePolicy.canUseVulkan(29, () -> {
            probed[0] = true;
            return true;
        }));
        assertTrue(probed[0]);
    }

    @Test
    public void outputDescriptionIncludesColorRangeOrQuestionMark() {
        MediaFormat full = MediaFormat.createVideoFormat("video/raw", 2944, 1840);
        full.setInteger(MediaFormat.KEY_COLOR_FORMAT, 2130708361);
        full.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
        full.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
        full.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_FULL);
        assertEquals("2944x1840 color 2130708361 standard 1 transfer 3 range 1",
                FullRangePolicy.outputDescription(full));

        MediaFormat withoutRange = MediaFormat.createVideoFormat("video/raw", 1920, 1080);
        withoutRange.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
        assertEquals("1920x1080 color ? standard 1 transfer ? range ?",
                FullRangePolicy.outputDescription(withoutRange));
    }

    private MediaCodecDecoderRenderer buildRenderer(Activity activity, boolean fullRange, boolean sdr10,
                                                    int videoFormat, String hevcDecoderName) {
        PreferenceConfiguration prefs = new PreferenceConfiguration();
        prefs.videoFormat = PreferenceConfiguration.FormatOption.FORCE_HEVC;
        prefs.fullRange = fullRange;
        MediaCodecDecoderRenderer renderer = new MediaCodecDecoderRenderer(activity, prefs,
                null, 0, false, false, "", null);
        renderer.setSdr10Requested(sdr10);
        ReflectionHelpers.setField(renderer, "videoFormat", videoFormat);
        ReflectionHelpers.setField(renderer, "refreshRate", 144);
        if (hevcDecoderName != null) {
            MediaCodecInfo info = MediaCodecInfoBuilder.newBuilder().setName(hevcDecoderName).build();
            ReflectionHelpers.setField(renderer, "hevcDecoder", info);
            ReflectionHelpers.setField(renderer, "hevcDecoder10", info);
            ReflectionHelpers.setField(renderer, "activeDecoderName", hevcDecoderName);
        }
        return renderer;
    }

    @Test
    public void mediaTekFullRangeSdrStartsDirectWithVulkanOffAndNeverProbes() {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            for (boolean sdr10 : new boolean[] {true, false}) {
                MediaCodecDecoderRenderer renderer = buildRenderer(activity.get(), true, sdr10,
                        sdr10 ? MoonBridge.VIDEO_FORMAT_H265_MAIN10 : MoonBridge.VIDEO_FORMAT_H265,
                        "c2.mtk.hevc.decoder.lowlatency");
                ReflectionHelpers.callInstanceMethod(renderer, "setupUpscaling");
                assertEquals(false, ReflectionHelpers.getField(renderer, "wantVulkan"));
            }
            assertEquals(0, FakeVulkanBridge.probes);
        }
    }

    @Test
    public void mediaTekDecoderReportingFullRangeStaysDirectWithoutANotice() throws IOException {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            MediaCodecDecoderRenderer renderer = buildRenderer(activity.get(), true, true,
                    MoonBridge.VIDEO_FORMAT_H265_MAIN10, "c2.mtk.hevc.decoder.lowlatency");
            MediaCodec codec = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(renderer, "videoDecoder", codec);
                MediaFormat full = MediaFormat.createVideoFormat("video/raw", 2944, 1840);
                full.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
                full.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
                full.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_FULL);
                FakeCodec.output = full;
                ReflectionHelpers.callInstanceMethod(renderer, "onOutputFormatChanged");
                assertEquals(false, ReflectionHelpers.getField(renderer, "wantVulkan"));
                assertEquals(false, ReflectionHelpers.getField(renderer, "fullRangeFallbackAttempted"));
                assertEquals(0, FakeVulkanBridge.probes);
                assertNull(ShadowToast.getLatestToast());
            } finally {
                codec.release();
            }
        }
    }

    @Test
    public void outputFormatWithLimitedOrOmittedRangeSwitchesStreamToVulkanOnce() throws IOException {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            MediaCodecDecoderRenderer renderer = buildRenderer(activity.get(), true, true,
                    MoonBridge.VIDEO_FORMAT_H265_MAIN10, "c2.qti.hevc.decoder");
            MediaCodec codec = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(renderer, "videoDecoder", codec);
                MediaFormat limited = MediaFormat.createVideoFormat("video/raw", 2944, 1840);
                limited.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
                limited.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
                limited.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED);
                FakeCodec.output = limited;

                ReflectionHelpers.callInstanceMethod(renderer, "onOutputFormatChanged");

                AtomicInteger recovery = ReflectionHelpers.getField(renderer, "codecRecoveryType");
                assertEquals((int) ReflectionHelpers.getStaticField(MediaCodecDecoderRenderer.class,
                        "CR_RECOVERY_TYPE_RESTART"), recovery.get());
                assertEquals(true, ReflectionHelpers.getField(renderer, "wantVulkan"));
                assertEquals(true, ReflectionHelpers.getField(renderer, "rebuildForVulkan"));
                assertEquals(true, ReflectionHelpers.getField(renderer, "fullRangeFallbackAttempted"));
                shadowOf(Looper.getMainLooper()).idle();
                assertEquals(activity.get().getString(R.string.full_range_vulkan_switched),
                        ShadowToast.getTextOfLatestToast());

                // Running a second time (e.g., if Vulkan later declined) never loops.
                recovery.set(ReflectionHelpers.getStaticField(MediaCodecDecoderRenderer.class, "CR_RECOVERY_TYPE_NONE"));
                ReflectionHelpers.setField(renderer, "wantVulkan", false);
                ReflectionHelpers.setField(renderer, "rebuildForVulkan", false);
                ReflectionHelpers.callInstanceMethod(renderer, "onOutputFormatChanged");
                assertEquals((int) ReflectionHelpers.getStaticField(MediaCodecDecoderRenderer.class,
                        "CR_RECOVERY_TYPE_NONE"), recovery.get());
                assertEquals(false, ReflectionHelpers.getField(renderer, "wantVulkan"));
            } finally {
                codec.release();
            }
        }
    }

    @Test
    public void outputFormatHonoringFullRangeOrUnsupportedVulkanOrHandedBackDoesNotRestart() throws IOException {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            // Case 1: decoder reports COLOR_RANGE_FULL -> stays direct, no restart.
            MediaCodecDecoderRenderer ok = buildRenderer(activity.get(), true, false,
                    MoonBridge.VIDEO_FORMAT_H265, "c2.qti.hevc.decoder");
            MediaCodec codec1 = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(ok, "videoDecoder", codec1);
                MediaFormat full = MediaFormat.createVideoFormat("video/raw", 1920, 1080);
                full.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
                full.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
                full.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_FULL);
                FakeCodec.output = full;
                ReflectionHelpers.callInstanceMethod(ok, "onOutputFormatChanged");
                assertEquals(false, ReflectionHelpers.getField(ok, "wantVulkan"));
                assertEquals(0, ((AtomicInteger) ReflectionHelpers.getField(ok, "codecRecoveryType")).get());
            } finally {
                codec1.release();
            }

            // Case 2: decoder omits range while Vulkan is unsupported -> warns, no restart, never probes again.
            FakeVulkanBridge.supported = false;
            MediaCodecDecoderRenderer noVk = buildRenderer(activity.get(), true, false,
                    MoonBridge.VIDEO_FORMAT_H265, "c2.qti.hevc.decoder");
            MediaCodec codec2 = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(noVk, "videoDecoder", codec2);
                MediaFormat omitted = MediaFormat.createVideoFormat("video/raw", 1920, 1080);
                omitted.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
                omitted.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
                FakeCodec.output = omitted;
                ReflectionHelpers.callInstanceMethod(noVk, "onOutputFormatChanged");
                assertEquals(false, ReflectionHelpers.getField(noVk, "wantVulkan"));
                assertEquals(true, ReflectionHelpers.getField(noVk, "fullRangeFallbackAttempted"));
                assertEquals(0, ((AtomicInteger) ReflectionHelpers.getField(noVk, "codecRecoveryType")).get());
                assertTrue(LimeLog.getRedactedLog().contains("Vulkan renderer is unavailable"));
                shadowOf(Looper.getMainLooper()).idle();
                assertEquals(activity.get().getString(R.string.full_range_vulkan_unavailable),
                        ShadowToast.getTextOfLatestToast());
            } finally {
                codec2.release();
            }

            // Case 3: stream already handed back from Vulkan (tone-mapped HDR) -> never ping-pongs back.
            FakeVulkanBridge.supported = true;
            MediaCodecDecoderRenderer handedBack = buildRenderer(activity.get(), true, false,
                    MoonBridge.VIDEO_FORMAT_H265, "c2.qti.hevc.decoder");
            ReflectionHelpers.setField(handedBack, "vulkanHandedBack", true);
            MediaCodec codec3 = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(handedBack, "videoDecoder", codec3);
                MediaFormat limited = MediaFormat.createVideoFormat("video/raw", 1920, 1080);
                limited.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED);
                FakeCodec.output = limited;
                ReflectionHelpers.callInstanceMethod(handedBack, "onOutputFormatChanged");
                assertEquals(false, ReflectionHelpers.getField(handedBack, "wantVulkan"));
                assertEquals(0, ((AtomicInteger) ReflectionHelpers.getField(handedBack, "codecRecoveryType")).get());
            } finally {
                codec3.release();
            }
        }
    }

    @Test
    public void hdrOutputDoesNotConsumeFallbackBeforeLaterSwitchToSdr() throws IOException {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            // Starts as HDR (10-bit without sdr10 -> assumesHdr() is true).
            MediaCodecDecoderRenderer renderer = buildRenderer(activity.get(), true, false,
                    MoonBridge.VIDEO_FORMAT_H265_MAIN10, "c2.qti.hevc.decoder");
            MediaCodec codec = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(renderer, "videoDecoder", codec);
                MediaFormat fmt = MediaFormat.createVideoFormat("video/raw", 2944, 1840);
                fmt.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT2020);
                fmt.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_ST2084);
                fmt.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED);
                FakeCodec.output = fmt;

                // While HDR, fallback does not fire and stays armed.
                ReflectionHelpers.callInstanceMethod(renderer, "onOutputFormatChanged");
                assertEquals(false, ReflectionHelpers.getField(renderer, "fullRangeFallbackAttempted"));
                assertEquals(false, ReflectionHelpers.getField(renderer, "wantVulkan"));

                // Once the PC reports SDR, the next output format change switches to Vulkan.
                ReflectionHelpers.setField(renderer, "currentHdrMode", Boolean.FALSE);
                fmt.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
                fmt.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
                fmt.setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED);
                ReflectionHelpers.callInstanceMethod(renderer, "onOutputFormatChanged");
                assertEquals(true, ReflectionHelpers.getField(renderer, "fullRangeFallbackAttempted"));
                assertEquals(true, ReflectionHelpers.getField(renderer, "wantVulkan"));
                assertEquals(true, ReflectionHelpers.getField(renderer, "rebuildForVulkan"));
            } finally {
                codec.release();
            }
        }
    }

    @Test
    public void vulkanFailingToStartAfterTheSwitchSaysTheStreamStaysDirect() throws IOException {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            MediaCodecDecoderRenderer renderer = buildRenderer(activity.get(), true, false,
                    MoonBridge.VIDEO_FORMAT_H265, "c2.qti.hevc.decoder");
            ReflectionHelpers.setField(renderer, "outputDisplay",
                    activity.get().getWindowManager().getDefaultDisplay());
            MediaCodec codec = MediaCodec.createDecoderByType("video/avc");
            try {
                ReflectionHelpers.setField(renderer, "videoDecoder", codec);
                ReflectionHelpers.setField(renderer, "wantVulkan", true);
                ReflectionHelpers.setField(renderer, "fullRangeFallbackAttempted", true);
                Object session = ReflectionHelpers.getField(renderer, "session");
                ReflectionHelpers.callInstanceMethod(session, "configureAndStartDecoder",
                        ClassParameter.from(MediaFormat.class,
                                MediaFormat.createVideoFormat("video/hevc", 1920, 1080)));
                assertEquals(false, ReflectionHelpers.getField(renderer, "wantVulkan"));
                shadowOf(Looper.getMainLooper()).idle();
                assertEquals(activity.get().getString(R.string.full_range_vulkan_unavailable),
                        ShadowToast.getTextOfLatestToast());
            } finally {
                codec.release();
            }
        }
    }

    @Test
    public void rebuildForVulkanReleasesOldCodecBeforeReinitializingAndClearsFlag() throws IOException {
        try (ActivityController<Activity> activity = Robolectric.buildActivity(Activity.class).create()) {
            MediaCodecDecoderRenderer renderer = buildRenderer(activity.get(), true, false,
                    MoonBridge.VIDEO_FORMAT_H265, null);
            MediaCodec codec = MediaCodec.createDecoderByType("video/avc");
            ReflectionHelpers.setField(renderer, "videoDecoder", codec);
            ReflectionHelpers.setField(renderer, "wantVulkan", true);
            ReflectionHelpers.setField(renderer, "rebuildForVulkan", true);
            AtomicInteger recovery = ReflectionHelpers.getField(renderer, "codecRecoveryType");
            recovery.set(ReflectionHelpers.getStaticField(MediaCodecDecoderRenderer.class, "CR_RECOVERY_TYPE_RESTART"));
            ReflectionHelpers.setField(renderer, "codecRecoveryThreadQuiescedFlags",
                    (int) ReflectionHelpers.getStaticField(MediaCodecDecoderRenderer.class, "CR_FLAG_INPUT_THREAD"));

            // No HEVC decoder configured so initializeDecoder fails cleanly after release(), proving
            // rebuild() released the old codec directly instead of calling stop() or reset().
            ReflectionHelpers.callInstanceMethod(renderer, "doCodecRecoveryIfRequired",
                    ClassParameter.from(int.class,
                            (int) ReflectionHelpers.getStaticField(MediaCodecDecoderRenderer.class, "CR_FLAG_RENDER_THREAD")));
            assertEquals(Collections.singletonList("release"), FakeCodec.calls);
            assertNotNull(renderer.getDecoderFailure());
        }
    }
}
