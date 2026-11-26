//! How the interface sizes itself for the screen it is on (Nightjar #144, #146): one rule for
//! Android (`ViewportScaler`) and the desktop (`ui/viewport_scale.rs`), from the window's size in
//! dp, the form factor and the user's interface size. Never from a device's name.
//!
//! Three things come out of it. [`Scale::ui`] multiplies the system's density (Android) or the
//! zoom (desktop): a TV is normalised to one canvas whatever density its vendor reports, and a
//! desktop at its default size fits a 4K screen that the system leaves unscaled. [`Scale::density`]
//! tightens paddings on a short handheld screen, where targets must not shrink. The rest places
//! the stage: its margin, the hero's share, the cover grid's columns and the widths text is held to.
//!
//! The invariant every consumer leans on: a target is never under 48 system dp where a finger or
//! a mouse meets it, and on a TV never under 72 lines of a 1080-line picture (the 48 dp of the
//! 720p reference at its 1.5× optical scale).

/// Which of the design's layouts the window gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormFactor {
    Phone,
    Handheld,
    Tv,
    Desktop,
}

/// What the platform knows about the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// The window, or the pane beside a fold's hinge, in the system's dp (points on the desktop).
    pub width_dp: f32,
    pub height_dp: f32,
    /// The screen's physical dots per inch; `0` when the system does not say.
    pub dpi: f32,
    pub form: FormFactor,
    /// The user's interface size in percent; 100 is the default.
    pub user_scale_pct: u32,
    /// A TV screen that lays out compact (Settings).
    pub tv_compact: bool,
}

/// Smallest target where a finger or a pointer meets it, in system dp (`layout.touchMin`).
pub const TOUCH_MIN_DP: f32 = 48.0;
/// A TV's optical scale over the 720p reference (`layout.tvScaleFactor`): 48 reference dp are 72
/// lines of a 1080-line picture.
pub const TV_SCALE_FACTOR: f32 = 1.5;
/// The canvas every TV is normalised to, in dp: Android's own for 1080p.
pub const TV_CANVAS_DP: (f32, f32) = (960.0, 540.0);
/// A compact TV screen's share of the interface size.
pub const TV_COMPACT: f32 = 0.85;
/// The widest the stage's content grows (`layout.stageMaxWidth`): a 32:9 monitor keeps its covers
/// and rows in the middle instead of spreading eight columns over a metre.
pub const STAGE_MAX_WIDTH_DP: f32 = 1760.0;
/// The widest a hero's title and text run.
pub const TEXT_MAX_WIDTH_DP: f32 = 720.0;
/// Columns of a cover grid at most.
pub const MAX_COLUMNS: u32 = 8;
/// A desktop taller than this in points is a screen its system left unscaled (a TV or a 4K
/// monitor at 100 %): no desktop monitor at an ordinary scale reaches it.
const DESKTOP_UNSCALED_HEIGHT: f32 = 1600.0;
/// The height in points a desktop's default size aims at on such a screen.
const DESKTOP_FIT_HEIGHT: f32 = 1080.0;

/// How the interface is sized and placed. Sizes are design dp: multiply by [`Scale::ui`] for
/// system dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    /// The factor on the system's density or zoom.
    pub ui: f32,
    /// How much paddings and gaps tighten, 0.75 to 1. Targets and type keep their size.
    pub density: f32,
    /// The factor on display type (hero, large title), 0.72 to 1, so a title fits a short stage.
    pub type_scale: f32,
    /// The stage's margin at each side: overscan-safe on a TV.
    pub margin_dp: f32,
    /// The hero's share of the stage's height.
    pub hero_height: f32,
    /// Columns of the cover grid.
    pub columns: u32,
    /// The smallest target, in design dp: 48 system dp or more once multiplied by `ui`.
    pub touch_min_dp: f32,
    /// The width the stage's content is held to.
    pub stage_max_width_dp: f32,
    /// The width a hero's text is held to.
    pub text_max_width_dp: f32,
}

impl Scale {
    /// The stage's size in design dp.
    pub fn stage(&self, viewport: &Viewport) -> (f32, f32) {
        (viewport.width_dp / self.ui, viewport.height_dp / self.ui)
    }

