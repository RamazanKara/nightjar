//! Unit tests for `video_codec` pure codec math, `FullRangePolicy` (`#153`), and
//! `VideoRendererChoice`.

use rubylight_protocol::hdr::{DV_81, DV_101, HLG, PQ};

use super::renderer::{
    ColorRange, FrameWarpReason, MIN_SDK, Renderer, RendererReason, can_use_vulkan, choose,
    frame_warp_asked, needs_vulkan_probe, output_fallback_reason, range_name, vulkan_presents,
};
use super::*;

#[test]
fn full_range_alone_stays_direct_without_a_vulkan_probe() {
    // Vulkan off, full-range SDR: no decoder name enters the choice, so a MediaTek one stays direct.
    let choice = choose(
        false,
        false,
        34,
        || panic!("must not probe when Vulkan is off"),
        false,
        false,
        false,
        false,
    );
    assert_eq!(choice.renderer, Renderer::Direct);
    assert_eq!(choice.reason, RendererReason::None);
}

#[test]
fn output_fallback_detects_only_a_dropped_full_range() {
    let reason =
        |range, aspects| output_fallback_reason(true, false, false, false, false, range, aspects);
    // Full range off, HDR, Dolby Vision, packed 4:4:4 and PyroWave stay on their own paths.
    for [full, hdr, dolby, packed, pyro] in [
        [false, false, false, false, false],
        [true, true, false, false, false],
        [true, false, true, false, false],
        [true, false, false, true, false],
        [true, false, false, false, true],
    ] {
        assert_eq!(
            output_fallback_reason(full, hdr, dolby, packed, pyro, ColorRange::Limited, true),
            None
        );
    }
    // A decoder reporting full range, or no colour keys at all, gives nothing to object to.
    assert_eq!(reason(ColorRange::Full, true), None);
    assert_eq!(reason(ColorRange::Absent, false), None);

    let limited = reason(ColorRange::Limited, true).expect("limited range triggers fallback");
    assert!(limited.contains("limited (2)"));
    let unknown = reason(ColorRange::Unknown(9), true).expect("unknown range triggers fallback");
    assert!(unknown.contains("unknown (9)"));
    let omitted = reason(ColorRange::Absent, true)
        .expect("omitted range with color aspects triggers fallback");
    assert!(omitted.contains("omitted"));
    assert_eq!(range_name(ColorRange::Absent), "unspecified");
    assert_eq!(range_name(ColorRange::Full), "full (1)");
}

#[test]
fn can_use_vulkan_does_not_probe_below_api_29() {
    let mut probed = false;
    assert!(!can_use_vulkan(28, || {
        probed = true;
        true
    }));
    assert!(!probed);
    assert!(can_use_vulkan(MIN_SDK, || {
        probed = true;
        true
    }));
    assert!(probed);
}

#[test]
fn video_renderer_choice_lazy_probe_and_all_cases() {
    let mut probed = false;
    let off = choose(
        false,
        false,
        34,
        || {
            probed = true;
            true
        },
        false,
        false,
        false,
        false,
    );
    assert!(!probed);
    assert_eq!(off.renderer, Renderer::Direct);
    assert_eq!(off.reason, RendererReason::None);
    assert!(!off.upscaling_overridden);

    // Probe must not run when pyroWave, packed444, or API < 29 rules Vulkan out.
    for (pyro, sdk, packed) in [(true, 34, false), (false, 28, false), (false, 34, true)] {
        let mut called = false;
        let _ = choose(
            true,
            pyro,
            sdk,
            || {
                called = true;
                true
            },
            packed,
            false,
            false,
            false,
        );
        assert!(!called);
    }

    // Frame Warp matrix checks
    let fw_unasked = choose(false, false, 29, || true, false, false, true, false);
    assert_eq!(fw_unasked.renderer, Renderer::Vulkan);
    assert!(fw_unasked.frame_warp);
    assert_eq!(fw_unasked.reason, RendererReason::None);
    assert!(!fw_unasked.upscaling_overridden);

    let fw_hdr_gles = choose(false, false, 34, || false, false, false, true, true);
    assert_eq!(fw_hdr_gles.renderer, Renderer::Direct);
    assert!(!fw_hdr_gles.frame_warp);
    assert_eq!(fw_hdr_gles.frame_warp_reason, FrameWarpReason::Hdr);

    let fw_packed = choose(true, false, 34, || true, true, false, true, false);
    assert_eq!(fw_packed.renderer, Renderer::Gles);
    assert_eq!(fw_packed.reason, RendererReason::Packed444);
    assert!(!fw_packed.frame_warp);
    assert_eq!(fw_packed.frame_warp_reason, FrameWarpReason::FullChroma);

    let fw_pyro = choose(true, true, 34, || true, false, true, true, true);
    assert_eq!(fw_pyro.renderer, Renderer::Direct);
    assert_eq!(fw_pyro.reason, RendererReason::PyroWave);
    assert!(fw_pyro.frame_warp);
    assert!(!fw_pyro.upscaling_overridden);

    let mut probes = 0;
    let pref_unsupported = choose(
        true,
        false,
        34,
        || {
            probes += 1;
            false
        },
        false,
        false,
        false,
        false,
    );
    assert_eq!(probes, 1);
    assert_eq!(pref_unsupported.renderer, Renderer::Direct);
    assert_eq!(pref_unsupported.reason, RendererReason::Unsupported);

    assert!(needs_vulkan_probe(false, false, 29, false, true));
    assert!(!needs_vulkan_probe(false, false, 28, false, true));
    assert!(frame_warp_asked(true, true));
    assert!(!frame_warp_asked(true, false));
    assert!(vulkan_presents(false, 34, || true, false, true, true));
    assert!(!vulkan_presents(false, 34, || true, false, true, false));
}

