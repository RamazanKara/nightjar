//! The JSON round trip of `NightjarPolicyJni.doctorDiagnose`: what Java sends reaches
//! `nightjar_policy::doctor_v2::diagnose` unchanged, and its verdict comes back in the stable
//! snake_case shape `StreamDoctorVerdict.java` reads.
use super::*;
use serde_json::{Value, json};

const SNAPSHOT: &str = include_str!("../../../native-api/tests/fixtures/stream_doctor.json");

fn request(findings: &[&str], host: Value) -> Value {
    json!({
        "report": {
            "findings": findings, "samples": 20, "recent": true, "worst_loss_percent": 3.0,
            "worst_decode_ms": 12.5, "worst_host_p95_ms": 6.0, "worst_display_p95_ms": 4.0,
            "stalls": 0, "stall_period_ms": 0, "host_includes_send": true, "late_samples": 0
        },
        "host": host,
        "stream": {
            "width": 2560, "height": 1440, "fps": 120, "hdr": false, "ten_bit": false,
            "pyrowave": false, "screen_width": 3200, "screen_height": 1800
        },
        "device": { "constrained": true, "upscaler": "off", "receive_buffer": null },
        "abilities": { "bitrate": true, "live_kbps": 45000, "resize": true }
    })
}

fn answer(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

/// The verdict the policy gives for the same facts, as Java receives it.
fn expected(findings: &[Finding], host: Option<&DoctorSnapshot>) -> Value {
    let report = Report {
        findings: findings.to_vec(),
        samples: 20,
        recent: true,
        worst_loss_percent: 3.0,
        worst_decode_ms: 12.5,
        worst_host_p95_ms: 6.0,
        worst_display_p95_ms: 4.0,
        stalls: 0,
        stall_period_ms: 0,
        host_includes_send: true,
        late_samples: 0,
    };
    let stream = StreamFacts {
        width: 2560,
        height: 1440,
        fps: 120,
        hdr: false,
        ten_bit: false,
        pyrowave: false,
        packed_from_memory: false,
        screen_width: 3200,
        screen_height: 1800,
    };
    let device = DeviceFacts {
        platform: Platform::Android,
        constrained: true,
        metalfx: false,
        upscaler: ActiveUpscaler::Off,
        receive_buffer: None,
    };
    let can = Abilities {
        bitrate: true,
        live_kbps: 45_000,
        resize: true,
    };
    json!({ "ok": diagnose(&report, host, &stream, &device, &can) })
}

#[test]
fn a_decoder_that_falls_behind_gets_the_policys_verdict_in_its_wire_shape() {
    let got = answer(&request(&["DECODER"], Value::Null));
    assert_eq!(got, expected(&[Finding::Decoder], None));
    let ok = &got["ok"];
    assert_eq!(ok["cause"], "decode_pressure");
    assert_eq!(ok["stage"], "decode");
    assert_eq!(ok["key"]["kind"], "decode_ms");
    assert_eq!(ok["key"]["ms"], 12.5);
    assert_eq!(ok["remedies"][0]["remedy"], "resolution_stepdown");
    assert_eq!(ok["remedies"][0]["upscaler"], "sgsr1");
    assert!(ok["remedies"][0]["width"].as_u64().unwrap() < 2560);
    assert_eq!(ok["advice"][0]["advice"], "lower_frame_rate");
    assert_eq!(ok["upscaler"]["verdict"], "fine");
    assert_eq!(ok["host_measured"], false);
}

#[test]
fn the_hosts_snapshot_names_its_stage_and_its_own_step_down() {
    let snapshot: DoctorSnapshot = serde_json::from_str(SNAPSHOT).unwrap();
    let host: Value = serde_json::from_str(SNAPSHOT).unwrap();
    let got = answer(&request(&[], host.clone()));
    assert_eq!(got, expected(&[], Some(&snapshot)));
    let ok = &got["ok"];
    assert_eq!(ok["cause"], "host_overload");
    assert_eq!(ok["stage"], "host_encode");
    assert_eq!(ok["key"]["kind"], "host_lost");
    assert_eq!(ok["key"]["stage"], "encode");
    assert_eq!(ok["key"]["lost_ms"], 642.5);
    assert_eq!(ok["host_measured"], true);
    assert_eq!(
        ok["advice"][0],
        json!({"advice": "use_upscaler", "upscaler": "sgsr1"})
    );

    // A network finding with the host's step-down open: the bitrate goes through the host.
    let mut open = host;
    open["host_stages"]["late_pictures"] = json!(0);
    open["eligible_remedies"][0] = json!({"id": "bitrate_stepdown", "configurable": true,
        "target_kbps": 28800, "reason": null, "message": null});
    let ok = &answer(&request(&["NETWORK"], open))["ok"];
    assert_eq!(ok["cause"], "network_jitter");
    assert_eq!(
        ok["remedies"][0],
        json!({"remedy": "bitrate_stepdown", "target_kbps": 36000, "on_host": true})
    );
    assert_eq!(ok["key"], json!({"kind": "loss_percent", "percent": 3.0}));
}

#[test]
fn a_classic_host_gets_this_devices_verdict_and_the_classic_trial() {
    let ok = &answer(&request(&["NETWORK"], Value::Null))["ok"];
    assert_eq!(ok["cause"], "network_jitter");
    assert_eq!(
        ok["remedies"],
        json!([{"remedy": "bitrate_stepdown", "target_kbps": 36000, "on_host": false}])
    );
    assert_eq!(ok["advice"], json!([{"advice": "wired_or_quieter_wifi"}]));
}

#[test]
fn the_first_seconds_gather_and_offer_nothing() {
    let mut warmup = request(&["NETWORK"], Value::Null);
    warmup["report"]["samples"] = json!(2);
    let ok = &answer(&warmup)["ok"];
    assert_eq!(ok["cause"], "gathering");
    assert_eq!(ok["remedies"], json!([]));
    assert_eq!(ok["key"], json!({"kind": "none"}));

    // Without a frame rate the frame interval is unknown: null, never NaN.
    let mut no_rate = request(&["DECODER"], Value::Null);
    no_rate["stream"]["fps"] = json!(0);
    assert_eq!(answer(&no_rate)["ok"]["key"]["budget_ms"], Value::Null);
}

#[test]
fn the_receive_buffer_and_the_upscaler_in_use_reach_the_verdict() {
    let mut short = request(&["NETWORK"], Value::Null);
    short["device"]["receive_buffer"] = json!([425_984, 4_194_304]);
    let ok = &answer(&short)["ok"];
    assert_eq!(ok["cause"], "receive_buffer_short");
    assert_eq!(
        ok["key"],
        json!({"kind": "receive_buffer", "granted_kib": 416, "asked_kib": 4096})
    );
    assert_eq!(ok["advice"][0]["linux_command"], false);

    let mut costly = request(&["DECODER"], Value::Null);
    costly["device"]["upscaler"] = json!("fsr1");
    let ok = &answer(&costly)["ok"];
    assert_eq!(
        ok["upscaler"],
        json!({"verdict": "too_costly", "lighter": "sgsr1"})
    );

    let mut hdr = request(&[], Value::Null);
    hdr["stream"]["hdr"] = json!(true);
    hdr["device"]["upscaler"] = json!("sgsr1");
    let ok = &answer(&hdr)["ok"];
    assert_eq!(ok["cause"], "healthy");
    assert_eq!(
        ok["upscaler"],
        json!({"verdict": "blocked", "blocker": "hdr"})
    );
}

#[test]
fn a_malformed_request_is_refused_with_its_reason() {
    let mut unknown = request(&["LAG"], Value::Null);
    let got = answer(&unknown);
    assert_eq!(got["error"], "invalid");
    assert!(got["detail"].as_str().unwrap().contains("LAG"));
    unknown = request(&[], Value::Null);
    unknown["device"]["upscaler"] = json!("metal_fx");
    assert_eq!(answer(&unknown)["error"], "invalid");
    unknown = request(&[], Value::Null);
    unknown["abilities"]["percent"] = json!(50);
    assert_eq!(answer(&unknown)["error"], "invalid");
    let bad: Value = serde_json::from_str(&call("not json")).unwrap();
    assert_eq!(bad["error"], "invalid");
}
