//! Unit tests for the desktop 4:4:4 and 10-bit decoder probe and its fixtures.

use super::*;
use nightjar_core::color::{SDR10_FORMATS, YUV444_FORMATS};

#[test]
fn software_decoding_offers_every_4_4_4_and_10_bit_format_when_planes_draw() {
    let (decodable, reason) = probe(media::Hardware::None, YUV444_FORMATS | SDR10_FORMATS, |_| Ok(()), &mut ProbeBudget::default());
    assert_eq!(decodable, YUV444_FORMATS | SDR10_FORMATS, "{reason}");
    assert_eq!(reason, "");
}

#[test]
fn without_a_plane_pass_nothing_planar_is_offered_and_the_reason_says_why() {
    // Software Main10 pictures are planar too, so 10-bit goes with 4:4:4.
    let (decodable, reason) = probe(media::Hardware::None, YUV444_FORMATS | SDR10_FORMATS, |ten| {
        Err(format!("no R{} planes", if ten { 16 } else { 8 }))
    }, &mut ProbeBudget::default());
    assert_eq!(decodable, 0);
    assert!(
        reason.contains("HEVC 4:4:4: no R8 planes")
            && reason.contains("AV1 Main10: no R16 planes"),
        "{reason}"
    );
    // 8-bit planes alone: 4:4:4 is offered, 10-bit is not.
    let (decodable, _) = probe(media::Hardware::None, YUV444_FORMATS | SDR10_FORMATS, |ten| {
        if ten {
            Err("no R16 planes".into())
        } else {
            Ok(())
        }
    }, &mut ProbeBudget::default());
    assert_eq!(decodable, YUV444_FORMATS);
}

/// 10-bit launches decoded the 4:4:4 fixtures too (about 12 ms) with 4:4:4 off.
#[test]
fn only_the_wanted_formats_are_decoded() {
    let (decodable, reason) = probe(media::Hardware::None, SDR10_FORMATS, |_| Err("refused".into()), &mut ProbeBudget::default());
    assert_eq!(decodable, 0);
    assert!(reason.contains("HEVC Main10") && !reason.contains("4:4:4"), "{reason}");
    assert_eq!(probe(media::Hardware::None, 0, |_| Err("refused".into()), &mut ProbeBudget::default()), (0, String::new()));
}

/// The 128x128 fixtures were under NVDEC's smallest HEVC picture (144x144), so the probe
/// refused HEVC on every NVIDIA GPU; the 64x64 4:4:4 ones never offered NVIDIA 4:4:4. The
/// 4:4:4 fixtures are standard lossy pictures (not lossless), so they stay compact too.
#[test]
fn the_fixtures_are_above_every_backends_smallest_picture_and_stay_small() {
    for &(format, codec, name, data) in BASE_FIXTURES.iter().chain(&FIXTURES) {
        let (info, _) = decode(media::Hardware::None, media::Pictures::default(), codec, data, &mut ProbeBudget::default())
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(info.width >= 256 && info.height >= 256, "{name}: {}x{}", info.width, info.height);
        assert_eq!(
            (info.width as u32, info.height as u32),
            (FIXTURE_WIDTH, FIXTURE_HEIGHT),
            "{name}: the probe opens the decoder for this size"
        );
        let limit = if format & VIDEO_FORMAT_MASK_YUV444 == 0 { 8 } else { 16 };
        assert!(data.len() <= limit * 1024, "{name}: {} bytes", data.len());
    }
}

