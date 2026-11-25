package com.limelight.binding.video;

import android.app.Activity;
import android.app.Application;
import android.media.MediaCodecInfo;
import android.media.MediaCodecInfo.CodecCapabilities;
import android.media.MediaCodecInfo.CodecProfileLevel;
import android.media.MediaFormat;
import android.view.Display;

import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.preferences.PreferenceConfiguration;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.Robolectric;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.Shadows;
import org.robolectric.android.controller.ActivityController;
import org.robolectric.annotation.Config;
import org.robolectric.shadows.MediaCodecInfoBuilder;
import org.robolectric.shadows.MediaCodecInfoBuilder.CodecCapabilitiesBuilder;
import org.robolectric.shadows.ShadowMediaCodecList;
import org.robolectric.util.ReflectionHelpers;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;

/**
 * Frame Warp draws through the Vulkan renderer even with "Render through Vulkan" off
 * (VideoRendererChoice), so the HDR format asked for before the stream must count on it: a Dolby
 * Vision decoder drawn by Vulkan flickered in HDR alone on a Lenovo TB323FU (Rubylight #104), where
 * the PC listed Frame Warp and the stream asked for Dolby Vision 8.1.
 */
@RunWith(RobolectricTestRunner.class)
@Config(sdk = 34, application = Application.class,
        shadows = {KeyframeSafetyNetWiringTest.QueueingCodec.class, com.limelight.PcViewLifecycleTest.NoNativeMoonBridge.class},
        instrumentedPackages = "com.limelight.nvstream.jni")
public class FrameWarpHdrFormatTest {
    private static final int OFFERED = HdrFormatPolicy.DV_81 | HdrFormatPolicy.PQ;

    private ActivityController<Activity> activity;

    @Before
    public void setUp() {
        ReflectionHelpers.setStaticField(MediaCodecHelper.class, "initialized", true);
        // An earlier test class's cached codec list would hide the decoders added below.
        MediaCodecHelper.forgetCodecList();
        // The Vulkan renderer runs here, without loading its native library.
        ReflectionHelpers.setStaticField(VulkanRendererBridge.class, "supported", Boolean.TRUE);
        ReflectionHelpers.setStaticField(VulkanRendererBridge.class, "hdrColorSpaces", true);
        activity = Robolectric.buildActivity(Activity.class).create();
        ShadowMediaCodecList.addCodec(decoder("c2.qti.hevc.decoder", "video/hevc", CodecProfileLevel.HEVCProfileMain10));
        ShadowMediaCodecList.addCodec(decoder("c2.dolby.decoder.hevc", HdrFormatPolicy.MIME_DOLBY_VISION,
                CodecProfileLevel.DolbyVisionProfileDvheSt));
    }

    @After
    public void tearDown() {
        ReflectionHelpers.setStaticField(VulkanRendererBridge.class, "supported", null);
        ReflectionHelpers.setStaticField(VulkanRendererBridge.class, "hdrColorSpaces", false);
        MediaCodecHelper.forgetCodecList();
        activity.close();
    }

    @Test
    public void frameWarpOnADolbyVisionScreenAsksForHdr10() {
        MediaCodecDecoderRenderer renderer = renderer(prefs(true, false));

        assertEquals("Frame Warp's Vulkan renderer draws the picture: HDR10, never Dolby Vision", 0,
                renderer.pickHdrFormat(OFFERED, MoonBridge.VIDEO_FORMAT_H265_MAIN10, false, true));
        assertTrue("the notice names Frame Warp", renderer.vulkanForFrameWarp());
        assertTrue("vulkanSurface counts Frame Warp when asked to",
                MediaCodecDecoderRenderer.vulkanSurface(prefs(true, false), true) != HdrFormatPolicy.DrawnSurface.NONE);
    }

    @Test
    public void dolbyVisionStaysWhereNothingDrawsThroughVulkan() {
        assertEquals("the PC does not list Frame Warp", HdrFormatPolicy.DV_81,
                renderer(prefs(true, false)).pickHdrFormat(OFFERED, MoonBridge.VIDEO_FORMAT_H265_MAIN10, false, false));
        MediaCodecDecoderRenderer off = renderer(prefs(false, false));
        assertEquals("Frame Warp off", HdrFormatPolicy.DV_81,
                off.pickHdrFormat(OFFERED, MoonBridge.VIDEO_FORMAT_H265_MAIN10, false, true));
        assertFalse(off.vulkanForFrameWarp());
        assertEquals(HdrFormatPolicy.DrawnSurface.NONE, MediaCodecDecoderRenderer.vulkanSurface(prefs(false, false), true));
    }

