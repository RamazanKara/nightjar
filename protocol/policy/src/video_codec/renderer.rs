//! Pure full-range SDR (`FullRangePolicy`, `#153`) and video renderer selection
//! (`VideoRendererChoice`) policies.

use super::MIN_SDK_Q;

pub const MIN_SDK: i32 = MIN_SDK_Q;

/// Reported `MediaFormat.KEY_COLOR_RANGE` normalized across the JNI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRange {
    Absent,
    Full,
    Limited,
    Unknown(i32),
}

impl ColorRange {
    pub const fn from_jni(code: i32, raw: i32) -> Self {
        match code {
            0 => Self::Absent,
            1 => Self::Full,
            2 => Self::Limited,
            _ => Self::Unknown(raw),
        }
    }
}

/// Renderer selected to present a `MediaCodec` stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Renderer {
    Vulkan = 0,
    Gles = 1,
    Direct = 2,
}

/// Why Vulkan, though requested, is not the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RendererReason {
    None = 0,
    PyroWave = 1,
    ApiLevel = 2,
    Unsupported = 3,
    Packed444 = 4,
}

/// Why Frame Warp, though offered, is not drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameWarpReason {
    None = 0,
    Hdr = 1,
    FullChroma = 2,
}

/// Result of [`choose`] / [`choose_resolved`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoRendererDecision {
    pub renderer: Renderer,
    pub reason: RendererReason,
    pub upscaling_overridden: bool,
    pub frame_warp: bool,
    pub frame_warp_reason: FrameWarpReason,
}

impl VideoRendererDecision {
    /// Packs the decision into 9 bits of an `i32` for zero-allocation JNI return.
    pub const fn pack(self) -> i32 {
        (self.renderer as i32)
            | ((self.reason as i32) << 2)
            | ((self.upscaling_overridden as i32) << 5)
            | ((self.frame_warp as i32) << 6)
            | ((self.frame_warp_reason as i32) << 7)
    }
}

/// Human-readable range label used in [`output_fallback_reason`].
pub fn range_name(color_range: ColorRange) -> String {
    match color_range {
        ColorRange::Absent => "unspecified".to_owned(),
        ColorRange::Full => "full (1)".to_owned(),
        ColorRange::Limited => "limited (2)".to_owned(),
        ColorRange::Unknown(raw) => format!("unknown ({raw})"),
    }
}

/// Whether an SDR stream running without Vulkan must switch to Vulkan based on the decoder's
/// reported output color format, returning a log reason or `None`.
#[allow(clippy::too_many_arguments)]
pub fn output_fallback_reason(
    full_range: bool,
    hdr: bool,
    dolby_vision: bool,
    packed444: bool,
    pyro_wave: bool,
    output_color_range: ColorRange,
    has_color_aspects: bool,
) -> Option<String> {
    if !full_range || hdr || dolby_vision || packed444 || pyro_wave {
        return None;
    }
    match output_color_range {
        ColorRange::Full => None,
        ColorRange::Absent => has_color_aspects.then(|| {
            "omitted color range (defaults to limited) for a full-range SDR stream".to_owned()
        }),
        other => Some(format!(
            "reported {} color range for a full-range SDR stream",
            range_name(other)
        )),
    }
}

/// Whether the API level supports Vulkan (`sdk_int >= 29`).
pub const fn can_use_vulkan_sdk(sdk_int: i32) -> bool {
    sdk_int >= MIN_SDK
}

/// Whether the Vulkan renderer can take over on `sdk_int` and GPU; probes `vulkan_supported` lazily.
pub fn can_use_vulkan(sdk_int: i32, vulkan_supported: impl FnOnce() -> bool) -> bool {
    can_use_vulkan_sdk(sdk_int) && vulkan_supported()
}

/// Whether `choose` needs to probe Vulkan support at all (at most once across all options).
pub const fn needs_vulkan_probe(
    vulkan_preferred: bool,
    pyro_wave: bool,
    sdk_int: i32,
    packed444: bool,
    frame_warp: bool,
) -> bool {
    !pyro_wave && !packed444 && sdk_int >= MIN_SDK && (vulkan_preferred || frame_warp)
}