/// Nightjar #169: lossless 4:4:4 probe fixtures carried HEVC Level 8.5 (`level_idc=255`) with
/// `transquant_bypass_enabled_flag=1` (and H.264 `qpprime_y_zero_transform_bypass_flag=1` with
/// 16 reference frames), which hardware decoders reject even on GPUs that decode normal 4:4:4
/// streams, forcing Nightjar onto packed full chroma. Parse the Annex B headers directly so a
/// regenerated fixture can never silently reintroduce lossless mode or an out-of-spec level.
#[test]
fn the_4_4_4_fixtures_are_lossy_and_within_hardware_decoder_limits() {
    fn rbsp(nal: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(nal.len());
        let mut zeros = 0u8;
        for &b in nal {
            if zeros == 2 && b == 3 {
                zeros = 0;
                continue;
            }
            out.push(b);
            zeros = if b == 0 { zeros.saturating_add(1) } else { 0 };
        }
        out
    }
    fn annex_b_nals(data: &[u8]) -> Vec<&[u8]> {
        let mut starts = Vec::new();
        let mut i = 0;
        while i + 3 <= data.len() {
            if i + 4 <= data.len() && data[i..i + 4] == [0, 0, 0, 1] {
                starts.push((i, i + 4));
                i += 4;
            } else if data[i..i + 3] == [0, 0, 1] {
                starts.push((i, i + 3));
                i += 3;
            } else {
                i += 1;
            }
        }
        starts.iter().enumerate().map(|(idx, &(_, payload))| {
            let end = starts.get(idx + 1).map_or(data.len(), |&(next, _)| next);
            &data[payload..end]
        }).collect()
    }
    struct Bits<'a> { data: &'a [u8], bit: usize }
    impl<'a> Bits<'a> {
        fn u(&mut self, n: usize) -> u32 {
            let mut v = 0u32;
            for _ in 0..n {
                v = (v << 1) | u32::from((self.data[self.bit / 8] >> (7 - (self.bit % 8))) & 1);
                self.bit += 1;
            }
            v
        }
        fn ue(&mut self) -> u32 {
            let mut z = 0usize;
            while self.u(1) == 0 { z += 1; }
            if z == 0 { 0 } else { ((1u32 << z) - 1) + self.u(z) }
        }
        fn se(&mut self) -> i32 {
            let k = self.ue();
            if k & 1 == 1 { k.div_ceil(2) as i32 } else { -((k / 2) as i32) }
        }
    }

    // HEVC 4:4:4 8-bit and 10-bit: RExt Main 4:4:4 (non-Intra), level <= 6.2 (186), transquant_bypass=0.
    for (name, bytes, expect_max_8bit) in [
        ("hevc-444-8.bin", include_bytes!("../../../tests/fixtures/hevc-444-8.bin").as_slice(), 1u32),
        ("hevc-444-10.bin", include_bytes!("../../../tests/fixtures/hevc-444-10.bin").as_slice(), 0u32),
    ] {
        let nals = annex_b_nals(bytes);
        let sps = nals.iter().find(|n| (n[0] >> 1) & 0x3f == 33).unwrap_or_else(|| panic!("{name}: no SPS"));
        let sps_rbsp = rbsp(&sps[2..]);
        let mut b = Bits { data: &sps_rbsp, bit: 0 };
        let _vps_id = b.u(4);
        let _max_sub_layers_minus1 = b.u(3);
        let _temporal_nesting = b.u(1);
        let _profile_space = b.u(2);
        let _tier_flag = b.u(1);
        let profile_idc = b.u(5);
        let _compat = b.u(32);
        let _flags4 = b.u(4);
        let max_12bit = b.u(1);
        let max_10bit = b.u(1);
        let max_8bit = b.u(1);
        let max_422 = b.u(1);
        let max_420 = b.u(1);
        let max_mono = b.u(1);
        let intra = b.u(1);
        let _rest_constraints_hi = b.u(20);
        let _rest_constraints_lo = b.u(17);
        let level_idc = b.u(8);
        assert_eq!(profile_idc, 4, "{name}: must be HEVC RExt");
        assert_eq!((max_12bit, max_10bit, max_8bit, max_422, max_420, max_mono, intra),
            (1, 1, expect_max_8bit, 0, 0, 0, 0), "{name}: must be Main 4:4:4 (non-Intra)");
        assert!(level_idc > 0 && level_idc <= 186, "{name}: level_idc {level_idc} exceeds Level 6.2 (255 = Level 8.5)");

        let pps = nals.iter().find(|n| (n[0] >> 1) & 0x3f == 34).unwrap_or_else(|| panic!("{name}: no PPS"));
        let pps_rbsp = rbsp(&pps[2..]);
        let mut b = Bits { data: &pps_rbsp, bit: 0 };
        let _pps_id = b.ue();
        let _sps_id = b.ue();
        let _dep_slice = b.u(1);
        let _output_flag = b.u(1);
        let _extra_bits = b.u(3);
        let _sign_hide = b.u(1);
        let _cabac_init = b.u(1);
        let _ref_l0 = b.ue();
        let _ref_l1 = b.ue();
        let _init_qp = b.se();
        let _constrained_intra = b.u(1);
        let _transform_skip = b.u(1);
        if b.u(1) == 1 { let _diff_depth = b.ue(); }
        let _cb_qp = b.se();
        let _cr_qp = b.se();
        let _slice_qp = b.u(1);
        let _wpred = b.u(1);
        let _wbipred = b.u(1);
        let transquant_bypass = b.u(1);
        assert_eq!(transquant_bypass, 0, "{name}: transquant_bypass_enabled_flag must be 0 (not lossless)");
    }

    // H.264 4:4:4: High 4:4:4 Predictive (not Intra), level <= 5.1, no transform bypass, <= 4 refs.
    let h264 = include_bytes!("../../../tests/fixtures/h264-444-8.bin");
    let nals = annex_b_nals(h264);
    let sps = nals.iter().find(|n| n[0] & 0x1f == 7).expect("h264-444-8.bin: no SPS");
    let sps_rbsp = rbsp(&sps[1..]);
    let mut b = Bits { data: &sps_rbsp, bit: 0 };
    let profile_idc = b.u(8);
    let constraint_flags = b.u(8);
    let level_idc = b.u(8);
    let _sps_id = b.ue();
    let chroma_format = b.ue();
    let _sep_plane = b.u(1);
    let _bd_luma = b.ue();
    let _bd_chroma = b.ue();
    let qpprime_bypass = b.u(1);
    let scaling_matrix = b.u(1);
    let _log2_frame_num = b.ue();
    let poc_type = b.ue();
    if poc_type == 0 { let _poc_lsb = b.ue(); }
    let max_refs = b.ue();
    assert_eq!(profile_idc, 244, "h264-444-8.bin: must be High 4:4:4 Predictive");
    assert_eq!(constraint_flags & 0x10, 0, "h264-444-8.bin: must not be Intra-only");
    assert!(level_idc > 0 && level_idc <= 51, "h264-444-8.bin: level_idc {level_idc}");
    assert_eq!(chroma_format, 3, "h264-444-8.bin: must be 4:4:4");
    assert_eq!(qpprime_bypass, 0, "h264-444-8.bin: qpprime_y_zero_transform_bypass_flag must be 0");
    assert_eq!(scaling_matrix, 0);
    assert!((1..=4).contains(&max_refs), "h264-444-8.bin: max_num_ref_frames {max_refs}");

    // AV1 4:4:4: Sequence Header OBU carries High profile (seq_profile == 1).
    let av1 = include_bytes!("../../../tests/fixtures/av1-444-8.bin");
    let mut pos = 0usize;
    let mut seq_profile = None;
    while pos < av1.len() {
        let header = av1[pos];
        pos += 1;
        let obu_type = (header >> 3) & 0x0f;
        let has_ext = (header >> 2) & 1 != 0;
        let has_size = (header >> 1) & 1 != 0;
        if has_ext { pos += 1; }
        let size = if has_size {
            let mut val = 0usize;
            for shift in (0..56).step_by(7) {
                let byte = av1[pos];
                pos += 1;
                val |= usize::from(byte & 0x7f) << shift;
                if byte & 0x80 == 0 { break; }
            }
            val
        } else {
            av1.len() - pos
        };
        if obu_type == 1 {
            seq_profile = Some((av1[pos] >> 5) & 0x07);
            break;
        }
        pos += size;
    }
    assert_eq!(seq_profile, Some(1), "av1-444-8.bin: must be AV1 High profile (4:4:4)");
}

