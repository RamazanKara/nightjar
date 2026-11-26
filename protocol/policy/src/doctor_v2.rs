//! Stream Doctor 2.0: one plain-language finding from this device's report ([`health::Report`])
//! and, where the host offers `stream.doctor_v2`, the host's own stage attribution
//! ([`DoctorSnapshot`]). It names the stage that cost the stream its smoothness (the network,
//! the PC's capture or encoder, this device's decoder, or the display), the numbers behind it,
//! the remedies the player may try for this stream, and whether an upscaler earns its cost here.
//!
//! Both apps run this (Android through JNI), so they show the same verdict. It decides; it never
//! acts: every remedy is an offer the player accepts, for this stream, with an undo, and nothing
//! here changes a setting. Upscaler guidance names only the spatial upscalers Nightjar ships
//! ([`RecommendedUpscaler`]); a host without the feature still gets a finding from this device's
//! own measurements.
use rubylight_native_api::doctor::{DoctorSnapshot, HostStage, RemedyId};
use serde::Serialize;

use crate::health::{Finding, Report};
use crate::resolution_scale;

/// The host's late pictures name a stage only when they are at least this share of the window's
/// pictures, in per mille: 2 %, a stutter a player sees every second at 60 fps.
pub const HOST_LATE_PER_MILLE: u64 = 20;
/// Below this frame loss a short receive buffer is not what the player sees.
pub const RECEIVE_LOSS_PERCENT: f32 = 0.5;

/// What cost the stream its smoothness, in the order the doctor checks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RootCause {
    /// Too little recent video to judge: the first seconds, or a menu held open.
    Gathering,
    /// Nothing repeated on either side.
    Healthy,
    /// The system granted the video socket less receive buffer than asked, and video was lost.
    ReceiveBufferShort,
    /// The PC's capture, conversion or encoder fell behind.
    HostOverload,
    /// This device's decoder took a frame interval or more, or it is hot.
    DecodePressure,
    /// Loss, jitter or bunched frames between the PC and this device.
    NetworkJitter,
    /// Frames were decoded in time but shown late.
    DisplayPacing,
}

/// The stage a finding names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    None,
    Network,
    /// The PC's capture, conversion or encoder.
    HostEncode,
    /// The PC's packetizing and sending, before the network.
    HostSend,
    Decode,
    Present,
}

/// The number a finding stands on, shown beside it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeyNumber {
    None,
    /// The worst frame loss of the window.
    LossPercent {
        percent: f32,
    },
    /// Seconds of the window with frames that arrived bunched.
    LateSeconds {
        late: u32,
        of: u32,
    },
    /// The host's late pictures and the time they lost in `stage`.
    HostLost {
        stage: HostStageName,
        lost_ms: f32,
        late: u64,
        pictures: u64,
    },
    /// The host's present-to-sent p95 against the frame interval, without its stage report.
    HostP95 {
        ms: f32,
        budget_ms: f32,
    },
    DecodeMs {
        ms: f32,
        budget_ms: f32,
    },
    DisplayP95 {
        ms: f32,
        budget_ms: f32,
    },
    /// The receive buffer the system granted, against the one asked for.
    ReceiveBuffer {
        granted_kib: u32,
        asked_kib: u32,
    },
}

/// A host stage by its wire name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HostStageName {
    Capture,
    Convert,
    Encode,
    Packetize,
    Send,
}

/// The spatial upscalers Nightjar ships. Temporal upscaling was retired and has no value here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendedUpscaler {
    None,
    Sgsr1,
    Fsr1,
    /// macOS only.
    MetalFx,
}

/// The upscaler running now, as the app reads its setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveUpscaler {
    Off,
    Sgsr1,
    Fsr1,
    MetalFx,
    /// The GPU vendor's video super resolution: costly, judged like FSR 1.
    Vendor,
}

/// Why no client upscaler applies to this stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpscaleBlocker {
    Hdr,
    TenBit,
    PyroWave,
    /// The stream is at least as large as the screen: there is nothing to upscale.
    NoUpscale,
}

/// Whether the upscaler in use earns its cost on this device and stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum UpscalerVerdict {
    /// None runs, or it runs and nothing speaks against it.
    Fine,
    /// It runs on a stream no smaller than the screen: a pass per frame for no added pixels.
    NotNeeded,
    /// The display or decoder falls behind with it on: a lighter one, or none.
    TooCostly { lighter: RecommendedUpscaler },
    /// It is set, but this stream cannot use it.
    Blocked { blocker: UpscaleBlocker },
}

/// The client platform, for upscaler choices and platform advice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Android,
    Windows,
    Linux,
    MacOs,
}

