//! Stream Doctor 2.0's verdict for the app's `StreamDoctorController`
//! (`NightjarPolicyJni.doctorDiagnose`): one JSON request in, `{"ok": <Diagnosis>}` or
//! `{"error": "invalid", "detail": ...}` out, from `nightjar_policy::doctor_v2::diagnose`, the
//! code desktop Nightjar runs, so both apps reach the same finding.
//!
//! The request carries the report Android's `StreamHealth` made (`report`, its findings by their
//! Java names), the host's `GET /stream/doctor` answer while the host offers `stream.doctor_v2`
//! (`host`, else null), the stream's facts (`stream`), this device's (`device`: `constrained`,
//! `upscaler` `off|sgsr1|fsr1|vendor`, `receive_buffer` `[granted, asked]` bytes or null) and what
//! the stream can change now (`abilities`). The platform is Android. Android decodes packed full
//! chroma on the GPU, so a stream never copies it through memory here.

use jni::JNIEnv;
use jni::objects::{JClass, JString};
use jni::sys::jstring;
use nightjar_core::http::native_api::doctor::DoctorSnapshot;
use nightjar_policy::doctor_v2::{
    Abilities, ActiveUpscaler, DeviceFacts, Diagnosis, Platform, StreamFacts, diagnose,
};
use nightjar_policy::health::{Finding, Report};
use serde::Deserialize;
use serde_json::json;

use crate::native_api;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportJson {
    findings: Vec<String>,
    samples: usize,
    recent: bool,
    worst_loss_percent: f32,
    worst_decode_ms: f32,
    worst_host_p95_ms: f32,
    worst_display_p95_ms: f32,
    #[serde(default)]
    stalls: usize,
    #[serde(default)]
    stall_period_ms: u64,
    #[serde(default)]
    host_includes_send: bool,
    #[serde(default)]
    late_samples: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StreamJson {
    width: u32,
    height: u32,
    fps: u32,
    #[serde(default)]
    hdr: bool,
    #[serde(default)]
    ten_bit: bool,
    #[serde(default)]
    pyrowave: bool,
    screen_width: u32,
    screen_height: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum UpscalerJson {
    Off,
    Sgsr1,
    Fsr1,
    Vendor,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceJson {
    #[serde(default = "constrained")]
    constrained: bool,
    upscaler: UpscalerJson,
    #[serde(default)]
    receive_buffer: Option<[u32; 2]>,
}

/// A phone, handheld or TV: the lighter upscaler.
fn constrained() -> bool {
    true
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AbilitiesJson {
    bitrate: bool,
    live_kbps: u32,
    resize: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    report: ReportJson,
    #[serde(default)]
    host: Option<DoctorSnapshot>,
    stream: StreamJson,
    device: DeviceJson,
    abilities: AbilitiesJson,
}

fn finding(name: &str) -> Result<Finding, String> {
    Finding::ALL
        .into_iter()
        .find(|f| f.name() == name)
        .ok_or_else(|| format!("unknown finding {name:?}"))
}

fn verdict(request: &str) -> Result<Diagnosis, String> {
    let r: Request = serde_json::from_str(request).map_err(|e| e.to_string())?;
    let report = Report {
        findings: r
            .report
            .findings
            .iter()
            .map(|name| finding(name))
            .collect::<Result<_, _>>()?,
        samples: r.report.samples,
        recent: r.report.recent,
        worst_loss_percent: r.report.worst_loss_percent,
        worst_decode_ms: r.report.worst_decode_ms,
        worst_host_p95_ms: r.report.worst_host_p95_ms,
        worst_display_p95_ms: r.report.worst_display_p95_ms,
        stalls: r.report.stalls,
        stall_period_ms: r.report.stall_period_ms,
        host_includes_send: r.report.host_includes_send,
        late_samples: r.report.late_samples,
    };
    let stream = StreamFacts {
        width: r.stream.width,
        height: r.stream.height,
        fps: r.stream.fps,
        hdr: r.stream.hdr,
        ten_bit: r.stream.ten_bit,
        pyrowave: r.stream.pyrowave,
        packed_from_memory: false,
        screen_width: r.stream.screen_width,
        screen_height: r.stream.screen_height,
    };
    let device = DeviceFacts {
        platform: Platform::Android,
        constrained: r.device.constrained,
        metalfx: false,
        upscaler: match r.device.upscaler {
            UpscalerJson::Off => ActiveUpscaler::Off,
            UpscalerJson::Sgsr1 => ActiveUpscaler::Sgsr1,
            UpscalerJson::Fsr1 => ActiveUpscaler::Fsr1,
            UpscalerJson::Vendor => ActiveUpscaler::Vendor,
        },
        receive_buffer: r
            .device
            .receive_buffer
            .map(|[granted, asked]| (granted, asked)),
    };
    let can = Abilities {
        bitrate: r.abilities.bitrate,
        live_kbps: r.abilities.live_kbps,
        resize: r.abilities.resize,
    };
    Ok(diagnose(&report, r.host.as_ref(), &stream, &device, &can))
}

/// The answer to one request: `{"ok": <Diagnosis>}`, or `{"error": "invalid", "detail"}`.
pub fn call(request: &str) -> String {
    match verdict(request) {
        Ok(diagnosis) => json!({ "ok": diagnosis }).to_string(),
        Err(detail) => json!({ "error": "invalid", "detail": detail }).to_string(),
    }
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_limelight_nvstream_jni_NightjarPolicyJni_doctorDiagnose(
    mut env: JNIEnv,
    _: JClass,
    request: JString,
) -> jstring {
    let answer = match native_api::string(&mut env, &request) {
        Some(request) => call(&request),
        None => json!({"error": "invalid", "detail": "the request is not a string"}).to_string(),
    };
    match env.new_string(answer) {
        Ok(answer) => answer.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[cfg(test)]
mod tests;
