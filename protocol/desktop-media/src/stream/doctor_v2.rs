//! Stream Doctor 2.0 on the desktop, for one stream: reads the host's doctor
//! (`GET /stream/doctor`) on a worker thread about once a second, only while the doctor's card is
//! open and only from a host that offers `stream.doctor_v2`, and keeps the last answer; puts what
//! the stream knows of itself and this device into `nightjar_policy::doctor_v2::diagnose`; and
//! runs the remedy the player picks on the card: the host's bitrate step-down and its undo, the
//! classic bitrate trial elsewhere (`Streaming::health_trial`), or a resolution step-down through
//! the stream size's own mode change, whose undo lasts this stream only (a reconnect starts a new
//! [`DoctorV2`]). A refused step shows the PC's own words and changes nothing. No saved setting
//! changes here, and the upscaler the doctor names is advice for the next stream: nothing
//! switches the renderer.

use nightjar_policy::doctor_v2::{
    self as policy, Abilities, ActiveUpscaler, DeviceFacts, Diagnosis, Platform, Remedy,
    StreamFacts,
};
use rubylight_native_api::doctor::{DoctorAction, DoctorSnapshot, RemedyId};

use super::*;

mod state;

pub use state::{DoctorV2, POLL_EVERY, Pick, Poll, Step};

/// What a worker brings back.
pub enum Answer {
    /// A read of `GET /stream/doctor`; the error is for the log.
    Snapshot(Result<DoctorSnapshot, String>),
    /// The host's answer to its step-down or the undo; the error is the PC's own words.
    Applied {
        undo: bool,
        result: Result<DoctorSnapshot, String>,
    },
}

/// The platform this build runs on.
pub fn platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(target_os = "linux") {
        Platform::Linux
    } else {
        Platform::Windows
    }
}

/// The upscaler a setting runs, as the doctor judges it. MetalFX runs only on macOS.
pub fn active_upscaler(upscaling: config::Upscaling, macos: bool) -> ActiveUpscaler {
    match upscaling {
        config::Upscaling::Bilinear => ActiveUpscaler::Off,
        config::Upscaling::Fsr1 => ActiveUpscaler::Fsr1,
        config::Upscaling::Sgsr1 => ActiveUpscaler::Sgsr1,
        config::Upscaling::Ai => ActiveUpscaler::Vendor,
        config::Upscaling::MetalFx if macos => ActiveUpscaler::MetalFx,
        config::Upscaling::MetalFx => ActiveUpscaler::Off,
    }
}

/// The text of an error the PC answered: its own plain words when it gave them.
fn plain(error: &HttpError) -> String {
    match error {
        HttpError::Api { error, .. } => error.message.clone(),
        other => other.to_string(),
    }
}

/// What the card shows for one frame (`doctor_card::View`).
pub struct Look {
    pub diagnosis: Diagnosis,
    pub host: Option<DoctorSnapshot>,
    pub owner: bool,
    pub upscaler: ActiveUpscaler,
    pub busy: bool,
    pub undo_bitrate: bool,
    pub undo_resolution: bool,
    pub replaced: bool,
    pub technical: String,
}

impl Streaming {
    /// The host offers its doctor for this stream.
    fn doctor_offered(&self) -> bool {
        self.facts.role != AppRole::InputOnly
            && self.plan.info.capabilities.has(Feature::StreamDoctorV2)
    }

    /// The card for this frame, with the window's size in pixels: reads the host's doctor when
    /// a read is due. Call it only while the card is drawn, so a closed card asks nothing.
    pub fn doctor_look(&mut self, screen: (u32, u32)) -> Look {
        let offered = self.doctor_offered();
        let shared = &self.shared;
        self.doctor_v2.poll.read(offered, Instant::now(), || {
            let Some(client) = lock(&shared.host_client).clone() else {
                shared.notify(Notice::Doctor(Answer::Snapshot(
                    Err("not connected".into()),
                )));
                return;
            };
            let shared = shared.clone();
            std::thread::spawn(move || {
                let result = client.stream_doctor().map_err(|e| e.to_string());
                shared.notify(Notice::Doctor(Answer::Snapshot(result)));
            });
        });
        let report = self
            .doctor
            .report(self.started.elapsed().as_millis() as u64);
        let trial = self.trial_offer(&report);
        let busy = self.bitrate_busy || self.doctor_v2.applying;
        let resize = self.facts.reconfigure && self.quality_mode == QualityMode::Off;
        let stream_kbps = self.facts.bitrate_kbps.max(0) as u32;
        let can =
            self.doctor_v2
                .abilities(trial, self.automatic_bitrate(), busy, stream_kbps, resize);
        let format = self.facts.format;
        let hdr = present::hdr_negotiated(self.settings.hdr, format);
        let packed = *lock(&self.shared.packed_from_memory);
        let stream = StreamFacts {
            width: self.facts.width.max(0) as u32,
            height: self.facts.height.max(0) as u32,
            fps: self.facts.fps.max(0) as u32,
            hdr,
            ten_bit: !hdr && format & VIDEO_FORMAT_MASK_10BIT != 0,
            pyrowave: format & VIDEO_FORMAT_MASK_PYROWAVE != 0,
            packed_from_memory: packed.is_some(),
            screen_width: screen.0,
            screen_height: screen.1,
        };
        // MetalFX is offered where its probe passed and nothing but the stream's size held it back.
        #[cfg(target_os = "macos")]
        let metalfx = metalfx_supported()
            && !self.upscale_failed
            && self
                .metalfx_blocked
                .is_none_or(|why| why == "no upscale needed");
        #[cfg(not(target_os = "macos"))]
        let metalfx = false;
        let bytes = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let device = DeviceFacts {
            platform: platform(),
            constrained: crate::handheld::gamescope(),
            metalfx,
            upscaler: active_upscaler(self.settings.upscaling, cfg!(target_os = "macos")),
            receive_buffer: self
                .doctor
                .receive_buffer
                .map(|b| (bytes(b.granted), bytes(b.asked))),
        };
        let host = self.doctor_v2.snapshot.as_ref().filter(|_| offered);
        let diagnosis = policy::diagnose(&report, host, &stream, &device, &can);
        let mut technical = doctor::report_text(&report);
        let hints = [
            doctor::packed_hint(&report, packed),
            self.doctor.receive_buffer_hint(),
        ];
        for hint in hints.into_iter().flatten() {
            technical.push_str("\n\n");
            technical.push_str(&hint);
        }
        Look {
            diagnosis,
            // A guest only while the PC knows the stream and names another device its owner.
            owner: host.is_none_or(|host| !host.session_active || host.caller_is_owner),
            host: host.cloned(),
            upscaler: device.upscaler,
            busy,
            undo_bitrate: self.doctor_v2.host_trial().is_some() || trial == TrialOffer::Undo,
            undo_resolution: self.doctor_v2.resize_undo(self.quality_ceiling).is_some(),
            replaced: self.doctor_v2.replaced(),
            technical,
        }
    }

