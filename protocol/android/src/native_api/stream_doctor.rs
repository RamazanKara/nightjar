//! Stream Doctor 2.0's native API operations for Java (`NativeApiClient.streamDoctor` and
//! `streamDoctorApply`, used by `StreamDoctorController` while its card is open):
//! `stream_doctor` reads the host's snapshot (`GET /stream/doctor`) and `stream_doctor.apply`
//! applies or undoes one session-only remedy (argument: the `DoctorApplyRequest` JSON). A refusal
//! is the host's API error with its plain-English `message`, so nothing is half applied.
//! [`super::request`] tries these before the rest of [`super::execute`].

use super::{argument_error, ok};
use nightjar_core::http::native_api::doctor::{
    DoctorAction, DoctorApplyRequest, DoctorSnapshot, RemedyId,
};
use nightjar_core::http::{Error, HostClient};

/// The calls of `HostClient` the doctor makes, so the dispatch is testable without a host.
pub trait DoctorHost {
    fn stream_doctor(&self) -> Result<DoctorSnapshot, Error>;
    fn stream_doctor_apply(
        &self,
        action: DoctorAction,
        remedy: RemedyId,
        trial_id: Option<String>,
    ) -> Result<DoctorSnapshot, Error>;
}

impl DoctorHost for HostClient {
    fn stream_doctor(&self) -> Result<DoctorSnapshot, Error> {
        HostClient::stream_doctor(self)
    }
    fn stream_doctor_apply(
        &self,
        action: DoctorAction,
        remedy: RemedyId,
        trial_id: Option<String>,
    ) -> Result<DoctorSnapshot, Error> {
        HostClient::stream_doctor_apply(self, action, remedy, trial_id)
    }
}

/// The envelope for a Stream Doctor operation, or None for any other.
pub fn execute(host: &dyn DoctorHost, op: &str, argument: &str) -> Option<String> {
    Some(match op {
        "stream_doctor" => ok(host.stream_doctor()),
        "stream_doctor.apply" => match serde_json::from_str::<DoctorApplyRequest>(argument) {
            Ok(request) if request.remedy == RemedyId::Unknown => {
                argument_error("stream doctor: unknown remedy".to_owned())
            }
            Ok(request) if request.action == DoctorAction::Undo && request.trial_id.is_none() => {
                argument_error("stream doctor: an undo names its trial".to_owned())
            }
            Ok(request) => {
                ok(host.stream_doctor_apply(request.action, request.remedy, request.trial_id))
            }
            Err(e) => argument_error(format!("stream doctor: {e}")),
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nightjar_core::http::native_api::error::{ApiError, ErrorCode};
    use std::sync::Mutex;

    const SNAPSHOT: &str = include_str!("../../../native-api/tests/fixtures/stream_doctor.json");

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        refuse: bool,
    }

    impl DoctorHost for Fake {
        fn stream_doctor(&self) -> Result<DoctorSnapshot, Error> {
            self.calls.lock().unwrap().push("read".into());
            Ok(serde_json::from_str(SNAPSHOT).unwrap())
        }
        fn stream_doctor_apply(
            &self,
            action: DoctorAction,
            remedy: RemedyId,
            trial_id: Option<String>,
        ) -> Result<DoctorSnapshot, Error> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("{action:?} {remedy:?} {trial_id:?}"));
            if self.refuse {
                return Err(Error::Api {
                    status: ErrorCode::NotWritable.status(),
                    error: Box::new(ApiError::new(
                        ErrorCode::NotWritable,
                        "The bitrate changed since the trial. Undo would replace that change.",
                    )),
                });
            }
            Ok(serde_json::from_str(SNAPSHOT).unwrap())
        }
    }

    fn parse(envelope: Option<String>) -> serde_json::Value {
        serde_json::from_str(&envelope.expect("a Stream Doctor operation")).unwrap()
    }

    #[test]
    fn reads_the_snapshot_and_applies_and_undoes_through_the_host() {
        let fake = Fake::default();
        let read = parse(execute(&fake, "stream_doctor", ""));
        assert_eq!(read["ok"]["caller_is_owner"], true);
        assert_eq!(read["ok"]["host_stages"]["dominant_stage"], "encode");
        assert_eq!(read["ok"]["active_trials"][0]["trial_id"], "t-3");
        let apply = r#"{"action":"apply","remedy":"bitrate_stepdown","trial_id":null}"#;
        assert_eq!(
            parse(execute(&fake, "stream_doctor.apply", apply))["ok"]["schema_version"],
            1
        );
        let undo = r#"{"action":"undo","remedy":"bitrate_stepdown","trial_id":"t-3"}"#;
        parse(execute(&fake, "stream_doctor.apply", undo));
        assert_eq!(
            *fake.calls.lock().unwrap(),
            [
                "read",
                "Apply BitrateStepdown None",
                "Undo BitrateStepdown Some(\"t-3\")"
            ]
        );
    }

    #[test]
    fn a_refusal_keeps_the_hosts_words_and_a_bad_request_never_reaches_the_host() {
        let fake = Fake {
            refuse: true,
            ..Fake::default()
        };
        let undo = r#"{"action":"undo","remedy":"bitrate_stepdown","trial_id":"t-3"}"#;
        let refused = parse(execute(&fake, "stream_doctor.apply", undo));
        assert_eq!(refused["error"]["code"], "not_writable");
        assert_eq!(refused["error"]["http_status"], 422);
        assert!(
            refused["error"]["message"]
                .as_str()
                .unwrap()
                .contains("The bitrate changed since the trial")
        );
        for bad in [
            r#"{"action":"apply","remedy":"fec_auto"}"#,
            r#"{"action":"undo","remedy":"bitrate_stepdown"}"#,
            r#"{"action":"apply","remedy":"bitrate_stepdown","percent":50}"#,
            "",
        ] {
            let answer = parse(execute(&fake, "stream_doctor.apply", bad));
            assert_eq!(answer["error"]["kind"], "invalid_argument", "{bad}");
        }
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn other_operations_go_on_to_the_rest_of_the_dispatch() {
        let fake = Fake::default();
        for op in ["session", "stream_doctor.undo", "doctor", ""] {
            assert_eq!(execute(&fake, op, ""), None, "{op}");
        }
        assert!(fake.calls.lock().unwrap().is_empty());
    }
}
