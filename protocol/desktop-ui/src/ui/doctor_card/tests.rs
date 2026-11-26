//! The card's words in English and German (each finding, number, remedy, advice and upscaler
//! verdict), what it offers a guest and while it gathers, that keys reach its buttons, and its
//! screenshots.
use super::*;
use crate::strings::{Lang, with_lang};
use egui::{Context, Key, Modifiers, Pos2, Rect};
use nightjar_policy::doctor_v2::{
    Advice, HostStageName, KeyNumber, RecommendedUpscaler, Stage, UpscaleBlocker, UpscalerVerdict,
};
use rubylight_native_api::doctor::{EncoderFacts, HostStages, PipelineRates, StageTimes};

const CAUSES: [(RootCause, Stage); 9] = [
    (RootCause::Gathering, Stage::None),
    (RootCause::Healthy, Stage::None),
    (RootCause::ReceiveBufferShort, Stage::Network),
    (RootCause::HostOverload, Stage::HostEncode),
    (RootCause::DecodePressure, Stage::Decode),
    (RootCause::NetworkJitter, Stage::Network),
    (RootCause::NetworkJitter, Stage::HostSend),
    (RootCause::DisplayPacing, Stage::Present),
    (RootCause::HostOverload, Stage::None),
];

const BITRATE: Remedy = Remedy::BitrateStepdown {
    target_kbps: 36_000,
    on_host: true,
};
const RESIZE: Remedy = Remedy::ResolutionStepdown {
    width: 2176,
    height: 1224,
    upscaler: RecommendedUpscaler::Sgsr1,
};

fn diagnosis(cause: RootCause, stage: Stage, key: KeyNumber) -> Diagnosis {
    Diagnosis {
        cause,
        stage,
        key,
        remedies: Vec::new(),
        advice: Vec::new(),
        upscaler: UpscalerVerdict::Fine,
        host_measured: false,
    }
}

fn keys() -> Vec<KeyNumber> {
    let host = |stage| KeyNumber::HostLost {
        stage,
        lost_ms: 640.0,
        late: 94,
        pictures: 1800,
    };
    vec![
        KeyNumber::LossPercent { percent: 6.0 },
        KeyNumber::LateSeconds { late: 7, of: 30 },
        host(HostStageName::Capture),
        host(HostStageName::Convert),
        host(HostStageName::Encode),
        host(HostStageName::Packetize),
        host(HostStageName::Send),
        KeyNumber::HostP95 {
            ms: 22.4,
            budget_ms: 8.3,
        },
        KeyNumber::DecodeMs {
            ms: 14.5,
            budget_ms: 8.3,
        },
        KeyNumber::DisplayP95 {
            ms: 19.0,
            budget_ms: 16.7,
        },
        KeyNumber::ReceiveBuffer {
            granted_kib: 208,
            asked_kib: 4096,
        },
    ]
}

fn advices() -> Vec<Advice> {
    vec![
        Advice::WiredOrQuieterWifi,
        Advice::PeriodicScans {
            period_ms: 10_000,
            stalls: 6,
        },
        Advice::PeriodicScans {
            period_ms: 150_000,
            stalls: 4,
        },
        Advice::RaiseReceiveBuffer {
            asked_bytes: 4_194_304,
            linux_command: true,
        },
        Advice::RaiseReceiveBuffer {
            asked_bytes: 4_194_304,
            linux_command: false,
        },
        Advice::EaseThePc,
        Advice::FullChromaOff,
        Advice::LowerFrameRate,
        Advice::CoolDown,
        Advice::FramePacing,
        Advice::UseUpscaler {
            upscaler: RecommendedUpscaler::Sgsr1,
        },
        Advice::UseUpscaler {
            upscaler: RecommendedUpscaler::Fsr1,
        },
        Advice::UseUpscaler {
            upscaler: RecommendedUpscaler::MetalFx,
        },
    ]
}

fn verdicts() -> Vec<UpscalerVerdict> {
    let mut verdicts = vec![
        UpscalerVerdict::NotNeeded,
        UpscalerVerdict::TooCostly {
            lighter: RecommendedUpscaler::None,
        },
        UpscalerVerdict::TooCostly {
            lighter: RecommendedUpscaler::Sgsr1,
        },
    ];
    for blocker in [
        UpscaleBlocker::Hdr,
        UpscaleBlocker::TenBit,
        UpscaleBlocker::PyroWave,
        UpscaleBlocker::NoUpscale,
    ] {
        verdicts.push(UpscalerVerdict::Blocked { blocker });
    }
    verdicts
}

