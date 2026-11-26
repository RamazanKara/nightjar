//! The words of the Stream Doctor card ([`super::show`]): the name of the stage a finding of
//! `nightjar_policy::doctor_v2` points at, the number behind it, its two plain sentences, the
//! remedies' buttons, the advice, the upscaler guidance and the host's stage numbers for the
//! technical drawer. The English literals are the keys of `strings_70/<code>.rs`; numbers take
//! the language's decimal comma and digit grouping. Upscalers are named by their product names,
//! the same in every language.

use nightjar_policy::doctor_v2::{
    ActiveUpscaler, Advice, Diagnosis, HostStageName, KeyNumber, RecommendedUpscaler, Remedy,
    RootCause, Stage, UpscaleBlocker, UpscalerVerdict,
};
use rubylight_native_api::doctor::DoctorSnapshot;

use crate::strings::{Lang, current, tr, trf};

/// `value` with `decimals`, with a decimal comma where the language writes one; "no data yet"
/// for a value the stream has not measured.
pub fn decimal(value: f32, decimals: usize) -> String {
    if !value.is_finite() || value < 0.0 {
        return tr("no data yet").to_owned();
    }
    let text = format!("{value:.decimals$}");
    if current().decimal_comma() {
        text.replace('.', ",")
    } else {
        text
    }
}

/// A whole number with its thousands grouped as the language groups them: 1,800, 1.800 or 1 800.
pub fn whole(value: u64) -> String {
    let separator = match current() {
        Lang::En | Lang::Ja | Lang::Ko | Lang::ZhHans | Lang::ZhHant => ',',
        Lang::Fr | Lang::Pl | Lang::Ru | Lang::Uk => '\u{a0}',
        Lang::De | Lang::Es | Lang::It | Lang::PtBr | Lang::Tr | Lang::Vi => '.',
    };
    let digits = value.to_string();
    let mut text = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            text.push(separator);
        }
        text.push(digit);
    }
    text
}

/// Megabits per second of `kbps`: whole when it is ("36"), else to one decimal ("4.5").
pub fn mbps(kbps: u32) -> String {
    if kbps.is_multiple_of(1000) {
        whole(u64::from(kbps / 1000))
    } else {
        decimal(kbps as f32 / 1000.0, 1)
    }
}

/// The card's title: the stage that cost the stream its smoothness, or that nothing did.
pub fn title(diagnosis: &Diagnosis) -> &'static str {
    match (diagnosis.cause, diagnosis.stage) {
        (RootCause::Gathering, _) => tr("Gathering a few seconds of video…"),
        (RootCause::Healthy, _) | (_, Stage::None) => tr("Your stream is running smoothly"),
        (_, Stage::Network) => tr("Wi-Fi or network"),
        (_, Stage::HostEncode) => tr("The PC’s encoder"),
        (_, Stage::HostSend) => tr("The PC’s network sending"),
        (_, Stage::Decode) => tr("This device’s decoder"),
        (_, Stage::Present) => tr("The display"),
    }
}

/// What happened and what usually helps, in two plain sentences.
pub fn explanation(diagnosis: &Diagnosis) -> &'static str {
    match diagnosis.cause {
        RootCause::Gathering => {
            tr("Play for a few seconds with this menu closed, then come back just after a stutter.")
        }
        RootCause::Healthy => tr(
            "Nothing repeated in the last 30 seconds. A single short spike can still cause a stutter now and then.",
        ),
        RootCause::NetworkJitter if diagnosis.stage == Stage::HostSend => tr(
            "The PC sent finished frames late, so they reached this device unevenly. Closing other work on the PC or a slightly lower bitrate usually helps.",
        ),
        RootCause::NetworkJitter => tr(
            "Your Wi-Fi or network delivers frames in uneven bursts or loses some, which causes brief stutters. A slightly lower bitrate usually smooths this out.",
        ),
        RootCause::HostOverload => tr(
            "Your PC’s GPU or encoder fell behind the game’s frame rate. A lower stream resolution takes load off the PC.",
        ),
        RootCause::DecodePressure => tr(
            "This device’s video decoder took longer than one frame to finish a picture. A lower stream resolution or frame rate keeps motion smooth.",
        ),
        RootCause::ReceiveBufferShort => tr(
            "Your system capped the network receive buffer below what high-bitrate video needs, so bursts of packets were dropped. A larger limit, or a slightly lower bitrate, stops the loss.",
        ),
        RootCause::DisplayPacing => tr(
            "Frames arrived and were decoded in time, but the screen showed them late. Another frame-pacing mode or the screen’s refresh rate usually fixes this.",
        ),
    }
}