/// The probe stopped at the first backend that opened, so a backend that opens but cannot
/// decode the format (Vulkan video without the profile) hid the ones after it.
#[test]
fn a_backend_that_opens_but_fails_has_the_ones_after_it_tried() {
    use media::Hardware::{Cuda, V4l2m2m, Vaapi, Vulkan};
    let gpu = std::ptr::dangling_mut::<std::ffi::c_void>();
    let mut asked = Vec::new();
    let decoded = first_that_decodes(vec![Vulkan(gpu), Vaapi, V4l2m2m, Cuda], |order| {
        asked.push(order.clone());
        match order[0] {
            Vulkan(_) => Ok((order[0], Err("no picture".to_owned()))),
            // VAAPI does not open, so V4L2 M2M is the first that does.
            Vaapi => Ok((V4l2m2m, Ok(7))),
            _ => unreachable!(),
        }
    });
    assert_eq!(decoded, Ok((7, V4l2m2m)));
    assert_eq!(asked, [vec![Vulkan(gpu), Vaapi, V4l2m2m, Cuda], vec![Vaapi, V4l2m2m, Cuda]]);
    // Every backend failing, or none opening, names each reason.
    let failed = first_that_decodes(vec![Vaapi, Cuda], |order| {
        Ok::<_, String>((order[0], Err::<(), _>(format!("{} failed", order[0].label()))))
    });
    let reason = failed.unwrap_err();
    assert!(reason.contains("VAAPI failed") && reason.contains("NVDEC failed"), "{reason}");
    let unopened = first_that_decodes(vec![Vaapi], |_| Err::<(_, Result<(), _>), _>("nothing opens".to_owned()));
    assert_eq!(unopened, Err("nothing opens".to_owned()));
}

