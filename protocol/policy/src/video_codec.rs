//! Pure video codec, capability, HDR static info, and frame-gap/thermal policy logic extracted
//! from Android's `MediaCodecDecoderRenderer` and its companion video policies.
//!
//! Submodules:
//! - [`renderer`]: `FullRangePolicy` (`#153`) and `VideoRendererChoice`.
//! - [`hdr`]: `Sdr10Policy`, `HdrPolicy`, and `HdrFormatPolicy`.

pub mod hdr;
pub mod renderer;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_hdr;

/// Video format bits (matching `MoonBridge.VIDEO_FORMAT_*` and `nightjar_core::sdp::VIDEO_FORMAT_*`).
pub const VIDEO_FORMAT_H264: u32 = 0x0001;
pub const VIDEO_FORMAT_H264_HIGH8_444: u32 = 0x0004;
pub const VIDEO_FORMAT_H265: u32 = 0x0100;
pub const VIDEO_FORMAT_H265_MAIN10: u32 = 0x0200;
pub const VIDEO_FORMAT_H265_REXT8_444: u32 = 0x0400;
pub const VIDEO_FORMAT_H265_REXT10_444: u32 = 0x0800;
pub const VIDEO_FORMAT_AV1_MAIN8: u32 = 0x1000;
pub const VIDEO_FORMAT_AV1_MAIN10: u32 = 0x2000;
pub const VIDEO_FORMAT_AV1_HIGH8_444: u32 = 0x4000;
pub const VIDEO_FORMAT_AV1_HIGH10_444: u32 = 0x8000;
pub const VIDEO_FORMAT_PYROWAVE: u32 = 0x01_0000;
pub const VIDEO_FORMAT_PYROWAVE_444: u32 = 0x02_0000;
pub const VIDEO_FORMAT_PYROWAVE_MAIN10: u32 = 0x04_0000;
pub const VIDEO_FORMAT_PYROWAVE_MAIN10_444: u32 = 0x08_0000;

pub const VIDEO_FORMAT_MASK_H264: u32 = 0x000F;
pub const VIDEO_FORMAT_MASK_H265: u32 = 0x0F00;
pub const VIDEO_FORMAT_MASK_AV1: u32 = 0xF000;
pub const VIDEO_FORMAT_MASK_PYROWAVE: u32 = 0x0F_0000;
pub const VIDEO_FORMAT_MASK_10BIT: u32 = 0x0C_AA00;
pub const VIDEO_FORMAT_MASK_YUV444: u32 = 0x0A_CC04;

/// Decoder capability bits (matching `MoonBridge.CAPABILITY_*`).
pub const CAPABILITY_DIRECT_SUBMIT: u32 = 0x1;
pub const CAPABILITY_REFERENCE_FRAME_INVALIDATION_AVC: u32 = 0x2;
pub const CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC: u32 = 0x4;
pub const CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1: u32 = 0x40;

pub const fn capability_slices_per_frame(slices: u8) -> u32 {
    ((slices as i8 as i32) << 24) as u32
}

/// Minimum Android API level (`Build.VERSION_CODES.Q`, Android 10) for Vulkan and `setParameters` HDR metadata.
pub const MIN_SDK_Q: i32 = 29;

/// Packed 4:4:4 mode bits that are 10-bit (codec * 2 + depth).
pub const PACKED_10BIT_MODES: i32 = 0b1010;

/// Host `ServerCodecModeSupport` bits for PyroWave SDR and 10-bit.
pub const HOST_PYROWAVE_SDR: u32 = 0x0080_0000 | 0x0100_0000;
pub const HOST_PYROWAVE_10BIT: u32 = 0x0200_0000 | 0x0400_0000;

pub const PYROWAVE_NOT_OFFERED: i32 = -1;
pub const PYROWAVE_FULL_CHROMA: i32 = -2;
pub const PYROWAVE_NO_HDR_ON_PC: i32 = -3;

pub const ISSUE_HOST_NO_TEN_BIT: &str = "host doesn't offer 10-bit full chroma";
pub const ISSUE_NO_TEN_BIT_IN_COMMON: &str = "this device and PC share no 10-bit full chroma mode";
pub const ISSUE_NO_HDR10_DISPLAY: &str = "this display does not report HDR10";
pub const ISSUE_NO_TEN_BIT_OUTPUT: &str = "the GPU offers no 10-bit BT.2020 PQ output surface";

/// Canonical `HdrChange` enum matching `MediaCodecDecoderRenderer.HdrChange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum HdrChange {
    None = 0,
    Parameters = 1,
    Restart = 2,
}

