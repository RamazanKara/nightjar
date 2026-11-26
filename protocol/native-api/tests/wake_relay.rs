//! The `wake/relay` examples of `docs/native-api.md`. Nightjar's tests read the same files.
use rubylight_native_api::feature;
use rubylight_native_api::wake_relay::{RelayEnroll, RelayEnrolled, WakeRelayInfo};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value as Json};

fn round_trip<T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug>(name: &str) -> T {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    assert!(!text.contains('\r'), "{name}.json must use LF");
    let expected: Json = serde_json::from_str(&text).unwrap();
    let typed: T = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
    assert_eq!(serde_json::to_value(&typed).unwrap(), expected, "{name}");
    typed
}

#[test]
fn the_feature_name_matches_the_reservation() {
    assert_eq!(feature::WAKE_RELAY, "wake.relay");
}

#[test]
fn wake_relay_info_round_trips_and_tolerates_a_newer_host() {
    let info: WakeRelayInfo = round_trip("wake_relay");
    let bundle = info.bundle.unwrap();
    assert_eq!(bundle.relays.len(), 1);
    assert_eq!(bundle.relays[0].secret.len(), 64);
    // A host without relays, or a caller without launch: no bundle; unknown fields are fine.
    let bare: WakeRelayInfo = serde_json::from_value(json!({
        "enabled": false, "target_id": "x", "epoch": 0, "later": 1
    }))
    .unwrap();
    assert!(bare.bundle.is_none() && bare.relays.is_empty());
}

#[test]
fn relay_assignments_round_trip() {
    let enrolled: RelayEnrolled = round_trip("wake_relay_enrolled");
    let assignment = enrolled.assignment.unwrap();
    assert_eq!(assignment.macs, vec!["10:7b:44:0a:0b:0c".to_owned()]);
    assert_eq!(assignment.refresh_seconds, 300);
}

#[test]
fn enroll_requests_are_strict() {
    let enroll: RelayEnroll =
        serde_json::from_str(r#"{"action":"enroll","name":"Living Room NUC"}"#).unwrap();
    assert_eq!(
        enroll,
        RelayEnroll::Enroll {
            name: Some("Living Room NUC".into())
        }
    );
    assert_eq!(
        serde_json::from_str::<RelayEnroll>(r#"{"action":"enroll"}"#).unwrap(),
        RelayEnroll::Enroll { name: None }
    );
    assert_eq!(
        serde_json::from_str::<RelayEnroll>(r#"{"action":"rotate_epoch"}"#).unwrap(),
        RelayEnroll::RotateEpoch
    );
    assert_eq!(
        serde_json::from_str::<RelayEnroll>(r#"{"action":"refresh"}"#).unwrap(),
        RelayEnroll::Refresh
    );
    assert_eq!(
        serde_json::to_value(RelayEnroll::Revoke {
            relay: "ab".into()
        })
        .unwrap(),
        json!({"action": "revoke", "relay": "ab"})
    );
    // A typo, an unknown action, or a MAC smuggled in fails loudly.
    for wrong in [
        r#"{"action":"enrol"}"#,
        r#"{"action":"enroll","mac":"aa:bb:cc:dd:ee:ff"}"#,
        r#"{"action":"revoke"}"#,
        r#"{}"#,
    ] {
        assert!(serde_json::from_str::<RelayEnroll>(wrong).is_err(), "{wrong}");
    }
}
