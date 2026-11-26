//! The launcher's size on the desktop (Nightjar #146): the Interface size setting's Auto (its
//! default, stored as 0) through `nightjar_policy::viewport_scale`, the rule Android shares. A
//! screen its system left unscaled and taller than 1600 points (a 4K monitor or TV at 100 %, a
//! Steam Deck docked to one) fits the interface to its height, so it reads as on 1080p; any size
//! the user chose applies exactly as before.

use nightjar_policy::viewport_scale::{scale, FormFactor, Viewport};

/// Auto's size in percent for a monitor `height_points` tall (the OS scale divided out) at the
/// OS scale `os_scale`. A system that already scales its screen keeps 100 %, and so does a screen
/// whose size is not known.
pub fn auto_percent(height_points: Option<f32>, os_scale: f32) -> u32 {
    let Some(height) = height_points.filter(|h| h.is_finite() && *h > 0.0) else {
        return 100;
    };
    if os_scale > 1.01 {
        return 100;
    }
    let viewport = Viewport {
        // Only the height decides on the desktop; a 16:9 width keeps the layout's other numbers sane.
        width_dp: height * 16.0 / 9.0,
        height_dp: height,
        dpi: 0.0,
        form: FormFactor::Desktop,
        user_scale_pct: 100,
        tv_compact: false,
    };
    let largest = crate::config::UI_ZOOM_STEPS.iter().copied().max().unwrap_or(250);
    // Never past the largest size the setting offers, as before.
    ((scale(&viewport).ui * 100.0).round() as u32).min(largest)
}

/// The launcher's zoom in percent for the Interface size setting `ui_zoom` (0 is Auto): Auto
/// follows the screen, any other size is that step exactly.
pub fn zoom_percent(ui_zoom: u32, height_points: Option<f32>, os_scale: f32) -> u32 {
    if ui_zoom == 0 {
        auto_percent(height_points, os_scale)
    } else {
        crate::config::nearest_zoom(ui_zoom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_size_is_unchanged_on_ordinary_screens() {
        assert_eq!(zoom_percent(0, Some(1080.0), 1.0), 100, "1080p");
        assert_eq!(zoom_percent(0, Some(800.0), 1.0), 100, "a Deck's own 1280x800 panel");
        assert_eq!(zoom_percent(0, Some(1440.0), 1.0), 100, "1440p unscaled keeps today's size");
        assert_eq!(zoom_percent(0, Some(1600.0), 1.0), 100, "2560x1600");
        assert_eq!(zoom_percent(0, None, 1.0), 100, "an unknown screen");
    }

    #[test]
    fn the_default_size_fits_an_unscaled_4k_screen() {
        assert_eq!(zoom_percent(0, Some(2160.0), 1.0), 200, "4K at 100 %, a docked Deck on a TV");
        assert_eq!(zoom_percent(0, Some(1440.0), 1.5), 100, "4K at 150 %: the OS already scales");
        assert_eq!(zoom_percent(0, Some(1080.0), 2.0), 100, "4K at 200 %");
        assert_eq!(zoom_percent(0, Some(1728.0), 1.25), 100, "4K at 125 %: the OS scales too");
        assert_eq!(zoom_percent(0, Some(4320.0), 1.0), 250, "8K: the largest size the setting has");
    }

    #[test]
    fn a_size_the_user_chose_is_exact() {
        // The setting's steps are 85, 100, 115, 130, 150, 200 and 250 %; each applies as it is.
        for step in crate::config::UI_ZOOM_STEPS {
            assert_eq!(zoom_percent(step, Some(1080.0), 1.0), step);
            assert_eq!(zoom_percent(step, Some(2160.0), 1.0), step, "{step} % even on 4K");
        }
        assert_eq!(zoom_percent(125, Some(2160.0), 1.0), 130, "a hand-written size snaps to its step, as before");
        assert_eq!(zoom_percent(100, Some(2160.0), 1.0), 100, "a chosen 100 % is not Auto");
        assert_eq!(zoom_percent(85, Some(800.0), 1.0), 85);
    }
}