/// Every text the card's words make, in the language in use.
fn every_text() -> Vec<String> {
    let mut texts = Vec::new();
    for (cause, stage) in CAUSES {
        let d = diagnosis(cause, stage, KeyNumber::None);
        texts.push(copy::title(&d).to_owned());
        texts.push(copy::explanation(&d).to_owned());
    }
    texts.extend(
        keys()
            .iter()
            .map(|key| copy::key_line(key).expect("a number has a line")),
    );
    texts.extend(advices().iter().map(copy::advice));
    for verdict in verdicts() {
        for active in [
            ActiveUpscaler::Sgsr1,
            ActiveUpscaler::Fsr1,
            ActiveUpscaler::MetalFx,
            ActiveUpscaler::Vendor,
        ] {
            texts.push(
                copy::upscaler(&verdict, active).expect("a verdict other than Fine has a line"),
            );
        }
    }
    texts.extend([BITRATE, RESIZE].iter().map(copy::remedy));
    texts.push(copy::host_lines(&host()).expect("the host sent numbers"));
    texts
}

fn host() -> DoctorSnapshot {
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
            fps: 60,
            live_bitrate_kbps: 45_000,
            fec_percent: 20,
        }),
        host_stages: Some(HostStages {
            pictures: 1800,
            late_pictures: 94,
            dominant_stage: Some(rubylight_native_api::doctor::HostStage::Encode),
            lost_us: StageTimes {
                capture: 18_200,
                convert: 11_400,
                encode: 642_500,
                packetize: 4_100,
                send: 22_000,
            },
            pipeline_rates: Some(PipelineRates {
                source_mhz: Some(60_000),
                capture_mhz: Some(60_000),
                submit_mhz: Some(57_100),
                output_mhz: Some(57_000),
                repeat_mhz: None,
            }),
        }),
        active_trials: Vec::new(),
        eligible_remedies: Vec::new(),
    }
}

#[test]
fn every_finding_has_its_words_in_english_and_german() {
    let english = with_lang(Lang::En, every_text);
    let german = with_lang(Lang::De, every_text);
    assert_eq!(english.len(), german.len());
    for (en, de) in english.iter().zip(&german) {
        assert!(
            !en.trim().is_empty() && !de.trim().is_empty(),
            "{en:?} / {de:?}"
        );
        assert_ne!(en, de, "no German for {en:?}");
    }
}

#[test]
fn numbers_are_written_as_the_language_writes_them() {
    let loss = KeyNumber::LossPercent { percent: 6.0 };
    let host = keys()[4];
    let decode = KeyNumber::DecodeMs {
        ms: 14.5,
        budget_ms: 8.3,
    };
    with_lang(Lang::En, || {
        assert_eq!(copy::key_line(&loss).unwrap(), "6.0 % of frames lost");
        assert_eq!(
            copy::key_line(&host).unwrap(),
            "94 of 1,800 pictures left the PC late, 640 ms lost encoding"
        );
        assert_eq!(
            copy::key_line(&decode).unwrap(),
            "Decoding took up to 14.5 ms; a frame lasts 8.3 ms"
        );
        assert_eq!(
            copy::remedy(&BITRATE),
            "Lower bitrate to 36 Mbps (this stream)"
        );
        assert_eq!(
            copy::remedy(&RESIZE),
            "Lower resolution to 2176×1224 (this stream)"
        );
        assert_eq!(copy::mbps(4_500), "4.5");
        let verdict = UpscalerVerdict::NotNeeded;
        assert_eq!(
            copy::upscaler(&verdict, ActiveUpscaler::Sgsr1).unwrap(),
            "SGSR 1 costs more than it gives here: the stream is already as large as the screen."
        );
        let linux = Advice::RaiseReceiveBuffer {
            asked_bytes: 4_194_304,
            linux_command: true,
        };
        assert!(
            copy::advice(&linux).ends_with("\nsudo sysctl -w net.core.rmem_max=4194304"),
            "the command as it is typed"
        );
    });
    with_lang(Lang::De, || {
        assert!(copy::key_line(&loss).unwrap().contains("6,0 %"));
        assert!(copy::key_line(&host).unwrap().contains("1.800"));
        let decode = copy::key_line(&decode).unwrap();
        assert!(
            decode.contains("14,5") && decode.contains("8,3"),
            "{decode}"
        );
        assert_eq!(copy::mbps(4_500), "4,5");
        assert!(
            copy::remedy(&RESIZE).contains("2176×1224"),
            "sizes are not grouped"
        );
        let lines = copy::host_lines(&host_snapshot()).unwrap();
        assert!(lines.contains("642,5") && lines.contains("57,1"), "{lines}");
    });
    with_lang(Lang::Fr, || assert_eq!(copy::whole(1_800), "1\u{a0}800"));
}

fn host_snapshot() -> DoctorSnapshot {
    host()
}

