package com.limelight.binding.video;

import java.nio.ByteBuffer;
import java.util.List;

import android.media.MediaCodec;
import android.media.MediaFormat;
import android.os.Build;
import android.os.Bundle;

import com.limelight.LimeLog;
import com.limelight.nvstream.jni.MoonBridge;
import com.limelight.nvstream.jni.NightjarPolicyJni;

/**
 * Handles MediaCodec HDR color configuration, runtime HDR10 static metadata
 * parameter updates, output-format metadata verification, and restart policy
 * checks for {@link MediaCodecDecoderRenderer}.
 */
final class MediaCodecHdrControl {
    private MediaCodecHdrControl() {}

    // Sets the HDR metadata, or clears a stale copy. A format that never had the key is left alone:
    // before Android 10 clearing means a null entry, which an older OMX decoder (Fire OS 7) may
    // refuse in configure(), and Artemis never writes the key without metadata either.
    static void applyHdrStaticInfo(MediaFormat format, ByteBuffer hdrStaticInfo) {
        if (hdrStaticInfo != null) {
            format.setByteBuffer(MediaFormat.KEY_HDR_STATIC_INFO, hdrStaticInfo);
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            format.removeKey(MediaFormat.KEY_HDR_STATIC_INFO);
        } else if (format.containsKey(MediaFormat.KEY_HDR_STATIC_INFO)) {
            format.setByteBuffer(MediaFormat.KEY_HDR_STATIC_INFO, null);
        }
    }

    static ByteBuffer createHdrStaticInfo(byte[] metadata) {
        if (metadata == null || metadata.length < 24) {
            return null;
        }
        // The wire layout is CTA-861.3 without the type byte, followed by an
        // optional full-frame luminance field that Android does not accept.
        ByteBuffer info = ByteBuffer.allocate(25);
        info.put((byte) 0).put(metadata, 0, 24).rewind();
        return info;
    }

    static void applyConfigureColor(MediaFormat format, boolean hdr, int colorSpace, byte[] hdrMetadata,
                                    int hdrTransfer) {
        format.setInteger(MediaFormat.KEY_COLOR_TRANSFER, hdr ? hdrTransfer : MediaFormat.COLOR_TRANSFER_SDR_VIDEO);
        format.setInteger(MediaFormat.KEY_COLOR_STANDARD, hdr ? MediaFormat.COLOR_STANDARD_BT2020 :
                colorSpace == MoonBridge.COLORSPACE_REC_601 ?
                        MediaFormat.COLOR_STANDARD_BT601_NTSC : MediaFormat.COLOR_STANDARD_BT709);
        applyHdrStaticInfo(format, hdr ? createHdrStaticInfo(hdrMetadata) : null);
    }

    static boolean restartRecreates(String decoderName, List<String> tuningKeys) {
        if (decoderName == null || tuningKeys == null || tuningKeys.isEmpty()) return false;
        return NightjarPolicyJni.videoRestartRecreates(decoderName, String.join("\n", tuningKeys));
    }

    static void setHdrMode(MediaCodecDecoderRenderer r, boolean enabled, byte[] hdrMetadata) {
        // A callback during recovery must not have its new metadata or restart request overwritten.
        if (r.decoderFailure != null) return;
        synchronized (r.codecRecoveryMonitor) {
            if (r.stopping) {
                return;
            }
            if (r.packing != null) {
                r.currentHdrMode = enabled;
                r.currentHdrMetadata = enabled && hdrMetadata != null ? hdrMetadata.clone() : null;
                if (r.upscaler != null) {
                    if (enabled && !r.codecSelector.packedHdrAvailable()) r.upscalingPolicy.fail(UpscalingPolicy.Reason.HDR);
                    else r.upscaler.setHdrMode(enabled, r.currentHdrMetadata);
                }
                return;
            }
            if ((r.videoFormat & MoonBridge.VIDEO_FORMAT_MASK_PYROWAVE) != 0) {
                r.currentHdrMode = enabled;
                r.currentHdrMetadata = enabled && hdrMetadata != null && hdrMetadata.length >= 24 ? hdrMetadata.clone() : null;
                r.pyroWaveRenderer.setHdrMode(enabled, r.currentHdrMetadata);
                return;
            }
            byte[] metadata = enabled && hdrMetadata != null && hdrMetadata.length >= 24 ? hdrMetadata : null;
            boolean firstReport = r.currentHdrMode == null;
            boolean previousMode = !firstReport ? r.currentHdrMode : r.codecSelector.assumesHdr();
            boolean vulkanUnchanged = !firstReport
                    && previousMode == enabled
                    && java.util.Arrays.equals(r.currentHdrMetadata, metadata);
            r.currentHdrMode = enabled;
            // The Vulkan renderer switches its output between SDR and HDR10 itself
            VulkanRendererBridge vulkan = r.vulkanRenderer;
            if (vulkan != null && !vulkanUnchanged) vulkan.setHdrMode(enabled, metadata);
            if (enabled && r.upscaler != null) r.upscalingPolicy.fail(UpscalingPolicy.Reason.HDR);
            MediaCodecDecoderRenderer.HdrChange change = MediaCodecDecoderRenderer.hdrChange(
                    previousMode, enabled, r.currentHdrMetadata, metadata, Build.VERSION.SDK_INT);
            if (change == MediaCodecDecoderRenderer.HdrChange.NONE) {
                return;
            }
            r.currentHdrMetadata = metadata == null ? null : metadata.clone();
            if (change == MediaCodecDecoderRenderer.HdrChange.PARAMETERS &&
                    (!MediaCodecDecoderRenderer.decoderCarriesHdrMetadata(vulkan != null) ||
                            setRunningHdrStaticInfo(r, r.currentHdrMetadata))) {
                return;
            }
            // HACK: Reset codec recovery attempt counter, since this is an expected "recovery"
            r.codecRecoveryAttempts = 0;
            // The host's first HDR report restarts a 10-bit stream once: it adds metadata or turns HDR off.
            r.session.requestRestart(enabled ? (metadata != null ? "HDR metadata" : "HDR on") : "HDR off");
        }
    }

