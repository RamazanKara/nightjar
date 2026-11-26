//! The doctor's reads of the host only while it is open and offered, where each pick goes, and
//! the undos: the host's by its trial, the resolution's for this stream only.
use super::*;
use rubylight_native_api::doctor::{ActiveTrial, EligibleRemedy, EncoderFacts};
use rubylight_native_api::error::{ApiError, ErrorCode};

const MODE: Reconfigure = Reconfigure {
    width: 2560,
    height: 1440,
    fps_millihz: 120_000,
};

fn snapshot(configurable: bool, trials: Vec<ActiveTrial>) -> DoctorSnapshot {
    DoctorSnapshot {
        schema_version: 1,
        session_active: true,
        caller_is_owner: true,
        window_seconds: 30,
        encoder: Some(EncoderFacts {
            backend: "amf".into(),
            codec: "hevc".into(),
            hdr: false,
            width: 2560,
            height: 1440,
            fps: 120,
            live_bitrate_kbps: 45_000,
            fec_percent: 20,
        }),
        host_stages: None,
        active_trials: trials,
        eligible_remedies: vec![EligibleRemedy {
            id: RemedyId::BitrateStepdown,
            configurable,
            target_kbps: configurable.then_some(36_000),
            reason: (!configurable).then(|| "trial_active".into()),
            message: None,
        }],
    }
}

fn trial(can_undo: bool) -> ActiveTrial {
    ActiveTrial {
        trial_id: "t-3".into(),
        remedy: RemedyId::BitrateStepdown,
        applied_value: 36_000,
        previous_value: 45_000,
        can_undo,
    }
}

/// Frames of an open card, 16 ms apart, for `seconds`: how many reads they started. A read is
/// answered `answer_after` frames after it went out.
fn reads(offered: bool, seconds: u64, answer_after: u32) -> u32 {
    let mut poll = Poll::default();
    let start = Instant::now();
    let (mut started, mut waiting) = (0, None);
    for frame in 0..seconds * 1000 / 16 {
        let now = start + Duration::from_millis(frame * 16);
        if waiting == Some(0) {
            poll.answered();
            waiting = None;
        }
        waiting = waiting.map(|left: u32| left - 1);
        if poll.read(offered, now, || started += 1) {
            waiting = Some(answer_after);
        }
    }
    started
}

#[test]
fn a_host_without_the_doctor_is_never_asked() {
    assert_eq!(reads(false, 30, 1), 0);
}

#[test]
fn an_open_card_asks_once_a_second_and_never_twice_at_once() {
    // Answered within a frame: one read a second.
    assert_eq!(reads(true, 10, 1), 10);
    // An answer that takes three seconds holds the next read until it comes.
    assert_eq!(reads(true, 10, 3000 / 16), 4);
}

#[test]
fn a_closed_card_asks_nothing() {
    // The card reads only while it is drawn: frames that do not draw it never call `read`, and
    // the first frame it is drawn again reads at once.
    let mut poll = Poll::default();
    let start = Instant::now();
    let mut started = 0;
    assert!(poll.read(true, start, || started += 1));
    poll.answered();
    // Closed for a minute: nothing is called.
    let reopened = start + Duration::from_secs(60);
    assert!(poll.read(true, reopened, || started += 1));
    assert_eq!(started, 2);
}

#[test]
fn each_remedy_goes_its_own_way() {
    let doctor = DoctorV2::default();
    let on_host = Remedy::BitrateStepdown {
        target_kbps: 36_000,
        on_host: true,
    };
    let classic = Remedy::BitrateStepdown {
        target_kbps: 36_000,
        on_host: false,
    };
    let smaller = Remedy::ResolutionStepdown {
        width: 2176,
        height: 1224,
        upscaler: policy::RecommendedUpscaler::Fsr1,
    };
    assert_eq!(
        doctor.step(Pick::Apply(on_host), MODE, TrialOffer::None),
        Some(Step::HostApply)
    );
    assert_eq!(
        doctor.step(Pick::Apply(classic), MODE, TrialOffer::Try),
        Some(Step::Trial(TrialOffer::Try))
    );
    // The classic trial no longer offered (automatic bitrate started meanwhile): nothing runs.
    assert_eq!(
        doctor.step(Pick::Apply(classic), MODE, TrialOffer::None),
        None
    );
    let request = Reconfigure {
        width: 2176,
        height: 1224,
        fps_millihz: 120_000,
    };
    assert_eq!(
        doctor.step(Pick::Apply(smaller), MODE, TrialOffer::None),
        Some(Step::Mode(request))
    );
    // The classic trial's undo when the host has no trial of its own.
    assert_eq!(
        doctor.step(Pick::UndoBitrate, MODE, TrialOffer::Undo),
        Some(Step::Trial(TrialOffer::Undo))
    );
    assert_eq!(doctor.step(Pick::UndoBitrate, MODE, TrialOffer::None), None);
}

#[test]
fn the_hosts_trial_is_undone_by_its_id_only_while_it_may_be() {
    let mut doctor = DoctorV2 {
        snapshot: Some(snapshot(false, vec![trial(true)])),
        ..DoctorV2::default()
    };
    assert_eq!(
        doctor.step(Pick::UndoBitrate, MODE, TrialOffer::None),
        Some(Step::HostUndo("t-3".into()))
    );
    assert!(!doctor.replaced());
    // The player changed the bitrate since: Undo would clobber it, so there is none.
    doctor.snapshot = Some(snapshot(false, vec![trial(false)]));
    assert_eq!(doctor.host_trial(), None);
    assert!(doctor.replaced());
    assert_eq!(doctor.step(Pick::UndoBitrate, MODE, TrialOffer::None), None);
}

