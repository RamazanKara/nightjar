//! Stateful & orchestration JNI exports for `NightjarSessionJni`, bridging
//! `nightjar_core::connect::{negotiate, preflight, launch, actions}` to Android
//! (`NvConnection`).

use std::cell::RefCell;
use std::net::{IpAddr, Ipv4Addr};

use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString, JValue};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jintArray, jlong, jlongArray, jstring};
use nightjar_core::connect::actions::{HardwareMetricsPoller, HardwareStepError};
#[path = "connect_launch.rs"]
pub mod connect_launch;

use nightjar_core::connect::negotiate::{
    NegotiationNotice, PlatformVideoFormats, TwoStepConfig, formats_after_pick, negotiate_two_step,
    negotiate_video_formats, pyro_wave_hdr_lost, sharp_color_input,
};
use nightjar_core::connect::preflight::{
    JumboPreflight, JumboSkip, RouteLocation, effective_android_requested_packet_size,
    first_bitrate_kbps, jumbo_preflight, location_for_route, probes_first_bitrate,
    pyro_wave_link_notice, pyro_wave_start_kbps, reset_packet_size, resume_packet_size,
    run_jumbo_probe,
};
use nightjar_core::http::{self, pace::Measurement};
use nightjar_core::sharp_color::{Pick, Why};
use nightjar_policy::http::ServerInfo;
use serde_json::{Value, json};

use self::connect_launch::{app_role_from_ordinal, control_from_ordinal};

use crate::hosts::jstr;
use crate::http::new_jstring;
use crate::policy_recovery::long_array;
use crate::slab::Slab;

pub(crate) static METRICS_POLLERS: Slab<HardwareMetricsPoller> = Slab::new();

pub(crate) fn clear_metrics_pollers() {
    METRICS_POLLERS.drain_all();
}

pub(crate) fn jb(val: bool) -> jboolean {
    if val { JNI_TRUE } else { JNI_FALSE }
}

fn int_array(env: &JNIEnv, values: &[i32]) -> jintArray {
    let Ok(arr) = env.new_int_array(values.len() as i32) else {
        return std::ptr::null_mut();
    };
    if env.set_int_array_region(&arr, 0, values).is_err() {
        return std::ptr::null_mut();
    }
    arr.into_raw()
}

fn why_tag(why: &Why) -> &'static str {
    match why {
        Why::Off => "off",
        Why::PyroWave => "pyrowave",
        Why::Hdr => "hdr",
        Why::Decoder(_) => "decoder",
        Why::Host => "host",
        Why::Device => "device",
        Why::NoCommonMode => "no_common_mode",
    }
}

fn why_detail(why: &Why) -> &str {
    match why {
        Why::Decoder(detail) => detail.as_str(),
        _ => "",
    }
}

fn format_sharp_pick(pick: &Pick) -> String {
    match pick {
        Pick::Native { formats } => format!("native 4:4:4, formats 0x{formats:x}"),
        Pick::Packed {
            modes,
            native_refusal,
        } => {
            let why = why_tag(native_refusal);
            let detail = why_detail(native_refusal);
            if detail.is_empty() {
                format!("packed full chroma, modes 0x{modes:x} (native 4:4:4 skipped: {why})")
            } else {
                format!(
                    "packed full chroma, modes 0x{modes:x} (native 4:4:4 skipped: {why}: {detail})"
                )
            }
        }
        Pick::Without(why) => {
            let tag = why_tag(why);
            let detail = why_detail(why);
            if detail.is_empty() {
                format!("neither ({tag})")
            } else {
                format!("neither ({tag}: {detail})")
            }
        }
    }
}

pub(crate) fn build_server_info(
    paired: bool,
    current_game: i32,
    current_game_uuid: &str,
    session_idle: bool,
    has_bandwidth_probe: bool,
    host_confirms_replacement: bool,
) -> ServerInfo {
    ServerInfo::synthetic(
        paired,
        current_game,
        current_game_uuid,
        session_idle,
        has_bandwidth_probe,
        host_confirms_replacement,
        None,
    )
}