    private static boolean setRunningHdrStaticInfo(MediaCodecDecoderRenderer r, byte[] metadata) {
        MediaCodec decoder = r.videoDecoder;
        if (decoder == null || r.codecRecoveryType.get() != MediaCodecDecoderRenderer.CR_RECOVERY_TYPE_NONE) {
            return true;
        }
        ByteBuffer info = createHdrStaticInfo(metadata);
        Bundle params = new Bundle();
        params.putByteArray(MediaFormat.KEY_HDR_STATIC_INFO, info.array());
        try {
            decoder.setParameters(params);
        } catch (IllegalStateException e) {
            LimeLog.warning("Decoder refused the HDR metadata as a parameter (" + e + "); restarting it");
            return false;
        }
        LimeLog.info("HDR metadata set on the running decoder; its output format is checked for it");
        // The count first: the output thread reads it after the volatile check it pairs with.
        r.hdrStaticInfoOutputs = 0;
        r.uncheckedHdrStaticInfo = info.array();
        return true;
    }

    static void checkRunningHdrStaticInfo(MediaCodecDecoderRenderer r) {
        if (r.uncheckedHdrStaticInfo == null) {
            return;
        }
        synchronized (r.codecRecoveryMonitor) {
            byte[] expected = r.uncheckedHdrStaticInfo;
            MediaFormat format = r.outputFormat;
            if (expected == null || r.codecRecoveryType.get() != MediaCodecDecoderRenderer.CR_RECOVERY_TYPE_NONE) {
                return;
            }
            if (!MediaCodecDecoderRenderer.decoderCarriesHdrMetadata(r.vulkanRenderer != null)) {
                r.uncheckedHdrStaticInfo = null;
                return;
            }
            if (r.dolbyVisionActive && format != null && format.containsKey(MediaFormat.KEY_COLOR_TRANSFER) &&
                    format.getInteger(MediaFormat.KEY_COLOR_TRANSFER) != MediaFormat.COLOR_TRANSFER_ST2084) {
                r.uncheckedHdrStaticInfo = null;
                LimeLog.info("Dolby Vision decoder maps the picture itself; output format has no HDR10 static metadata");
                return;
            }
            ByteBuffer carried = format != null && format.containsKey(MediaFormat.KEY_HDR_STATIC_INFO) ?
                    format.getByteBuffer(MediaFormat.KEY_HDR_STATIC_INFO) : null;
            if (carriesHdrStaticInfo(carried, expected)) {
                r.uncheckedHdrStaticInfo = null;
                LimeLog.info("Decoder output carries the HDR metadata after " + r.hdrStaticInfoOutputs + " frames");
                return;
            }
            if (++r.hdrStaticInfoOutputs < MediaCodecDecoderRenderer.HDR_STATIC_INFO_OUTPUTS) {
                return;
            }
            r.uncheckedHdrStaticInfo = null;
            LimeLog.warning("Decoder took the HDR metadata as a parameter, but its output format lacks it after " +
                    MediaCodecDecoderRenderer.HDR_STATIC_INFO_OUTPUTS + " frames; restarting it with the metadata");
            r.codecRecoveryAttempts = 0;
            r.session.requestRestart("HDR metadata missing from the output");
        }
    }

    static boolean carriesHdrStaticInfo(ByteBuffer carried, byte[] expected) {
        if (carried == null || expected == null || expected.length != 25 || carried.remaining() < expected.length) {
            return false;
        }
        byte[] bytes = new byte[expected.length];
        carried.duplicate().get(bytes);
        return NightjarPolicyJni.videoCarriesHdrStaticInfo(bytes, expected);
    }
}