#[test]
fn a_resolution_step_down_is_undone_to_the_size_before_the_first() {
    let mut doctor = DoctorV2::default();
    let first = Reconfigure {
        width: 2176,
        height: 1224,
        ..MODE
    };
    let second = Reconfigure {
        width: 1920,
        height: 1080,
        ..MODE
    };
    doctor.resized(MODE, first);
    assert_eq!(doctor.resize_undo(first), Some(MODE));
    doctor.resized(first, second);
    assert_eq!(
        doctor.step(Pick::UndoResolution, second, TrialOffer::None),
        Some(Step::Mode(MODE))
    );
    // At another rate since: the size comes back at the rate the stream has now.
    let slower = Reconfigure {
        fps_millihz: 60_000,
        ..second
    };
    assert_eq!(
        doctor.resize_undo(slower),
        Some(Reconfigure {
            fps_millihz: 60_000,
            ..MODE
        })
    );
    // The player chose another size: Undo would undo theirs, so there is none.
    let theirs = Reconfigure {
        width: 1600,
        height: 900,
        ..MODE
    };
    assert_eq!(
        doctor.step(Pick::UndoResolution, theirs, TrialOffer::None),
        None
    );
    doctor.resize_undone();
    assert_eq!(doctor.resize_undo(second), None);
}

#[test]
fn a_new_stream_starts_without_the_last_ones_undo() {
    // A reconnect builds a new stream, and with it a new doctor.
    let mut before = DoctorV2::default();
    before.resized(
        MODE,
        Reconfigure {
            width: 1920,
            height: 1080,
            ..MODE
        },
    );
    let after = DoctorV2::default();
    let asked = Reconfigure {
        width: 1920,
        height: 1080,
        ..MODE
    };
    assert!(before.resize_undo(asked).is_some());
    assert_eq!(
        after.step(Pick::UndoResolution, asked, TrialOffer::None),
        None
    );
    assert!(after.snapshot.is_none() && after.host_trial().is_none());
}

#[test]
fn a_host_with_the_doctor_decides_the_bitrate_step_down() {
    let host = DoctorV2 {
        snapshot: Some(snapshot(true, Vec::new())),
        ..DoctorV2::default()
    };
    let can = host.abilities(TrialOffer::None, false, false, 40_000, true);
    assert!(can.bitrate && can.resize);
    assert_eq!(
        can.live_kbps, 45_000,
        "the encoder's live bitrate, as the host reports it"
    );
    // Never while this device's automatic bitrate owns it, or another request runs.
    assert!(
        !host
            .abilities(TrialOffer::Try, true, false, 40_000, true)
            .bitrate
    );
    assert!(
        !host
            .abilities(TrialOffer::Try, false, true, 40_000, true)
            .bitrate
    );
    // Nor on top of the classic trial while it is in force: its Undo comes first.
    assert!(
        !host
            .abilities(TrialOffer::Undo, false, false, 40_000, true)
            .bitrate
    );
    // The host's trial runs: no second one, from the host or the classic trial.
    let trying = DoctorV2 {
        snapshot: Some(snapshot(false, vec![trial(true)])),
        ..DoctorV2::default()
    };
    assert!(
        !trying
            .abilities(TrialOffer::Try, false, false, 40_000, true)
            .bitrate
    );
    // Without the host's doctor the classic trial's offer decides.
    let classic = DoctorV2::default();
    let can = classic.abilities(TrialOffer::Try, false, false, 40_000, false);
    assert!(can.bitrate && !can.resize);
    assert_eq!(can.live_kbps, 40_000);
    assert!(
        !classic
            .abilities(TrialOffer::None, false, false, 40_000, true)
            .bitrate
    );
    // While a step is on its way, nothing else is offered.
    let applying = DoctorV2 {
        applying: true,
        ..DoctorV2::default()
    };
    let can = applying.abilities(TrialOffer::Try, false, false, 40_000, true);
    assert!(!can.bitrate && !can.resize);
}

#[test]
fn a_refusal_shows_the_pcs_own_words() {
    let refused = HttpError::Api {
        status: 422,
        error: Box::new(ApiError::new(
            ErrorCode::NotWritable,
            "The bitrate changed since the trial, so it was kept.",
        )),
    };
    assert_eq!(
        plain(&refused),
        "The bitrate changed since the trial, so it was kept."
    );
}

#[test]
fn metalfx_is_judged_only_on_macos() {
    use config::Upscaling;
    assert_eq!(
        active_upscaler(Upscaling::MetalFx, true),
        ActiveUpscaler::MetalFx
    );
    assert_eq!(
        active_upscaler(Upscaling::MetalFx, false),
        ActiveUpscaler::Off
    );
    assert_eq!(
        active_upscaler(Upscaling::Bilinear, true),
        ActiveUpscaler::Off
    );
    assert_eq!(
        active_upscaler(Upscaling::Ai, false),
        ActiveUpscaler::Vendor
    );
    assert_eq!(
        active_upscaler(Upscaling::Sgsr1, false),
        ActiveUpscaler::Sgsr1
    );
    assert_eq!(platform() == Platform::MacOs, cfg!(target_os = "macos"));
}