/// Canonical PyroWave drop classification returned over JNI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum PyroWaveDrop {
    None = 0,
    NotOffered = PYROWAVE_NOT_OFFERED,
    FullChroma = PYROWAVE_FULL_CHROMA,
    NoHdrOnPc = PYROWAVE_NO_HDR_ON_PC,
    DeviceReason = 1,
    MissingSwapchain = 2,
    HdrFailure = 3,
}

/// Canonical thermal overlay label classification matching `MediaCodecDecoderRenderer.thermalState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ThermalOverlayState {
    Normal = 0,
    Warm = 1,
    WarmHolding = 2,
    Hot = 3,
    HotEasing = 4,
}

/// Thermal levels matching `ThermalMonitor`.
pub const THERMAL_NORMAL: i32 = 0;
pub const THERMAL_WARM: i32 = 1;
pub const THERMAL_HOT: i32 = 2;

/// Frame-gap accounting delta computed by [`record_frame_gap`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameGapDelta {
    pub total_frames: i32,
    pub decoder_drops: i32,
    pub frames_lost: i32,
    pub frame_loss_events: i32,
}

const PYROWAVE_PRIORITY: [u32; 4] = [
    VIDEO_FORMAT_PYROWAVE_MAIN10_444,
    VIDEO_FORMAT_PYROWAVE_MAIN10,
    VIDEO_FORMAT_PYROWAVE_444,
    VIDEO_FORMAT_PYROWAVE,
];

/// Input buffer size hint for MediaCodec (`KEY_MAX_INPUT_SIZE`), matching
/// `MediaCodecDecoderRenderer.maxInputSize`.
///
/// Android's Codec2/OMX HEVC and AV1 decoders default to ~368 KiB at 720p and ~768 KiB–1 MiB at
/// 1080p (`minCompressionRatio = 4`), so omitting `KEY_MAX_INPUT_SIZE` below 2 MiB forced a
/// mid-stream codec teardown/restart (`requestInputBufferGrowth`) on the first ~1.4 MiB
/// 4-FEC-block IDR or ~2–4 MiB wired keyframe.
///
/// Preserves the exact integer division order `((bitrate * 1000 / 8) / min(fps, 60)) * 4` so
/// 90/120/240 fps streams keep 60 fps keyframe headroom, takes at least `width * height`
/// (8.3 MB at 4K), and enforces a 2 MiB floor on sub-1080p streams and a 4 MiB floor at 1080p+
/// or 100 Mbps+, capped at 32 MiB.
pub fn max_input_size(bitrate_kbps: i32, fps: i32, width: i32, height: i32) -> i32 {
    if bitrate_kbps <= 0 || fps <= 0 || width <= 0 || height <= 0 {
        return 0;
    }
    let effective_fps = i64::from(fps.min(60));
    let four_frames = (i64::from(bitrate_kbps) * 1000 / 8 / effective_fps) * 4;
    let one_byte_per_pixel = i64::from(width) * i64::from(height);
    let floor = if one_byte_per_pixel >= 1920 * 1080 || bitrate_kbps >= 100_000 {
        4 * 1024 * 1024
    } else {
        2 * 1024 * 1024
    };
    four_frames
        .max(one_byte_per_pixel)
        .max(floor)
        .min(32 * 1024 * 1024) as i32
}

/// Grown input buffer size when `required_bytes` overflows `capacity`, or 0 when growing cannot help.
pub fn grown_input_size(required_bytes: i64, capacity: i32, previous_request: i32) -> i32 {
    if required_bytes <= i64::from(capacity)
        || required_bytes <= i64::from(previous_request)
        || required_bytes > i64::from(i32::MAX)
    {
        return 0;
    }
    let doubled = i64::from(capacity) * 2;
    required_bytes.max(doubled).min(i64::from(i32::MAX)) as i32
}

/// How the running decoder handles an incoming HDR mode / metadata report.
pub fn hdr_change(
    previous_mode: bool,
    enabled: bool,
    previous_metadata: Option<&[u8]>,
    metadata: Option<&[u8]>,
    sdk_int: i32,
) -> HdrChange {
    if previous_mode == enabled && previous_metadata == metadata {
        return HdrChange::None;
    }
    if previous_mode == enabled && enabled && metadata.is_some() && sdk_int >= MIN_SDK_Q {
        return HdrChange::Parameters;
    }
    HdrChange::Restart
}

/// Whether a decoder restart must release and recreate the codec instead of `stop()` + `configure()`.
pub fn restart_recreates(decoder_name: &str, tuning_keys_lines: &str) -> bool {
    let lower = decoder_name.to_ascii_lowercase();
    if lower.starts_with("c2.mtk.") {
        tuning_keys_lines
            .lines()
            .any(|key| key.starts_with("vendor.mtk.vdec.low-latency.mode=") && !key.ends_with("=0"))
    } else if lower.starts_with("c2.dolby.") {
        tuning_keys_lines
            .lines()
            .any(|key| key.starts_with("vendor.qti-ext-") && !key.ends_with("=0"))
    } else {
        // Tegra's Dolby Vision decoder, whatever its keys: it and the hardware composer crashed on
        // a Shield (nightjar#193), so no restart reuses its state.
        lower.starts_with("omx.nvidia.dovi")
    }
}

