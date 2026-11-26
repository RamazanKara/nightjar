//! The preview cards on the settings page (`settings_previews`): a card under each of its rows
//! with the value's name and what its marks mean, following the value at once, found by a search
//! for its words, gone with its row, and the screenshots that show them in every look.

use super::*;
use crate::config::{ReduceMotion, Theme, Upscaling, VideoScaling};
use crate::ui::settings::{frame_pacing_choice, set_frame_pacing_choice, FramePacingChoice, Preview};

/// The four rows with a card, with the section each is in.
const CARDS: [(Preview, &str, &str); 4] = [
    (Preview::Hdr, "Video", "HDR"),
    (Preview::VideoSize, "Display & image", "Video size"),
    (Preview::FramePacing, "Display & image", "Frame pacing"),
    (Preview::Upscaling, "Display & image", "Upscaling"),
];

/// The page with every fold open and VRR off, which would take Frame pacing's row away.
fn cards_page(changed: Settings) -> Frames {
    let mut f = settings_with(Settings { vrr: false, ..changed });
    open_folds(&mut f);
    f
}

/// The words in the card under the row labelled `label`, as drawn: the value's name, then the legend.
fn card_words<'f>(f: &'f Frames, label: &str) -> Vec<&'f str> {
    let row = f.placed.iter().find(|(t, ..)| t == label).unwrap_or_else(|| panic!("{label} is not drawn: {:?}", f.drawn)).1;
    let band = row.center().y + 30.0..row.center().y + 30.0 + design_tokens::SIZE_PREVIEW_CARD;
    f.placed.iter().filter(|(_, r, _)| band.contains(&r.center().y) && r.left() > row.left()).map(|(t, ..)| t.as_str()).collect()
}

fn each_shape(shape: &egui::Shape, visit: &mut dyn FnMut(&egui::Shape)) {
    match shape {
        egui::Shape::Vec(inner) => {
            for shape in inner {
                each_shape(shape, visit);
            }
        }
        other => visit(other),
    }
}

/// The video as the Video size card draws it: the frame under the row with an amber line two
/// points wide.
fn video_drawn(f: &Frames) -> Rect {
    let row = f.placed.iter().find(|(t, ..)| t == "Video size").expect("the row").1;
    let band = row.center().y + 20.0..row.center().y + 34.0 + design_tokens::SIZE_PREVIEW_CARD;
    let mut found = None;
    for clipped in &f.shapes {
        each_shape(&clipped.shape, &mut |shape| {
            if let egui::Shape::Rect(rect) = shape {
                if rect.stroke.color == *MOON && rect.stroke.width == 2.0 && band.contains(&rect.rect.center().y) {
                    found = Some(rect.rect);
                }
            }
        });
    }
    found.expect("the Video size card's video")
}

#[test]
fn each_card_sits_under_its_row_with_the_value_and_what_its_marks_mean() {
    assert_eq!(CARDS.map(|card| card.0), [Preview::Hdr, Preview::VideoSize, Preview::FramePacing, Preview::Upscaling]);
    assert_eq!(Preview::ALL.len(), CARDS.len(), "every card has its row here");
    let mut f = cards_page(Settings::default());
    for (card, section, label) in CARDS {
        in_section(&mut f, section);
        let value = card.caption(&f.l.settings);
        let mut words = vec![value];
        words.extend(card.legend().iter().map(|(_, meaning)| *meaning));
        assert_eq!(card_words(&f, label), words, "{label}");
        // The card says the value the row's own control says. A switch says none.
        assert!(card == Preview::Hdr || row_words(&f, label).contains(&value), "{label}: {:?}", row_words(&f, label));
    }
    assert_fits(&f, "the cards at 1280", false);
}