/// A one-tap remedy for this stream, undone with one tap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "remedy", rename_all = "snake_case")]
pub enum Remedy {
    /// The live bitrate a fifth lower: through the host's doctor when it offers it, else the
    /// classic trial (`health::BitrateTrial`).
    BitrateStepdown { target_kbps: u32, on_host: bool },
    /// The stream's resolution one step down (`resolution_scale`), through the host's mode
    /// change; `upscaler` is what to turn on for the next stream to keep it sharp.
    ResolutionStepdown {
        width: u32,
        height: u32,
        upscaler: RecommendedUpscaler,
    },
}

/// Advice the player follows on their own: a setting for the next stream, or a change outside
/// Nightjar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "advice", rename_all = "snake_case")]
pub enum Advice {
    /// A cable, or a less busy 5 or 6 GHz Wi-Fi channel.
    WiredOrQuieterWifi,
    /// Stalls on a beat of `period_ms`: background Wi-Fi scans.
    PeriodicScans { period_ms: u64, stalls: u32 },
    /// Raise the system's receive buffer limit; Linux gets the command.
    RaiseReceiveBuffer {
        asked_bytes: u32,
        linux_command: bool,
    },
    /// Close other work on the PC, or cap the game's frame rate.
    EaseThePc,
    /// Turn off Sharper colored text (packed 4:4:4), copied through memory on this device.
    FullChromaOff,
    /// Lower the stream's frame rate in Settings.
    LowerFrameRate,
    /// Let the device cool down.
    CoolDown,
    /// Compare frame-pacing modes and the screen's refresh rate.
    FramePacing,
    /// Turn on `upscaler` with a lower stream resolution, from the next stream.
    UseUpscaler { upscaler: RecommendedUpscaler },
}

/// What the stream and this device are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamFacts {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub hdr: bool,
    /// 10-bit SDR.
    pub ten_bit: bool,
    pub pyrowave: bool,
    /// Packed full chroma copied through memory (`pipeline::packed_from_memory`).
    pub packed_from_memory: bool,
    /// The screen or window the stream is drawn into, in pixels.
    pub screen_width: u32,
    pub screen_height: u32,
}

/// What this device can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceFacts {
    pub platform: Platform,
    /// An integrated GPU, a handheld, a battery or a phone: the lighter upscaler.
    pub constrained: bool,
    /// MetalFX probed and nothing blocks it (macOS only).
    pub metalfx: bool,
    pub upscaler: ActiveUpscaler,
    /// The system granted and asked receive buffer, in bytes, when known.
    pub receive_buffer: Option<(u32, u32)>,
}

/// What the stream can change now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Abilities {
    /// The classic trial's conditions hold (`health::BitrateTrial::offer` is `Try`), or the host's
    /// doctor offers its step-down.
    pub bitrate: bool,
    /// The encoder's live bitrate, the base of a step-down.
    pub live_kbps: u32,
    /// The host takes a mode change (0x5532) for this stream.
    pub resize: bool,
}

/// The doctor's verdict for one look.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Diagnosis {
    pub cause: RootCause,
    pub stage: Stage,
    pub key: KeyNumber,
    /// The one-tap remedies, best first.
    pub remedies: Vec<Remedy>,
    pub advice: Vec<Advice>,
    pub upscaler: UpscalerVerdict,
    /// The host's stage report was part of the verdict.
    pub host_measured: bool,
}

fn budget_ms(fps: u32) -> f32 {
    if fps > 0 {
        1000.0 / fps as f32
    } else {
        f32::NAN
    }
}

fn stage_name(stage: HostStage) -> Option<HostStageName> {
    Some(match stage {
        HostStage::Capture => HostStageName::Capture,
        HostStage::Convert => HostStageName::Convert,
        HostStage::Encode => HostStageName::Encode,
        HostStage::Packetize => HostStageName::Packetize,
        HostStage::Send => HostStageName::Send,
        HostStage::Unknown => return None,
    })
}

/// The host stage that lost enough of the window to name, with its numbers.
fn host_attribution(host: Option<&DoctorSnapshot>) -> Option<KeyNumber> {
    let stages = host?.host_stages.as_ref()?;
    let stage = stage_name(stages.dominant_stage?)?;
    if stages.pictures == 0 || stages.late_pictures * 1000 < stages.pictures * HOST_LATE_PER_MILLE {
        return None;
    }
    let lost = &stages.lost_us;
    let lost_us = match stage {
        HostStageName::Capture => lost.capture,
        HostStageName::Convert => lost.convert,
        HostStageName::Encode => lost.encode,
        HostStageName::Packetize => lost.packetize,
        HostStageName::Send => lost.send,
    };
    Some(KeyNumber::HostLost {
        stage,
        lost_ms: lost_us as f32 / 1000.0,
        late: stages.late_pictures,
        pictures: stages.pictures,
    })
}