/// Nightjar issue 77: on Qualcomm Iris the HEVC and Main10 probes sometimes made no picture of
/// the fixture at all, while the same decoder decoded the stream once it was reopened. A V4L2
/// M2M "no picture" is retried once, in a bounded time; a device that does not open, another
/// backend's failure, or a picture in software is not (a second open cannot fix those).
#[test]
fn only_a_v4l2_decoder_that_made_no_picture_is_probed_again() {
    use media::Hardware::{Cuda, None as Software, Vaapi, V4l2m2m};
    let order = [Vaapi, V4l2m2m, Cuda];
    let no_picture = format!("{}: the decoder made no picture of the fixture", V4l2m2m.label());
    assert!(retry_worthwhile(&order, &no_picture));
    // The device refused the open, or its picture came out in software: not a stale decoder.
    assert!(!retry_worthwhile(&order, "V4L2 M2M: Opening the decoder: Device or resource busy"));
    assert!(!retry_worthwhile(&order, "V4L2 M2M: the picture came out in memory, not the hardware decoder's"));
    // Another backend's failure is not the one a second V4L2 open can fix.
    assert!(!retry_worthwhile(&order, "VAAPI: the decoder made no picture of the fixture"));
    assert!(!retry_worthwhile(
        &order,
        "VAAPI: the decoder made no picture of the fixture; V4L2 M2M: Opening the decoder: Device or resource busy"
    ));
    assert!(retry_worthwhile(&order, &format!("{no_picture}; NVDEC: the decoder failed on the fixture")));
    assert!(!retry_worthwhile(&[Vaapi, Cuda], &no_picture), "no V4L2 decoder in the order");
    assert!(!retry_worthwhile(&[Software], &no_picture));
    // The retry is bounded, and the wait is longer than the quarter second that proved too short.
    assert_eq!(V4L2_PROBE_RETRIES, 1);
    assert!(ASYNC_PROBE_WAIT >= std::time::Duration::from_millis(1_000));
}

/// Nightjar issue 77: an Iris whose V4L2 M2M decoder never answers must not add more than a
/// couple of seconds to a start, though one start checks up to five asynchronous fixtures. The
/// two checks share a budget; a retry's settle comes out of it too, and once it is spent each
/// attempt still gets the old quarter second, without a retry: at most the budget more than
/// before it, about 3 s in all.
#[test]
fn the_v4l2_probe_time_of_a_start_is_bounded_across_formats() {
    use std::time::Duration;
    let mut budget = ProbeBudget::default();
    assert!(V4L2_PROBE_BUDGET <= Duration::from_secs(2), "a start waits about two seconds at most");
    assert_eq!(budget.left(), V4L2_PROBE_BUDGET);
    assert_eq!(budget.wait(), ASYNC_PROBE_WAIT, "the first attempt gets the long wait");
    budget.spent(budget.wait());
    assert!(budget.retry(), "after a timeout the one retry still fits");
    let retry_wait = budget.wait();
    assert_eq!(retry_wait, budget.left(), "the retry waits what is left");
    assert_eq!(budget.left(), V4L2_PROBE_BUDGET - ASYNC_PROBE_WAIT - V4L2_PROBE_SETTLE);
    budget.spent(retry_wait);
    assert_eq!(budget.left(), Duration::ZERO, "the two attempts spend the start's budget");
    assert!(!budget.retry(), "a second retry has nothing left");
    assert_eq!(budget.wait(), ASYNC_PROBE_FLOOR, "later formats still get the old quarter second");
    // The worst start: the first fixture's two attempts spend the budget, four more wait the floor.
    assert!(V4L2_PROBE_BUDGET + ASYNC_PROBE_FLOOR * 4 <= Duration::from_secs(3));
    // A slow picture is charged too, so slow successes cannot add up past the budget.
    let mut slow = ProbeBudget::default();
    slow.spent(Duration::from_millis(1_400));
    assert_eq!(slow.wait(), Duration::from_millis(600));
}

