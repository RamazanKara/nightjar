//! JNI bridge for `NightjarPolicyJni` video renderer selection (`FullRangePolicy`,
//! `VideoRendererChoice`) and `MediaCodecDecoderRenderer` pure codec/capability/stats math.
//!
//! `Sdr10Policy`, `HdrPolicy`, and `HdrFormatPolicy` live in [`hdr`].

pub mod hdr;

use jni::JNIEnv;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{JNI_FALSE, jboolean, jbyte, jint, jintArray, jlong, jstring};
use nightjar_policy::video_codec::{
    self,
    renderer::{self, ColorRange},
};

use crate::policy::{int_array, jbool};
use crate::policy_recovery::new_jstring;
use crate::string;

fn opt_jstring(env: &JNIEnv, text: Option<String>) -> jstring {
    match text {
        Some(s) => new_jstring(env, &s),
        None => std::ptr::null_mut(),
    }
}

fn opt_bytes(env: &JNIEnv, arr: &JByteArray) -> Option<Vec<u8>> {
    if arr.is_null() {
        None
    } else {
        env.convert_byte_array(arr).ok()
    }
}

// --- FullRangePolicy (#153) & VideoRendererChoice ---

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_fullRangeOutputFallbackReason(
    env: JNIEnv,
    _: JClass,
    full_range: jboolean,
    hdr: jboolean,
    dolby_vision: jboolean,
    packed444: jboolean,
    pyro_wave: jboolean,
    color_range_code: jint,
    raw_color_range: jint,
    has_color_aspects: jboolean,
) -> jstring {
    let reason = renderer::output_fallback_reason(
        full_range != JNI_FALSE,
        hdr != JNI_FALSE,
        dolby_vision != JNI_FALSE,
        packed444 != JNI_FALSE,
        pyro_wave != JNI_FALSE,
        ColorRange::from_jni(color_range_code, raw_color_range),
        has_color_aspects != JNI_FALSE,
    );
    opt_jstring(&env, reason)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_fullRangeCanUseVulkanSdk(
    _: JNIEnv,
    _: JClass,
    sdk_int: jint,
) -> jboolean {
    jbool(renderer::can_use_vulkan_sdk(sdk_int))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoRendererNeedsVulkanProbe(
    _: JNIEnv,
    _: JClass,
    vulkan_preferred: jboolean,
    pyro_wave: jboolean,
    sdk_int: jint,
    packed444: jboolean,
    frame_warp: jboolean,
) -> jboolean {
    jbool(renderer::needs_vulkan_probe(
        vulkan_preferred != JNI_FALSE,
        pyro_wave != JNI_FALSE,
        sdk_int,
        packed444 != JNI_FALSE,
        frame_warp != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoRendererChoose(
    _: JNIEnv,
    _: JClass,
    vulkan_preferred: jboolean,
    pyro_wave: jboolean,
    sdk_int: jint,
    vulkan_supported: jboolean,
    packed444: jboolean,
    upscaling_on: jboolean,
    frame_warp: jboolean,
    hdr_or_ten_bit: jboolean,
) -> jint {
    renderer::choose_resolved(
        vulkan_preferred != JNI_FALSE,
        pyro_wave != JNI_FALSE,
        sdk_int,
        vulkan_supported != JNI_FALSE,
        packed444 != JNI_FALSE,
        upscaling_on != JNI_FALSE,
        frame_warp != JNI_FALSE,
        hdr_or_ten_bit != JNI_FALSE,
    )
    .pack()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoRendererFrameWarpAsked(
    _: JNIEnv,
    _: JClass,
    frame_warp: jboolean,
    frame_warp_listed: jboolean,
) -> jboolean {
    jbool(renderer::frame_warp_asked(
        frame_warp != JNI_FALSE,
        frame_warp_listed != JNI_FALSE,
    ))
}

// --- MediaCodecDecoderRenderer pure codec, capability & stats math ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoMaxInputSize(
    _: JNIEnv,
    _: JClass,
    bitrate_kbps: jint,
    fps: jint,
    width: jint,
    height: jint,
) -> jint {
    video_codec::max_input_size(bitrate_kbps, fps, width, height)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoGrownInputSize(
    _: JNIEnv,
    _: JClass,
    required_bytes: jlong,
    capacity: jint,
    previous_request: jint,
) -> jint {
    video_codec::grown_input_size(required_bytes, capacity, previous_request)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoHdrChange(
    env: JNIEnv,
    _: JClass,
    previous_mode: jboolean,
    enabled: jboolean,
    previous_metadata: JByteArray,
    metadata: JByteArray,
    sdk_int: jint,
) -> jint {
    let prev = opt_bytes(&env, &previous_metadata);
    let curr = opt_bytes(&env, &metadata);
    video_codec::hdr_change(
        previous_mode != JNI_FALSE,
        enabled != JNI_FALSE,
        prev.as_deref(),
        curr.as_deref(),
        sdk_int,
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoRestartRecreates(
    mut env: JNIEnv,
    _: JClass,
    decoder_name: JString,
    tuning_keys_lines: JString,
) -> jboolean {
    let Some(name) = string(&mut env, &decoder_name) else {
        return JNI_FALSE;
    };
    let keys = string(&mut env, &tuning_keys_lines).unwrap_or_default();
    jbool(video_codec::restart_recreates(&name, &keys))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoDecoderCarriesHdrMetadata(
    _: JNIEnv,
    _: JClass,
    vulkan_presents: jboolean,
) -> jboolean {
    jbool(video_codec::decoder_carries_hdr_metadata(
        vulkan_presents != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoCarriesHdrStaticInfo(
    env: JNIEnv,
    _: JClass,
    carried: JByteArray,
    expected: JByteArray,
) -> jboolean {
    let Some(carried_bytes) = opt_bytes(&env, &carried) else {
        return JNI_FALSE;
    };
    let Some(expected_bytes) = opt_bytes(&env, &expected) else {
        return JNI_FALSE;
    };
    jbool(video_codec::carries_hdr_static_info(
        &carried_bytes,
        &expected_bytes,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoYuv444FallbackIsHdr(
    _: JNIEnv,
    _: JClass,
    format: jint,
    sdr10: jboolean,
) -> jboolean {
    jbool(video_codec::yuv444_fallback_is_hdr(
        format as u32,
        sdr10 != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoPyroWaveFullChromaDrop(
    _: JNIEnv,
    _: JClass,
    packed444: jboolean,
    yuv444: jboolean,
) -> jboolean {
    jbool(video_codec::pyrowave_full_chroma_drop(
        packed444 != JNI_FALSE,
        yuv444 != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoPyroWaveDropReason(
    _: JNIEnv,
    _: JClass,
    chosen: jboolean,
    full_chroma: jboolean,
    device_reason: jint,
    host_codecs: jint,
    formats: jint,
    choice: jint,
    hdr_failure: jint,
) -> jint {
    video_codec::pyrowave_drop_reason(
        chosen != JNI_FALSE,
        full_chroma != JNI_FALSE,
        device_reason,
        host_codecs as u32,
        formats as u32,
        choice as u32,
        hdr_failure,
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoPyroWaveCandidateFormats(
    mut env: JNIEnv,
    _: JClass,
    formats: jint,
) -> jintArray {
    let candidates: Vec<i32> = video_codec::pyrowave_candidate_formats(formats as u32)
        .into_iter()
        .map(|f| f as i32)
        .collect();
    int_array(&mut env, &candidates)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoHostTenBitIssue(
    env: JNIEnv,
    _: JClass,
    device_modes: jint,
    host_modes: jint,
) -> jstring {
    opt_jstring(
        &env,
        video_codec::host_ten_bit_issue(device_modes, host_modes).map(str::to_owned),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoHostTenBitDrop(
    mut env: JNIEnv,
    _: JClass,
    ten_bit_offered: jboolean,
    drop_at_launch: JString,
    negotiated_format: jint,
    host_modes: jint,
) -> jstring {
    let launch_drop = string(&mut env, &drop_at_launch);
    let drop = video_codec::host_ten_bit_drop(
        ten_bit_offered != JNI_FALSE,
        launch_drop.as_deref(),
        negotiated_format as u32,
        host_modes,
    );
    opt_jstring(&env, drop)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoTenBitOutputIssue(
    env: JNIEnv,
    _: JClass,
    display_hdr10: jboolean,
    gpu_ten_bit_output: jboolean,
) -> jstring {
    opt_jstring(
        &env,
        video_codec::ten_bit_output_issue(
            display_hdr10 != JNI_FALSE,
            gpu_ten_bit_output != JNI_FALSE,
        )
        .map(str::to_owned),
    )
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoCapabilitiesFor(
    _: JNIEnv,
    _: JClass,
    format: jint,
    sdr10: jboolean,
    has_runner: jboolean,
    runner_slices: jbyte,
    runner_direct_submit: jboolean,
    slices8: jbyte,
    slices10: jbyte,
    direct_submit8: jboolean,
    rfi_avc: jboolean,
    rfi_hevc: jboolean,
    rfi_av1: jboolean,
    rfi_hevc10: jboolean,
    rfi_av1_10: jboolean,
) -> jint {
    video_codec::capabilities_for(
        format as u32,
        sdr10 != JNI_FALSE,
        has_runner != JNI_FALSE,
        runner_slices as u8,
        runner_direct_submit != JNI_FALSE,
        slices8 as u8,
        slices10 as u8,
        direct_submit8 != JNI_FALSE,
        rfi_avc != JNI_FALSE,
        rfi_hevc != JNI_FALSE,
        rfi_av1 != JNI_FALSE,
        rfi_hevc10 != JNI_FALSE,
        rfi_av1_10 != JNI_FALSE,
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoApplyDolbyVisionRfi(
    _: JNIEnv,
    _: JClass,
    capabilities: jint,
    dolby_vision_corrupts: jboolean,
    dolby_vision_av1_corrupts: jboolean,
    hdr_format: jint,
) -> jlong {
    let (caps, hevc, av1) = video_codec::apply_dolby_vision_rfi(
        capabilities as u32,
        dolby_vision_corrupts != JNI_FALSE,
        dolby_vision_av1_corrupts != JNI_FALSE,
        hdr_format as u32,
    );
    i64::from(caps) | (i64::from(hevc) << 32) | (i64::from(av1) << 33)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoRecordFrameGap(
    _: JNIEnv,
    _: JClass,
    gap: jint,
    new_decoder_drops: jlong,
) -> jlong {
    let delta = video_codec::record_frame_gap(gap, new_decoder_drops);
    i64::from(delta.decoder_drops as u32) | (i64::from(delta.frames_lost as u32) << 32)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_videoThermalState(
    _: JNIEnv,
    _: JClass,
    level: jint,
    holds_bitrate: jboolean,
    eases_load: jboolean,
) -> jint {
    video_codec::thermal_state(level, holds_bitrate != JNI_FALSE, eases_load != JNI_FALSE) as jint
}
