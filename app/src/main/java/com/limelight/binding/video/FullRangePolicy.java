package com.limelight.binding.video;

import android.media.MediaFormat;

import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;

import java.util.function.BooleanSupplier;

/**
 * Full-range SDR color handling on MediaCodec, apart from MediaCodec itself so each rule can be
 * unit tested.
 *
 * <p>The host encodes full-range YUV (0..255 / 0..1023) and signals {@code video_full_range_flag=1}
 * whenever Full color range is enabled. A hardware decoder that ignores
 * {@link MediaFormat#KEY_COLOR_RANGE} and reports limited or unspecified range in its output format
 * has Codec2 stamp a limited-range dataspace onto the surface, which crushes or washes out blacks on
 * direct output. Only then does an SDR stream switch to {@link VulkanRendererBridge}, which applies
 * {@code VK_SAMPLER_YCBCR_RANGE_ITU_FULL} explicitly, and the user is told. The decoder's vendor
 * alone never picks the renderer.
 *
 * <p>With Full color range off (the default), nothing here runs and no Vulkan probe is made. HDR,
 * Dolby Vision, PyroWave and packed 4:4:4 keep their own color paths untouched.
 */
public final class FullRangePolicy {
    private FullRangePolicy() {}

    /**
     * Whether an SDR stream running without the Vulkan renderer must switch to Vulkan based on the
     * decoder's reported {@link MediaFormat}, returning a short reason for the log or {@code null}
     * when the current path should be kept.
     */
    public static String outputFallbackReason(boolean fullRange, boolean hdr, boolean dolbyVision,
                                              boolean packed444, boolean pyroWave,
                                              MediaFormat outputFormat) {
        Integer range = outputFormat != null && outputFormat.containsKey(MediaFormat.KEY_COLOR_RANGE) ?
                outputFormat.getInteger(MediaFormat.KEY_COLOR_RANGE) : null;
        boolean hasColorAspects = outputFormat != null &&
                (outputFormat.containsKey(MediaFormat.KEY_COLOR_STANDARD) ||
                        outputFormat.containsKey(MediaFormat.KEY_COLOR_TRANSFER));
        return outputFallbackReason(fullRange, hdr, dolbyVision, packed444, pyroWave, range,
                hasColorAspects);
    }

    /**
     * Pure form of {@link #outputFallbackReason(boolean, boolean, boolean, boolean, boolean, MediaFormat)}.
     *
     * @param outputColorRange {@link MediaFormat#KEY_COLOR_RANGE} from the output format, or null when absent
     * @param hasColorAspects  whether the output format reported {@code color-standard} or {@code color-transfer}
     */
    public static String outputFallbackReason(boolean fullRange, boolean hdr, boolean dolbyVision,
                                              boolean packed444, boolean pyroWave,
                                              Integer outputColorRange, boolean hasColorAspects) {
        int rangeCode = outputColorRange == null ? NightjarPolicyJni.COLOR_RANGE_ABSENT :
                outputColorRange == MediaFormat.COLOR_RANGE_FULL ? NightjarPolicyJni.COLOR_RANGE_FULL :
                        outputColorRange == MediaFormat.COLOR_RANGE_LIMITED ? NightjarPolicyJni.COLOR_RANGE_LIMITED :
                                NightjarPolicyJni.COLOR_RANGE_UNKNOWN;
        int rawRange = outputColorRange == null ? 0 : outputColorRange;
        return NightjarPolicyJni.fullRangeOutputFallbackReason(fullRange, hdr, dolbyVision, packed444,
                pyroWave, rangeCode, rawRange, hasColorAspects);
    }

    /** Whether the Vulkan renderer can take over on this API level and GPU; probes lazily. */
    public static boolean canUseVulkan(int sdkInt, BooleanSupplier vulkanSupported) {
        return NightjarPolicyJni.fullRangeCanUseVulkanSdk(sdkInt) && vulkanSupported.getAsBoolean();
    }

    /** Sets the initial colour keys on a decoder's base {@link MediaFormat}. */
    public static void applyBaseColor(MediaFormat format, int preferredColorRange, int preferredColorSpace,
                                      boolean assumesHdr) {
        format.setInteger(MediaFormat.KEY_COLOR_RANGE,
                preferredColorRange == MoonBridge.COLOR_RANGE_FULL ?
                        MediaFormat.COLOR_RANGE_FULL : MediaFormat.COLOR_RANGE_LIMITED);
        // If the stream is HDR-capable, the decoder detects transitions in colour standard/transfer.
        if (!assumesHdr) {
            format.setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
            switch (preferredColorSpace) {
                case MoonBridge.COLORSPACE_REC_601:
                    format.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT601_NTSC);
                    break;
                case MoonBridge.COLORSPACE_REC_709:
                    format.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT709);
                    break;
                case MoonBridge.COLORSPACE_REC_2020:
                    format.setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT2020);
                    break;
            }
        }
    }

    /** Concise output-format summary for problem-report stream rows, including colour range. */
    public static String outputDescription(MediaFormat format) {
        return outputKey(format, MediaFormat.KEY_WIDTH) + "x" + outputKey(format, MediaFormat.KEY_HEIGHT) +
                " color " + outputKey(format, MediaFormat.KEY_COLOR_FORMAT) +
                " standard " + outputKey(format, MediaFormat.KEY_COLOR_STANDARD) +
                " transfer " + outputKey(format, MediaFormat.KEY_COLOR_TRANSFER) +
                " range " + outputKey(format, MediaFormat.KEY_COLOR_RANGE);
    }

    private static String outputKey(MediaFormat format, String key) {
        return format.containsKey(key) ? String.valueOf(format.getInteger(key)) : "?";
    }
}