/// The best spatial upscaler for a stream of `stream` pixels on a `screen`, or why none applies.
pub fn recommend_upscaler(
    stream: &StreamFacts,
    device: &DeviceFacts,
    width: u32,
    height: u32,
) -> Result<RecommendedUpscaler, UpscaleBlocker> {
    if stream.hdr {
        return Err(UpscaleBlocker::Hdr);
    }
    if stream.ten_bit {
        return Err(UpscaleBlocker::TenBit);
    }
    if stream.pyrowave {
        return Err(UpscaleBlocker::PyroWave);
    }
    if stream.screen_width <= width || stream.screen_height <= height {
        return Err(UpscaleBlocker::NoUpscale);
    }
    Ok(match device.platform {
        Platform::MacOs if device.metalfx => RecommendedUpscaler::MetalFx,
        Platform::Android => RecommendedUpscaler::Sgsr1,
        _ if device.constrained => RecommendedUpscaler::Sgsr1,
        _ => RecommendedUpscaler::Fsr1,
    })
}

/// Whether the upscaler in use earns its cost, given the verdict's cause.
pub fn judge_upscaler(
    stream: &StreamFacts,
    device: &DeviceFacts,
    cause: RootCause,
) -> UpscalerVerdict {
    if device.upscaler == ActiveUpscaler::Off {
        return UpscalerVerdict::Fine;
    }
    match recommend_upscaler(stream, device, stream.width, stream.height) {
        Err(UpscaleBlocker::NoUpscale) => UpscalerVerdict::NotNeeded,
        Err(blocker) => UpscalerVerdict::Blocked { blocker },
        Ok(_) if matches!(cause, RootCause::DisplayPacing | RootCause::DecodePressure) => {
            let lighter = match device.upscaler {
                ActiveUpscaler::Fsr1 | ActiveUpscaler::Vendor => RecommendedUpscaler::Sgsr1,
                ActiveUpscaler::Sgsr1 | ActiveUpscaler::MetalFx | ActiveUpscaler::Off => {
                    RecommendedUpscaler::None
                }
            };
            UpscalerVerdict::TooCostly { lighter }
        }
        Ok(_) => UpscalerVerdict::Fine,
    }
}

/// The next smaller stream resolution: the next step of the screen's own (100, 85, 75, 67, 50 %),
/// or the screen's own for a stream larger than the screen. `None` at the smallest step.
pub fn step_down(stream: &StreamFacts) -> Option<(u32, u32)> {
    let (screen_w, screen_h) = (stream.screen_width, stream.screen_height);
    if screen_w == 0 || screen_h == 0 || stream.width == 0 || stream.height == 0 {
        return None;
    }
    let current = u64::from(stream.width) * u64::from(stream.height);
    resolution_scale::SCALES
        .into_iter()
        .map(|percent| resolution_scale::scaled(screen_w, screen_h, percent))
        .find(|&(w, h)| u64::from(w) * u64::from(h) * 100 < current * 97)
        .filter(|&(w, h)| (w, h) != (stream.width, stream.height))
}

