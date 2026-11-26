use super::*;
use rubylight_native_api::doctor::{EligibleRemedy, HostStages, StageTimes};

fn report(findings: &[Finding]) -> Report {
    Report {
        findings: findings.to_vec(),
        samples: 20,
        recent: true,
        worst_loss_percent: 0.0,
        worst_decode_ms: 3.0,
        worst_host_p95_ms: 6.0,
        worst_display_p95_ms: 4.0,
        stalls: 0,
        stall_period_ms: 0,
        host_includes_send: true,
        late_samples: 0,
    }
}

fn stream() -> StreamFacts {
    StreamFacts {
        width: 2560,
        height: 1440,
        fps: 120,
        hdr: false,
        ten_bit: false,
        pyrowave: false,
        packed_from_memory: false,
        screen_width: 2560,
        screen_height: 1440,
    }
}

fn device(platform: Platform) -> DeviceFacts {
    DeviceFacts {
        platform,
        constrained: false,
        metalfx: false,
        upscaler: ActiveUpscaler::Off,
        receive_buffer: None,
    }
}

const CAN: Abilities = Abilities {
    bitrate: true,
    live_kbps: 45_000,
    resize: true,
};

/// The host's snapshot with `late` of 1800 pictures late, most time lost in `stage`.
fn host(stage: HostStage, late: u64, step_down: bool) -> DoctorSnapshot {
    let mut lost = StageTimes::default();
    match stage {
        HostStage::Encode => lost.encode = 640_000,
        HostStage::Send => lost.send = 300_000,
        _ => lost.capture = 100_000,
    }
    DoctorSnapshot {
        schema_version: 1,
        session_active: true,
        caller_is_owner: true,
        window_seconds: 30,
        encoder: None,
        host_stages: Some(HostStages {
            pictures: 1800,
            late_pictures: late,
            dominant_stage: Some(stage),
            lost_us: lost,
            pipeline_rates: None,
        }),
        active_trials: Vec::new(),
        eligible_remedies: vec![EligibleRemedy {
            id: RemedyId::BitrateStepdown,
            configurable: step_down,
            target_kbps: step_down.then_some(36_000),
            reason: None,
            message: None,
        }],
    }
}

#[test]
fn the_first_seconds_and_a_clean_stream_offer_nothing() {
    let gathering = Report {
        samples: 2,
        ..report(&[Finding::Host])
    };
    let d = diagnose(
        &gathering,
        None,
        &stream(),
        &device(Platform::Windows),
        &CAN,
    );
    assert_eq!((d.cause, d.remedies.len()), (RootCause::Gathering, 0));
    let stale = Report {
        recent: false,
        ..report(&[Finding::Network])
    };
    assert_eq!(
        diagnose(&stale, None, &stream(), &device(Platform::Windows), &CAN).cause,
        RootCause::Gathering
    );
    let clean = diagnose(
        &report(&[]),
        Some(&host(HostStage::Encode, 2, true)),
        &stream(),
        &device(Platform::Windows),
        &CAN,
    );
    assert_eq!(
        (clean.cause, clean.stage),
        (RootCause::Healthy, Stage::None)
    );
    assert!(clean.remedies.is_empty() && clean.advice.is_empty());
    assert!(clean.host_measured);
}