/// Nightjar #77: an earlier hanging V4L2 format (e.g. HEVC 8-bit) must not exhaust the entire
/// shared start budget and deprive HEVC Main10 of its own wait and retry; fast successes return
/// their unspent slice to the parent budget.
#[test]
fn per_format_v4l2_budget_isolates_later_formats_and_reclaims_unspent_time() {
    use std::time::Duration;
    let mut shared = ProbeBudget::default();
    // Format 1 of 2 hangs completely across both attempts.
    let mut first = shared.for_format(2);
    let w1 = first.wait();
    assert!(w1 >= ASYNC_PROBE_FLOOR && w1 < V4L2_PROBE_BUDGET);
    first.spent(w1);
    assert!(first.retry(), "format 1 gets its own retry inside its isolated share");
    let w1_retry = first.wait();
    first.spent(w1_retry);
    shared.reclaim(first);
    // Format 2 of 2 STILL has enough budget for a full attempt AND its own retry.
    let mut second = shared.for_format(1);
    assert!(second.wait() >= Duration::from_millis(500), "wait {:?}", second.wait());
    second.spent(second.wait());
    assert!(second.retry(), "format 2 keeps its own retry even after format 1 hung");
    shared.reclaim(second);

    // Fast success on format 1 returns unspent budget to format 2.
    let mut fast = ProbeBudget::default();
    let mut f1 = fast.for_format(2);
    f1.spent(Duration::from_millis(80));
    fast.reclaim(f1);
    assert_eq!(fast.left(), V4L2_PROBE_BUDGET - Duration::from_millis(80));
}

/// Nightjar #77: async V4L2 M2M decoders (Qualcomm Iris Main10) receive up to `ASYNC_PROBE_FRAMES`
/// duplicate IDR frames with distinct PTS before `drain()`, stopping early if a picture arrives
/// immediately or if a follow-up send fails.
#[test]
fn async_probe_feeds_multiple_frames_before_draining_and_short_circuits_on_early_frame() {
    // 1. No early frame -> sends 0, 1, 2 then drains once.
    let mut state = (Vec::new(), 0usize);
    let got = feed_async_probe::<_, u32>(
        &mut state,
        ASYNC_PROBE_FRAMES,
        |s, pts| {
            s.0.push(pts);
            0
        },
        |s| {
            s.1 += 1;
            0
        },
        |_| None,
    );
    assert_eq!(got, Ok(None));
    assert_eq!(state.0, vec![0, 1, 2]);
    assert_eq!(state.1, 1);

    // 2. Frame arrives after first send -> returns immediately without draining or extra sends.
    state = (Vec::new(), 0);
    let got = feed_async_probe(
        &mut state,
        ASYNC_PROBE_FRAMES,
        |s, pts| {
            s.0.push(pts);
            0
        },
        |s| {
            s.1 += 1;
            0
        },
        |_| Some(Ok(42u32)),
    );
    assert_eq!(got, Ok(Some(42)));
    assert_eq!(state.0, vec![0]);
    assert_eq!(state.1, 0);

    // 3. Rejection on first send is fatal; rejection on second send still proceeds to drain.
    let mut unit = ();
    let err = feed_async_probe::<_, u32>(&mut unit, ASYNC_PROBE_FRAMES, |_, _| -22, |_| 0, |_| None);
    assert_eq!(err, Err((false, -22)));
    state = (Vec::new(), 0);
    let ok = feed_async_probe::<_, u32>(
        &mut state,
        ASYNC_PROBE_FRAMES,
        |s, pts| {
            s.0.push(pts);
            if pts == 0 { 0 } else { -11 }
        },
        |s| {
            s.1 += 1;
            0
        },
        |_| None,
    );
    assert_eq!(ok, Ok(None));
    assert_eq!(state.0, vec![0, 1]);
    assert_eq!(state.1, 1);
}

