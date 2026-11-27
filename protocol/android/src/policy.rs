//! JNI bridge for `com.limelight.nvstream.jni.NightjarPolicyJni` (ABR, ClientHint, PyroWave).
//!
//! Quality, Foundation ABR, and PyroWave PaceSearch live in `policy_quality.rs`.

use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jfloat, jint, jintArray, jlong, jstring};
use nightjar_policy::bitrate::{AdaptiveBitrateController, Codec, built_in_floor_kbps};
use nightjar_policy::client_hint::{self, Bitrate as ClientHintBitrate};
use nightjar_policy::pyrowave::{self, BitrateController as PyroWaveBitrateController};
use nightjar_policy::stats::Stats;

use crate::slab::MutexSlab;

struct AbrState {
    controller: AdaptiveBitrateController,
    thermal_level: u32,
    shift_ms: u64,
}

static ABR_SLAB: MutexSlab<AbrState> = MutexSlab::new();
static CLIENT_HINT_SLAB: MutexSlab<ClientHintBitrate> = MutexSlab::new();
static PYROWAVE_SLAB: MutexSlab<PyroWaveBitrateController> = MutexSlab::new();

pub(crate) fn jbool(v: bool) -> jboolean {
    if v { JNI_TRUE } else { JNI_FALSE }
}

pub(crate) fn int_array(env: &mut JNIEnv, values: &[i32]) -> jintArray {
    let Ok(arr) = env.new_int_array(values.len() as i32) else {
        return std::ptr::null_mut();
    };
    if env.set_int_array_region(&arr, 0, values).is_err() {
        return std::ptr::null_mut();
    }
    arr.into_raw()
}

fn codec_from_ordinal(ordinal: jint) -> Option<Codec> {
    match ordinal {
        0 => Some(Codec::H264),
        1 => Some(Codec::Hevc),
        2 => Some(Codec::Av1),
        _ => None,
    }
}

pub(crate) fn non_neg_u32(v: jint) -> u32 {
    u32::try_from(v).unwrap_or(0)
}

pub(crate) fn non_neg_u64(v: jlong) -> u64 {
    u64::try_from(v).unwrap_or(0)
}

pub(crate) fn shifted_u64(v: jlong, shift: u64) -> u64 {
    (i128::from(v) + i128::from(shift)).max(0) as u64
}