#[test]
fn an_encoder_that_falls_behind_is_the_host_not_the_network() {
    // Frames also arrived bunched: the host's own report names its encoder.
    let r = Report {
        late_samples: 12,
        ..report(&[Finding::Bursts, Finding::Host])
    };
    let d = diagnose(
        &r,
        Some(&host(HostStage::Encode, 94, true)),
        &stream(),
        &device(Platform::Windows),
        &CAN,
    );
    assert_eq!(
        (d.cause, d.stage),
        (RootCause::HostOverload, Stage::HostEncode)
    );
    assert_eq!(
        d.key,
        KeyNumber::HostLost {
            stage: HostStageName::Encode,
            lost_ms: 640.0,
            late: 94,
            pictures: 1800
        }
    );
    // 1440p on a 1440p screen steps to 85 %, and FSR 1 keeps it sharp on a desktop GPU.
    assert_eq!(
        d.remedies,
        [Remedy::ResolutionStepdown {
            width: 2176,
            height: 1224,
            upscaler: RecommendedUpscaler::Fsr1
        }]
    );
    assert!(d.advice.contains(&Advice::UseUpscaler {
        upscaler: RecommendedUpscaler::Fsr1
    }));
    assert!(d.advice.contains(&Advice::EaseThePc));
    // Without the host's report, its long present-to-sent time still names the PC.
    let alone = diagnose(
        &report(&[Finding::Host]),
        None,
        &stream(),
        &device(Platform::Windows),
        &CAN,
    );
    assert_eq!(alone.cause, RootCause::HostOverload);
    assert!(matches!(alone.key, KeyNumber::HostP95 { .. }));
}

#[test]
fn a_few_late_host_pictures_are_no_host_finding() {
    let r = Report {
        worst_loss_percent: 6.0,
        ..report(&[Finding::Network])
    };
    let d = diagnose(
        &r,
        Some(&host(HostStage::Encode, 10, true)),
        &stream(),
        &device(Platform::Android),
        &CAN,
    );
    assert_eq!(
        (d.cause, d.stage),
        (RootCause::NetworkJitter, Stage::Network)
    );
    assert_eq!(d.key, KeyNumber::LossPercent { percent: 6.0 });
    assert_eq!(
        d.remedies,
        [Remedy::BitrateStepdown {
            target_kbps: 36_000,
            on_host: true
        }]
    );
    assert_eq!(d.advice, [Advice::WiredOrQuieterWifi]);
}

#[test]
fn wifi_bursts_and_scans_name_the_network_with_their_numbers() {
    let r = Report {
        late_samples: 9,
        stalls: 6,
        stall_period_ms: 120_000,
        ..report(&[Finding::Bursts, Finding::Periodic])
    };
    // A host without the doctor: the classic trial.
    let d = diagnose(&r, None, &stream(), &device(Platform::Linux), &CAN);
    assert_eq!(d.cause, RootCause::NetworkJitter);
    assert_eq!(d.key, KeyNumber::LateSeconds { late: 9, of: 20 });
    assert_eq!(
        d.remedies,
        [Remedy::BitrateStepdown {
            target_kbps: 36_000,
            on_host: false
        }]
    );
    assert_eq!(
        d.advice,
        [
            Advice::PeriodicScans {
                period_ms: 120_000,
                stalls: 6
            },
            Advice::WiredOrQuieterWifi
        ]
    );
    assert!(!d.host_measured);
    // The PC's own send stage: the PC's network adapter, not the Wi-Fi.
    let sending = diagnose(
        &report(&[]),
        Some(&host(HostStage::Send, 90, true)),
        &stream(),
        &device(Platform::Linux),
        &CAN,
    );
    assert_eq!(
        (sending.cause, sending.stage),
        (RootCause::NetworkJitter, Stage::HostSend)
    );
    assert_eq!(sending.advice, [Advice::EaseThePc]);
}

#[test]
fn a_short_receive_buffer_names_its_numbers_and_the_linux_command() {
    let r = Report {
        worst_loss_percent: 1.2,
        ..report(&[])
    };
    let mut linux = device(Platform::Linux);
    linux.receive_buffer = Some((212_992, 4 << 20));
    let d = diagnose(&r, None, &stream(), &linux, &CAN);
    assert_eq!(
        (d.cause, d.stage),
        (RootCause::ReceiveBufferShort, Stage::Network)
    );
    assert_eq!(
        d.key,
        KeyNumber::ReceiveBuffer {
            granted_kib: 208,
            asked_kib: 4096
        }
    );
    assert_eq!(
        d.advice,
        [Advice::RaiseReceiveBuffer {
            asked_bytes: 4 << 20,
            linux_command: true
        }]
    );
    let mut windows = linux;
    windows.platform = Platform::Windows;
    let d = diagnose(&r, None, &stream(), &windows, &CAN);
    assert_eq!(
        d.advice,
        [Advice::RaiseReceiveBuffer {
            asked_bytes: 4 << 20,
            linux_command: false
        }]
    );
    // No loss: the short buffer is not what the player sees.
    assert_eq!(
        diagnose(&report(&[]), None, &stream(), &linux, &CAN).cause,
        RootCause::Healthy
    );
}