/// The number the finding stands on, in one line; None when it has none.
pub fn key_line(key: &KeyNumber) -> Option<String> {
    let ms = |value: f32| decimal(value, 1);
    Some(match *key {
        KeyNumber::None => return None,
        KeyNumber::LossPercent { percent } => trf("{} % of frames lost", &[&decimal(percent, 1)]),
        KeyNumber::LateSeconds { late, of } => trf(
            "Frames arrived bunched in {} of the last {} seconds",
            &[&late, &of],
        ),
        KeyNumber::HostLost {
            stage,
            lost_ms,
            late,
            pictures,
        } => {
            let (late, pictures) = (whole(late), whole(pictures));
            let lost = whole(lost_ms.max(0.0).round() as u64);
            let args: [&dyn std::fmt::Display; 3] = [&late, &pictures, &lost];
            match stage {
                HostStageName::Capture => trf(
                    "{} of {} pictures left the PC late, {} ms lost capturing",
                    &args,
                ),
                HostStageName::Convert => trf(
                    "{} of {} pictures left the PC late, {} ms lost converting colors",
                    &args,
                ),
                HostStageName::Encode => trf(
                    "{} of {} pictures left the PC late, {} ms lost encoding",
                    &args,
                ),
                HostStageName::Packetize => trf(
                    "{} of {} pictures left the PC late, {} ms lost packetizing",
                    &args,
                ),
                HostStageName::Send => trf(
                    "{} of {} pictures left the PC late, {} ms lost sending",
                    &args,
                ),
            }
        }
        KeyNumber::HostP95 {
            ms: took,
            budget_ms,
        } => trf(
            "The PC took up to {} ms per frame; a frame lasts {} ms",
            &[&ms(took), &ms(budget_ms)],
        ),
        KeyNumber::DecodeMs {
            ms: took,
            budget_ms,
        } => trf(
            "Decoding took up to {} ms; a frame lasts {} ms",
            &[&ms(took), &ms(budget_ms)],
        ),
        KeyNumber::DisplayP95 {
            ms: took,
            budget_ms,
        } => trf(
            "Frames waited up to {} ms to be shown; a frame lasts {} ms",
            &[&ms(took), &ms(budget_ms)],
        ),
        KeyNumber::ReceiveBuffer {
            granted_kib,
            asked_kib,
        } => trf(
            "The system granted {} KiB of the {} KiB receive buffer asked for",
            &[&whole(u64::from(granted_kib)), &whole(u64::from(asked_kib))],
        ),
    })
}

/// A remedy's button, with its target and that it holds for this stream only.
pub fn remedy(remedy: &Remedy) -> String {
    match *remedy {
        Remedy::BitrateStepdown { target_kbps, .. } => trf(
            "Lower bitrate to {} Mbps (this stream)",
            &[&mbps(target_kbps)],
        ),
        Remedy::ResolutionStepdown { width, height, .. } => trf(
            "Lower resolution to {}×{} (this stream)",
            &[&width, &height],
        ),
    }
}

/// A stall rhythm: seconds below 90 s, else minutes to the half minute.
fn period(period_ms: u64) -> String {
    let seconds = (period_ms + 500) / 1000;
    if seconds < 90 {
        return trf("{} s", &[&seconds]);
    }
    let minutes = (seconds as f32 / 30.0).round() / 2.0;
    let minutes = if minutes.fract() == 0.0 {
        format!("{minutes:.0}")
    } else {
        decimal(minutes, 1)
    };
    trf("{} min", &[&minutes])
}

/// One line of advice the player follows on their own.
pub fn advice(advice: &Advice) -> String {
    match *advice {
        Advice::WiredOrQuieterWifi => {
            tr("Use a cable, or a less busy 5 or 6 GHz Wi-Fi channel.").to_owned()
        }
        Advice::PeriodicScans { period_ms, stalls } => trf(
            "{} stalls came about every {}: background Wi-Fi scans on this device or the PC cause a rhythm like this.",
            &[&stalls, &period(period_ms)],
        ),
        Advice::RaiseReceiveBuffer {
            asked_bytes,
            linux_command: true,
        } => trf(
            "Raise the system’s receive buffer limit with:\nsudo sysctl -w net.core.rmem_max={}",
            &[&asked_bytes],
        ),
        Advice::RaiseReceiveBuffer { asked_bytes, .. } => trf(
            "Allow a larger network receive buffer on this device: {} KiB were asked for.",
            &[&whole(u64::from(asked_bytes.div_ceil(1024)))],
        ),
        Advice::EaseThePc => {
            tr("Close other work on the PC, or cap the game’s frame rate.").to_owned()
        }
        Advice::FullChromaOff => tr(
            "Turn off Sharper colored text: this device copies each picture through memory, and that takes longer than a frame.",
        )
        .to_owned(),
        Advice::LowerFrameRate => {
            tr("Lower the stream’s frame rate in Settings for the next stream.").to_owned()
        }
        Advice::CoolDown => {
            tr("Let this device cool down: it reported high thermal pressure.").to_owned()
        }
        Advice::FramePacing => {
            tr("Compare frame-pacing modes and the screen’s refresh rate in Settings.").to_owned()
        }
        Advice::UseUpscaler { upscaler } => trf(
            "For the next stream: a lower resolution with {} keeps the picture sharp.",
            &[&recommended_name(upscaler)],
        ),
    }
}