#[test]
fn max_input_size_and_grown_input_size_match_exact_arithmetic() {
    assert_eq!(max_input_size(80_000, 60, 1920, 1080), 0);
    assert_eq!(max_input_size(250_000, 120, 1920, 1080), 0);
    assert_eq!(max_input_size(0, 60, 3840, 2160), 0);
    assert_eq!(max_input_size(500_000, 0, 3840, 2160), 0);
    // Exact integer division order: (1_000_000 * 1000 / 8 / 60) * 4 = 2083333 * 4 = 8333332
    assert_eq!(max_input_size(1_000_000, 60, 1920, 1080), 8_333_332);
    assert_eq!(max_input_size(158_988, 60, 3840, 2160), 3840 * 2160);
    assert_eq!(max_input_size(80_000, 60, 2560, 1440), 2560 * 1440);
    assert_eq!(max_input_size(2_000_000, 24, 3840, 2160), 32 * 1024 * 1024);

    let mib = 1024 * 1024;
    assert_eq!(grown_input_size(3 * i64::from(mib), 2 * mib, 0), 4 * mib);
    assert_eq!(grown_input_size(5 * i64::from(mib), 2 * mib, 0), 5 * mib);
    assert_eq!(grown_input_size(2 * i64::from(mib), 2 * mib, 0), 0);
    assert_eq!(grown_input_size(3 * i64::from(mib), 2 * mib, 4 * mib), 0);
    assert_eq!(
        grown_input_size(5 * i64::from(mib), 3 * mib, 4 * mib),
        6 * mib
    );
    assert_eq!(grown_input_size(i64::from(i32::MAX) + 1, 2 * mib, 0), 0);
    assert_eq!(
        grown_input_size(i64::from(i32::MAX), i32::MAX - 1, 0),
        i32::MAX
    );
}