#[test]
fn no_word_names_a_retired_upscaler_or_calls_one_experimental() {
    for lang in Lang::all() {
        for text in with_lang(lang, every_text) {
            let lower = text.to_lowercase();
            for word in ["sgsr 2", "sgsr2", "temporal", "experimental"] {
                assert!(!lower.contains(word), "{lang:?}: {text}");
            }
        }
    }
}

/// One frame of the card in a dock-wide column; answers what it returned and the texts drawn.
fn frame(
    ctx: &Context,
    view: &View<'_>,
    events: Vec<egui::Event>,
) -> (Option<Action>, Vec<(String, Rect)>) {
    let input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 1400.0))),
        events,
        ..Default::default()
    };
    let mut action = None;
    let mut output = ctx.run_ui(input, |ui| {
        ui.set_width(256.0);
        action = show(ui, view);
    });
    // No painter here: the font atlas is dropped, as egui asks.
    output.textures_delta.clear();
    let mut texts = Vec::new();
    fn walk(shape: &egui::Shape, out: &mut Vec<(String, Rect)>) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|shape| walk(shape, out)),
            egui::Shape::Text(text) => {
                out.push((text.galley.text().to_owned(), text.visual_bounding_rect()))
            }
            _ => {}
        }
    }
    output
        .shapes
        .iter()
        .for_each(|clipped| walk(&clipped.shape, &mut texts));
    (action, texts)
}

fn context() -> Context {
    crate::theme::own_palette();
    let ctx = Context::default();
    crate::theme::apply(&ctx);
    crate::theme::set_over_video(&ctx, true);
    ctx
}

fn network() -> Diagnosis {
    Diagnosis {
        remedies: vec![BITRATE],
        advice: vec![
            Advice::PeriodicScans {
                period_ms: 10_000,
                stalls: 6,
            },
            Advice::WiredOrQuieterWifi,
        ],
        ..diagnosis(
            RootCause::NetworkJitter,
            Stage::Network,
            KeyNumber::LossPercent { percent: 6.0 },
        )
    }
}

fn overload() -> Diagnosis {
    Diagnosis {
        remedies: vec![RESIZE],
        advice: vec![
            Advice::UseUpscaler {
                upscaler: RecommendedUpscaler::Fsr1,
            },
            Advice::EaseThePc,
        ],
        upscaler: UpscalerVerdict::Fine,
        host_measured: true,
        ..diagnosis(RootCause::HostOverload, Stage::HostEncode, keys()[4])
    }
}

fn view<'a>(diagnosis: &'a Diagnosis, host: Option<&'a DoctorSnapshot>) -> View<'a> {
    View {
        diagnosis,
        host,
        owner: true,
        upscaler: ActiveUpscaler::Off,
        busy: false,
        undo_bitrate: false,
        undo_resolution: false,
        replaced: false,
        technical: "Active samples: 30.",
    }
}

fn shows(texts: &[(String, Rect)], wanted: &str) -> bool {
    texts.iter().any(|(text, _)| text == wanted)
}

#[test]
fn a_guest_sees_the_finding_but_not_the_pcs_remedies() {
    let d = network();
    let ctx = context();
    let mut guest = view(&d, None);
    guest.owner = false;
    let (_, texts) = with_lang(Lang::En, || frame(&ctx, &guest, Vec::new()));
    assert!(shows(&texts, "Wi-Fi or network") && shows(&texts, "6.0 % of frames lost"));
    assert!(shows(&texts, "Host device only"));
    assert!(!shows(&texts, "Lower bitrate to 36 Mbps (this stream)"));
}

#[test]
fn gathering_offers_no_remedy_but_keeps_an_undo() {
    let d = Diagnosis {
        remedies: vec![BITRATE],
        ..diagnosis(RootCause::Gathering, Stage::None, KeyNumber::None)
    };
    let ctx = context();
    let mut gathering = view(&d, None);
    assert!(offered(&gathering).is_empty());
    gathering.undo_resolution = true;
    let (_, texts) = with_lang(Lang::En, || frame(&ctx, &gathering, Vec::new()));
    assert!(shows(&texts, "Gathering a few seconds of video…"));
    assert!(!shows(&texts, "Lower bitrate to 36 Mbps (this stream)"));
    assert!(
        shows(&texts, "Undo resolution change"),
        "a step already taken can always be undone"
    );
    let d = diagnosis(RootCause::Healthy, Stage::None, KeyNumber::None);
    let (_, texts) = with_lang(Lang::En, || frame(&ctx, &view(&d, None), Vec::new()));
    assert!(shows(&texts, "Your stream is running smoothly") && shows(&texts, "Copy report"));
}