/// A recommended upscaler's product name.
pub fn recommended_name(upscaler: RecommendedUpscaler) -> &'static str {
    match upscaler {
        RecommendedUpscaler::None => "",
        RecommendedUpscaler::Sgsr1 => "SGSR 1",
        RecommendedUpscaler::Fsr1 => "FSR 1",
        RecommendedUpscaler::MetalFx => "MetalFX",
    }
}

/// The name of the upscaler the settings run.
pub fn active_name(upscaler: ActiveUpscaler) -> &'static str {
    match upscaler {
        ActiveUpscaler::Off => "",
        ActiveUpscaler::Sgsr1 => "SGSR 1",
        ActiveUpscaler::Fsr1 => "FSR 1",
        ActiveUpscaler::MetalFx => "MetalFX",
        ActiveUpscaler::Vendor => tr("AI enhancement"),
    }
}

/// The one line of upscaler guidance, when the upscaler in use does not earn its cost here.
pub fn upscaler(verdict: &UpscalerVerdict, active: ActiveUpscaler) -> Option<String> {
    let name = active_name(active);
    Some(match *verdict {
        UpscalerVerdict::Fine => return None,
        UpscalerVerdict::NotNeeded
        | UpscalerVerdict::Blocked {
            blocker: UpscaleBlocker::NoUpscale,
        } => trf(
            "{} costs more than it gives here: the stream is already as large as the screen.",
            &[&name],
        ),
        UpscalerVerdict::TooCostly {
            lighter: RecommendedUpscaler::None,
        } => trf(
            "{} costs more than it gives here: the screen or decoder falls behind with it on. Turn it off for the next stream.",
            &[&name],
        ),
        UpscalerVerdict::TooCostly { lighter } => trf(
            "{} costs more than it gives here: the screen or decoder falls behind with it on. {} is lighter for the next stream.",
            &[&name, &recommended_name(lighter)],
        ),
        UpscalerVerdict::Blocked {
            blocker: UpscaleBlocker::Hdr,
        } => trf(
            "{} does not run on this stream: HDR video is shown without an upscaler.",
            &[&name],
        ),
        UpscalerVerdict::Blocked {
            blocker: UpscaleBlocker::TenBit,
        } => trf(
            "{} does not run on this stream: 10-bit video is shown without an upscaler.",
            &[&name],
        ),
        UpscalerVerdict::Blocked {
            blocker: UpscaleBlocker::PyroWave,
        } => trf(
            "{} does not run on this stream: PyroWave video is shown without an upscaler.",
            &[&name],
        ),
    })
}

/// The host's own numbers for the technical drawer: its encoder, its late pictures and the time
/// each stage lost, and its frame rates. None while it sent none.
pub fn host_lines(host: &DoctorSnapshot) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(encoder) = &host.encoder {
        let (backend, codec) = (encoder.backend.to_uppercase(), encoder.codec.to_uppercase());
        lines.push(if encoder.live_bitrate_kbps > 0 {
            trf(
                "PC encoder: {} {}, {} Mbps",
                &[&backend, &codec, &mbps(encoder.live_bitrate_kbps)],
            )
        } else {
            trf("PC encoder: {} {}", &[&backend, &codec])
        });
    }
    if let Some(stages) = &host.host_stages {
        lines.push(trf(
            "PC over the last {} s: {} of {} pictures late",
            &[
                &host.window_seconds,
                &whole(stages.late_pictures),
                &whole(stages.pictures),
            ],
        ));
        let ms = |us: u64| decimal(us as f32 / 1000.0, 1);
        let lost = &stages.lost_us;
        lines.push(trf(
            "Time lost: capture {} ms, color conversion {} ms, encoding {} ms, packetizing {} ms, sending {} ms",
            &[
                &ms(lost.capture),
                &ms(lost.convert),
                &ms(lost.encode),
                &ms(lost.packetize),
                &ms(lost.send),
            ],
        ));
        if let Some(rates) = &stages.pipeline_rates {
            let fps = |mhz: Option<u32>| {
                mhz.map_or_else(|| "–".to_owned(), |mhz| decimal(mhz as f32 / 1000.0, 1))
            };
            lines.push(trf(
                "Frames per second on the PC: game {}, captured {}, encoded {}, sent {}",
                &[
                    &fps(rates.source_mhz),
                    &fps(rates.capture_mhz),
                    &fps(rates.submit_mhz),
                    &fps(rates.output_mhz),
                ],
            ));
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}
