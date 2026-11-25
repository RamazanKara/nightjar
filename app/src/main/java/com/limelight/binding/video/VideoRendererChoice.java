package com.limelight.binding.video;

import com.limelight.nvstream.jni.NightjarPolicyJni;

import java.util.function.BooleanSupplier;

/**
 * Which renderer presents a MediaCodec stream. Pure, so each rule can be unit tested.
 *
 * With "Render through Vulkan" off nothing changes: the direct SurfaceView path, or the GLES
 * presenter when an upscaler is selected. With it on, Vulkan is used unless something rules it
 * out, and then the {@link Reason} says what, so the user is told instead of silently getting a
 * different renderer.
 *
 * Frame Warp (docs/frame-warp.md) shifts each picture in a shader, so a stream that draws it needs
 * one: PyroWave's own renderer, which draws it as it is; else Vulkan where it runs, asked for or
 * not, or the GLES presenter without upscaling. Only SDR reaches the GLES path, and packed full
 * chroma has its own; then the stream draws without it and {@link FrameWarpReason} says why.
 * Without Frame Warp the choice is the same as it always was.
 */
public final class VideoRendererChoice {
    public enum Renderer {
        VULKAN,
        /** The GLES presenter may be used (upscaling, packed 4:4:4, or Frame Warp); it can still decline later. */
        GLES,
        /** The decoder renders straight into the SurfaceView. */
        DIRECT
    }

    /** Why Vulkan, though asked for, is not the renderer. NONE when it is, or was not asked for. */
    public enum Reason {
        NONE, PYROWAVE, API_LEVEL, UNSUPPORTED, PACKED_444
    }

    /** Why Frame Warp, though the stream offers it, is not drawn. NONE when it is, or was not offered. */
    public enum FrameWarpReason {
        NONE,
        /** HDR or 10-bit video without Vulkan: the GLES presenter shows SDR only. */
        HDR,
        /** Packed full chroma is drawn by its own reconstruction. */
        FULL_CHROMA
    }

    public static final int MIN_SDK = 29;

    public final Renderer renderer;
    public final Reason reason;
    /**
     * An upscaler was selected but is not applied: Vulkan is used, or the GLES presenter draws Frame
     * Warp instead.
     */
    public final boolean upscalingOverridden;
    /** The renderer draws Frame Warp. */
    public final boolean frameWarp;
    public final FrameWarpReason frameWarpReason;

    private VideoRendererChoice(Renderer renderer, Reason reason, boolean upscalingOverridden, boolean frameWarp,
                                FrameWarpReason frameWarpReason) {
        this.renderer = renderer;
        this.reason = reason;
        this.upscalingOverridden = upscalingOverridden;
        this.frameWarp = frameWarp;
        this.frameWarpReason = frameWarpReason;
    }

    public static VideoRendererChoice choose(boolean vulkanPreferred, boolean pyroWave, int sdkInt,
                                             BooleanSupplier vulkanSupported, boolean packed444,
                                             UpscalingPolicy.Mode upscaling) {
        return choose(vulkanPreferred, pyroWave, sdkInt, vulkanSupported, packed444, upscaling, false, false);
    }

    /**
     * As {@link #choose(boolean, boolean, int, BooleanSupplier, boolean, UpscalingPolicy.Mode)}, for a stream
     * that offers Frame Warp when {@code frameWarp} is true; false gives exactly that choice.
     *
     * @param hdrOrTenBit the stream is HDR or 10-bit, which the GLES presenter does not show
     */
    public static VideoRendererChoice choose(boolean vulkanPreferred, boolean pyroWave, int sdkInt,
                                             BooleanSupplier vulkanSupported, boolean packed444,
                                             UpscalingPolicy.Mode upscaling, boolean frameWarp,
                                             boolean hdrOrTenBit) {
        boolean upscale = upscaling != UpscalingPolicy.Mode.OFF;
        boolean supported = NightjarPolicyJni.videoRendererNeedsVulkanProbe(
                vulkanPreferred, pyroWave, sdkInt, packed444, frameWarp)
                && vulkanSupported.getAsBoolean();
        int packed = NightjarPolicyJni.videoRendererChoose(
                vulkanPreferred, pyroWave, sdkInt, supported, packed444, upscale, frameWarp,
                hdrOrTenBit);
        return new VideoRendererChoice(
                Renderer.values()[packed & 0x3],
                Reason.values()[(packed >>> 2) & 0x7],
                ((packed >>> 5) & 1) != 0,
                ((packed >>> 6) & 1) != 0,
                FrameWarpReason.values()[(packed >>> 7) & 0x3]);
    }
}