    /// The smallest target in system dp.
    pub fn touch_min_system_dp(&self) -> f32 {
        self.touch_min_dp * self.ui
    }
}

fn user(viewport: &Viewport) -> f32 {
    (viewport.user_scale_pct.clamp(50, 300) as f32) / 100.0
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() && value > 0.0 { value } else { fallback }
}

/// The factor on the system's density.
fn ui(viewport: &Viewport, width: f32, height: f32) -> f32 {
    let user = user(viewport);
    match viewport.form {
        // Android's density is right for a hand; only the user's own size applies.
        FormFactor::Phone | FormFactor::Handheld => user,
        FormFactor::Tv => {
            // One canvas for every TV: a set that reports 1920x1080 dp for 4K draws like one that
            // reports 960x540. The shorter side decides, so an ultrawide panel is not blown up.
            let canvas = (width / TV_CANVAS_DP.0).min(height / TV_CANVAS_DP.1);
            let compact = if viewport.tv_compact { TV_COMPACT } else { 1.0 };
            // Never under 72 lines of 1080 for a 48 dp target: 0.75 of the canvas.
            (canvas * user * compact).max(canvas * 0.75).clamp(0.5, 4.0)
        }
        FormFactor::Desktop => {
            if viewport.user_scale_pct != 100 {
                // The user's own size is exact, as it always was.
                user
            } else if height > DESKTOP_UNSCALED_HEIGHT {
                // #146: a 4K screen at the system's 100 % draws at the size 1080p has.
                ((height / DESKTOP_FIT_HEIGHT) * 20.0).round().clamp(20.0, 60.0) / 20.0
            } else {
                1.0
            }
        }
    }
}