#[test]
fn a_card_shows_the_new_value_in_the_frame_that_chose_it() {
    let mut f = cards_page(Settings::default());
    in_section(&mut f, "Display & image");
    assert_eq!(card_words(&f, "Video size")[0], "Fit");
    click_value(&mut f, "Video size", "Fit");
    click_last(&mut f, "Stretch");
    assert_eq!(f.l.settings.video_scaling, VideoScaling::Stretch);
    assert_eq!(card_words(&f, "Video size")[0], "Stretch");
    // Frame pacing: one choice over three keys, and the card names the choice.
    click_value(&mut f, "Frame pacing", Preview::FramePacing.caption(&Settings::default()));
    click_last(&mut f, "Balanced");
    assert_eq!(frame_pacing_choice(&f.l.settings), FramePacingChoice::Balanced);
    assert_eq!(card_words(&f, "Frame pacing")[0], "Balanced");
    set_frame_pacing_choice(&mut f.l.settings, FramePacingChoice::HostTimes);
    f.frame(Vec::new());
    assert_eq!(card_words(&f, "Frame pacing")[0], "PC’s frame times");
    // The sharpness of the row under Upscaling is the card's too: it changes no word, and the
    // picture's numbers follow it.
    let soft = Preview::Upscaling.picture(&Settings { upscaling: Upscaling::Fsr1, upscale_sharpness: 0, ..Settings::default() });
    let sharp = Preview::Upscaling.picture(&Settings { upscaling: Upscaling::Fsr1, upscale_sharpness: 100, ..Settings::default() });
    assert_ne!(soft.pose(), sharp.pose());
}

/// A change of value moves the picture on a short spring and leaves it still; with reduced motion
/// it is there in the frame that changed it.
#[test]
fn a_picture_springs_to_a_new_value_and_cuts_with_reduced_motion() {
    for reduce in [false, true] {
        let mut f = cards_page(Settings { reduce_motion: if reduce { ReduceMotion::Reduce } else { ReduceMotion::Full }, ..Settings::default() });
        in_section(&mut f, "Display & image");
        let fit = video_drawn(&f);
        f.frame(Vec::new());
        assert_eq!(video_drawn(&f), fit, "still at rest");
        // Fill: the video takes the screen's whole height.
        f.l.settings.video_scaling = VideoScaling::Fill;
        f.frame(Vec::new());
        let first = video_drawn(&f);
        for _ in 0..12 {
            f.settle();
        }
        let fill = video_drawn(&f);
        assert!(fill.height() > fit.height() + 8.0, "{fit:?} to {fill:?}");
        if reduce {
            assert!((first.height() - fill.height()).abs() < 0.01, "reduced motion cuts: {first:?}, then {fill:?}");
        } else {
            assert!(first.height() > fit.height() && first.height() < fill.height() - 0.5, "full motion is on its way after one frame: {fit:?}, {first:?}, {fill:?}");
        }
        f.frame(Vec::new());
        assert_eq!(video_drawn(&f), fill, "and still again");
    }
}

/// A search finds a row by its card's words, and a row a search leaves out takes its card along.
#[test]
fn a_search_finds_a_row_by_its_cards_words() {
    for (query, label) in [
        ("frames shown", "Frame pacing"),
        ("screen refresh", "Frame pacing"),
        ("what the game draws", "HDR"),
        ("stream pixels", "Upscaling"),
        ("1:1 pixels", "Video size"),
    ] {
        let mut f = cards_page(Settings::default());
        f.l.settings_view.query = query.into();
        f.settle();
        assert!(shows(&f, label), "{query:?} finds {label}: {:?}", f.drawn);
    }
    // Every card's legend is among its row's words, in the player's language.
    for card in Preview::ALL {
        let words = card.search_words();
        assert!(card.legend().iter().all(|(_, meaning)| words.contains(&tr(meaning))), "{card:?}");
    }
    let mut f = cards_page(Settings::default());
    in_section(&mut f, "Display & image");
    assert!(shows(&f, "Stream pixels") && shows(&f, "Frames shown"));
    f.l.settings_view.query = "borderless".into();
    f.settle();
    assert!(shows(&f, "Window") && !shows(&f, "Upscaling"), "{:?}", f.drawn);
    assert!(!shows(&f, "Stream pixels") && !shows(&f, "Frames shown"), "a row left out leaves its card out: {:?}", f.drawn);
}

