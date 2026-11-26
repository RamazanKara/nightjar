//! Stream DVR's native calls against the mock host of `native_api.rs`: the state, a clip,
//! a recording's stop and the clip list.

use super::*;

#[test]
fn stream_dvr_reads_the_state_saves_records_and_lists_clips() {
    use nightjar_core::http::native_api::dvr::{ClipList, DvrState, RecordAction, RecordResult};
    let host = serve(|method, path, body| match (method, path) {
        ("GET", "/api/native/v1/dvr") => (200, fixture("dvr")),
        ("POST", "/api/native/v1/dvr/clip") if body == b"{}" => {
            let clip: RecordResult = serde_json::from_str(&fixture("dvr_record")).unwrap();
            (200, json!({ "clip": clip.clip }).to_string())
        }
        ("POST", "/api/native/v1/dvr/clip") => (409, error_body(ErrorCode::DvrRefused, "pyrowave")),
        ("POST", "/api/native/v1/dvr/record") => (200, fixture("dvr_record")),
        ("GET", "/api/native/v1/dvr/clips") => (200, fixture("dvr_clips")),
        other => (404, error_body(ErrorCode::NotFound, &format!("{other:?}"))),
    });
    let client = paired_client(&host);
    let status = client.dvr_status().unwrap();
    assert_eq!(
        (status.state, status.ring_capacity_secs),
        (DvrState::Armed, 60)
    );
    let saved = client.save_dvr_clip(None).unwrap();
    assert_eq!(saved.clip.id, "20261125-210455-manual-9c01");
    match client.save_dvr_clip(Some(30)) {
        Err(Error::Api { status: 409, error }) => assert_eq!(error.code, ErrorCode::DvrRefused),
        other => panic!("{other:?}"),
    }
    let stopped = client.dvr_record(RecordAction::Stop).unwrap();
    assert!(stopped.clip.is_some());
    let list: ClipList = client.dvr_clips(500, Some("a b")).unwrap();
    assert_eq!(list, serde_json::from_str(&fixture("dvr_clips")).unwrap());

    let seen = host.seen.lock().unwrap();
    let bodies: Vec<Value> = seen
        .iter()
        .filter(|s| s.method == "POST")
        .map(|s| serde_json::from_slice(&s.body).unwrap())
        .collect();
    assert_eq!(
        bodies,
        [
            json!({}),
            json!({ "duration_secs": 30 }),
            json!({ "action": "stop" })
        ]
    );
    let clips = seen.last().unwrap();
    assert!(
        clips.query.starts_with("limit=200&cursor=a%20b"),
        "{}",
        clips.query
    );
}
