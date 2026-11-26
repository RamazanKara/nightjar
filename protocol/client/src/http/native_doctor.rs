//! Stream Doctor 2.0 over the native API (`docs/native-api.md`, "Stream Doctor"): which host
//! stage lost this device's stream time, and the host's session-only bitrate step-down, while
//! the host lists `stream.doctor_v2` ([`crate::http::Feature::StreamDoctorV2`]). The apps read
//! it only while their Stream Doctor card is open; `nightjar_policy::doctor_v2` turns it into a
//! finding.

use super::native::{ACTION_READ_TIMEOUT, MAX_JSON_BYTES};
use super::native_api::doctor::{DoctorAction, DoctorApplyRequest, DoctorSnapshot, RemedyId};
use super::{Error, HostClient, READ_TIMEOUT};

impl HostClient {
    /// `GET /stream/doctor`. Needs View or Launch.
    pub fn stream_doctor(&self) -> Result<DoctorSnapshot, Error> {
        self.native("stream/doctor", None, MAX_JSON_BYTES, READ_TIMEOUT)
    }

    /// `POST /stream/doctor/apply`: applies `remedy`, or undoes the trial `trial_id`, for this
    /// stream only. Needs Launch and the stream's ownership; a refusal is the host's error with
    /// its plain-English message.
    pub fn stream_doctor_apply(
        &self,
        action: DoctorAction,
        remedy: RemedyId,
        trial_id: Option<String>,
    ) -> Result<DoctorSnapshot, Error> {
        let request = DoctorApplyRequest {
            action,
            remedy,
            trial_id,
        };
        let body = serde_json::to_vec(&request)
            .map_err(|e| Error::InvalidArgument(format!("stream doctor: {e}")))?;
        self.native(
            "stream/doctor/apply",
            Some(&body),
            MAX_JSON_BYTES,
            ACTION_READ_TIMEOUT,
        )
    }
}