#[test]
fn a_slow_decoder_names_full_chroma_and_steps_down() {
    // #80: packed 4:4:4 copied through memory at 100 fps.
    let r = Report {
        worst_decode_ms: 80.8,
        ..report(&[Finding::Decoder])
    };
    let s = StreamFacts {
        fps: 100,
        packed_from_memory: true,
        ..stream()
    };
    let d = diagnose(
        &r,
        Some(&host(HostStage::Encode, 0, true)),
        &s,
        &device(Platform::Android),
        &CAN,
    );
    assert_eq!(
        (d.cause, d.stage),
        (RootCause::DecodePressure, Stage::Decode)
    );
    assert_eq!(
        d.key,
        KeyNumber::DecodeMs {
            ms: 80.8,
            budget_ms: 10.0
        }
    );
    assert_eq!(d.advice, [Advice::FullChromaOff, Advice::LowerFrameRate]);
    assert_eq!(
        d.remedies,
        [Remedy::ResolutionStepdown {
            width: 2176,
            height: 1224,
            upscaler: RecommendedUpscaler::Sgsr1
        }]
    );
    let hot = diagnose(
        &report(&[Finding::Heat]),
        None,
        &stream(),
        &device(Platform::Android),
        &CAN,
    );
    assert!(hot.advice.contains(&Advice::CoolDown));
    // A host that cannot change the stream's mode gets the advice alone.
    let fixed = Abilities {
        resize: false,
        ..CAN
    };
    assert!(
        diagnose(&r, None, &s, &device(Platform::Android), &fixed)
            .remedies
            .is_empty()
    );
}

#[test]
fn late_presentation_judges_the_upscaler() {
    let r = report(&[Finding::Presentation]);
    let small = StreamFacts {
        width: 1920,
        height: 1080,
        ..stream()
    };
    let mut d = device(Platform::Windows);
    d.upscaler = ActiveUpscaler::Fsr1;
    let fsr = diagnose(&r, None, &small, &d, &CAN);
    assert_eq!(
        (fsr.cause, fsr.stage),
        (RootCause::DisplayPacing, Stage::Present)
    );
    assert_eq!(fsr.advice, [Advice::FramePacing]);
    assert_eq!(
        fsr.upscaler,
        UpscalerVerdict::TooCostly {
            lighter: RecommendedUpscaler::Sgsr1
        }
    );
    d.upscaler = ActiveUpscaler::Sgsr1;
    assert_eq!(
        diagnose(&r, None, &small, &d, &CAN).upscaler,
        UpscalerVerdict::TooCostly {
            lighter: RecommendedUpscaler::None
        }
    );
    // A stream as large as the screen: the upscaler only costs.
    assert_eq!(
        judge_upscaler(&stream(), &d, RootCause::Healthy),
        UpscalerVerdict::NotNeeded
    );
    assert_eq!(
        judge_upscaler(&small, &d, RootCause::Healthy),
        UpscalerVerdict::Fine
    );
    let hdr = StreamFacts { hdr: true, ..small };
    assert_eq!(
        judge_upscaler(&hdr, &d, RootCause::Healthy),
        UpscalerVerdict::Blocked {
            blocker: UpscaleBlocker::Hdr
        }
    );
}