/// Whether the decoder's output must carry the PC's mastering metadata (false when Vulkan presents).
pub const fn decoder_carries_hdr_metadata(vulkan_presents: bool) -> bool {
    !vulkan_presents
}

/// Whether an output format's 25-byte `hdr-static-info` matches `expected` within +-1 per u16 field.
pub fn carries_hdr_static_info(carried: &[u8], expected: &[u8]) -> bool {
    if expected.len() != 25 || carried.len() < expected.len() {
        return false;
    }
    if carried[0] != expected[0] {
        return false;
    }
    for i in (1..expected.len()).step_by(2) {
        let got = i32::from(u16::from_le_bytes([carried[i], carried[i + 1]]));
        let want = i32::from(u16::from_le_bytes([expected[i], expected[i + 1]]));
        if (got - want).abs() > 1 {
            return false;
        }
    }
    true
}

/// Whether the 4:4:4 fallback toast explains that HDR took the stream instead of plain 4:4:4 refusal.
pub const fn yuv444_fallback_is_hdr(format: u32, sdr10: bool) -> bool {
    (format & VIDEO_FORMAT_MASK_10BIT) != 0 && !sdr10
}

/// Whether packed full chroma drops PyroWave (only when native 4:4:4 is off).
pub const fn pyrowave_full_chroma_drop(packed444: bool, yuv444: bool) -> bool {
    packed444 && !yuv444
}

/// Why PyroWave was dropped when the user forced it.
pub const fn pyrowave_drop_reason(
    chosen: bool,
    full_chroma: bool,
    device_reason: i32,
    host_codecs: u32,
    formats: u32,
    choice: u32,
    hdr_failure: i32,
) -> PyroWaveDrop {
    if !chosen {
        return PyroWaveDrop::None;
    }
    if full_chroma {
        return PyroWaveDrop::FullChroma;
    }
    if device_reason != 0 {
        return PyroWaveDrop::DeviceReason;
    }
    if (formats & VIDEO_FORMAT_MASK_PYROWAVE) == 0 {
        if (host_codecs & (HOST_PYROWAVE_SDR | HOST_PYROWAVE_10BIT)) == 0 {
            return PyroWaveDrop::NotOffered;
        }
        if (formats & VIDEO_FORMAT_MASK_10BIT) != 0 && (host_codecs & HOST_PYROWAVE_10BIT) == 0 {
            return PyroWaveDrop::NoHdrOnPc;
        }
        return PyroWaveDrop::None;
    }
    if choice == 0 {
        return PyroWaveDrop::MissingSwapchain;
    }
    if (choice & VIDEO_FORMAT_MASK_10BIT) == 0 && hdr_failure != 0 {
        PyroWaveDrop::HdrFailure
    } else {
        PyroWaveDrop::None
    }
}

/// PyroWave candidate formats in priority order matching `formats`.
pub fn pyrowave_candidate_formats(formats: u32) -> Vec<u32> {
    PYROWAVE_PRIORITY
        .into_iter()
        .filter(|&choice| (formats & choice) != 0)
        .collect()
}

/// Lazy helper picking the first PyroWave format in `formats` for which `setup` returns `true`.
pub fn first_pyrowave_format(formats: u32, mut setup: impl FnMut(u32) -> bool) -> u32 {
    for choice in pyrowave_candidate_formats(formats) {
        if setup(choice) {
            return choice;
        }
    }
    0
}

/// Why the PC's packed modes leave this device's verified modes an 8-bit mode without a 10-bit one.
/// Returns `None` when `host_modes < 0` (unknown PC), no mode is in common, or a 10-bit mode matches.
pub const fn host_ten_bit_issue(device_modes: i32, host_modes: i32) -> Option<&'static str> {
    if host_modes < 0
        || (host_modes & device_modes) == 0
        || (host_modes & device_modes & PACKED_10BIT_MODES) != 0
    {
        return None;
    }
    if (host_modes & PACKED_10BIT_MODES) == 0 {
        Some(ISSUE_HOST_NO_TEN_BIT)
    } else {
        Some(ISSUE_NO_TEN_BIT_IN_COMMON)
    }
}