/// What does not apply stays hidden, card and all: Frame pacing is not used while VRR is on.
#[test]
fn a_row_that_does_not_apply_takes_its_card_along() {
    let mut f = settings_with(Settings { vrr: true, ..Settings::default() });
    // A screen that can use VRR: without one the row has no VRR to stand aside for.
    f.l.tearing_supported = true;
    open_folds(&mut f);
    in_section(&mut f, "Display & image");
    assert!(!shows(&f, "Frame pacing") && !shows(&f, "Frames shown"), "{:?}", f.drawn);
    assert!(shows(&f, "Video size") && shows(&f, "Stream pixels"), "the other cards stay: {:?}", f.drawn);
}

/// A narrow window keeps the picture and wraps the words beside it; nothing leaves the card.
#[test]
fn the_cards_fit_a_narrow_window() {
    for width in [1280.0, 900.0, 800.0, 720.0] {
        let mut f = Frames::new(record("PC", true, false), Reach::Online);
        f.size = vec2(width, 4000.0);
        f.l.settings.vrr = false;
        f.l.settings_view.rail_more = true;
        f.l.settings_view.section = "Display & image";
        f.l.go(Screen::Settings);
        f.settle();
        open_folds(&mut f);
        assert!(shows(&f, "Frames shown") && shows(&f, "Stream pixels"), "at {width}: {:?}", f.drawn);
        assert_fits(&f, &format!("the cards at {width}"), false);
        let row = f.placed.iter().find(|(t, ..)| t == "Frame pacing").expect("the row").1;
        let words: Vec<Rect> = f.placed.iter().filter(|(t, ..)| ["Frames shown", "Frames arrive", "Screen refresh"].contains(&t.as_str())).map(|p| p.1).collect();
        assert_eq!(words.len(), 3, "at {width}");
        for word in words {
            assert!(word.top() > row.bottom() && word.bottom() < row.center().y + 34.0 + design_tokens::SIZE_PREVIEW_CARD, "at {width} the legend stays inside its card: {word:?} under {row:?}");
        }
    }
}

/// Turns the wheel until the row labelled `label` sits `from_top` under the window's top.
fn scroll_row_to(f: &mut Frames, label: &str, from_top: f32) {
    for _ in 0..2 {
        let Some(at) = f.placed.iter().find(|(t, ..)| t == label).map(|p| p.1.center().y) else { return };
        let off = at - from_top;
        if off.abs() < 4.0 {
            break;
        }
        f.frame(vec![egui::Event::PointerMoved((f.size / 2.0).to_pos2())]);
        f.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, -off),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        }]);
        for _ in 0..3 {
            f.settle();
        }
    }
    // The pointer gone, the row under it lets its hint go.
    f.frame(vec![egui::Event::PointerGone]);
    for _ in 0..2 {
        f.settle();
    }
}