// --- HardwareMetricsPoller (Slab) ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_metricsPollerCreate(
    _: JNIEnv,
    _: JClass,
) -> jlong {
    METRICS_POLLERS.insert(HardwareMetricsPoller::new())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_metricsPollerDestroy(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    METRICS_POLLERS.remove(handle);
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_metricsPollerStop(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) {
    if let Some(poller) = METRICS_POLLERS.get(handle) {
        poller.stop();
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_metricsPollerIsUnavailable(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
) -> jboolean {
    jb(METRICS_POLLERS
        .get(handle)
        .is_some_and(|p| p.is_unavailable()))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_metricsPollerDue(
    _: JNIEnv,
    _: JClass,
    handle: jlong,
    now_ms: jlong,
    wanted: jboolean,
) -> jint {
    let Some(poller) = METRICS_POLLERS.get(handle) else {
        return -1;
    };
    poller
        .due(now_ms.max(0) as u64, wanted != 0)
        .map_or(-1, |g| g as jint)
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_metricsPollerFinishStep(
    mut env: JNIEnv,
    _: JClass,
    handle: jlong,
    generation: jint,
    now_ms: jlong,
    ok: jboolean,
    sampled_at_unix_ms: jlong,
    interval_ms: jint,
    http_status: jint,
    unsupported_version: jboolean,
) -> jlongArray {
    let Some(poller) = METRICS_POLLERS.get(handle) else {
        return long_array(&mut env, &[0, 0, 1]);
    };
    let result = if ok != 0 {
        Ok((sampled_at_unix_ms, interval_ms.max(0) as u32))
    } else if unsupported_version != 0 || matches!(http_status, 403 | 404 | 409) {
        Err(HardwareStepError::Unavailable)
    } else {
        Err(HardwareStepError::Failed)
    };
    let (show, delay, unavail) =
        poller.finish_step_sample(generation.max(0) as u32, now_ms.max(0) as u64, result);
    long_array(
        &mut env,
        &[
            i64::from(show),
            delay.min(i64::MAX as u64) as i64,
            i64::from(unavail),
        ],
    )
}

// --- Two-step video format & SharpColor negotiation ---

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_negotiateVideoFormats(
    _: JNIEnv,
    _: JClass,
    formats: jint,
    server_formats: jint,
) -> jint {
    negotiate_video_formats(formats as u32, server_formats as u32) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_pyroWaveHdrLost(
    _: JNIEnv,
    _: JClass,
    common_formats: jint,
    negotiated_formats: jint,
) -> jboolean {
    jb(pyro_wave_hdr_lost(
        common_formats as u32,
        negotiated_formats as u32,
    ))
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_sharpColorRequestJson(
    env: JNIEnv,
    _: JClass,
    offered: jint,
    sdr10: jboolean,
    pyrowave: jboolean,
    packed_engine: jboolean,
    host_scm: jint,
    host_packed: jint,
    packed_refused_before: jboolean,
) -> jstring {
    let input = sharp_color_input(
        offered as u32,
        sdr10 != 0,
        pyrowave != 0,
        packed_engine != 0,
        host_scm as u32,
        (0..=255)
            .contains(&host_packed)
            .then_some(host_packed as u8),
        packed_refused_before != 0,
    );
    let body = json!({
        "auto": input.auto,
        "hdr": input.hdr,
        "sdr10": input.sdr10,
        "pyrowave": input.pyrowave,
        "packed_engine": input.packed_engine,
        "base": input.base as i32,
        "decodable": input.decodable as i32,
        "decoder_reason": input.decoder_reason,
        "host_scm": input.host_scm as i32,
        "host_packed": input.host_packed.map(i32::from).unwrap_or(-1),
        "packed_refused_before": input.packed_refused_before,
    });
    new_jstring(&env, &body.to_string())
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_formatsAfterSharpColor(
    mut env: JNIEnv,
    _: JClass,
    pick_kind: JString,
    formats_or_modes: jint,
    offered: jint,
    hdr: jboolean,
) -> jint {
    let kind = jstr(&mut env, &pick_kind).unwrap_or_default();
    let pick = match kind.as_str() {
        "native" => Pick::Native {
            formats: formats_or_modes as u32,
        },
        "packed" => Pick::Packed {
            modes: (formats_or_modes.max(0) as u32) as u8,
            native_refusal: Why::Off,
        },
        _ => Pick::Without(Why::Off),
    };
    formats_after_pick(&pick, offered as u32, hdr != 0) as jint
}

struct JniNegotiationBridge<'a, 'local> {
    env: RefCell<JNIEnv<'local>>,
    callback: &'a JObject<'local>,
}

impl PlatformVideoFormats for JniNegotiationBridge<'_, '_> {
    fn prepare_packed_modes(&mut self, probe_formats: u32) -> u8 {
        if self.callback.is_null() {
            return 0;
        }
        let mut env = self.env.borrow_mut();
        let out = env.with_local_frame(8, |env| {
            let res = env.call_method(
                self.callback,
                "preparePackedModes",
                "(I)I",
                &[JValue::Int(probe_formats as jint)],
            );
            match res {
                Ok(v) => Ok(v.i().unwrap_or(0).clamp(0, 255) as u8),
                Err(e) => {
                    let _ = env.exception_clear();
                    Err(e)
                }
            }
        });
        out.unwrap_or(0)
    }

    fn prepare_video_formats(&mut self, common_formats: u32, host_scm: u32) -> u32 {
        if self.callback.is_null() {
            return common_formats;
        }
        let mut env = self.env.borrow_mut();
        let out = env.with_local_frame(8, |env| {
            let res = env.call_method(
                self.callback,
                "prepareVideoFormats",
                "(II)I",
                &[
                    JValue::Int(common_formats as jint),
                    JValue::Int(host_scm as jint),
                ],
            );
            match res {
                Ok(v) => Ok(v.i().unwrap_or(common_formats as jint) as u32),
                Err(e) => {
                    let _ = env.exception_clear();
                    Err(e)
                }
            }
        });
        out.unwrap_or(common_formats)
    }

    fn pick_hdr_format(
        &mut self,
        host_hdr_formats: i32,
        negotiated_formats: u32,
        packed_444: bool,
        frame_warp: bool,
    ) -> (i32, u32) {
        if self.callback.is_null() {
            return (-1, negotiated_formats);
        }
        let mut env = self.env.borrow_mut();
        let out = env.with_local_frame(8, |env| {
            let res = env.call_method(
                self.callback,
                "pickHdrFormat",
                "(IIZZ)J",
                &[
                    JValue::Int(host_hdr_formats),
                    JValue::Int(negotiated_formats as jint),
                    JValue::Bool(jb(packed_444)),
                    JValue::Bool(jb(frame_warp)),
                ],
            );
            match res {
                Ok(v) => {
                    let packed = v.j().unwrap_or(0);
                    let hdr_fmt = (packed >> 32) as i32;
                    let fmts = (packed as i32) as u32;
                    Ok((hdr_fmt, fmts))
                }
                Err(e) => {
                    let _ = env.exception_clear();
                    Err(e)
                }
            }
        });
        out.unwrap_or((-1, negotiated_formats))
    }

    fn video_capabilities(&mut self, negotiated_formats: u32, hdr_format: i32) -> u32 {
        if self.callback.is_null() {
            return 0;
        }
        let mut env = self.env.borrow_mut();
        let out = env.with_local_frame(8, |env| {
            let res = env.call_method(
                self.callback,
                "videoCapabilities",
                "(II)I",
                &[
                    JValue::Int(negotiated_formats as jint),
                    JValue::Int(hdr_format),
                ],
            );
            match res {
                Ok(v) => Ok(v.i().unwrap_or(0) as u32),
                Err(e) => {
                    let _ = env.exception_clear();
                    Err(e)
                }
            }
        });
        out.unwrap_or(0)
    }
}

fn notice_tag(notice: NegotiationNotice) -> &'static str {
    match notice {
        NegotiationNotice::Sdr10GivenWayToPacked => "sdr10_given_way_to_packed",
        NegotiationNotice::Sdr10GivenWayToYuv444 => "sdr10_given_way_to_yuv444",
        NegotiationNotice::Sdr10NotAccepted => "sdr10_not_accepted",
        NegotiationNotice::Sdr10Accepted => "sdr10_accepted",
        NegotiationNotice::HdrUnavailable => "hdr_unavailable",
        NegotiationNotice::HdrAccepted => "hdr_accepted",
        NegotiationNotice::PyroWaveHdrLost => "pyrowave_hdr_lost",
        NegotiationNotice::PyroWaveHdrAccepted => "pyrowave_hdr_accepted",
        NegotiationNotice::PackedUnavailableFromHost => "packed_unavailable_from_host",
        NegotiationNotice::PackedUnavailableFromDevice => "packed_unavailable_from_device",
        NegotiationNotice::PackedAvailable => "packed_available",
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_negotiateTwoStep(
    env: JNIEnv,
    _: JClass,
    initial_formats: jint,
    host_scm: jint,
    host_hdr_formats: jint,
    frame_warp: jboolean,
    sharp_color: jboolean,
    sdr10: jboolean,
    pyrowave: jboolean,
    packed_engine: jboolean,
    host_packed: jint,
    packed_refused_before: jboolean,
    callback: JObject,
) -> jstring {
    let mut bridge = JniNegotiationBridge {
        env: RefCell::new(env),
        callback: &callback,
    };
    let cfg = TwoStepConfig {
        initial_formats: initial_formats as u32,
        host_scm: host_scm as u32,
        host_hdr_formats,
        frame_warp: frame_warp != 0,
        sharp_color: sharp_color != 0,
        sdr10: sdr10 != 0,
        pyrowave: pyrowave != 0,
        packed_engine: packed_engine != 0,
        host_packed: (0..=255)
            .contains(&host_packed)
            .then_some(host_packed as u8),
        packed_refused_before: packed_refused_before != 0,
    };
    let outcome = negotiate_two_step(cfg, &mut bridge);
    let notices: Vec<&str> = outcome.notices.iter().copied().map(notice_tag).collect();
    let pick_text = outcome.sharp_pick.as_ref().map(format_sharp_pick);
    let decodable = outcome
        .sharp_input
        .as_ref()
        .map_or(0, |i| i.decodable as i32);
    let body = json!({
        "offered_formats": outcome.offered_formats as i32,
        "common_formats": outcome.common_formats as i32,
        "negotiated_formats": outcome.negotiated_formats as i32,
        "negotiated_10bit": outcome.negotiated_10bit,
        "negotiated_hdr": outcome.negotiated_hdr,
        "hdr_format": outcome.hdr_format,
        "video_capabilities": outcome.video_capabilities as i32,
        "packed_444": outcome.packed_444,
        "native_444": outcome.native_444,
        "keep_sdr10": outcome.keep_sdr10,
        "sharp_pick_text": pick_text,
        "sharp_decodable": decodable,
        "notices": notices,
    });
    let env = bridge.env.into_inner();
    new_jstring(&env, &body.to_string())
}

// --- Preflight, link probes, and PyroWave sizing ---

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_firstBitrateKbps(
    mut env: JNIEnv,
    _: JClass,
    configured_kbps: jint,
    link_probe_enabled: jboolean,
    automatic_bitrate: jboolean,
    resume_only: jboolean,
    control_ordinal: jint,
    role_ordinal: jint,
    small_stream_input_only: jboolean,
    negotiated_formats: jint,
    paired: jboolean,
    has_bandwidth_probe: jboolean,
    host_callback: JObject,
) -> jint {
    let info = build_server_info(paired != 0, 0, "", true, has_bandwidth_probe != 0, false);
    let should_probe = probes_first_bitrate(
        link_probe_enabled != 0,
        automatic_bitrate != 0,
        resume_only != 0,
        control_from_ordinal(control_ordinal),
        app_role_from_ordinal(role_ordinal),
        small_stream_input_only != 0,
        negotiated_formats as u32,
        &info,
    );
    first_bitrate_kbps(configured_kbps.max(0) as u32, should_probe, || {
        if host_callback.is_null() {
            return Err(http::Error::Cancelled);
        }
        let reply = env.with_local_frame(8, |env| {
            let res = env.call_method(&host_callback, "probeLinkMbps", "()Ljava/lang/String;", &[]);
            connect_launch::read_jstring_result(env, res)
        });
        let text = reply.map_err(|_| http::Error::Io(std::io::Error::other("JNI probe error")))?;
        parse_f64_envelope(&text)
    }) as jint
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_pyroWaveStartKbps(
    _: JNIEnv,
    _: JClass,
    negotiated_formats: jint,
    configured_kbps: jint,
    automatic_bitrate: jboolean,
    width: jint,
    height: jint,
    fps: jint,
    link_mbps: jint,
    bitrate_limit_kbps: jint,
) -> jint {
    pyro_wave_start_kbps(
        negotiated_formats as u32,
        configured_kbps.max(0) as u32,
        automatic_bitrate != 0,
        width.max(0) as u32,
        height.max(0) as u32,
        fps.max(0) as u32,
        link_mbps.max(0) as u32,
        bitrate_limit_kbps.max(0) as u32,
    ) as jint
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_pyroWaveLinkNotice(
    env: JNIEnv,
    _: JClass,
    negotiated_formats: jint,
    width: jint,
    height: jint,
    fps: jint,
    link_mbps: jint,
    configured_kbps: jint,
    automatic_bitrate: jboolean,
) -> jintArray {
    let Some(notice) = pyro_wave_link_notice(
        negotiated_formats as u32,
        width.max(0) as u32,
        height.max(0) as u32,
        fps.max(0) as u32,
        link_mbps.max(0) as u32,
        configured_kbps.max(0) as u32,
        automatic_bitrate != 0,
    ) else {
        return std::ptr::null_mut();
    };
    int_array(&env, &[notice.asked_kbps as i32, notice.cap_kbps as i32])
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_packet_size_values(
    location_ordinal: i32,
    is_private_addr: bool,
    vpn_or_tunnel: bool,
    offered_mtu: i32,
    burst_probe: bool,
    requested_packet_size: i32,
    jumbo_frames: bool,
    auto_reconnect: bool,
    negotiated_packet_size: i32,
    probe_fn: impl FnMut(u32, u32, u32, u32) -> Result<Measurement, http::Error>,
) -> [i32; 2] {
    let base_loc = match location_ordinal {
        1 => RouteLocation::Local,
        2 => RouteLocation::Remote,
        _ => RouteLocation::Auto,
    };
    let req = effective_android_requested_packet_size(requested_packet_size.max(0) as u32);
    let (loc, route_pkt) = location_for_route(base_loc, vpn_or_tunnel);
    let addr = if is_private_addr {
        Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)))
    } else {
        Some(IpAddr::V4(Ipv4Addr::new(198, 51, 100, 1)))
    };
    let plan = jumbo_preflight(
        loc,
        addr,
        offered_mtu.max(0) as u16,
        burst_probe,
        req,
        jumbo_frames,
        auto_reconnect,
    );
    match plan {
        // A resume keeps the running session's packets, as on desktop (0: none negotiated).
        JumboPreflight::Skip(JumboSkip::Resume) => {
            let negotiated = negotiated_packet_size.max(0) as u32;
            [resume_packet_size(negotiated, route_pkt, req), 0]
        }
        JumboPreflight::Skip(_) => [reset_packet_size(route_pkt, req), 0],
        JumboPreflight::Probe { candidate } => {
            let outcome = run_jumbo_probe(candidate, route_pkt, req, jumbo_frames, probe_fn);
            [outcome.packet_size, i32::from(outcome.fallback_notice)]
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarSessionJni_resolvePacketSizePreflight(
    mut env: JNIEnv,
    _: JClass,
    location_ordinal: jint,
    is_private_addr: jboolean,
    vpn_or_tunnel: jboolean,
    offered_mtu: jint,
    burst_probe: jboolean,
    requested_packet_size: jint,
    jumbo_frames: jboolean,
    auto_reconnect: jboolean,
    negotiated_packet_size: jint,
    host_callback: JObject,
) -> jintArray {
    let out = resolve_packet_size_values(
        location_ordinal,
        is_private_addr != 0,
        vpn_or_tunnel != 0,
        offered_mtu,
        burst_probe != 0,
        requested_packet_size,
        jumbo_frames != 0,
        auto_reconnect != 0,
        negotiated_packet_size,
        |kbps, size, fps, pace| {
            if host_callback.is_null() {
                return Err(http::Error::Cancelled);
            }
            let reply = env.with_local_frame(8, |env| {
                let res = env.call_method(
                    &host_callback,
                    "probePace",
                    "(IIII)Ljava/lang/String;",
                    &[
                        JValue::Int(kbps as jint),
                        JValue::Int(size as jint),
                        JValue::Int(fps as jint),
                        JValue::Int(pace as jint),
                    ],
                );
                connect_launch::read_jstring_result(env, res)
            });
            let text =
                reply.map_err(|_| http::Error::Io(std::io::Error::other("JNI pace probe")))?;
            parse_pace_envelope(&text)
        },
    );
    int_array(&env, &out)
}

pub(crate) fn parse_envelope(text: &str) -> Result<Value, http::Error> {
    let val: Value =
        serde_json::from_str(text).map_err(|e| http::Error::Malformed(e.to_string()))?;
    if let Some(ok) = val.get("ok") {
        return Ok(ok.clone());
    }
    let Some(err) = val.get("error") else {
        return Err(http::Error::Malformed("missing envelope result".into()));
    };
    let kind = err.get("kind").and_then(Value::as_str).unwrap_or("io");
    let code = err.get("code").and_then(Value::as_i64).unwrap_or(0) as i32;
    let msg = err
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Err(match kind {
        "status" => http::Error::Status { code, message: msg },
        "cancelled" => http::Error::Cancelled,
        "malformed" => http::Error::Malformed(msg),
        _ => http::Error::Io(std::io::Error::other(msg)),
    })
}

fn parse_f64_envelope(text: &str) -> Result<f64, http::Error> {
    parse_envelope(text)?
        .as_f64()
        .ok_or_else(|| http::Error::Malformed("expected number".into()))
}

fn parse_pace_envelope(text: &str) -> Result<Measurement, http::Error> {
    let ok = parse_envelope(text)?;
    Ok(Measurement {
        expected: ok.get("expected").and_then(Value::as_u64).unwrap_or(0) as u32,
        sent: ok.get("sent").and_then(Value::as_u64).unwrap_or(0) as u32,
        received: ok.get("received").and_then(Value::as_u64).unwrap_or(0) as u32,
        sender_ms: ok.get("sender_ms").and_then(Value::as_f64).unwrap_or(0.0),
        loss: ok.get("loss").and_then(Value::as_f64).unwrap_or(0.0),
        worst_window_loss: ok
            .get("worst_window_loss")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        frames: ok.get("frames").and_then(Value::as_u64).unwrap_or(0) as u32,
        damaged_frames: ok
            .get("damaged_frames")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        excessive_loss_frames: ok
            .get("excessive_loss_frames")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        missing_tail_frames: ok
            .get("missing_tail_frames")
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32,
        worst_frame_loss: ok
            .get("worst_frame_loss")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    })
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