/// The size and placement for `viewport`.
pub fn scale(viewport: &Viewport) -> Scale {
    let width = finite(viewport.width_dp, 360.0);
    let height = finite(viewport.height_dp, 640.0);
    let ui = ui(viewport, width, height);
    let (stage_w, stage_h) = (width / ui, height / ui);
    let landscape = stage_w >= stage_h;

    let density = match viewport.form {
        FormFactor::Tv => 1.0,
        // #144: a 720p handheld (360 to 480 dp tall) keeps its hero on one screen.
        _ if landscape => (stage_h / 540.0).clamp(0.75, 1.0),
        _ => 1.0,
    };
    let type_scale = match viewport.form {
        FormFactor::Tv => 1.0,
        _ if landscape => (stage_h / 600.0).clamp(0.72, 1.0),
        _ => (stage_w / 360.0).clamp(0.85, 1.0),
    };
    let margin_dp = match viewport.form {
        FormFactor::Phone => 16.0,
        FormFactor::Handheld => {
            if stage_h >= 400.0 { 24.0 } else { 16.0 }
        }
        FormFactor::Tv => 48.0,
        FormFactor::Desktop => {
            if stage_w >= 1100.0 {
                48.0
            } else if stage_w >= 800.0 {
                32.0
            } else {
                24.0
            }
        }
    };
    let hero_height = match viewport.form {
        FormFactor::Phone if !landscape => 0.46,
        FormFactor::Phone => 0.62,
        FormFactor::Handheld => 0.58,
        FormFactor::Tv => {
            if viewport.tv_compact { 0.5 } else { 0.56 }
        }
        // A tall window does not need a taller hero: about 400 dp of it.
        FormFactor::Desktop => (400.0 / stage_h).clamp(0.36, 0.56),
    };
    let stage_max_width_dp = STAGE_MAX_WIDTH_DP;
    let content = stage_w.min(stage_max_width_dp) - 2.0 * margin_dp;
    let (cover, gap, floor) = match viewport.form {
        FormFactor::Phone => (100.0, 12.0, 3),
        FormFactor::Handheld => (124.0, 14.0, 4),
        FormFactor::Tv => (112.0, 14.0, 6),
        FormFactor::Desktop => (152.0, 16.0, 3),
    };
    let fit = ((content + gap) / (cover + gap)).floor().max(1.0) as u32;
    let columns = match viewport.form {
        // A TV's grid is the reference's seven, eight where the screen lays out compact.
        FormFactor::Tv => {
            if viewport.tv_compact { 8 } else { 7 }
        }
        _ => fit.clamp(floor, MAX_COLUMNS),
    };
    let touch_min_dp = match viewport.form {
        // Focus, not a finger: the design's 48 dp, which `ui` keeps at 72 lines of 1080 or more.
        FormFactor::Tv => TOUCH_MIN_DP,
        _ => TOUCH_MIN_DP / ui.min(1.0),
    };
    Scale {
        ui,
        density,
        type_scale,
        margin_dp,
        hero_height,
        columns,
        touch_min_dp,
        stage_max_width_dp,
        text_max_width_dp: TEXT_MAX_WIDTH_DP.min(content.max(240.0)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(form: FormFactor, width_dp: f32, height_dp: f32) -> Viewport {
        Viewport { width_dp, height_dp, dpi: 0.0, form, user_scale_pct: 100, tv_compact: false }
    }

    /// The lines of a 1080-line picture a 48 dp target covers on a TV.
    fn tv_target_lines(viewport: &Viewport) -> f32 {
        let s = scale(viewport);
        s.touch_min_system_dp() / viewport.height_dp * 1080.0
    }

    #[test]
    fn a_tv_is_one_canvas_whatever_density_it_reports() {
        // 1080p at xhdpi, 4K reported as 1920x1080 dp, 720p at tvdpi, 4K at xxxhdpi.
        for (w, h) in [(960.0, 540.0), (1920.0, 1080.0), (961.5, 540.8), (960.0, 540.0), (1280.0, 720.0)] {
            let viewport = view(FormFactor::Tv, w, h);
            let s = scale(&viewport);
            let (stage_w, stage_h) = s.stage(&viewport);
            assert!((stage_w - 960.0).abs() < 2.0 && (stage_h - 540.0).abs() < 2.0, "{w}x{h}: {stage_w}x{stage_h}");
            assert_eq!(s.margin_dp, 48.0, "overscan-safe");
            assert_eq!(s.columns, 7);
            assert!((tv_target_lines(&viewport) - 96.0).abs() < 0.5);
        }
    }

    #[test]
    fn a_tv_target_never_drops_under_72_lines_of_1080() {
        for pct in [50, 75, 85, 100, 115, 150] {
            for compact in [false, true] {
                for (w, h) in [(960.0, 540.0), (1920.0, 1080.0), (1280.0, 720.0), (1280.0, 540.0)] {
                    let viewport = Viewport { user_scale_pct: pct, tv_compact: compact, ..view(FormFactor::Tv, w, h) };
                    assert!(tv_target_lines(&viewport) >= 72.0 - 0.01, "{pct}% compact={compact} {w}x{h}: {}", tv_target_lines(&viewport));
                }
            }
        }
        let compact = scale(&Viewport { tv_compact: true, ..view(FormFactor::Tv, 960.0, 540.0) });
        assert!((compact.ui - TV_COMPACT).abs() < 1e-6);
        assert_eq!(compact.columns, 8);
    }

    #[test]
    fn a_target_is_48_system_dp_or_more_under_every_hand() {
        for form in [FormFactor::Phone, FormFactor::Handheld, FormFactor::Desktop] {
            for pct in [50, 75, 85, 100, 125, 200] {
                for (w, h) in [(360.0, 640.0), (640.0, 360.0), (853.0, 480.0), (1280.0, 720.0), (1280.0, 800.0), (3840.0, 2160.0), (5120.0, 1440.0)] {
                    let viewport = Viewport { user_scale_pct: pct, ..view(form, w, h) };
                    let s = scale(&viewport);
                    assert!(s.touch_min_system_dp() >= TOUCH_MIN_DP - 0.01, "{form:?} {pct}% {w}x{h}: {}", s.touch_min_system_dp());
                }
            }
        }
    }

    #[test]
    fn a_720p_handheld_tightens_its_paddings_and_keeps_its_targets() {
        // 1280x720 at density 2.0 and at 1.5.
        let small = scale(&view(FormFactor::Handheld, 640.0, 360.0));
        assert_eq!(small.ui, 1.0, "text and targets keep Android's size");
        assert_eq!(small.density, 0.75);
        assert_eq!(small.margin_dp, 16.0);
        assert!(small.type_scale < 0.75);
        assert_eq!(small.touch_min_dp, 48.0);
        assert!(small.columns >= 4);
        let roomy = scale(&view(FormFactor::Handheld, 853.0, 480.0));
        assert!(roomy.density > small.density && roomy.density < 1.0);
        assert_eq!(roomy.margin_dp, 24.0);
        assert_eq!(roomy.columns, 5);
        let tablet = scale(&view(FormFactor::Handheld, 1280.0, 800.0));
        assert_eq!((tablet.density, tablet.type_scale), (1.0, 1.0));
    }

    #[test]
    fn a_phone_keeps_three_covers_to_a_row() {
        for width in [320.0, 360.0, 393.0, 412.0, 430.0] {
            let s = scale(&view(FormFactor::Phone, width, 800.0));
            assert_eq!(s.columns, 3, "{width} dp");
            assert_eq!(s.margin_dp, 16.0);
            assert_eq!(s.ui, 1.0);
        }
        assert!(scale(&view(FormFactor::Phone, 320.0, 640.0)).type_scale < 1.0);
        assert_eq!(scale(&view(FormFactor::Phone, 673.0, 841.0)).columns, 5, "an unfolded pane");
    }

    #[test]
    fn a_desktop_at_its_default_size_fits_an_unscaled_4k_screen() {
        assert_eq!(scale(&view(FormFactor::Desktop, 1280.0, 800.0)).ui, 1.0);
        assert_eq!(scale(&view(FormFactor::Desktop, 1920.0, 1080.0)).ui, 1.0);
        assert_eq!(scale(&view(FormFactor::Desktop, 2560.0, 1440.0)).ui, 1.0, "a 1440p monitor is not a TV");
        assert_eq!(scale(&view(FormFactor::Desktop, 2560.0, 1600.0)).ui, 1.0);
        assert_eq!(scale(&view(FormFactor::Desktop, 3840.0, 2160.0)).ui, 2.0, "#146");
        assert_eq!(scale(&view(FormFactor::Desktop, 5120.0, 2880.0)).ui, 2.65);
        // The user's own size is exact, on any screen.
        let own = Viewport { user_scale_pct: 150, ..view(FormFactor::Desktop, 3840.0, 2160.0) };
        assert_eq!(scale(&own).ui, 1.5);
        let small = Viewport { user_scale_pct: 85, ..view(FormFactor::Desktop, 1280.0, 800.0) };
        assert_eq!(scale(&small).ui, 0.85);
    }

    #[test]
    fn a_wide_desktop_holds_its_stage_and_its_text() {
        let desk = scale(&view(FormFactor::Desktop, 1280.0, 800.0));
        assert_eq!((desk.columns, desk.margin_dp), (7, 48.0));
        assert_eq!(desk.hero_height, 0.5);
        let narrow = scale(&view(FormFactor::Desktop, 900.0, 700.0));
        assert_eq!((narrow.columns, narrow.margin_dp), (5, 32.0));
        for (w, h) in [(2560.0, 1080.0), (3440.0, 1440.0), (5120.0, 1440.0)] {
            let s = scale(&view(FormFactor::Desktop, w, h));
            assert_eq!(s.columns, MAX_COLUMNS, "{w}x{h}");
            assert_eq!(s.stage_max_width_dp, STAGE_MAX_WIDTH_DP);
            assert_eq!(s.text_max_width_dp, TEXT_MAX_WIDTH_DP);
            assert!(s.hero_height >= 0.36 && s.hero_height <= 0.56);
        }
    }

    #[test]
    fn nonsense_from_the_system_still_gives_a_layout() {
        for (w, h) in [(0.0, 0.0), (f32::NAN, 500.0), (-10.0, f32::INFINITY)] {
            for form in [FormFactor::Phone, FormFactor::Handheld, FormFactor::Tv, FormFactor::Desktop] {
                let s = scale(&Viewport { user_scale_pct: 0, ..view(form, w, h) });
                assert!(s.ui.is_finite() && s.ui > 0.0);
                assert!(s.columns >= 1 && s.columns <= MAX_COLUMNS);
                assert!(s.text_max_width_dp >= 240.0);
            }
        }
    }
}
