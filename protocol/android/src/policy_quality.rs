//! JNI bridge for `NightjarPolicyJni`: `AdaptiveQualityController`, `FoundationAbr`, and
//! `PyroWavePace` + `PaceSearch`.

use jni::JNIEnv;
use jni::objects::{JBooleanArray, JClass, JString};
use jni::sys::{JNI_FALSE, jboolean, jdouble, jfloat, jint, jintArray, jlong, jstring};
use nightjar_core::http::foundation::{AbrAction, HostAbr};
use nightjar_core::http::pace::{self, Measurement, PaceSearch};
use nightjar_core::http::{jumbo_probe_passes, jumbo_probe_size};
use nightjar_policy::quality::{
    self, AdaptiveQualityController, Bounds, DecodeTiming, Mode, Target,
};
use nightjar_policy::stats::Stats;
use serde_json::{Value, json};

use crate::native_api;
use crate::policy::{int_array, jbool, non_neg_u32, non_neg_u64, shifted_u64};
use crate::slab::MutexSlab;

struct QualityEntry {
    ctrl: AdaptiveQualityController,
    shift_ms: u64,
}

static QUALITY_SLAB: MutexSlab<QualityEntry> = MutexSlab::new();
static FOUNDATION_ABR_SLAB: MutexSlab<HostAbr> = MutexSlab::new();
static PACE_SEARCH_SLAB: MutexSlab<PaceSearch> = MutexSlab::new();

fn throw_illegal_argument(env: &mut JNIEnv, message: &str) {
    let _ = env.throw_new("java/lang/IllegalArgumentException", message);
}

fn mode_from_ordinal(ordinal: jint) -> Mode {
    match ordinal {
        1 => Mode::Motion,
        2 => Mode::Detail,
        3 => Mode::Battery,
        _ => Mode::Off,
    }
}