/// The preview cards in every look worth one: night, day, OLED and high contrast, each card in
/// three of its values, at 1280x800 (150 %), in a narrow window and in a wide one.
/// `NIGHTJAR_SHOTS=<dir> cargo test -p nightjar-desktop-ui --lib screenshots -- --ignored`.
#[test]
#[ignore = "writes PNGs; set NIGHTJAR_SHOTS"]
fn settings_preview_screenshots() {
    let Some(dir) = std::env::var_os("NIGHTJAR_SHOTS").map(std::path::PathBuf::from) else { return };
    std::fs::create_dir_all(&dir).expect("the folder for the screenshots");
    let values: [(&str, fn(&mut Settings)); 3] = [
        ("a", |s| {
            (s.hdr, s.video_scaling, s.upscaling, s.upscale_sharpness) = (false, VideoScaling::Fit, Upscaling::Fsr1, 40);
            set_frame_pacing_choice(s, FramePacingChoice::Balanced);
        }),
        ("b", |s| {
            (s.hdr, s.video_scaling, s.upscaling) = (true, VideoScaling::Fill, Upscaling::Bilinear);
            set_frame_pacing_choice(s, FramePacingChoice::Latency);
        }),
        ("c", |s| {
            (s.hdr, s.video_scaling, s.upscaling, s.upscale_sharpness) = (true, VideoScaling::Stretch, Upscaling::Sgsr1, 100);
            set_frame_pacing_choice(s, FramePacingChoice::Smoothness);
        }),
    ];
    const NORMAL: (f32, f32, f32) = (1280.0, 800.0, 1.5);
    const NARROW: (f32, f32, f32) = (720.0, 900.0, 1.0);
    const WIDE: (f32, f32, f32) = (2560.0, 1440.0, 1.0);
    // The looks and sizes nobody sees every day first; `NIGHTJAR_SHOTS_ONLY` keeps the cases
    // whose name has it.
    let cases = [
        (NORMAL, "high-contrast", 0), (NORMAL, "high-contrast", 1), (NARROW, "night", 0), (NARROW, "high-contrast", 1),
        (WIDE, "night", 1), (NORMAL, "oled", 0), (WIDE, "day", 0),
        (NORMAL, "night", 0), (NORMAL, "night", 1), (NORMAL, "night", 2), (NORMAL, "day", 0), (NORMAL, "day", 1),
    ];
    let only = std::env::var("NIGHTJAR_SHOTS_ONLY").unwrap_or_default();
    for ((width, height, ppp), look, value) in cases {
        let (value, set) = values[value];
        let name = format!("{}x{}-{look}-{value}", width as u32, height as u32);
        if !name.contains(&only) {
            continue;
        }
        let mut f = home_with_games(1);
        f.size = vec2(width, height);
        f.native_ppp = ppp;
        f.shots = true;
        f.l.logo = None;
        f.l.settings.theme = match look {
            "day" => Theme::Light,
            "oled" => Theme::Oled,
            _ => Theme::Dark,
        };
        f.l.settings.high_contrast = look == "high-contrast";
        f.l.settings.vrr = false;
        set(&mut f.l.settings);
        f.l.settings_view.rail_more = true;
        f.l.settings_view.more = [true; 9];
        f.l.settings_view.section = "Display & image";
        f.l.go(Screen::Settings);
        f.settle();
        f.settle();
        scroll_row_to(&mut f, "Video size", 190.0);
        f.shoot(&dir, &format!("preview-size-pacing-{name}"));
        scroll_row_to(&mut f, "Upscaling", 190.0);
        f.shoot(&dir, &format!("preview-upscaling-{name}"));
        f.l.settings_view.section = "Video";
        f.l.settings_view.to_top = true;
        f.settle();
        f.settle();
        scroll_row_to(&mut f, "HDR", 190.0);
        f.shoot(&dir, &format!("preview-hdr-{name}"));
    }
    // The presets row over Video's rows, with a preset chosen and with none (Custom): beside
    // the label, under it, and as two lines of two.
    for width in [1280.0, 900.0, 680.0, 520.0] {
        if !"presets".contains(&only) {
            continue;
        }
        for (name, fps) in [("native", Settings::default().fps), ("custom", 37)] {
            let mut f = home_with_games(1);
            f.size = vec2(width, 800.0);
            f.shots = true;
            f.l.logo = None;
            f.l.settings.fps = fps;
            f.l.settings_view.rail_more = true;
            f.l.settings_view.section = "Video";
            f.l.go(Screen::Settings);
            for _ in 0..3 {
                f.settle();
            }
            f.shoot(&dir, &format!("presets-{}-{name}", width as u32));
        }
    }
}
