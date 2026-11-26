//! JNI bridge for `NightjarPolicyJni`: video pacing (`FramePacingPolicy`), vsync & fractional hold
//! (`vsync`), Balanced slot planning (`RefreshSlots`), and display frame-rate policy
//! (`DisplayFrameRatePolicy`).

use jni::JNIEnv;
use jni::objects::{JClass, JFloatArray, JIntArray, JLongArray};
use jni::sys::{JNI_FALSE, jboolean, jdouble, jfloat, jint, jlong};
use nightjar_policy::video_pacing::{
    self, ArrHalfRateCheck, ContentRateVote, PanelMode, RefreshSlots,
};
use nightjar_policy::vsync::{self, HoldStampState};

use crate::policy::{jbool, non_neg_u32};
use crate::slab::MutexSlab;

static HOLD_STAMP_SLAB: MutexSlab<HoldStampState> = MutexSlab::new();
static REFRESH_SLOTS_SLAB: MutexSlab<RefreshSlots> = MutexSlab::new();
static ARR_HALF_RATE_SLAB: MutexSlab<ArrHalfRateCheck> = MutexSlab::new();
static CONTENT_RATE_VOTE_SLAB: MutexSlab<ContentRateVote> = MutexSlab::new();

macro_rules! read_arr {
    ($env:expr, $arr:expr, $ty:ty, $zero:expr, $method:ident) => {{
        let n = if $arr.is_null() {
            0
        } else {
            $env.get_array_length($arr).unwrap_or(0)
        }
        .max(0) as usize;
        let mut v = vec![$zero; n];
        if n > 0 && $env.$method($arr, 0, &mut v).is_err() {
            v.clear();
        }
        v
    }};
}

fn throw_invalid_rate(env: &mut JNIEnv) {
    let _ = env.throw_new(
        "java/lang/IllegalArgumentException",
        "Invalid stream frame rate",
    );
}

fn write_longs(env: &mut JNIEnv, out: &JLongArray, values: &[i64]) -> jboolean {
    let ok = !out.is_null()
        && env
            .get_array_length(out)
            .is_ok_and(|n| n >= values.len() as i32);
    jbool(ok && env.set_long_array_region(out, 0, values).is_ok())
}