#[test]
fn hdr_change_restart_recreates_and_static_info_match() {
    let mut first = [0u8; 24];
    first[0] = 1;
    let mut next = first;
    next[23] = 2;
    assert_eq!(
        hdr_change(true, true, None, Some(&first), 29),
        HdrChange::Parameters
    );
    assert_eq!(
        hdr_change(true, true, Some(&first), Some(&next), 34),
        HdrChange::Parameters
    );
    assert_eq!(
        hdr_change(true, true, Some(&first), Some(&first), 29),
        HdrChange::None
    );
    assert_eq!(hdr_change(false, false, None, None, 29), HdrChange::None);
    assert_eq!(
        hdr_change(false, true, None, Some(&first), 29),
        HdrChange::Restart
    );
    assert_eq!(
        hdr_change(true, false, Some(&first), None, 29),
        HdrChange::Restart
    );
    assert_eq!(
        hdr_change(true, true, None, Some(&first), 28),
        HdrChange::Restart
    );
    assert_eq!(
        hdr_change(true, true, Some(&first), None, 29),
        HdrChange::Restart
    );

    let vendor_mtk = "low-latency=1\nvendor.mtk.vdec.low-latency.mode=1\nvendor.mtk.vdec.low-latency.mode.value=1";
    let standard = "low-latency=1\npriority=0";
    assert!(restart_recreates("c2.mtk.dvhe.st.decoder", vendor_mtk));
    assert!(restart_recreates("c2.mtk.hevc.decoder", vendor_mtk));
    assert!(!restart_recreates("c2.mtk.av1.decoder", standard));
    assert!(!restart_recreates(
        "c2.mtk.av1.decoder",
        "vendor.mtk.vdec.low-latency.mode=0"
    ));
    assert!(!restart_recreates("OMX.MTK.VIDEO.DECODER.HEVC", vendor_mtk));
    assert!(!restart_recreates(
        "c2.qti.hevc.decoder",
        "vendor.qti-ext-dec-low-latency.enable=1"
    ));
    assert!(restart_recreates(
        "c2.dolby.decoder.hevc",
        "low-latency=1\nvendor.qti-ext-dec-picture-order.enable=1"
    ));
    assert!(!restart_recreates(
        "c2.dolby.decoder.hevc",
        "vendor.qti-ext-dec-picture-order.enable=0"
    ));
    assert!(restart_recreates("OMX.Nvidia.DOVI.decode", ""));
    assert!(restart_recreates("OMX.Nvidia.DOVI.decode", standard));
    assert!(!restart_recreates("OMX.Nvidia.h265.decode", ""));

    assert!(!decoder_carries_hdr_metadata(true));
    assert!(decoder_carries_hdr_metadata(false));

    let b1fix: [u8; 25] = [
        0x00, 0x48, 0x8a, 0x08, 0x39, 0x34, 0x21, 0xaa, 0x9b, 0x96, 0x19, 0xfc, 0x08, 0x13, 0x3d,
        0x42, 0x40, 0x90, 0x01, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    assert!(carries_hdr_static_info(&b1fix, &b1fix));
    let mut rounded = b1fix;
    rounded[1] = 0x47;
    rounded[15] = 0x43;
    assert!(carries_hdr_static_info(&rounded, &b1fix));
    assert!(!carries_hdr_static_info(&b1fix[..24], &b1fix));
    let mut off_by_two = b1fix;
    off_by_two[17] = 0x92;
    assert!(!carries_hdr_static_info(&off_by_two, &b1fix));
    let mut wrong_desc = b1fix;
    wrong_desc[0] = 1;
    assert!(!carries_hdr_static_info(&wrong_desc, &b1fix));
}

#[test]
fn pyrowave_host_ten_bit_capabilities_frame_gap_and_thermal_rules() {
    assert!(yuv444_fallback_is_hdr(VIDEO_FORMAT_H265_MAIN10, false));
    assert!(!yuv444_fallback_is_hdr(VIDEO_FORMAT_H265_MAIN10, true));
    assert!(!yuv444_fallback_is_hdr(VIDEO_FORMAT_H265, false));

    assert!(pyrowave_full_chroma_drop(true, false));
    assert!(!pyrowave_full_chroma_drop(true, true));
    let pc_none = 0x300;
    let pc_sdr = pc_none | HOST_PYROWAVE_SDR;
    let pc_all = pc_sdr | HOST_PYROWAVE_10BIT;
    let both = VIDEO_FORMAT_PYROWAVE
        | VIDEO_FORMAT_PYROWAVE_MAIN10
        | VIDEO_FORMAT_H265
        | VIDEO_FORMAT_H265_MAIN10;
    let hevc_hdr = VIDEO_FORMAT_H265 | VIDEO_FORMAT_H265_MAIN10;
    assert_eq!(
        pyrowave_drop_reason(true, false, 123, pc_all, both, 0, 0),
        PyroWaveDrop::DeviceReason
    );
    assert_eq!(
        pyrowave_drop_reason(true, false, 0, pc_none, hevc_hdr, 0, 0),
        PyroWaveDrop::NotOffered
    );
    assert_eq!(
        pyrowave_drop_reason(true, false, 0, pc_all, both, 0, 99),
        PyroWaveDrop::MissingSwapchain
    );
    assert_eq!(
        pyrowave_drop_reason(true, false, 0, pc_all, both, VIDEO_FORMAT_PYROWAVE, 99),
        PyroWaveDrop::HdrFailure
    );
    assert_eq!(
        pyrowave_drop_reason(true, false, 0, pc_sdr, hevc_hdr, 0, 0),
        PyroWaveDrop::NoHdrOnPc
    );
    assert_eq!(
        pyrowave_drop_reason(true, true, 0, pc_all, hevc_hdr, 0, 0),
        PyroWaveDrop::FullChroma
    );

    let mut visited = Vec::new();
    let picked = first_pyrowave_format(
        VIDEO_FORMAT_PYROWAVE | VIDEO_FORMAT_PYROWAVE_444 | VIDEO_FORMAT_PYROWAVE_MAIN10,
        |fmt| {
            visited.push(fmt);
            fmt == VIDEO_FORMAT_PYROWAVE_444
        },
    );
    assert_eq!(picked, VIDEO_FORMAT_PYROWAVE_444);
    assert_eq!(
        visited,
        vec![VIDEO_FORMAT_PYROWAVE_MAIN10, VIDEO_FORMAT_PYROWAVE_444]
    );

    // host_ten_bit_issue vs host_ten_bit_drop when host_modes < 0
    assert_eq!(host_ten_bit_issue(0b0011, -1), None);
    assert_eq!(
        host_ten_bit_issue(0b0011, 0b0001),
        Some(ISSUE_HOST_NO_TEN_BIT)
    );
    assert_eq!(
        host_ten_bit_issue(0b0111, 0b1101),
        Some(ISSUE_NO_TEN_BIT_IN_COMMON)
    );
    assert_eq!(
        host_ten_bit_drop(true, None, VIDEO_FORMAT_H265, -1).as_deref(),
        Some(ISSUE_HOST_NO_TEN_BIT)
    );
    assert_eq!(
        host_ten_bit_drop(true, None, VIDEO_FORMAT_H265, 0b1001).as_deref(),
        Some(ISSUE_NO_TEN_BIT_IN_COMMON)
    );
    assert_eq!(
        ten_bit_output_issue(false, false),
        Some(ISSUE_NO_HDR10_DISPLAY)
    );
    assert_eq!(
        ten_bit_output_issue(true, false),
        Some(ISSUE_NO_TEN_BIT_OUTPUT)
    );
    assert_eq!(ten_bit_output_issue(true, true), None);

    let caps = capabilities_for(
        VIDEO_FORMAT_H265_MAIN10,
        false,
        true,
        4,
        true,
        2,
        4,
        false,
        true,
        true,
        true,
        false,
        false,
    );
    assert_eq!(
        caps,
        capability_slices_per_frame(4)
            | CAPABILITY_REFERENCE_FRAME_INVALIDATION_AVC
            | CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC
            | CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1
            | CAPABILITY_DIRECT_SUBMIT
    );
    let (stripped, hevc_rfi, av1_rfi) = apply_dolby_vision_rfi(caps, true, true, u32::from(DV_81));
    assert!(hevc_rfi);
    assert!(!av1_rfi);
    assert_eq!(stripped & CAPABILITY_REFERENCE_FRAME_INVALIDATION_HEVC, 0);
    assert_ne!(stripped & CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1, 0);
    let (stripped_av1, hevc_rfi2, av1_rfi2) =
        apply_dolby_vision_rfi(caps, true, true, u32::from(DV_101));
    assert!(!hevc_rfi2);
    assert!(av1_rfi2);
    assert_eq!(
        stripped_av1 & CAPABILITY_REFERENCE_FRAME_INVALIDATION_AV1,
        0
    );
    let (_, h_no, a_no) = apply_dolby_vision_rfi(caps, true, true, u32::from(PQ | HLG));
    assert!(!h_no && !a_no);

    let gap_mixed = record_frame_gap(20, 16);
    assert_eq!(gap_mixed.decoder_drops, 16);
    assert_eq!(gap_mixed.frames_lost, 4);
    assert_eq!(gap_mixed.frame_loss_events, 1);
    let gap_overflow = record_frame_gap(3, 5);
    assert_eq!(gap_overflow.decoder_drops, 3);
    assert_eq!(gap_overflow.frames_lost, 0);

    assert_eq!(
        thermal_state(THERMAL_HOT, false, true),
        ThermalOverlayState::HotEasing
    );
    assert_eq!(
        thermal_state(THERMAL_HOT, true, false),
        ThermalOverlayState::Hot
    );
    assert_eq!(
        thermal_state(THERMAL_WARM, true, false),
        ThermalOverlayState::WarmHolding
    );
    assert_eq!(
        thermal_state(THERMAL_WARM, false, true),
        ThermalOverlayState::Warm
    );
    assert_eq!(
        thermal_state(THERMAL_NORMAL, true, true),
        ThermalOverlayState::Normal
    );
}