    @Test
    public void renderThroughVulkanKeepsItsOwnNotice() {
        MediaCodecDecoderRenderer renderer = renderer(prefs(true, true));
        assertEquals(0, renderer.pickHdrFormat(OFFERED, MoonBridge.VIDEO_FORMAT_H265_MAIN10, false, true));
        assertFalse("the user asked for Vulkan; its notice names that setting", renderer.vulkanForFrameWarp());
    }

    @Test
    public void fullRangeKeepsDolbyVisionNegotiationAndUsesLimitedRangeForDv() {
        PreferenceConfiguration direct = prefs(false, false);
        direct.enableHdr = true;
        direct.fullRange = true;
        MediaCodecDecoderRenderer directRenderer = renderer(direct);
        MediaCodecInfo dv8 = decoder("c2.dolby.decoder.hevc", HdrFormatPolicy.MIME_DOLBY_VISION,
                CodecProfileLevel.DolbyVisionProfileDvheSt);
        MediaCodecInfo dv10 = decoder("c2.dolby.decoder.av1", HdrFormatPolicy.MIME_DOLBY_VISION,
                CodecProfileLevel.DolbyVisionProfileDvav110);
        ReflectionHelpers.setField(directRenderer, "dolbyVisionDecoder", dv8);
        ReflectionHelpers.setField(directRenderer, "dolbyVisionAv1Decoder", dv10);
        assertEquals("full color range still asks for Dolby Vision", HdrFormatPolicy.DV_81,
                directRenderer.pickHdrFormat(OFFERED, MoonBridge.VIDEO_FORMAT_H265_MAIN10, false, false));

        // The host codes a Dolby Vision stream limited range with its RPUs (nightjar#193).
        ReflectionHelpers.setField(directRenderer, "announcedHdrFormat", HdrFormatPolicy.DV_81);
        ReflectionHelpers.setField(directRenderer, "videoFormat", MoonBridge.VIDEO_FORMAT_H265_MAIN10);
        assertSame(dv8, ReflectionHelpers.callInstanceMethod(directRenderer, "dolbyVisionDecoderForStream"));
        assertEquals(MoonBridge.COLOR_RANGE_LIMITED, directRenderer.getPreferredColorRange());

        // HDR10 keeps full color range.
        ReflectionHelpers.setField(directRenderer, "announcedHdrFormat", 0);
        assertEquals(MoonBridge.COLOR_RANGE_FULL, directRenderer.getPreferredColorRange());
    }

    private static PreferenceConfiguration prefs(boolean frameWarp, boolean vulkanRender) {
        PreferenceConfiguration prefs = new PreferenceConfiguration();
        prefs.width = 3040;
        prefs.height = 1904;
        prefs.frameWarp = frameWarp;
        prefs.vulkanRender = vulkanRender;
        return prefs;
    }

    private MediaCodecDecoderRenderer renderer(PreferenceConfiguration prefs) {
        MediaCodecDecoderRenderer renderer = new MediaCodecDecoderRenderer(activity.get(), prefs, null, 0, false, true, "", null);
        Display display = activity.get().getWindowManager().getDefaultDisplay();
        Shadows.shadowOf(display).setDisplayHdrCapabilities(display.getDisplayId(), 750, 750, 0.5f,
                Display.HdrCapabilities.HDR_TYPE_DOLBY_VISION, Display.HdrCapabilities.HDR_TYPE_HDR10);
        ReflectionHelpers.setField(renderer, "outputDisplay", display);
        return renderer;
    }

    private static MediaCodecInfo decoder(String name, String mime, int... profiles) {
        MediaFormat capabilities = MediaFormat.createVideoFormat(mime, 3840, 2160);
        CodecProfileLevel[] levels = new CodecProfileLevel[profiles.length];
        for (int i = 0; i < profiles.length; i++) {
            levels[i] = new CodecProfileLevel();
            levels[i].profile = profiles[i];
        }
        return MediaCodecInfoBuilder.newBuilder().setName(name).setIsHardwareAccelerated(true).setCapabilities(
                CodecCapabilitiesBuilder.newBuilder().setMediaFormat(capabilities)
                        .setProfileLevels(levels)
                        .setColorFormats(new int[] {CodecCapabilities.COLOR_FormatYUV420Flexible}).build()).build();
    }
}