/// The doctor's verdict. `host` is the host's snapshot while it offers `stream.doctor_v2`.
pub fn diagnose(
    report: &Report,
    host: Option<&DoctorSnapshot>,
    stream: &StreamFacts,
    device: &DeviceFacts,
    can: &Abilities,
) -> Diagnosis {
    let host_key = host_attribution(host);
    let host_measured = host.is_some_and(|host| host.host_stages.is_some());
    let has = |finding: Finding| report.findings.contains(&finding);
    let budget = budget_ms(stream.fps);
    let host_stage = match host_key {
        Some(KeyNumber::HostLost {
            stage: HostStageName::Packetize | HostStageName::Send,
            ..
        }) => Some(Stage::HostSend),
        Some(_) => Some(Stage::HostEncode),
        None => None,
    };
    let network = has(Finding::Network) || has(Finding::Bursts) || has(Finding::Periodic);
    let short_buffer = device
        .receive_buffer
        .filter(|(granted, asked)| granted < asked);
    let lossy = report.worst_loss_percent >= RECEIVE_LOSS_PERCENT;
    let (cause, stage, key) = if !report.recent || report.samples < 3 {
        (RootCause::Gathering, Stage::None, KeyNumber::None)
    } else if let Some((granted, asked)) = short_buffer
        .filter(|_| (has(Finding::Network) || lossy) && host_stage != Some(Stage::HostEncode))
    {
        let key = KeyNumber::ReceiveBuffer {
            granted_kib: granted.div_ceil(1024),
            asked_kib: asked.div_ceil(1024),
        };
        (RootCause::ReceiveBufferShort, Stage::Network, key)
    } else if host_stage == Some(Stage::HostEncode) {
        (
            RootCause::HostOverload,
            Stage::HostEncode,
            host_key.unwrap_or(KeyNumber::None),
        )
    } else if has(Finding::Decoder) || has(Finding::Heat) {
        let key = KeyNumber::DecodeMs {
            ms: report.worst_decode_ms,
            budget_ms: budget,
        };
        (RootCause::DecodePressure, Stage::Decode, key)
    } else if network || host_stage == Some(Stage::HostSend) {
        let key = if host_stage == Some(Stage::HostSend) {
            host_key.unwrap_or(KeyNumber::None)
        } else if report.worst_loss_percent >= 2.0 {
            KeyNumber::LossPercent {
                percent: report.worst_loss_percent,
            }
        } else if has(Finding::Bursts) {
            KeyNumber::LateSeconds {
                late: report.late_samples as u32,
                of: report.samples as u32,
            }
        } else if report.worst_loss_percent > 0.0 {
            KeyNumber::LossPercent {
                percent: report.worst_loss_percent,
            }
        } else {
            // Jitter or periodic stalls without loss: "0.0 % lost" would be no reason to
            // stand on; the advice carries the stalls' numbers.
            KeyNumber::None
        };
        let stage = host_stage.unwrap_or(Stage::Network);
        (RootCause::NetworkJitter, stage, key)
    } else if has(Finding::Host) {
        // The host's present-to-sent time was long, but it sent no stage report to name one.
        let key = KeyNumber::HostP95 {
            ms: report.worst_host_p95_ms,
            budget_ms: budget,
        };
        (RootCause::HostOverload, Stage::HostEncode, key)
    } else if has(Finding::Presentation) {
        let key = KeyNumber::DisplayP95 {
            ms: report.worst_display_p95_ms,
            budget_ms: budget,
        };
        (RootCause::DisplayPacing, Stage::Present, key)
    } else {
        (RootCause::Healthy, Stage::None, KeyNumber::None)
    };
    let mut remedies = Vec::new();
    let mut advice = Vec::new();
    let on_host = host.is_some_and(|host| {
        host.eligible_remedies
            .iter()
            .any(|r| r.id == RemedyId::BitrateStepdown && r.configurable)
    });
    let bitrate = (can.bitrate && can.live_kbps > 0).then(|| Remedy::BitrateStepdown {
        target_kbps: rubylight_native_api::doctor::TRIAL_FLOOR_KBPS
            .max((u64::from(can.live_kbps) * 4 / 5) as u32),
        on_host,
    });
    let resize = |upscaler| {
        step_down(stream)
            .filter(|_| can.resize)
            .map(|(width, height)| Remedy::ResolutionStepdown {
                width,
                height,
                upscaler,
            })
    };
    let upscaler_for_step = || {
        step_down(stream)
            .and_then(|(w, h)| recommend_upscaler(stream, device, w, h).ok())
            .unwrap_or(RecommendedUpscaler::None)
    };
    match cause {
        RootCause::Gathering | RootCause::Healthy => {}
        RootCause::ReceiveBufferShort => {
            remedies.extend(bitrate);
            if let Some((_, asked)) = short_buffer {
                let linux_command = device.platform == Platform::Linux;
                advice.push(Advice::RaiseReceiveBuffer {
                    asked_bytes: asked,
                    linux_command,
                });
            }
        }
        RootCause::NetworkJitter => {
            remedies.extend(bitrate);
            if report.stall_period_ms > 0 {
                advice.push(Advice::PeriodicScans {
                    period_ms: report.stall_period_ms,
                    stalls: report.stalls as u32,
                });
            }
            advice.push(if stage == Stage::HostSend {
                Advice::EaseThePc
            } else {
                Advice::WiredOrQuieterWifi
            });
        }
        RootCause::HostOverload => {
            let upscaler = upscaler_for_step();
            remedies.extend(resize(upscaler));
            if upscaler != RecommendedUpscaler::None && device.upscaler == ActiveUpscaler::Off {
                advice.push(Advice::UseUpscaler { upscaler });
            }
            advice.push(Advice::EaseThePc);
        }
        RootCause::DecodePressure => {
            if stream.packed_from_memory {
                advice.push(Advice::FullChromaOff);
            }
            remedies.extend(resize(upscaler_for_step()));
            advice.push(Advice::LowerFrameRate);
            if has(Finding::Heat) {
                advice.push(Advice::CoolDown);
            }
        }
        RootCause::DisplayPacing => advice.push(Advice::FramePacing),
    }
    let upscaler = judge_upscaler(stream, device, cause);
    Diagnosis {
        cause,
        stage,
        key,
        remedies,
        advice,
        upscaler,
        host_measured,
    }
}

#[cfg(test)]
mod tests;