/// Unified renderer decision given an already-probed (when [`needs_vulkan_probe`] is true)
/// `vulkan_supported` result.
#[allow(clippy::too_many_arguments)]
pub const fn choose_resolved(
    vulkan_preferred: bool,
    pyro_wave: bool,
    sdk_int: i32,
    vulkan_supported: bool,
    packed444: bool,
    upscaling_on: bool,
    frame_warp: bool,
    hdr_or_ten_bit: bool,
) -> VideoRendererDecision {
    let without = if pyro_wave {
        Renderer::Direct
    } else if upscaling_on || packed444 {
        Renderer::Gles
    } else {
        Renderer::Direct
    };

    let plain = if !vulkan_preferred {
        VideoRendererDecision {
            renderer: without,
            reason: RendererReason::None,
            upscaling_overridden: false,
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::None,
        }
    } else if pyro_wave {
        VideoRendererDecision {
            renderer: without,
            reason: RendererReason::PyroWave,
            upscaling_overridden: false,
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::None,
        }
    } else if packed444 {
        VideoRendererDecision {
            renderer: without,
            reason: RendererReason::Packed444,
            upscaling_overridden: false,
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::None,
        }
    } else if sdk_int < MIN_SDK {
        VideoRendererDecision {
            renderer: without,
            reason: RendererReason::ApiLevel,
            upscaling_overridden: false,
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::None,
        }
    } else if !vulkan_supported {
        VideoRendererDecision {
            renderer: without,
            reason: RendererReason::Unsupported,
            upscaling_overridden: false,
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::None,
        }
    } else {
        VideoRendererDecision {
            renderer: Renderer::Vulkan,
            reason: RendererReason::None,
            upscaling_overridden: upscaling_on,
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::None,
        }
    };

    if !frame_warp {
        plain
    } else if pyro_wave {
        VideoRendererDecision {
            frame_warp: true,
            frame_warp_reason: FrameWarpReason::None,
            ..plain
        }
    } else if packed444 {
        VideoRendererDecision {
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::FullChroma,
            ..plain
        }
    } else if matches!(plain.renderer, Renderer::Vulkan)
        || (!vulkan_preferred && sdk_int >= MIN_SDK && vulkan_supported)
    {
        VideoRendererDecision {
            renderer: Renderer::Vulkan,
            reason: plain.reason,
            upscaling_overridden: upscaling_on,
            frame_warp: true,
            frame_warp_reason: FrameWarpReason::None,
        }
    } else if hdr_or_ten_bit {
        VideoRendererDecision {
            frame_warp: false,
            frame_warp_reason: FrameWarpReason::Hdr,
            ..plain
        }
    } else {
        VideoRendererDecision {
            renderer: Renderer::Gles,
            reason: plain.reason,
            upscaling_overridden: upscaling_on,
            frame_warp: true,
            frame_warp_reason: FrameWarpReason::None,
        }
    }
}

/// Unified renderer decision probing `vulkan_supported` lazily at most once.
#[allow(clippy::too_many_arguments)]
pub fn choose(
    vulkan_preferred: bool,
    pyro_wave: bool,
    sdk_int: i32,
    vulkan_supported: impl FnOnce() -> bool,
    packed444: bool,
    upscaling_on: bool,
    frame_warp: bool,
    hdr_or_ten_bit: bool,
) -> VideoRendererDecision {
    let supported =
        if needs_vulkan_probe(vulkan_preferred, pyro_wave, sdk_int, packed444, frame_warp) {
            vulkan_supported()
        } else {
            false
        };
    choose_resolved(
        vulkan_preferred,
        pyro_wave,
        sdk_int,
        supported,
        packed444,
        upscaling_on,
        frame_warp,
        hdr_or_ten_bit,
    )
}

/// Whether Frame Warp is asked for the next stream.
pub const fn frame_warp_asked(frame_warp_pref: bool, frame_warp_listed: bool) -> bool {
    frame_warp_pref && frame_warp_listed
}

/// Whether Vulkan will present a HEVC/AV1 HDR stream that is not packed.
pub fn vulkan_presents(
    vulkan_preferred: bool,
    sdk_int: i32,
    vulkan_supported: impl FnOnce() -> bool,
    upscaling_on: bool,
    frame_warp_pref: bool,
    frame_warp_listed: bool,
) -> bool {
    let decision = choose(
        vulkan_preferred,
        false,
        sdk_int,
        vulkan_supported,
        false,
        upscaling_on,
        frame_warp_asked(frame_warp_pref, frame_warp_listed),
        true,
    );
    matches!(decision.renderer, Renderer::Vulkan)
}