// --- Slab Lifecycle (0=HoldStamp, 1=RefreshSlots, 2=ArrHalfRate, 3=ContentRateVote) ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pacingSlabCreate(
    _: JNIEnv,
    _: JClass,
    kind: jint,
) -> jlong {
    match kind {
        0 => HOLD_STAMP_SLAB.insert(HoldStampState::new()),
        1 => REFRESH_SLOTS_SLAB.insert(RefreshSlots::new()),
        2 => ARR_HALF_RATE_SLAB.insert(ArrHalfRateCheck::new()),
        _ => CONTENT_RATE_VOTE_SLAB.insert(ContentRateVote::new()),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_pacingSlabDestroy(
    _: JNIEnv,
    _: JClass,
    kind: jint,
    handle: jlong,
) {
    match kind {
        0 => _ = HOLD_STAMP_SLAB.remove(handle),
        1 => _ = REFRESH_SLOTS_SLAB.remove(handle),
        2 => _ = ARR_HALF_RATE_SLAB.remove(handle),
        _ => _ = CONTENT_RATE_VOTE_SLAB.remove(handle),
    }
}

// --- HoldStamp & Vsync ---

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_holdStampStep(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    release_ns: jlong,
    vsync_ns: jlong,
    interval_ns: jlong,
    deadline_ns: jlong,
    refreshes: jint,
    every_frame: jboolean,
    vsync_ok: jboolean,
    out: JLongArray,
) -> jboolean {
    let Some(v) = HOLD_STAMP_SLAB.with(handle, |s| {
        let stamp = s.step(
            release_ns,
            vsync_ns,
            interval_ns,
            deadline_ns,
            non_neg_u32(refreshes),
            every_frame != JNI_FALSE,
            vsync_ok != JNI_FALSE,
        );
        [stamp, s.held_anchor_ns, i64::from(s.held_anchor_refreshes)]
    }) else {
        return JNI_FALSE;
    };
    write_longs(&mut env, &out, &v)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_vsyncTiming(
    _: JNIEnv,
    _: JClass,
    op: jint,
    a: jlong,
    b: jlong,
    c: jlong,
    d: jlong,
    e: jint,
    f: jboolean,
) -> jlong {
    match op {
        0 => vsync::first_vsync_ns(a, b, c, d),
        1 => vsync::next_vsync_time_ns(a, b, c, d),
        2 => vsync::latch_slack_ns(a, b, c, d),
        3 => jlong::from(vsync::early_for_turn(a, b, c, d)),
        _ => vsync::held_target_ns(a, b, c, d, non_neg_u32(e), f != JNI_FALSE),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_vsyncHeldReportGrid(
    mut env: JNIEnv,
    _: JClass,
    vsync_ns: jlong,
    interval_ns: jlong,
    anchor_ns: jlong,
    refreshes: jint,
    out: JLongArray,
) -> jboolean {
    let (v, i) = vsync::held_report_grid(vsync_ns, interval_ns, anchor_ns, non_neg_u32(refreshes));
    write_longs(&mut env, &out, &[v, i])
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_vsyncRefreshesPerFrame(
    _: JNIEnv,
    _: JClass,
    refresh_hz: jdouble,
    fps: jdouble,
    vrr: jboolean,
    fractional: jboolean,
) -> jint {
    video_pacing::refreshes_per_frame(refresh_hz, fps, vrr != JNI_FALSE, fractional != JNI_FALSE)
        as jint
}

// --- FramePacingPolicy ---

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_framePacingBool(
    mut env: JNIEnv,
    _: JClass,
    op: jint,
    mode: jint,
    fa: jboolean,
    fb: jboolean,
    fc: jboolean,
    a: jlong,
    b: jlong,
    c: jlong,
    fps_milli_hz: jint,
) -> jboolean {
    let (ba, bb, bc) = (fa != JNI_FALSE, fb != JNI_FALSE, fc != JNI_FALSE);
    let res = match op {
        0 => Some(video_pacing::uses_pyro_wave_display_clock(mode, ba, bb)),
        1 => Some(video_pacing::drop_queued_frames(mode)),
        2 => Some(video_pacing::schedules_frames(ba, mode)),
        3 => Some(video_pacing::recovers_decoder_debt(ba, mode)),
        4 => Some(video_pacing::apply_output_pacing_fallback(ba, bb, bc, mode)),
        5 => Some(video_pacing::is_frame_due(a, b, c)),
        6 => video_pacing::balanced_paces_on_vsync(ba, bb, fps_milli_hz, c),
        7 => video_pacing::should_drain_output(mode, a, b, fps_milli_hz, c as i32),
        _ => video_pacing::has_output_drain_time_remaining(a, fps_milli_hz),
    };
    match res {
        Some(v) => jbool(v),
        None => {
            throw_invalid_rate(&mut env);
            JNI_FALSE
        }
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_framePacingTimeNs(
    mut env: JNIEnv,
    _: JClass,
    next: jboolean,
    due_ns: jlong,
    vsync_ns: jlong,
    fps_milli_hz: jint,
) -> jlong {
    if next == JNI_FALSE {
        return video_pacing::frame_interval_ns(fps_milli_hz).unwrap_or(-1);
    }
    video_pacing::next_frame_time_ns_milli_hz(due_ns, vsync_ns, fps_milli_hz).unwrap_or_else(|| {
        throw_invalid_rate(&mut env);
        0
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_framePacingIntOp(
    _: JNIEnv,
    _: JClass,
    op: jint,
    a: jint,
    b: jint,
    c: jint,
    d: jint,
    hz: jfloat,
) -> jint {
    match op {
        0 => video_pacing::pyro_wave_mode(a, b != 0, c != 0, d != 0),
        1 => video_pacing::limited_frame_rate(a, hz),
        _ => video_pacing::negotiated_frame_rate_milli_hz(a, b),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_framePacingLimitRefreshHz(
    _: JNIEnv,
    _: JClass,
    asked_hz: jfloat,
    start_hz: jfloat,
    fps: jint,
    battery_saver: jboolean,
    thermal_critical: jboolean,
    peak_hz: jfloat,
) -> jfloat {
    video_pacing::limit_refresh_hz(
        asked_hz,
        start_hz,
        fps,
        battery_saver != JNI_FALSE,
        thermal_critical != JNI_FALSE,
        peak_hz,
    )
}

// --- RefreshSlots (#119) ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_refreshSlotsOp(
    _: JNIEnv,
    _: JClass,
    op: jint,
    handle: jlong,
    a: jlong,
    b: jlong,
) -> jlong {
    match op {
        0 => {
            REFRESH_SLOTS_SLAB.with(handle, RefreshSlots::reset);
            0
        }
        1 => jlong::from(video_pacing::refresh_slots_applies(a, b)),
        2 => REFRESH_SLOTS_SLAB
            .with(handle, |s| s.planned())
            .unwrap_or(0),
        _ => REFRESH_SLOTS_SLAB.with(handle, |s| s.shared()).unwrap_or(0),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_refreshSlotsPlan(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    release_ns: jlong,
    vsync_ns: jlong,
    interval_ns: jlong,
    lead_ns: jlong,
    out: JLongArray,
) -> jboolean {
    if out.is_null() {
        return JNI_FALSE;
    }
    let Ok(len) = env.get_array_length(&out) else {
        return JNI_FALSE;
    };
    if len < 3 {
        return JNI_FALSE;
    }
    let Some(v) = REFRESH_SLOTS_SLAB.with(handle, |slots| {
        let s = slots.plan(release_ns, vsync_ns, interval_ns, lead_ns);
        [s.vsync_ns, s.target_ns, i64::from(s.shared), s.hold_ns]
    }) else {
        return JNI_FALSE;
    };
    let slice = if len >= 4 { &v[..] } else { &v[..3] };
    write_longs(&mut env, &out, slice)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_refreshSlotsEarliestClaimed(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    release_ns: jlong,
    vsync_ns: jlong,
    interval_ns: jlong,
    lead_ns: jlong,
) -> jboolean {
    jbool(
        REFRESH_SLOTS_SLAB
            .with(handle, |s| {
                s.earliest_claimed(release_ns, vsync_ns, interval_ns, lead_ns)
            })
            .unwrap_or(false),
    )
}

// --- DisplayFrameRatePolicy (#168) ---

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_displayIntGate(
    _: JNIEnv,
    _: JClass,
    op: jint,
    a: jint,
    b: jint,
    c: jint,
    d: jint,
    e: jint,
    f: jint,
    flag: jboolean,
    hz: jfloat,
) -> jint {
    match op {
        0 => video_pacing::arr_state(a, flag != JNI_FALSE, b != 0) as jint,
        1 => video_pacing::stream_frame_rate(hz),
        2 => jint::from(video_pacing::fps_follows_display(flag != JNI_FALSE, a, hz)),
        _ => jint::from(video_pacing::mode_fits_stream(a, b, c, d, e, f)),
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_displayBoolGate(
    _: JNIEnv,
    _: JClass,
    op: jint,
    sdk: jint,
    a: jboolean,
    b: jboolean,
    c: jboolean,
    d: jboolean,
    e: jboolean,
    f: jboolean,
) -> jboolean {
    let (ba, bb, bc, bd, be, bf) = (
        a != JNI_FALSE,
        b != JNI_FALSE,
        c != JNI_FALSE,
        d != JNI_FALSE,
        e != JNI_FALSE,
        f != JNI_FALSE,
    );
    jbool(match op {
        0 => video_pacing::adaptive_in_use(ba, bb, bc),
        1 => video_pacing::tracks_content_rate(ba, bb, bc, bd),
        2 => video_pacing::follows_content_rate(sdk, ba, bb, bc, bd, be),
        3 => video_pacing::pins_window_display_mode(sdk, ba, bb, bc, bd, be, bf),
        _ => video_pacing::syncs_window_display_mode_with_vote(ba, bb, bc, bd),
    })
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_displayRateCalc(
    mut env: JNIEnv,
    _: JClass,
    op: jint,
    a: jfloat,
    b: jfloat,
    c: jfloat,
    may_reduce: jboolean,
    seamless_hz: JFloatArray,
) -> jfloat {
    match op {
        0 => video_pacing::vrr_content_vote(
            a,
            b,
            may_reduce != JNI_FALSE,
            &read_arr!(&mut env, &seamless_hz, f32, 0.0_f32, get_float_array_region),
        ),
        1 => video_pacing::content_vote(a, b, may_reduce != JNI_FALSE, c),
        _ => video_pacing::arr_vote(a, b),
    }
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_displayWindowModeForVote(
    mut env: JNIEnv,
    _: JClass,
    sdk: jint,
    vote_hz: jfloat,
    flags: jint,
    active_meta: JIntArray,
    active_hz: jfloat,
    active_alts: JFloatArray,
    mode_ids: JIntArray,
    mode_ws: JIntArray,
    mode_hs: JIntArray,
    mode_hzs: JFloatArray,
) -> jint {
    let am = read_arr!(&mut env, &active_meta, i32, 0_i32, get_int_array_region);
    if am.len() < 3 {
        return -1;
    }
    let alts = read_arr!(&mut env, &active_alts, f32, 0.0_f32, get_float_array_region);
    let ids = read_arr!(&mut env, &mode_ids, i32, 0_i32, get_int_array_region);
    let ws = read_arr!(&mut env, &mode_ws, i32, 0_i32, get_int_array_region);
    let hs = read_arr!(&mut env, &mode_hs, i32, 0_i32, get_int_array_region);
    let hzs = read_arr!(&mut env, &mode_hzs, f32, 0.0_f32, get_float_array_region);
    let n = ids.len().min(ws.len()).min(hs.len()).min(hzs.len());
    let modes: Vec<PanelMode<'_>> = (0..n)
        .map(|i| PanelMode {
            id: ids[i],
            width: ws[i],
            height: hs[i],
            hz: hzs[i],
            alternatives: &[],
        })
        .collect();
    let active = PanelMode {
        id: am[0],
        width: am[1],
        height: am[2],
        hz: active_hz,
        alternatives: &alts,
    };
    video_pacing::window_mode_for_vote(
        sdk,
        vote_hz,
        flags & 1 != 0,
        flags & 2 != 0,
        &active,
        &modes,
    )
    .map_or(-1, |i| i as jint)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_displayMaxRefreshRate(
    mut env: JNIEnv,
    _: JClass,
    active_w: jint,
    active_h: jint,
    active_hz: jfloat,
    mode_ws: JIntArray,
    mode_hs: JIntArray,
    mode_hzs: JFloatArray,
    alt_counts: JIntArray,
    all_alts: JFloatArray,
    stream_w: jint,
    stream_h: jint,
) -> jfloat {
    let ws = read_arr!(&mut env, &mode_ws, i32, 0_i32, get_int_array_region);
    let hs = read_arr!(&mut env, &mode_hs, i32, 0_i32, get_int_array_region);
    let hzs = read_arr!(&mut env, &mode_hzs, f32, 0.0_f32, get_float_array_region);
    let counts = read_arr!(&mut env, &alt_counts, i32, 0_i32, get_int_array_region);
    let alts = read_arr!(&mut env, &all_alts, f32, 0.0_f32, get_float_array_region);
    let n = ws.len().min(hs.len()).min(hzs.len()).min(counts.len());
    let (mut modes, mut off) = (Vec::with_capacity(n), 0usize);
    for i in 0..n {
        let end = off
            .saturating_add(counts[i].max(0) as usize)
            .min(alts.len());
        modes.push(PanelMode {
            id: 0,
            width: ws[i],
            height: hs[i],
            hz: hzs[i],
            alternatives: &alts[off..end],
        });
        off = end;
    }
    let active = PanelMode {
        id: 0,
        width: active_w,
        height: active_h,
        hz: active_hz,
        alternatives: &[],
    };
    video_pacing::max_refresh_rate(&active, &modes, stream_w, stream_h)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_arrHalfRateStep(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    op: jint,
    now_ms: jlong,
    rendered: jfloat,
    presented: jfloat,
    display_hz: jfloat,
    stream_hz: jfloat,
) -> jboolean {
    if op < 0 {
        return jbool(ArrHalfRateCheck::is_half(
            rendered, presented, display_hz, stream_hz,
        ));
    }
    jbool(
        ARR_HALF_RATE_SLAB
            .with(handle, |c| match op {
                0 => c.update(now_ms, rendered, presented, display_hz, stream_hz),
                1 => c.flagged(),
                _ => {
                    c.reset();
                    false
                }
            })
            .unwrap_or(false),
    )
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_contentRateVoteStep(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    op: jint,
    now_ms: jlong,
    content_milli_hz: jint,
) -> jfloat {
    CONTENT_RATE_VOTE_SLAB
        .with(handle, |v| match op {
            0 => v.next(now_ms, content_milli_hz),
            1 => {
                if v.withdraw(now_ms, content_milli_hz) {
                    1.0
                } else {
                    0.0
                }
            }
            _ => {
                v.invalidate();
                0.0
            }
        })
        .unwrap_or(0.0)
}
