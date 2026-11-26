//! The Stream Doctor's state for one stream, apart from the stream: when the open card reads the
//! host's doctor, the host's last answer, what the stream may change now, where each pick on the
//! card goes, and the resolution step-down's undo, which lasts this stream only.

use super::*;

/// How often the open card reads the host's doctor.
pub const POLL_EVERY: Duration = Duration::from_secs(1);

/// When to read the host's doctor: while its card is open on a host that offers it, one read at
/// a time, [`POLL_EVERY`] apart.
#[derive(Default)]
pub struct Poll {
    asked: Option<Instant>,
    in_flight: bool,
}

impl Poll {
    /// Starts a read through `fetch` when one is due at `now`, and says whether it did. The card
    /// calls this only while it is drawn; `offered` is whether the host offers the doctor.
    pub fn read(&mut self, offered: bool, now: Instant, fetch: impl FnOnce()) -> bool {
        let recent = self
            .asked
            .is_some_and(|at| now.saturating_duration_since(at) < POLL_EVERY);
        if !offered || self.in_flight || recent {
            return false;
        }
        self.asked = Some(now);
        self.in_flight = true;
        fetch();
        true
    }

    /// The read came back, answered or not.
    pub fn answered(&mut self) {
        self.in_flight = false;
    }
}

/// A pick on the card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Apply(Remedy),
    UndoBitrate,
    UndoResolution,
}

/// What a pick asks of the stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// `POST /stream/doctor/apply`: the host's bitrate step-down.
    HostApply,
    /// `POST /stream/doctor/apply`: the undo of the host's trial with this id.
    HostUndo(String),
    /// The classic trial's step.
    Trial(TrialOffer),
    /// The stream size's mode change, to this mode.
    Mode(Reconfigure),
}

/// The card's state for one stream.
#[derive(Default)]
pub struct DoctorV2 {
    pub poll: Poll,
    /// The host's last answer, kept between reads.
    pub snapshot: Option<DoctorSnapshot>,
    /// The host's step-down or its undo is on its way.
    pub applying: bool,
    /// The mode before the doctor's first resolution step-down, and the mode it last asked for.
    pub(super) resized: Option<(Reconfigure, Reconfigure)>,
    /// The last read's failure, logged once.
    pub(super) read_error: Option<String>,
}

impl DoctorV2 {
    /// The host's bitrate trial Undo returns from, while it may be undone.
    pub fn host_trial(&self) -> Option<&str> {
        let trials = &self.snapshot.as_ref()?.active_trials;
        trials
            .iter()
            .find(|trial| trial.remedy == RemedyId::BitrateStepdown && trial.can_undo)
            .map(|trial| trial.trial_id.as_str())
    }

    /// The host's trial can no longer be undone: something else changed the bitrate since.
    pub fn replaced(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot
                .active_trials
                .iter()
                .any(|trial| trial.remedy == RemedyId::BitrateStepdown && !trial.can_undo)
        })
    }

    /// The mode Undo returns to while the stream still asks for the size the doctor chose
    /// (`mode`, at its current rate); none once the player chose another size.
    pub fn resize_undo(&self, mode: Reconfigure) -> Option<Reconfigure> {
        let (before, asked) = self.resized?;
        ((mode.width, mode.height) == (asked.width, asked.height)).then_some(Reconfigure {
            fps_millihz: mode.fps_millihz,
            ..before
        })
    }

    /// A resolution step-down went out from `before` to `asked`. A second one keeps the first's
    /// `before`: Undo returns to the size the player had.
    pub fn resized(&mut self, before: Reconfigure, asked: Reconfigure) {
        let before = self.resized.map_or(before, |(first, _)| first);
        self.resized = Some((before, asked));
    }

    pub fn resize_undone(&mut self) {
        self.resized = None;
    }

    /// What the stream can change now. A host with the doctor decides the bitrate step-down
    /// (its remedy is configurable), never while this device's own controllers own the bitrate,
    /// another request runs or the classic trial is in force; elsewhere the classic trial's offer
    /// (`trial`) decides.
    pub fn abilities(
        &self,
        trial: TrialOffer,
        automatic: bool,
        busy: bool,
        stream_kbps: u32,
        resize: bool,
    ) -> Abilities {
        let snapshot = self.snapshot.as_ref();
        let host = snapshot.and_then(|s| {
            s.eligible_remedies
                .iter()
                .find(|remedy| remedy.id == RemedyId::BitrateStepdown)
        });
        let bitrate = !self.applying
            && match host {
                Some(remedy) => {
                    remedy.configurable && !automatic && !busy && trial != TrialOffer::Undo
                }
                None => trial == TrialOffer::Try && self.host_trial().is_none(),
            };
        let live = snapshot
            .and_then(|s| s.encoder.as_ref())
            .map_or(0, |encoder| encoder.live_bitrate_kbps);
        Abilities {
            bitrate,
            live_kbps: if live > 0 { live } else { stream_kbps },
            resize: resize && !self.applying,
        }
    }

    /// The step `pick` takes, with the stream's mode (`mode`) and the classic trial's offer
    /// (`trial`); none when it no longer applies.
    pub fn step(&self, pick: Pick, mode: Reconfigure, trial: TrialOffer) -> Option<Step> {
        match pick {
            Pick::Apply(Remedy::BitrateStepdown { on_host: true, .. }) => Some(Step::HostApply),
            Pick::Apply(Remedy::BitrateStepdown { .. }) => {
                (trial == TrialOffer::Try).then_some(Step::Trial(TrialOffer::Try))
            }
            Pick::Apply(Remedy::ResolutionStepdown { width, height, .. }) => {
                Some(Step::Mode(Reconfigure {
                    width: u16::try_from(width).ok()?,
                    height: u16::try_from(height).ok()?,
                    fps_millihz: mode.fps_millihz,
                }))
            }
            Pick::UndoBitrate => match self.host_trial() {
                Some(id) => Some(Step::HostUndo(id.to_owned())),
                None => (trial == TrialOffer::Undo).then_some(Step::Trial(TrialOffer::Undo)),
            },
            Pick::UndoResolution => self.resize_undo(mode).map(Step::Mode),
        }
    }
}