// --- AdaptiveQualityController ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityDecodeBudgetMs(
    _: JNIEnv,
    _: JClass,
    fps: jint,
) -> jdouble {
    quality::decode_budget_ms(fps.max(1) as u32)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityCreate(
    mut env: JNIEnv,
    _: JClass,
    mode_ordinal: jint,
    width: jint,
    height: jint,
    fps: jint,
    min_short_edge: jint,
    min_fps: jint,
    align_width: jint,
    align_height: jint,
    now_ms: jlong,
) -> jlong {
    let bounds = Bounds {
        min_short_edge: non_neg_u32(min_short_edge),
        min_fps: non_neg_u32(min_fps),
        align_width: non_neg_u32(align_width),
        align_height: non_neg_u32(align_height),
    };
    let shift_ms = if now_ms < 0 { now_ms.unsigned_abs() } else { 0 };
    match AdaptiveQualityController::new(
        mode_from_ordinal(mode_ordinal),
        non_neg_u32(width),
        non_neg_u32(height),
        non_neg_u32(fps),
        bounds,
        shifted_u64(now_ms, shift_ms),
    ) {
        Ok(ctrl) => QUALITY_SLAB.insert(QualityEntry { ctrl, shift_ms }),
        Err(err) => {
            throw_illegal_argument(&mut env, &err.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    QUALITY_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityResumeAt(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    width: jint,
    height: jint,
    fps: jint,
) {
    QUALITY_SLAB.with(handle, |e| {
        e.ctrl.resume_at(Target {
            width: non_neg_u32(width),
            height: non_neg_u32(height),
            fps: non_neg_u32(fps),
        })
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityCeiling(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jintArray {
    let Some(t) = QUALITY_SLAB.with(handle, |e| e.ctrl.ceiling()) else {
        return std::ptr::null_mut();
    };
    int_array(&mut env, &[t.width as i32, t.height as i32, t.fps as i32])
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityCurrent(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jintArray {
    let Some(t) = QUALITY_SLAB.with(handle, |e| e.ctrl.current()) else {
        return std::ptr::null_mut();
    };
    int_array(&mut env, &[t.width as i32, t.height as i32, t.fps as i32])
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityLevels(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jintArray {
    let Some(flat) = QUALITY_SLAB.with(handle, |e| {
        let mut out = Vec::with_capacity(e.ctrl.levels().len() * 3);
        for t in e.ctrl.levels() {
            out.push(t.width as i32);
            out.push(t.height as i32);
            out.push(t.fps as i32);
        }
        out
    }) else {
        return std::ptr::null_mut();
    };
    int_array(&mut env, &flat)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityResizeCeiling(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    width: jint,
    height: jint,
    fps: jint,
    now_ms: jlong,
) -> jintArray {
    let res = QUALITY_SLAB.with(handle, |e| {
        let now = shifted_u64(now_ms, e.shift_ms);
        e.ctrl.resize_ceiling(non_neg_u32(width), non_neg_u32(height), non_neg_u32(fps), now)
    });
    match res {
        Some(Ok(Some(t))) => int_array(&mut env, &[t.width as i32, t.height as i32, t.fps as i32]),
        Some(Err(err)) => {
            throw_illegal_argument(&mut env, &err.to_string());
            std::ptr::null_mut()
        }
        _ => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualitySample(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    now_ms: jlong,
    recent_video: jboolean,
    poor_connection: jboolean,
    loss_percent: jfloat,
    decode_ms: jfloat,
    jitter_ms: jfloat,
    queue_ms: jfloat,
    thermal_level: jint,
    bitrate_has_room: jboolean,
    observe_tail: jboolean,
    has_timing: jboolean,
    timing_generation: jlong,
    timing_window_start_ns: jlong,
    timing_window_end_ns: jlong,
    timing_first_sample_ns: jlong,
    timing_last_sample_ns: jlong,
    timing_fps: jint,
    timing_samples: jint,
    timing_late_frames: jint,
) -> jintArray {
    let target = QUALITY_SLAB.with(handle, |e| {
        let stats = Stats {
            now_ms: shifted_u64(now_ms, e.shift_ms),
            recent_video: recent_video != JNI_FALSE,
            poor_connection: poor_connection != JNI_FALSE,
            loss_percent: Some(loss_percent),
            decode_ms: Some(decode_ms),
            jitter_ms: Some(jitter_ms),
            queue_ms: Some(queue_ms),
            thermal_level: non_neg_u32(thermal_level),
            ..Stats::default()
        };
        let room = bitrate_has_room != JNI_FALSE;
        if observe_tail == JNI_FALSE {
            e.ctrl.sample(&stats, room)
        } else if has_timing == JNI_FALSE {
            e.ctrl.sample_timed(&stats, room, None)
        } else {
            let timing = DecodeTiming {
                generation: timing_generation,
                window_start_ns: timing_window_start_ns,
                window_end_ns: timing_window_end_ns,
                first_sample_ns: timing_first_sample_ns,
                last_sample_ns: timing_last_sample_ns,
                fps: timing_fps,
                samples: timing_samples,
                late_frames: timing_late_frames,
            };
            e.ctrl.sample_timed(&stats, room, Some(&timing))
        }
    });
    match target.flatten() {
        Some(t) => int_array(&mut env, &[t.width as i32, t.height as i32, t.fps as i32]),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityApplied(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    width: jint,
    height: jint,
    fps: jint,
    now_ms: jlong,
) {
    let target = Target {
        width: non_neg_u32(width),
        height: non_neg_u32(height),
        fps: non_neg_u32(fps),
    };
    if let Some(Err(err)) = QUALITY_SLAB.with(handle, |e| {
        let now = shifted_u64(now_ms, e.shift_ms);
        e.ctrl.applied(target, now)
    }) {
        throw_illegal_argument(&mut env, &err.to_string());
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualitySuspend(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    QUALITY_SLAB.with(handle, |e| e.ctrl.suspend());
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityTakesBitrate(
    _: JNIEnv,
    _: JClass,
    held: jboolean,
    supported: jboolean,
    busy_on: jboolean,
    hint_on: jboolean,
    manual_bitrate: jboolean,
    host_link_notice: jboolean,
) -> jboolean {
    jbool(quality::quality_takes_bitrate(
        held != JNI_FALSE,
        supported != JNI_FALSE,
        busy_on != JNI_FALSE,
        hint_on != JNI_FALSE,
        manual_bitrate != JNI_FALSE,
        host_link_notice != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityHostSteersLinkBitrate(
    _: JNIEnv,
    _: JClass,
    host_link_notice: jboolean,
    pc_decides: jboolean,
    kept: jboolean,
    manual_bitrate: jboolean,
    busy_on: jboolean,
    hint_on: jboolean,
) -> jboolean {
    jbool(quality::host_steers_link_bitrate(
        host_link_notice != JNI_FALSE,
        pc_decides != JNI_FALSE,
        kept != JNI_FALSE,
        manual_bitrate != JNI_FALSE,
        busy_on != JNI_FALSE,
        hint_on != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_qualityHostLinkHasRoom(
    _: JNIEnv,
    _: JClass,
    link_state_wire: jint,
    current_kbps: jint,
    ceiling_kbps: jint,
) -> jboolean {
    let state = match link_state_wire {
        1 => quality::LinkState::Limited,
        2 => quality::LinkState::Floor,
        _ => quality::LinkState::Full,
    };
    jbool(quality::host_link_has_room(
        state,
        non_neg_u32(current_kbps),
        non_neg_u32(ceiling_kbps),
    ))
}

// --- FoundationAbr ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrCreate(
    _: JNIEnv,
    _: JClass,
    initial_kbps: jint,
    stream_kbps: jint,
    now_ms: jlong,
) -> jlong {
    FOUNDATION_ABR_SLAB.insert(HostAbr::from_initial(
        non_neg_u32(initial_kbps),
        non_neg_u32(stream_kbps),
        non_neg_u64(now_ms),
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    FOUNDATION_ABR_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrAdd(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    completed: jint,
    drops: jint,
    decoded: jint,
) {
    FOUNDATION_ABR_SLAB.with(handle, |abr| {
        abr.add(
            u64::from(non_neg_u32(completed)),
            u64::from(non_neg_u32(drops)),
            u64::from(non_neg_u32(decoded)),
        )
    });
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrAddRate(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    loss_percent: jdouble,
    fps: jdouble,
) {
    FOUNDATION_ABR_SLAB.with(handle, |abr| abr.add_rate(loss_percent, fps));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrDue(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
    now_ms: jlong,
    rtt_ms: jdouble,
) -> jstring {
    let feedback = FOUNDATION_ABR_SLAB
        .with(handle, |abr| {
            let rtt = (rtt_ms.is_finite() && rtt_ms >= 0.0).then_some(rtt_ms);
            abr.due(non_neg_u64(now_ms), rtt).map(|fb| {
                json!({
                    "packet_loss": fb.packet_loss,
                    "rtt_ms": fb.rtt_ms,
                    "decode_fps": fb.decode_fps,
                    "dropped_frames": fb.dropped_frames,
                    "current_bitrate": fb.current_bitrate,
                })
                .to_string()
            })
        })
        .flatten();
    let Some(json) = feedback else {
        return std::ptr::null_mut();
    };
    match env.new_string(json) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrAnswered(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    has_action: jboolean,
    new_bitrate: jint,
    bitrate_applied: jboolean,
) -> jint {
    let action = (has_action != JNI_FALSE).then(|| AbrAction {
        new_bitrate: (new_bitrate > 0).then_some(new_bitrate as u32),
        bitrate_applied: bitrate_applied != JNI_FALSE,
        bitrate_apply_error: None,
        reason: String::new(),
    });
    FOUNDATION_ABR_SLAB
        .with(handle, |abr| {
            abr.answered(action.as_ref()).unwrap_or(0) as jint
        })
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_foundationAbrCurrentKbps(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jint {
    FOUNDATION_ABR_SLAB
        .with(handle, |abr| abr.current_kbps as jint)
        .unwrap_or(0)
}

// --- PyroWavePace + PaceSearch ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceWireKbps(
    _: JNIEnv,
    _: JClass,
    image_kbps: jint,
    fps: jint,
    packet_size: jint,
    fec: jint,
    min_parity: jint,
) -> jint {
    pace::wire_kbps(
        non_neg_u32(image_kbps),
        non_neg_u32(fps),
        non_neg_u32(packet_size),
        non_neg_u32(fec),
        non_neg_u32(min_parity),
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceProbeKbps(
    _: JNIEnv,
    _: JClass,
    image_kbps: jint,
    fps: jint,
    packet_size: jint,
    fec: jint,
    min_parity: jint,
) -> jint {
    pace::probe_kbps(
        non_neg_u32(image_kbps),
        non_neg_u32(fps),
        non_neg_u32(packet_size),
        non_neg_u32(fec),
        non_neg_u32(min_parity),
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceFloorKbps(
    _: JNIEnv,
    _: JClass,
    probe_kbps: jint,
) -> jint {
    pace::floor_kbps(non_neg_u32(probe_kbps)) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceSearchCreate(
    _: JNIEnv,
    _: JClass,
    link_kbps: jint,
    floor_kbps: jint,
) -> jlong {
    PACE_SEARCH_SLAB.insert(PaceSearch::new(
        non_neg_u32(link_kbps),
        non_neg_u32(floor_kbps),
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceSearchDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    PACE_SEARCH_SLAB.remove(handle);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceSearchNextPace(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jint {
    PACE_SEARCH_SLAB
        .with(handle, |s| s.next_pace().map(|v| v as i32).unwrap_or(-1))
        .unwrap_or(-1)
}

fn parse_measurement(raw: &str) -> Option<Measurement> {
    let v = serde_json::from_str::<Value>(raw).ok()?;
    Some(Measurement {
        expected: v["expected"].as_u64().unwrap_or(0) as u32,
        sent: v["sent"].as_u64().unwrap_or(0) as u32,
        received: v["received"].as_u64().unwrap_or(0) as u32,
        sender_ms: v["sender_ms"].as_f64().unwrap_or(0.0),
        loss: v["loss"].as_f64().unwrap_or(100.0),
        worst_window_loss: v["worst_window_loss"].as_f64().unwrap_or(100.0),
        frames: v["frames"].as_u64().unwrap_or(0) as u32,
        damaged_frames: v["damaged_frames"].as_u64().unwrap_or(0) as u32,
        excessive_loss_frames: v["excessive_loss_frames"].as_u64().unwrap_or(0) as u32,
        missing_tail_frames: v["missing_tail_frames"].as_u64().unwrap_or(0) as u32,
        worst_frame_loss: v["worst_frame_loss"].as_f64().unwrap_or(100.0),
    })
}

fn measurement_value(m: &Measurement) -> Value {
    json!({
        "expected": m.expected,
        "sent": m.sent,
        "received": m.received,
        "sender_ms": m.sender_ms,
        "loss": m.loss,
        "worst_window_loss": m.worst_window_loss,
        "frames": m.frames,
        "damaged_frames": m.damaged_frames,
        "excessive_loss_frames": m.excessive_loss_frames,
        "missing_tail_frames": m.missing_tail_frames,
        "worst_frame_loss": m.worst_frame_loss,
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceExpectedPackets(
    _: JNIEnv,
    _: JClass,
    kbps: jint,
    packet_size: jint,
) -> jlong {
    i64::from(pace::expected_packets(
        non_neg_u32(kbps),
        non_neg_u32(packet_size),
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceBurstGroupPackets(
    _: JNIEnv,
    _: JClass,
    pace_kbps: jint,
    packet_size: jint,
    ipv6: jboolean,
) -> jlong {
    i64::from(pace::burst_group(
        non_neg_u32(pace_kbps),
        non_neg_u32(packet_size),
        ipv6 != JNI_FALSE,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceSummarize(
    env: JNIEnv,
    _: JClass,
    expected: jlong,
    sent: jlong,
    sender_ms: jdouble,
    arrived: JBooleanArray,
    fps: jint,
    group_packets: jlong,
) -> jstring {
    let arrived_vec = if arrived.is_null() {
        Vec::new()
    } else if let Ok(len) = env.get_array_length(&arrived) {
        let mut buf = vec![0u8; len as usize];
        if env.get_boolean_array_region(&arrived, 0, &mut buf).is_ok() {
            buf.into_iter().map(|b| b != 0).collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };
    let mut m = Measurement {
        expected: u32::try_from(expected.max(0)).unwrap_or(u32::MAX),
        sent: u32::try_from(sent.max(0)).unwrap_or(u32::MAX),
        sender_ms,
        ..Measurement::default()
    };
    pace::summarize(
        &mut m,
        &arrived_vec,
        non_neg_u32(fps),
        u32::try_from(group_packets.max(1)).unwrap_or(u32::MAX),
    );
    match env.new_string(measurement_value(&m).to_string()) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceMeasured(
    mut env: JNIEnv,
    _: JClass,
    measurement_json: JString,
) -> jboolean {
    let ok = native_api::string(&mut env, &measurement_json)
        .and_then(|s| parse_measurement(&s))
        .is_some_and(|m| m.measured());
    jbool(ok)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceQualified(
    mut env: JNIEnv,
    _: JClass,
    measurement_json: JString,
) -> jboolean {
    let ok = native_api::string(&mut env, &measurement_json)
        .and_then(|s| parse_measurement(&s))
        .is_some_and(|m| m.qualified());
    jbool(ok)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceJumboProbeSize(
    _: JNIEnv,
    _: JClass,
    configured: jint,
) -> jint {
    jumbo_probe_size(u16::try_from(configured.max(0)).unwrap_or(u16::MAX)) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceJumboPasses(
    mut env: JNIEnv,
    _: JClass,
    jumbo_json: JString,
    control_json: JString,
) -> jboolean {
    let Some(jumbo) = native_api::string(&mut env, &jumbo_json).and_then(|s| parse_measurement(&s))
    else {
        return JNI_FALSE;
    };
    let control = native_api::string(&mut env, &control_json).and_then(|s| parse_measurement(&s));
    jbool(jumbo_probe_passes(&jumbo, control.as_ref()))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceSearchRecord(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    pace_kbps: jint,
    measurement_json: JString,
) {
    let Some(m) =
        native_api::string(&mut env, &measurement_json).and_then(|s| parse_measurement(&s))
    else {
        return;
    };
    PACE_SEARCH_SLAB.with(handle, |s| s.record(non_neg_u32(pace_kbps), m));
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_paceSearchOutcome(
    env: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jstring {
    let reply = PACE_SEARCH_SLAB
        .with(handle, |s| {
            s.outcome().map(|o| {
                let mut v = measurement_value(&o.measurement);
                v["pace_kbps"] = json!(o.pace_kbps);
                v["qualified"] = json!(o.qualified);
                v.to_string()
            })
        })
        .flatten();
    let Some(reply) = reply else {
        return std::ptr::null_mut();
    };
    match env.new_string(reply) {
        Ok(s) => s.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}
