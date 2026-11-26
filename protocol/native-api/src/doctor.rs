//! `GET /stream/doctor` and `POST /stream/doctor/apply` (Stream Doctor 2.0),
//! while the host lists [`crate::feature::STREAM_DOCTOR_V2`]: which host stage
//! lost the caller's stream time over the last [`WINDOW_SECONDS`], the facts
//! of its encoder, and the session-only remedies the host can apply and undo.
//!
//! The host reads what it already measures (the stutter report's stage
//! attribution and the pipeline frame rates); nothing here is sampled per
//! frame for the doctor. The client fuses it with its own network, decode and
//! display measurements (`nightjar_policy::doctor_v2`).
use serde::{Deserialize, Serialize};

/// [`DoctorSnapshot::schema_version`] of this crate.
pub const SCHEMA_VERSION: u32 = 1;
/// The span the host's stage attribution covers, as the client's own report.
pub const WINDOW_SECONDS: u32 = 30;
/// A bitrate step-down never goes below this, as the client's own trial.
pub const TRIAL_FLOOR_KBPS: u32 = 500;

/// A stage of the host's path, in path order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostStage {
    /// The capture's age when the picture was taken, its wait included.
    Capture,
    /// Colour conversion up to the encoder accepting the picture.
    Convert,
    /// The encoder's work.
    Encode,
    /// The host's queue and packetizing.
    Packetize,
    /// The paced sender and Windows' send calls.
    Send,
    /// A stage from a newer host.
    #[serde(other)]
    Unknown,
}

/// A remedy the host applies and undoes for the caller's stream only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemedyId {
    /// The live encoder bitrate a fifth lower, not below [`TRIAL_FLOOR_KBPS`].
    BitrateStepdown,
    /// A remedy from a newer host; a request naming it is refused.
    #[serde(other)]
    Unknown,
}

/// `GET /stream/doctor`, and the answer of `POST /stream/doctor/apply`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorSnapshot {
    pub schema_version: u32,
    /// The caller has a stream on this host (as owner or guest).
    pub session_active: bool,
    /// The caller owns the game's stream: only the owner applies host remedies.
    pub caller_is_owner: bool,
    pub window_seconds: u32,
    /// Null without a stream.
    pub encoder: Option<EncoderFacts>,
    /// Null without a stream or before the first window of pictures.
    pub host_stages: Option<HostStages>,
    #[serde(default)]
    pub active_trials: Vec<ActiveTrial>,
    #[serde(default)]
    pub eligible_remedies: Vec<EligibleRemedy>,
}

/// The stream's encoder as it runs now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncoderFacts {
    /// The host's encoder name (`amf`, `nvenc`, `pyrowave`, or one from a newer host).
    pub backend: String,
    /// `h264`, `hevc`, `av1` or `pyrowave`.
    pub codec: String,
    pub hdr: bool,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// The encoder's live bitrate, 0 when unknown.
    pub live_bitrate_kbps: u32,
    /// The video FEC share of the next frame.
    pub fec_percent: u32,
}

/// Which host stage lost time over the window, from the stutter report.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStages {
    /// New pictures measured in the window.
    pub pictures: u64,
    /// Pictures at least half a frame later than the stream's usual path.
    pub late_pictures: u64,
    /// The stage with the most lost time, null when too little was lost.
    pub dominant_stage: Option<HostStage>,
    /// Time the late pictures lost, per stage, in microseconds.
    pub lost_us: StageTimes,
    /// The pipeline frame rates over the last second, null before the first.
    pub pipeline_rates: Option<PipelineRates>,
}

/// One value per [`HostStage`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageTimes {
    pub capture: u64,
    pub convert: u64,
    pub encode: u64,
    pub packetize: u64,
    pub send: u64,
}

/// Frames per second in mHz, null for a stage the host did not count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineRates {
    /// Frames the game or desktop presented.
    pub source_mhz: Option<u32>,
    pub capture_mhz: Option<u32>,
    /// Frames submitted to the encoder.
    pub submit_mhz: Option<u32>,
    /// Encoded frames sent.
    pub output_mhz: Option<u32>,
    /// Frames resent without new content.
    pub repeat_mhz: Option<u32>,
}

/// A remedy in force for this stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveTrial {
    pub trial_id: String,
    pub remedy: RemedyId,
    /// For [`RemedyId::BitrateStepdown`], kbps.
    pub applied_value: u32,
    pub previous_value: u32,
    /// False once something else changed the same setting: undo would clobber it.
    pub can_undo: bool,
}

/// A remedy the host can apply now, or why it cannot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EligibleRemedy {
    pub id: RemedyId,
    pub configurable: bool,
    /// The bitrate a [`RemedyId::BitrateStepdown`] asks for.
    #[serde(default)]
    pub target_kbps: Option<u32>,
    /// Why not, by a stable name (`not_owner`, `permission_required`,
    /// `at_minimum`, `trial_active`, `bitrate_unknown`, or one from a newer
    /// host); null when configurable.
    #[serde(default)]
    pub reason: Option<String>,
    /// The same in plain English, null when configurable.
    #[serde(default)]
    pub message: Option<String>,
}

/// Apply or undo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorAction {
    Apply,
    Undo,
}

/// The body of `POST /stream/doctor/apply`; an unknown field is refused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoctorApplyRequest {
    pub action: DoctorAction,
    pub remedy: RemedyId,
    /// The trial to undo; null for apply.
    #[serde(default)]
    pub trial_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_hosts_stage_and_remedy_still_parse() {
        let snapshot: DoctorSnapshot = serde_json::from_str(
            r#"{"schema_version":2,"session_active":true,"caller_is_owner":false,
            "window_seconds":30,"encoder":null,"host_stages":{"pictures":5,"late_pictures":1,
            "dominant_stage":"scanout","lost_us":{"capture":0,"convert":0,"encode":0,
            "packetize":0,"send":0},"pipeline_rates":null},
            "eligible_remedies":[{"id":"fec_auto","configurable":true}],"extra":1}"#,
        )
        .unwrap();
        let stages = snapshot.host_stages.unwrap();
        assert_eq!(stages.dominant_stage, Some(HostStage::Unknown));
        assert_eq!(snapshot.eligible_remedies[0].id, RemedyId::Unknown);
        assert!(snapshot.active_trials.is_empty());
    }

    #[test]
    fn a_request_refuses_unknown_fields() {
        let undo: DoctorApplyRequest = serde_json::from_str(
            r#"{"action":"undo","remedy":"bitrate_stepdown","trial_id":"t-1"}"#,
        )
        .unwrap();
        assert_eq!(undo.action, DoctorAction::Undo);
        assert!(
            serde_json::from_str::<DoctorApplyRequest>(
                r#"{"action":"apply","remedy":"bitrate_stepdown","percent":50}"#
            )
            .is_err()
        );
        let apply: DoctorApplyRequest =
            serde_json::from_str(r#"{"action":"apply","remedy":"bitrate_stepdown"}"#).unwrap();
        assert_eq!(apply.trial_id, None);
    }
}