/// The probe opens each fixture for its own size, as a stream's decoder is opened for the
/// stream's (Nightjar issue 77: a V4L2 M2M decoder opened without one reinitialises its
/// buffers on the driver's first format event and makes no picture).
#[test]
fn every_fixture_is_opened_for_its_size() {
    let plain = fixture_pictures(VIDEO_FORMAT_H265, media::VaapiNode::Default);
    assert_eq!((plain.width, plain.height), (FIXTURE_WIDTH, FIXTURE_HEIGHT));
    assert!(!plain.ten_bit && !plain.chroma444 && !plain.packed);
    let rext = fixture_pictures(VIDEO_FORMAT_H265_REXT10_444, media::VaapiNode::Mapped(128));
    assert!(rext.ten_bit && rext.chroma444);
    assert_eq!((rext.width, rext.height), (FIXTURE_WIDTH, FIXTURE_HEIGHT));
    assert_eq!(rext.vaapi_node, media::VaapiNode::Mapped(128));
}

#[test]
fn pictures_that_lost_their_chroma_or_depth_are_not_drawable() {
    let info = |kind, ten_bit| media::FrameInfo {
        width: 64,
        height: 64,
        kind,
        ten_bit,
        pts: 0,
        data: [std::ptr::null_mut(); 3],
        linesize: [0; 3],
        color_space: 1,
        color_range: 1,
        color_trc: 1,
        centred_chroma: false,
    };
    let planes = |_| Ok(());
    // Kind 4 is a picture nothing draws.
    assert!(drawable(&info(4, 0), true, &planes)
        .unwrap_err()
        .contains("not 4:4:4"));
    assert!(drawable(&info(media::FRAME_D3D11, 0), true, &planes).is_err());
    // VideoToolbox NV12/P010 is 4:2:0, never 4:4:4, and draws 10-bit directly through Metal
    // without requiring the plane shader (formerly broken by sharing kind 13 with D3D11 4:4:4).
    assert!(drawable(&info(media::FRAME_VIDEOTOOLBOX, 0), true, &planes).is_err());
    assert!(drawable(&info(media::FRAME_VIDEOTOOLBOX, 1), true, &planes).is_err());
    assert!(drawable(&info(media::FRAME_VIDEOTOOLBOX, 1), false, &|_| Err("no planes".into())).is_ok());
    // A D3D11VA 4:4:4 surface (AYUV, Y410, Y416) is drawn from its copy by the plane pass.
    for ten_bit in [0, 1] {
        assert!(drawable(&info(media::FRAME_D3D11_444, ten_bit), true, &planes).is_ok());
        assert!(drawable(&info(media::FRAME_D3D11_444, ten_bit), true, &|_| Err("no pass".into())).is_err());
    }
    // Linux: NVDEC's 10-bit 4:4:4 in memory, its samples in the high bits, goes through the planes.
    #[cfg(unix)]
    assert!(drawable(&info(media::FRAME_YUV444P16, 1), true, &planes).is_ok());
    assert!(drawable(&info(media::FRAME_YUV420P10, 1), true, &planes).is_err());
    assert!(drawable(&info(media::FRAME_YUV444P, 0), true, &planes).is_ok());
    assert!(drawable(&info(media::FRAME_YUV444P10, 1), true, &planes).is_ok());
    // Linux: ...unless it was mapped into Vulkan as one image the planes copy whole.
    #[cfg(unix)]
    for ten_bit in [0, 1] {
        assert!(drawable(&info(media::FRAME_VULKAN_INTERLEAVED444, ten_bit), true, &planes).is_ok());
    }
    // 10-bit: a hardware P010 surface draws through SDL, a software one through the planes.
    assert!(drawable(&info(media::FRAME_D3D11, 1), false, &planes).is_ok());
    assert!(drawable(&info(media::FRAME_P010, 1), false, &planes).is_ok());
    assert!(drawable(&info(media::FRAME_YUV420P10, 1), false, &planes).is_ok());
    assert!(drawable(&info(media::FRAME_D3D11, 0), false, &planes)
        .unwrap_err()
        .contains("not 10-bit"));
    assert!(drawable(&info(media::FRAME_NV12, 0), false, &planes).is_err());
    assert!(drawable(&info(media::FRAME_YUV420P10, 1), false, &|_| Err(
        "no planes".into()
    ))
    .is_err());
    // 10-bit 4:4:4 decoded into 8-bit pictures lost its depth.
    assert!(keeps_depth(&info(media::FRAME_D3D11_444, 0), true).unwrap_err().contains("8-bit"));
    assert!(keeps_depth(&info(media::FRAME_YUV444P, 0), true).is_err());
    assert!(keeps_depth(&info(media::FRAME_D3D11_444, 1), true).is_ok());
    assert!(keeps_depth(&info(media::FRAME_D3D11_444, 0), false).is_ok());
}