#[test]
fn a_click_or_the_keyboard_picks_a_remedy() {
    let d = network();
    let ctx = context();
    let card = view(&d, None);
    let (_, texts) = with_lang(Lang::En, || frame(&ctx, &card, Vec::new()));
    let at = texts
        .iter()
        .find(|(text, _)| text == "Lower bitrate to 36 Mbps (this stream)")
        .expect("the remedy's button")
        .1
        .center();
    let button = |pressed| egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let mut picked = None;
    for events in [
        vec![egui::Event::PointerMoved(at)],
        vec![button(true)],
        vec![button(false)],
    ] {
        picked = picked.or(with_lang(Lang::En, || frame(&ctx, &card, events)).0);
    }
    assert_eq!(picked, Some(Action::Apply(BITRATE)));
    // Tab reaches the first button, Enter runs it, as everywhere in the dock.
    let ctx = context();
    let key = |key| {
        vec![egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]
    };
    let mut picked = None;
    for events in [Vec::new(), key(Key::Tab), key(Key::Enter), Vec::new()] {
        picked = picked.or(with_lang(Lang::En, || frame(&ctx, &card, events)).0);
    }
    assert_eq!(picked, Some(Action::Apply(BITRATE)));
    // Busy: the buttons wait.
    let ctx = context();
    let busy = View {
        busy: true,
        ..view(&d, None)
    };
    let mut picked = None;
    for events in [Vec::new(), key(Key::Tab), key(Key::Enter), Vec::new()] {
        picked = picked.or(with_lang(Lang::En, || frame(&ctx, &busy, events)).0);
    }
    assert_eq!(picked, None);
}

#[test]
fn the_report_holds_the_card_and_the_measurements() {
    let d = overload();
    let host = host();
    let card = view(&d, Some(&host));
    let report = with_lang(Lang::En, || report(&card));
    assert!(
        report.starts_with(
            "The PC’s encoder\n94 of 1,800 pictures left the PC late, 640 ms lost encoding\n"
        ),
        "{report}"
    );
    assert!(
        report.contains(
            "• For the next stream: a lower resolution with FSR 1 keeps the picture sharp."
        )
    );
    assert!(
        report.contains("Active samples: 30.") && report.contains("PC encoder: AMF HEVC, 45 Mbps"),
        "{report}"
    );
}

/// One card painted at the dock's width over its surface.
fn shoot(
    dir: &std::path::Path,
    name: &str,
    lang: Lang,
    d: &Diagnosis,
    host: Option<&DoctorSnapshot>,
    expanded: bool,
) {
    let ctx = context();
    crate::live_glass::set_tier(&ctx, crate::live_glass::Tier::Live);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        &ctx,
        technical_id(),
        false,
    );
    state.set_open(expanded);
    state.store(&ctx);
    let mut card = view(d, host);
    card.technical = "Active samples: 30. Evidence covers the last 30 seconds.\n\nLargest observed values:\nFrame loss: 6.0 %\nDecode: 3.1 ms";
    let (width, height) = (300.0, 900.0);
    let mut textures = egui::TexturesDelta::default();
    let mut shapes = Vec::new();
    for _ in 0..4 {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, height))),
            ..Default::default()
        };
        let mut output = with_lang(lang, || {
            ctx.run_ui(input, |ui| {
                egui::Frame::new()
                    .fill(crate::ui::design_tokens::SURFACE_CONTROL_CENTER_BAKED_FILL)
                    .inner_margin(egui::Margin::same(22))
                    .show(ui, |ui| {
                        ui.set_width(256.0);
                        show(ui, &card)
                    });
            })
        });
        textures.append(std::mem::take(&mut output.textures_delta));
        shapes = output.shapes;
    }
    let primitives = ctx.tessellate(shapes, 1.0);
    let mut canvas = crate::shot::Canvas::new(width as usize, height as usize);
    canvas.update_textures(&textures);
    canvas.paint(&primitives, 1.0);
    canvas.save(&dir.join(format!("{name}.png")));
    textures.clear();
}

/// The Network and HostOverload cards, the drawer closed and open, in English and German:
/// `NIGHTJAR_SHOTS=<dir> cargo test -p nightjar-desktop-ui --lib doctor_card_screenshots -- --ignored`.
#[test]
#[ignore = "writes PNGs; set NIGHTJAR_SHOTS"]
fn doctor_card_screenshots() {
    let Some(dir) = std::env::var_os("NIGHTJAR_SHOTS").map(std::path::PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the folder for the screenshots");
    let host = host();
    let cases = [
        ("network", network(), None),
        ("host-overload", overload(), Some(&host)),
    ];
    for (name, d, host) in &cases {
        for (lang, code) in [(Lang::En, "en"), (Lang::De, "de")] {
            for (expanded, state) in [(false, "collapsed"), (true, "expanded")] {
                shoot(
                    &dir,
                    &format!("doctor-{name}-{state}-{code}"),
                    lang,
                    d,
                    *host,
                    expanded,
                );
            }
        }
    }
}