/// Why a packed stream that offered 10-bit negotiated an 8-bit format (`host_modes < 0` blames host).
pub fn host_ten_bit_drop(
    ten_bit_offered: bool,
    drop_at_launch: Option<&str>,
    negotiated_format: u32,
    host_modes: i32,
) -> Option<String> {
    if !ten_bit_offered
        || drop_at_launch.is_some()
        || (negotiated_format & VIDEO_FORMAT_MASK_10BIT) != 0
    {
        return drop_at_launch.map(str::to_owned);
    }
    if host_modes >= 0 && (host_modes & PACKED_10BIT_MODES) != 0 {
        Some(ISSUE_NO_TEN_BIT_IN_COMMON.to_owned())
    } else {
        Some(ISSUE_HOST_NO_TEN_BIT.to_owned())
    }
}

/// Why 10-bit packed output cannot be shown, checking display HDR10 before GPU 10-bit output.
pub const fn ten_bit_output_issue(
    display_hdr10: bool,
    gpu_ten_bit_output: bool,
) -> Option<&'static str> {
    if !display_hdr10 {
        Some(ISSUE_NO_HDR10_DISPLAY)
    } else if !gpu_ten_bit_output {
        Some(ISSUE_NO_TEN_BIT_OUTPUT)
    } else {
        None
    }
}

/// Builds the `MoonBridge` video capability bitmask for `format`.
#[allow(clippy::too_many_arguments)]
pub const fn capabilities_for(
    format: u32,
    sdr10: bool,
    has_runner: bool,
    runner_slices: u8,
    runner_direct_submit: bool,
    slices8: u8,
    slices10: u8,
    direct_submit8: bool,
    rfi_avc: bool,
    rfi_hevc: bool,
    rfi_av1: bool,
    rfi_hevc10: bool,
    rfi_av1_10: bool,
) -> u32 {
    let ten_bit = sdr10 && (format & VIDEO_FORMAT_MASK_10BIT) != 0;
    let hevc = (format & VIDEO_FORMAT_MASK_H265) != 0;
    let av1 = (format & VIDEO_FORMAT_MASK_AV1) != 0;
    let slices = if has_runner {
        runner_slices
    } else if ten_bit && hevc {
        slices10
    } else {
        slices8
    };
    let submit_direct = if has_runner {
        runner_direct_submit
    } else {
        direct_submit8
    };

    let mut capabilities = capability_slices_per_frame(slices);
    if rfi_avc {
        capabilities |= CAPABILITY_REFERENCE_FRAME_INVALIDATION_AVC;
    }
    if if ten_bit && hevc {
        rfi_hevc10
    } else {
        rfi_hevc
    } {
        capabilities |= CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC;
    }
    if if ten_bit && av1 { rfi_av1_10 } else { rfi_av1 } {
        capabilities |= CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1;
    }
    if submit_direct {
        capabilities |= CAPABILITY_DIRECT_SUBMIT;
    }
    capabilities
}

/// Strips HEVC/AV1 RFI when Dolby Vision erratum 23 corrupts that codec's decoder.
/// Returns `(capabilities, took_hevc_rfi, took_av1_rfi)`.
pub const fn apply_dolby_vision_rfi(
    mut capabilities: u32,
    dolby_vision_corrupts: bool,
    dolby_vision_av1_corrupts: bool,
    hdr_format: u32,
) -> (u32, bool, bool) {
    let hevc = dolby_vision_corrupts
        && (hdr_format & (rubylight_protocol::hdr::DOLBY_VISION_HEVC as u32)) != 0;
    let av1 = dolby_vision_av1_corrupts
        && (hdr_format & (rubylight_protocol::hdr::DOLBY_VISION_AV1 as u32)) != 0;
    if hevc {
        capabilities &= !CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC;
    }
    if av1 {
        capabilities &= !CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1;
    }
    (capabilities, hevc, av1)
}

/// Splits a frame-number gap into decoder drops vs network loss (`nightjar#80`).
pub fn record_frame_gap(gap: i32, new_decoder_drops: i64) -> FrameGapDelta {
    let decoder = i64::from(gap).min(new_decoder_drops).max(0) as i32;
    let (frames_lost, frame_loss_events) = if gap > decoder {
        (gap - decoder, 1)
    } else {
        (0, 0)
    };
    FrameGapDelta {
        total_frames: gap,
        decoder_drops: decoder,
        frames_lost,
        frame_loss_events,
    }
}

/// Classifies the thermal state label for the performance overlay.
pub const fn thermal_state(
    level: i32,
    holds_bitrate: bool,
    eases_load: bool,
) -> ThermalOverlayState {
    if level == THERMAL_HOT {
        if eases_load {
            ThermalOverlayState::HotEasing
        } else {
            ThermalOverlayState::Hot
        }
    } else if level == THERMAL_WARM {
        if holds_bitrate {
            ThermalOverlayState::WarmHolding
        } else {
            ThermalOverlayState::Warm
        }
    } else {
        ThermalOverlayState::Normal
    }
}