// --- AdaptiveBitrateController ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrBuiltInFloorKbps(
    _: JNIEnv,
    _: JClass,
    width: jint,
    height: jint,
    fps: jint,
    codec_ordinal: jint,
) -> jint {
    built_in_floor_kbps(
        non_neg_u32(width),
        non_neg_u32(height),
        non_neg_u32(fps),
        codec_from_ordinal(codec_ordinal),
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrCreate(
    _: JNIEnv,
    _: JClass,
    ceiling_kbps: jint,
    current_kbps: jint,
    fps: jint,
    now_ms: jlong,
    width: jint,
    height: jint,
    codec_ordinal: jint,
) -> jlong {
    let shift_ms = if now_ms < 0 { now_ms.unsigned_abs() } else { 0 };
    ABR_SLAB.insert(AbrState {
        controller: AdaptiveBitrateController::with_stream(
            non_neg_u32(ceiling_kbps),
            non_neg_u32(current_kbps),
            non_neg_u32(fps),
            shifted_u64(now_ms, shift_ms),
            non_neg_u32(width),
            non_neg_u32(height),
            codec_from_ordinal(codec_ordinal),
        ),
        thermal_level: 0,
        shift_ms,
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    ABR_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrSample(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    now_ms: jlong,
    recent_video: jboolean,
    poor_connection: jboolean,
    loss_percent: jfloat,
    decode_ms: jfloat,
    jitter_ms: jfloat,
    queue_ms: jfloat,
) -> jint {
    ABR_SLAB
        .with(handle, |s| {
            let stats = Stats {
                now_ms: shifted_u64(now_ms, s.shift_ms),
                recent_video: recent_video != JNI_FALSE,
                poor_connection: poor_connection != JNI_FALSE,
                loss_percent: Some(loss_percent),
                decode_ms: Some(decode_ms),
                jitter_ms: Some(jitter_ms),
                queue_ms: Some(queue_ms),
                thermal_level: s.thermal_level,
                ..Stats::default()
            };
            s.controller.sample(&stats).unwrap_or(0) as jint
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrLastReason(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jstring {
    let reason = ABR_SLAB
        .with(handle, |s| s.controller.last_reason().to_owned())
        .unwrap_or_default();
    match env.new_string(reason) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrSetThermalLevel(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    level: jint,
) {
    ABR_SLAB.with(handle, |s| s.thermal_level = non_neg_u32(level));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrSetFrameRate(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    fps: jint,
) {
    ABR_SLAB.with(handle, |s| s.controller.set_frame_rate(non_neg_u32(fps)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrSetStreamSize(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    width: jint,
    height: jint,
) {
    ABR_SLAB.with(handle, |s| {
        s.controller
            .set_stream_size(non_neg_u32(width), non_neg_u32(height))
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrHasRoomBeforeQualityReduction(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jboolean {
    jbool(
        ABR_SLAB
            .with(handle, |s| s.controller.has_room_before_quality_reduction())
            .unwrap_or(false),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrApplied(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    requested_kbps: jint,
    applied_kbps: jint,
    now_ms: jlong,
) {
    ABR_SLAB.with(handle, |s| {
        s.controller.applied(
            non_neg_u32(requested_kbps),
            non_neg_u32(applied_kbps),
            shifted_u64(now_ms, s.shift_ms),
        )
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrCeilingKbps(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jint {
    ABR_SLAB
        .with(handle, |s| s.controller.ceiling_kbps() as jint)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrFloorKbps(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jint {
    ABR_SLAB
        .with(handle, |s| s.controller.floor_kbps() as jint)
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_abrSuspend(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    ABR_SLAB.with(handle, |s| s.controller.suspend());
}

// --- ClientHintBitrateController ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintRange(
    mut env: JNIEnv,
    _: JClass,
    initial_kbps: jint,
) -> jintArray {
    let (low, high) = client_hint::range(non_neg_u32(initial_kbps));
    int_array(&mut env, &[low as i32, high as i32])
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintCreate(
    _: JNIEnv,
    _: JClass,
    initial_kbps: jint,
    limit_kbps: jint,
    now_ms: jlong,
) -> jlong {
    CLIENT_HINT_SLAB.insert(ClientHintBitrate::new(
        non_neg_u32(initial_kbps),
        non_neg_u32(limit_kbps),
        non_neg_u64(now_ms),
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    CLIENT_HINT_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintResumeFrom(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    kbps: jint,
) {
    CLIENT_HINT_SLAB.with(handle, |c| c.resume_from(non_neg_u32(kbps)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintSetThermalLevel(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    thermal_level: jint,
) {
    CLIENT_HINT_SLAB.with(handle, |c| c.set_thermal_level(non_neg_u32(thermal_level)));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintSample(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    now_ms: jlong,
    recent_video: jboolean,
    loss_percent: jfloat,
    queue_ms: jfloat,
) -> jint {
    CLIENT_HINT_SLAB
        .with(handle, |c| {
            let stats = Stats {
                now_ms: non_neg_u64(now_ms),
                recent_video: recent_video != JNI_FALSE,
                loss_percent: Some(loss_percent),
                queue_ms: Some(queue_ms),
                ..Stats::default()
            };
            c.sample(&stats).unwrap_or(0) as jint
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintApplied(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    kbps: jint,
    now_ms: jlong,
) {
    CLIENT_HINT_SLAB.with(handle, |c| {
        c.applied(non_neg_u32(kbps), non_neg_u64(now_ms))
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintRefused(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    CLIENT_HINT_SLAB.with(handle, |c| c.refused());
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintOverridden(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    kbps: jint,
    now_ms: jlong,
) {
    CLIENT_HINT_SLAB.with(handle, |c| {
        c.overridden(non_neg_u32(kbps), non_neg_u64(now_ms))
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_clientHintState(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jintArray {
    let state = CLIENT_HINT_SLAB
        .with(handle, |c| {
            [
                c.current_kbps() as i32,
                c.min_kbps() as i32,
                c.max_kbps() as i32,
                i32::from(c.pending()),
                i32::from(c.stopped()),
                i32::from(c.has_room_before_quality_reduction()),
            ]
        })
        .unwrap_or([0, 0, 0, 0, 1, 0]);
    int_array(&mut env, &state)
}

// --- PyroWaveBitrateFloor ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroFloorMinimumKbps(
    _: JNIEnv,
    _: JClass,
    width: jint,
    height: jint,
    fps: jint,
) -> jint {
    pyrowave::minimum_kbps(non_neg_u32(width), non_neg_u32(height), non_neg_u32(fps)) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroFloorDefaultKbps(
    _: JNIEnv,
    _: JClass,
    width: jint,
    height: jint,
    fps: jint,
    link_mbps: jint,
) -> jint {
    pyrowave::default_kbps(
        non_neg_u32(width),
        non_neg_u32(height),
        non_neg_u32(fps),
        non_neg_u32(link_mbps),
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroFloorStartKbps(
    _: JNIEnv,
    _: JClass,
    configured: jint,
    automatic: jboolean,
    width: jint,
    height: jint,
    fps: jint,
    link_mbps: jint,
) -> jint {
    if automatic == JNI_FALSE {
        return configured;
    }
    pyrowave::start_kbps(
        non_neg_u32(configured),
        true,
        non_neg_u32(width),
        non_neg_u32(height),
        non_neg_u32(fps),
        non_neg_u32(link_mbps),
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroFloorLinkNotice(
    mut env: JNIEnv,
    _: JClass,
    width: jint,
    height: jint,
    fps: jint,
    link_mbps: jint,
    configured_kbps: jint,
) -> jintArray {
    match pyrowave::link_notice(
        non_neg_u32(width),
        non_neg_u32(height),
        non_neg_u32(fps),
        non_neg_u32(link_mbps),
        non_neg_u32(configured_kbps),
    ) {
        Some(n) => int_array(
            &mut env,
            &[
                n.asked_kbps as i32,
                n.minimum_kbps as i32,
                n.cap_kbps as i32,
            ],
        ),
        None => std::ptr::null_mut(),
    }
}

// --- PyroWaveBitrateController ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroAbrCreate(
    _: JNIEnv,
    _: JClass,
    ceiling_kbps: jint,
    current_kbps: jint,
    fps: jint,
    now_ms: jlong,
) -> jlong {
    PYROWAVE_SLAB.insert(PyroWaveBitrateController::new(
        non_neg_u32(ceiling_kbps),
        non_neg_u32(current_kbps),
        non_neg_u32(fps),
        non_neg_u64(now_ms),
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroAbrDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    PYROWAVE_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroAbrSample(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    now_ms: jlong,
    recent_video: jboolean,
    poor_connection: jboolean,
    loss_percent: jfloat,
    queue_ms: jfloat,
    decode_ms: jfloat,
    jitter_ms: jfloat,
    thermal_level: jint,
) -> jint {
    PYROWAVE_SLAB
        .with(handle, |c| {
            let stats = Stats {
                now_ms: non_neg_u64(now_ms),
                recent_video: recent_video != JNI_FALSE,
                poor_connection: poor_connection != JNI_FALSE,
                loss_percent: Some(loss_percent),
                queue_ms: Some(queue_ms),
                decode_ms: Some(decode_ms),
                jitter_ms: Some(jitter_ms),
                thermal_level: non_neg_u32(thermal_level),
                ..Stats::default()
            };
            c.sample(&stats).unwrap_or(0) as jint
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroAbrApplied(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    requested_kbps: jint,
    applied_kbps: jint,
    now_ms: jlong,
) {
    PYROWAVE_SLAB.with(handle, |c| {
        c.applied(
            non_neg_u32(requested_kbps),
            non_neg_u32(applied_kbps),
            non_neg_u64(now_ms),
        )
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pyroAbrSuspend(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    PYROWAVE_SLAB.with(handle, |c| c.suspend());
}