#[test]
fn upscaler_choices_are_spatial_and_metalfx_only_on_macos() {
    let small = StreamFacts {
        width: 1920,
        height: 1080,
        ..stream()
    };
    let mut every = Vec::new();
    for platform in [
        Platform::Android,
        Platform::Windows,
        Platform::Linux,
        Platform::MacOs,
    ] {
        for (constrained, metalfx) in [(false, false), (false, true), (true, false), (true, true)] {
            for (hdr, ten_bit, pyrowave) in [
                (false, false, false),
                (true, false, false),
                (false, true, false),
                (false, false, true),
            ] {
                let s = StreamFacts {
                    hdr,
                    ten_bit,
                    pyrowave,
                    ..small
                };
                let d = DeviceFacts {
                    constrained,
                    metalfx,
                    ..device(platform)
                };
                let pick = recommend_upscaler(&s, &d, s.width, s.height);
                if let Ok(RecommendedUpscaler::MetalFx) = pick {
                    assert!(platform == Platform::MacOs && metalfx);
                }
                if hdr || ten_bit || pyrowave {
                    assert!(pick.is_err());
                }
                every.push(format!("{pick:?}"));
            }
        }
    }
    assert_eq!(
        recommend_upscaler(
            &small,
            &DeviceFacts {
                metalfx: true,
                ..device(Platform::MacOs)
            },
            1920,
            1080
        ),
        Ok(RecommendedUpscaler::MetalFx)
    );
    assert_eq!(
        recommend_upscaler(&small, &device(Platform::Android), 1920, 1080),
        Ok(RecommendedUpscaler::Sgsr1)
    );
    assert_eq!(
        recommend_upscaler(
            &small,
            &DeviceFacts {
                constrained: true,
                ..device(Platform::Windows)
            },
            1920,
            1080
        ),
        Ok(RecommendedUpscaler::Sgsr1)
    );
    assert_eq!(
        recommend_upscaler(&stream(), &device(Platform::Windows), 2560, 1440),
        Err(UpscaleBlocker::NoUpscale)
    );
    // No verdict, remedy or advice ever names a retired or experimental upscaler.
    let r = report(&[Finding::Presentation, Finding::Decoder]);
    for platform in [
        Platform::Android,
        Platform::Windows,
        Platform::Linux,
        Platform::MacOs,
    ] {
        let json =
            serde_json::to_string(&diagnose(&r, None, &small, &device(platform), &CAN)).unwrap();
        every.push(json);
    }
    let all = every.join(" ").to_lowercase();
    for word in ["sgsr2", "sgsr 2", "temporal", "experimental"] {
        assert!(!all.contains(word), "{word}");
    }
}

#[test]
fn a_step_down_follows_the_screens_steps() {
    assert_eq!(step_down(&stream()), Some((2176, 1224)));
    let at_75 = StreamFacts {
        width: 1920,
        height: 1080,
        ..stream()
    };
    assert_eq!(step_down(&at_75), Some((1708, 960)), "the 67 % step");
    // A stream larger than the screen steps to the screen's own size.
    let supersampled = StreamFacts {
        width: 3840,
        height: 2160,
        ..stream()
    };
    assert_eq!(step_down(&supersampled), Some((2560, 1440)));
    let smallest = StreamFacts {
        width: 1280,
        height: 720,
        ..stream()
    };
    assert_eq!(step_down(&smallest), None);
}

#[test]
fn the_verdict_serializes_with_stable_names() {
    let r = Report {
        worst_loss_percent: 6.0,
        ..report(&[Finding::Network])
    };
    let d = diagnose(&r, None, &stream(), &device(Platform::Android), &CAN);
    let json = serde_json::to_value(&d).unwrap();
    assert_eq!(json["cause"], "network_jitter");
    assert_eq!(json["stage"], "network");
    assert_eq!(json["key"]["kind"], "loss_percent");
    assert_eq!(json["remedies"][0]["remedy"], "bitrate_stepdown");
    assert_eq!(json["advice"][0]["advice"], "wired_or_quieter_wifi");
    assert_eq!(json["upscaler"]["verdict"], "fine");
}