/// The fixtures through D3D11VA on this PC's GPU: plain HEVC and Main10 decode wherever the GPU
/// has HEVC, and HEVC 4:4:4 decodes exactly where the GPU's DXVA lists the Range Extension
/// profile. Rubylight issue 84: FFmpeg 8 has no DXVA HEVC 4:4:4, so an RTX 5060 that lists the
/// profile refused every 4:4:4 stream until the FFmpeg patch (`ffmpeg-patches/windows`).
#[cfg(windows)]
#[test]
fn d3d11va_decodes_hevc_4_4_4_where_the_gpu_lists_it() {
    use windows::core::Interface;
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::*;
    let mut device = None;
    // SAFETY: A device of the test's own, kept alive past every use of its pointer.
    let made = unsafe {
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), D3D11_CREATE_DEVICE_VIDEO_SUPPORT, None,
            D3D11_SDK_VERSION, Some(&mut device), None, None)
    };
    let Some(device) = device.filter(|_| made.is_ok()) else {
        eprintln!("no hardware Direct3D 11 device here");
        return;
    };
    let hardware = media::Hardware::D3d11(device.as_raw());
    let base = probe_base(hardware, VIDEO_FORMAT_H265 | VIDEO_FORMAT_H265_MAIN10, &mut ProbeBudget::default());
    assert_eq!(base.decodable, VIDEO_FORMAT_H265 | VIDEO_FORMAT_H265_MAIN10, "{}", base.reason);
    for (format, ten_bit) in [(VIDEO_FORMAT_H265_REXT8_444, false), (VIDEO_FORMAT_H265_REXT10_444, true)] {
        let lists = matches!(
            crate::gpu_adapters::others_decoding_hevc444(device.as_raw(), ten_bit),
            Some((_, crate::gpu_adapters::Hevc444Gpus::Renderer))
        );
        let (decodable, reason) = probe(hardware, format, |_| Ok(()), &mut ProbeBudget::default());
        eprintln!("HEVC 4:4:4 (10-bit {ten_bit}): DXVA lists it {lists}, decodes {}: {reason}", decodable != 0);
        assert_eq!(decodable != 0, lists, "{reason}");
    }
}

/// Both settings on ask for HEVC RExt 4:4:4 10-bit: the software decoder makes 10-bit 4:4:4
/// planar pictures of the fixture, offered where the R16 planes draw.
#[test]
fn ten_bit_4_4_4_is_offered_where_its_pictures_draw() {
    use nightjar_core::color::YUV444_10_FORMATS;
    let (decodable, reason) = probe(media::Hardware::None, YUV444_10_FORMATS, |_| Ok(()), &mut ProbeBudget::default());
    assert_eq!((decodable, reason.as_str()), (VIDEO_FORMAT_H265_REXT10_444, ""));
    let (decodable, reason) = probe(media::Hardware::None, YUV444_10_FORMATS, |ten| {
        if ten { Err("no R16 planes".into()) } else { Ok(()) }
    }, &mut ProbeBudget::default());
    assert_eq!(decodable, 0);
    assert!(reason.contains("HEVC 4:4:4 10-bit: no R16 planes"), "{reason}");
}