    /// Runs what the player picked on the card, if it still applies.
    pub fn doctor_pick(&mut self, pick: Pick) {
        let report = self
            .doctor
            .report(self.started.elapsed().as_millis() as u64);
        let trial = self.trial_offer(&report);
        let Some(step) = self.doctor_v2.step(pick, self.quality_ceiling, trial) else {
            return;
        };
        match step {
            Step::HostApply => self.doctor_host(DoctorAction::Apply, None),
            Step::HostUndo(id) => self.doctor_host(DoctorAction::Undo, Some(id)),
            Step::Trial(offer) => self.health_trial(offer),
            Step::Mode(request) => {
                // As the stream size's own Apply, which automatic quality holds.
                if self.quality_mode != QualityMode::Off {
                    return;
                }
                let before = self.quality_ceiling;
                if !self.set_mode(request) {
                    return;
                }
                if pick == Pick::UndoResolution {
                    self.doctor_v2.resize_undone();
                } else {
                    self.doctor_v2.resized(before, request);
                }
            }
        }
    }

    /// The host's step-down or its undo, on a worker: the host's answer comes back as a notice.
    fn doctor_host(&mut self, action: DoctorAction, trial_id: Option<String>) {
        if self.doctor_v2.applying || self.bitrate_busy {
            return;
        }
        let Some(client) = lock(&self.shared.host_client).clone() else {
            return;
        };
        let shared = self.shared.clone();
        let undo = action == DoctorAction::Undo;
        self.doctor_v2.applying = true;
        self.bitrate_busy = true;
        std::thread::spawn(move || {
            let result = client
                .stream_doctor_apply(action, RemedyId::BitrateStepdown, trial_id)
                .map_err(|e| plain(&e));
            shared.notify(Notice::Doctor(Answer::Applied { undo, result }));
        });
    }

    /// A worker's answer: a read is kept; a step that went through moves the stream's bitrate
    /// to the PC's, and one refused leaves it as it was, with the PC's words as a toast.
    pub(super) fn doctor_answer(&mut self, answer: Answer) {
        match answer {
            Answer::Snapshot(Ok(snapshot)) => {
                self.doctor_v2.poll.answered();
                self.doctor_v2.read_error = None;
                self.doctor_v2.snapshot = Some(snapshot);
            }
            Answer::Snapshot(Err(error)) => {
                self.doctor_v2.poll.answered();
                if self.doctor_v2.read_error.as_ref() != Some(&error) {
                    eprintln!("nightjar: stream doctor: the PC's doctor did not answer: {error}");
                }
                self.doctor_v2.read_error = Some(error);
            }
            Answer::Applied { undo, result } => {
                self.doctor_v2.applying = false;
                self.bitrate_busy = false;
                match result {
                    Ok(snapshot) => {
                        let kbps = if undo {
                            snapshot.encoder.as_ref().map_or(0, |e| e.live_bitrate_kbps)
                        } else {
                            let trials = &snapshot.active_trials;
                            let trial = trials
                                .iter()
                                .find(|t| t.remedy == RemedyId::BitrateStepdown);
                            trial.map_or(0, |t| t.applied_value)
                        };
                        self.doctor_v2.snapshot = Some(snapshot);
                        if kbps > 0 {
                            self.facts.bitrate_kbps = i32::try_from(kbps).unwrap_or(i32::MAX);
                            self.bitrate_kbps = kbps;
                            let now = self.started.elapsed().as_millis() as u64;
                            if let Some(rates) = &mut self.rate_change {
                                rates.bitrate_set(kbps, now);
                            }
                            let mbps = doctor::number(f64::from(kbps) / 1000.0, 1);
                            self.show_toast(trf("PC applied {} Mbps", &[&mbps]));
                        }
                    }
                    Err(message) => {
                        eprintln!(
                            "nightjar: stream doctor: the PC refused the {}: {message}",
                            if undo { "undo" } else { "step-down" }
                        );
                        self.show_toast(message);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
