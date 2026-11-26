//! Stream resolutions relative to the screen's own: 100, 85, 75, 67 and 50 % of its native pixels,
//! the same steps in Android's and the desktop's Resolution picker (Nightjar #137). A step keeps
//! the screen's shape and gives sizes an encoder takes: even, and never under 320×240.

/// The steps the pickers offer, largest first. 100 % is the screen's own resolution.
pub const SCALES: [u32; 5] = [100, 85, 75, 67, 50];

/// The smallest stream either app starts.
pub const MIN_WIDTH: u32 = 320;
pub const MIN_HEIGHT: u32 = 240;

/// A saved scale as the pickers read it: `0` and anything from 100 up mean the screen's own
/// resolution, a smaller value stays within 25 to 100 %.
pub fn normalize(percent: u32) -> u32 {
    if percent == 0 || percent >= 100 {
        100
    } else {
        percent.clamp(25, 100)
    }
}

/// The exact ratio behind a step: 67 % is two thirds, so 1080p gives 720p and 2160p gives 1440p.
fn ratio(percent: u32) -> (u64, u64) {
    match percent {
        67 => (2, 3),
        33 => (1, 3),
        percent => (u64::from(percent), 100),
    }
}

/// The stream size for `percent` of a `native_width` × `native_height` screen: rounded to the
/// nearest pixel, then up to the next even number, with the smallest stream as the floor.
pub fn scaled(native_width: u32, native_height: u32, percent: u32) -> (u32, u32) {
    let percent = normalize(percent);
    let (width, height) = if percent == 100 {
        (native_width, native_height)
    } else {
        let (num, den) = ratio(percent);
        let scale = |value: u32| ((u64::from(value) * num + den / 2) / den) as u32;
        (scale(native_width), scale(native_height))
    };
    (even(width.max(MIN_WIDTH)), even(height.max(MIN_HEIGHT)))
}

fn even(value: u32) -> u32 {
    value.saturating_add(1) & !1
}

/// The steps a picker lists for this screen, largest first: every step whose size differs from
/// the one before it, so a small screen is not offered the floor twice.
pub fn choices(native_width: u32, native_height: u32) -> Vec<(u32, (u32, u32))> {
    let mut out: Vec<(u32, (u32, u32))> = Vec::with_capacity(SCALES.len());
    for percent in SCALES {
        let size = scaled(native_width, native_height, percent);
        if out.last().is_none_or(|(_, last)| *last != size) {
            out.push((percent, size));
        }
    }
    out
}

/// The step a saved size is, for a picker that stores width and height: `Some(percent)` when
/// the size is one of this screen's steps.
pub fn step_of(native_width: u32, native_height: u32, width: u32, height: u32) -> Option<u32> {
    choices(native_width, native_height).into_iter().find(|(_, size)| *size == (width, height)).map(|(percent, _)| percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_steps_of_common_screens() {
        assert_eq!(scaled(1920, 1080, 100), (1920, 1080));
        assert_eq!(scaled(1920, 1080, 85), (1632, 918));
        assert_eq!(scaled(1920, 1080, 75), (1440, 810));
        assert_eq!(scaled(1920, 1080, 67), (1280, 720));
        assert_eq!(scaled(1920, 1080, 50), (960, 540));
        assert_eq!(scaled(3840, 2160, 67), (2560, 1440));
        assert_eq!(scaled(3840, 2160, 50), (1920, 1080));
        assert_eq!(scaled(2560, 1440, 75), (1920, 1080));
    }

    #[test]
    fn sizes_are_even_and_keep_the_shape() {
        for (w, h) in [(1280, 720), (1920, 1200), (2400, 1080), (3440, 1440), (2208, 1840), (1366, 768), (2340, 1080)] {
            for percent in SCALES {
                let (sw, sh) = scaled(w, h, percent);
                assert_eq!((sw % 2, sh % 2), (0, 0), "{w}x{h} at {percent}%");
                let (native, scaled_shape) = (f64::from(w) / f64::from(h), f64::from(sw) / f64::from(sh));
                assert!((native - scaled_shape).abs() < 0.01, "{w}x{h} at {percent}%: {sw}x{sh}");
            }
        }
    }

    #[test]
    fn the_desktop_steps_of_6_4_1_are_unchanged() {
        // `scaled_native_resolution` rounded value*percent/100 and went up to even.
        assert_eq!(scaled(1920, 1080, 75), (1440, 810));
        assert_eq!(scaled(1920, 1080, 50), (960, 540));
        assert_eq!(scaled(2560, 1600, 75), (1920, 1200));
        assert_eq!(scaled(1366, 768, 75), (1026, 576));
        assert_eq!(scaled(1366, 768, 50), (684, 384));
    }

    #[test]
    fn a_saved_scale_reads_as_before() {
        assert_eq!(normalize(0), 100);
        assert_eq!(normalize(100), 100);
        assert_eq!(normalize(250), 100);
        assert_eq!(normalize(75), 75);
        assert_eq!(normalize(10), 25);
        assert_eq!(scaled(1920, 1080, 0), (1920, 1080));
        assert_eq!(scaled(1920, 1080, 60), (1152, 648), "a hand-written scale still works");
    }

    #[test]
    fn nothing_is_smaller_than_the_floor_and_no_step_repeats() {
        assert_eq!(scaled(480, 320, 50), (320, 240));
        assert_eq!(scaled(1919, 1079, 100), (1920, 1080), "an odd screen goes up to even");
        let tiny = choices(400, 300);
        assert_eq!(tiny.first(), Some(&(100, (400, 300))));
        assert!(tiny.windows(2).all(|pair| pair[0].1 != pair[1].1));
        assert_eq!(choices(1920, 1080).len(), SCALES.len());
    }

    #[test]
    fn a_saved_size_names_its_step() {
        assert_eq!(step_of(1920, 1080, 1280, 720), Some(67));
        assert_eq!(step_of(1920, 1080, 1920, 1080), Some(100));
        assert_eq!(step_of(2560, 1440, 1280, 720), Some(50));
        assert_eq!(step_of(1920, 1080, 1600, 900), None);
    }
}
