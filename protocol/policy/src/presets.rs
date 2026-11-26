//! Stream presets, Android's `StreamPreset`: one-tap sets of resolution, frame rate, bitrate, frame
//! pacing, upscaling, VRR and performance settings. The codec, HDR and 4:4:4 choices stay as they
//! are, and a PC's profile still takes priority over them.
//!
//! What both platforms share is here: which values a preset sets and when the settings read as
//! one. Each platform maps them onto its own settings. Bitrate, VRR and upscaling go back to their
//! defaults: on Android the codec's bitrate for the preset's size and rate, VRR off and no
//! upscaling; on desktop Automatic bitrate, VRR where the display has it and bilinear scaling. The
//! resolution is this screen's own, or two thirds of it for Wi-Fi / Away
//! ([`crate::resolution_scale::scaled`] at [`WIFI_AWAY_SCALE`]).

/// A stream preset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    /// This screen's resolution at its top refresh rate, with every latency setting on.
    Native,
    /// This screen's resolution at its top refresh rate, paced for the smoothest picture.
    SmoothTv,
    /// Two thirds of this screen's resolution at 60 fps, balanced pacing: for a weaker network.
    WifiAway,
    /// This screen's resolution at 30 fps, with the panel slowed to match and the GPU left alone.
    BatterySaver,
}

/// Frame pacing as a preset sets it (Android's `frame_pacing` values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pacing {
    /// The newest picture at each refresh: the lowest latency, and the default.
    Latency,
    /// No more pictures a second than the stream's frame rate.
    CapFps,
    /// One picture per refresh, the newest: even motion for about a refresh more latency.
    Balanced,
    /// Every picture waits for a refresh: the smoothest motion, for more latency.
    Smoothness,
}

impl Pacing {
    /// Android's saved `frame_pacing` value.
    pub fn key(self) -> &'static str {
        match self {
            Self::Latency => "latency",
            Self::CapFps => "cap-fps",
            Self::Balanced => "balanced",
            Self::Smoothness => "smoothness",
        }
    }
}

/// Battery saver's frame rate.
pub const BATTERY_SAVER_FPS: u32 = 30;

/// Wi-Fi / Away's frame rate, never above the screen's top rate.
pub const WIFI_AWAY_FPS: u32 = 60;

/// Wi-Fi / Away's resolution: two thirds of this screen's (a [`crate::resolution_scale`] step).
pub const WIFI_AWAY_SCALE: u32 = 67;

/// The power settings only Android has: they make a preset Custom only where they exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Power {
    /// Slow the panel to the stream's frame rate (`checkbox_reduce_refresh_rate`).
    pub reduce_refresh: bool,
    /// Ask the decoder for its performance mode (`checkbox_codec_performance`).
    pub codec_performance: bool,
    /// Keep the GPU at its top clocks (`checkbox_gpu_max_clocks`).
    pub gpu_max_clocks: bool,
}

/// The settings a preset is compared with, as the platform reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Current {
    /// The resolution as a share of this screen's: `Some(100)` for its own, `Some(67)` for two
    /// thirds, `None` for any other size.
    pub resolution_scale: Option<u32>,
    /// The screen's top rate as a stream frame rate ([`stream_frame_rate`]), where the platform
    /// knows it; Wi-Fi / Away never streams above it.
    pub screen_fps: Option<u32>,
    /// The frame rate follows the screen's top refresh rate.
    pub fps_follows_screen: bool,
    /// The fixed frame rate, where it doesn't follow the screen.
    pub fps: u32,
    /// The bitrate is the default for this resolution, frame rate and codec.
    pub default_bitrate: bool,
    /// VRR is at its default, or not offered on this display (then any saved choice reads as
    /// the default, as on Android).
    pub default_vrr: bool,
    /// Upscaling is at its default.
    pub default_upscaling: bool,
    /// The frame pacing, where it is one a preset sets.
    pub pacing: Option<Pacing>,
    /// Every latency switch this platform has is on.
    pub latency_switches_on: bool,
    /// Android's power settings; `None` on a platform without them.
    pub power: Option<Power>,
}

impl Preset {
    /// The presets in the chips' order; [`chosen`] takes the first that matches.
    pub const ALL: [Self; 4] = [Self::Native, Self::SmoothTv, Self::WifiAway, Self::BatterySaver];

    /// Its name, as the chips show it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "Native (recommended)",
            Self::SmoothTv => "Smooth TV",
            Self::WifiAway => "Wi-Fi / Away",
            Self::BatterySaver => "Battery saver",
        }
    }

    /// The frame rate it streams at: `None` for the screen's own top refresh rate
    /// ([`stream_frame_rate`]). Wi-Fi / Away's 60 fps is never above `screen_fps`, the screen's
    /// top rate where the platform knows it.
    pub fn fps(self, screen_fps: Option<u32>) -> Option<u32> {
        match self {
            Self::Native | Self::SmoothTv => None,
            Self::WifiAway => Some(screen_fps.map_or(WIFI_AWAY_FPS, |top| WIFI_AWAY_FPS.min(top.max(1)))),
            Self::BatterySaver => Some(BATTERY_SAVER_FPS),
        }
    }

    /// Its resolution as a share of this screen's, in percent ([`crate::resolution_scale`]).
    pub fn resolution_scale(self) -> u32 {
        match self {
            Self::WifiAway => WIFI_AWAY_SCALE,
            Self::Native | Self::SmoothTv | Self::BatterySaver => 100,
        }
    }

    pub fn pacing(self) -> Pacing {
        match self {
            Self::Native => Pacing::Latency,
            Self::SmoothTv => Pacing::Smoothness,
            Self::WifiAway => Pacing::Balanced,
            Self::BatterySaver => Pacing::CapFps,
        }
    }

    /// Whether it turns every latency switch back on. The others leave them as they are.
    pub fn sets_latency_switches(self) -> bool {
        self == Self::Native
    }

    /// Android's power settings it sets: Battery saver slows the panel, drops the codec's
    /// performance mode and the GPU's top clocks; the others undo the first two and leave the
    /// clocks, which are opt-in, alone (`None` there).
    pub fn power(self) -> (bool, bool, Option<bool>) {
        match self {
            Self::Native | Self::SmoothTv | Self::WifiAway => (false, true, None),
            Self::BatterySaver => (true, false, Some(false)),
        }
    }

    /// Whether these settings are this preset's.
    pub fn matches(self, current: &Current) -> bool {
        let fps = match self.fps(current.screen_fps) {
            None => current.fps_follows_screen,
            Some(fps) => !current.fps_follows_screen && current.fps == fps,
        };
        if current.resolution_scale != Some(self.resolution_scale())
            || !fps
            || !current.default_bitrate
            || !current.default_vrr
            || !current.default_upscaling
            || current.pacing != Some(self.pacing())
        {
            return false;
        }
        if let Some(power) = current.power {
            let (reduce_refresh, codec_performance, gpu_max_clocks) = self.power();
            if power.reduce_refresh != reduce_refresh
                || power.codec_performance != codec_performance
                || gpu_max_clocks.is_some_and(|on| power.gpu_max_clocks != on)
            {
                return false;
            }
        }
        !self.sets_latency_switches() || current.latency_switches_on
    }
}

/// The preset these settings are, or `None` for Custom.
pub fn chosen(current: &Current) -> Option<Preset> {
    Preset::ALL.into_iter().find(|preset| preset.matches(current))
}

/// The screen's top refresh rate as a stream frame rate, Android's
/// `DisplayFrameRatePolicy.streamFrameRate`: rounded, in the host's 1–1000 Hz launch range.
pub fn stream_frame_rate(panel_hz: f32) -> u32 {
    if panel_hz.is_nan() {
        return 1;
    }
    panel_hz.round().clamp(1.0, 1000.0) as u32
}


#[cfg(test)]
mod tests {
    use super::*;

    fn native() -> Current {
        Current {
            resolution_scale: Some(100),
            screen_fps: None,
            fps_follows_screen: true,
            fps: 0,
            default_bitrate: true,
            default_vrr: true,
            default_upscaling: true,
            pacing: Some(Pacing::Latency),
            latency_switches_on: true,
            power: None,
        }
    }

    fn battery() -> Current {
        Current { fps_follows_screen: false, fps: 30, pacing: Some(Pacing::CapFps), latency_switches_on: false, ..native() }
    }

    fn smooth_tv() -> Current {
        Current { pacing: Some(Pacing::Smoothness), latency_switches_on: false, ..native() }
    }

    fn wifi_away() -> Current {
        Current { resolution_scale: Some(67), fps_follows_screen: false, fps: 60, pacing: Some(Pacing::Balanced), latency_switches_on: false, ..native() }
    }

    /// What a platform reads after it set `preset`'s values (`latency_switches_on` as it was).
    fn set(preset: Preset, screen_fps: Option<u32>, latency_switches_on: bool) -> Current {
        let fps = preset.fps(screen_fps);
        Current {
            resolution_scale: Some(preset.resolution_scale()),
            screen_fps,
            fps_follows_screen: fps.is_none(),
            fps: fps.unwrap_or(0),
            pacing: Some(preset.pacing()),
            latency_switches_on: preset.sets_latency_switches() || latency_switches_on,
            ..native()
        }
    }

    #[test]
    fn the_defaults_are_native() {
        assert_eq!(chosen(&native()), Some(Preset::Native));
        assert_eq!(chosen(&battery()), Some(Preset::BatterySaver));
        assert_eq!(chosen(&smooth_tv()), Some(Preset::SmoothTv));
        assert_eq!(chosen(&wifi_away()), Some(Preset::WifiAway));
    }

    #[test]
    fn each_preset_set_reads_as_itself_and_no_other() {
        for screen_fps in [None, Some(30), Some(60), Some(120)] {
            for switches in [false, true] {
                for preset in Preset::ALL {
                    let current = set(preset, screen_fps, switches);
                    assert_eq!(chosen(&current), Some(preset), "{current:?}");
                    for other in Preset::ALL.into_iter().filter(|o| *o != preset) {
                        assert!(!other.matches(&current), "{other:?} matches {preset:?}: {current:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_four_presets_differ_from_each_other() {
        let values = |p: Preset| (p.resolution_scale(), p.fps(Some(120)), p.pacing(), p.sets_latency_switches(), p.power());
        for a in Preset::ALL {
            for b in Preset::ALL.into_iter().filter(|b| *b != a) {
                assert_ne!(values(a), values(b), "{a:?} and {b:?}");
            }
        }
        assert_eq!(Preset::ALL.map(Preset::label), ["Native (recommended)", "Smooth TV", "Wi-Fi / Away", "Battery saver"]);
    }

    #[test]
    fn native_and_battery_saver_keep_their_values() {
        assert_eq!((Preset::Native.resolution_scale(), Preset::Native.fps(Some(144)), Preset::Native.pacing()), (100, None, Pacing::Latency));
        assert_eq!((Preset::Native.sets_latency_switches(), Preset::Native.power()), (true, (false, true, None)));
        assert_eq!((Preset::BatterySaver.resolution_scale(), Preset::BatterySaver.fps(Some(24)), Preset::BatterySaver.pacing()), (100, Some(30), Pacing::CapFps));
        assert_eq!((Preset::BatterySaver.sets_latency_switches(), Preset::BatterySaver.power()), (false, (true, false, Some(false))));
    }

    #[test]
    fn smooth_tv_and_wifi_away_set_their_outcomes() {
        assert_eq!((Preset::SmoothTv.resolution_scale(), Preset::SmoothTv.fps(Some(120)), Preset::SmoothTv.pacing().key()), (100, None, "smoothness"));
        assert_eq!((Preset::WifiAway.resolution_scale(), Preset::WifiAway.pacing().key()), (67, "balanced"));
        assert_eq!(crate::resolution_scale::scaled(1920, 1080, Preset::WifiAway.resolution_scale()), (1280, 720), "two thirds");
        // 60 fps, never above the screen's top rate.
        assert_eq!(Preset::WifiAway.fps(None), Some(60));
        assert_eq!(Preset::WifiAway.fps(Some(120)), Some(60));
        assert_eq!(Preset::WifiAway.fps(Some(50)), Some(50));
        for preset in [Preset::SmoothTv, Preset::WifiAway] {
            assert!(!preset.sets_latency_switches(), "{preset:?} leaves the latency switches");
            assert_eq!(preset.power(), Preset::Native.power(), "{preset:?} powers like Native");
            assert!(preset.matches(&Current { latency_switches_on: true, ..set(preset, Some(120), false) }));
        }
        assert!(!Preset::WifiAway.matches(&Current { resolution_scale: Some(100), ..wifi_away() }));
        assert!(!Preset::WifiAway.matches(&Current { fps: 50, ..wifi_away() }), "50 fps only where the screen tops out there");
        assert!(Preset::WifiAway.matches(&Current { fps: 50, screen_fps: Some(50), ..wifi_away() }));
    }

    #[test]
    fn any_preset_value_changed_reads_as_custom() {
        let changes: [fn(&mut Current); 8] = [
            |c| c.resolution_scale = None,
            |c| c.resolution_scale = Some(75),
            |c| c.fps_follows_screen = false,
            |c| c.default_bitrate = false,
            |c| c.default_vrr = false,
            |c| c.default_upscaling = false,
            |c| c.pacing = None,
            |c| c.latency_switches_on = false,
        ];
        for change in changes {
            let mut current = native();
            change(&mut current);
            if current.fps_follows_screen {
                assert_eq!(chosen(&current), None, "{current:?}");
            } else {
                // A fixed rate other than 30 or 60 is no preset either.
                current.fps = 45;
                assert_eq!(chosen(&current), None, "{current:?}");
            }
        }
        for base in [smooth_tv(), wifi_away()] {
            // Not the frame rate's source (Wi-Fi / Away's is fixed already) or the latency switches.
            for (_, change) in changes.iter().enumerate().filter(|(i, _)| ![2, 7].contains(i)) {
                let mut current = base;
                change(&mut current);
                assert_eq!(chosen(&current), None, "{current:?}");
            }
        }
    }

    #[test]
    fn battery_saver_leaves_the_latency_switches_alone() {
        assert!(Preset::BatterySaver.matches(&Current { latency_switches_on: true, ..battery() }));
        assert!(!Preset::BatterySaver.matches(&Current { fps: 60, ..battery() }));
        assert!(Preset::Native.sets_latency_switches());
        assert!(!Preset::BatterySaver.sets_latency_switches());
    }

    #[test]
    fn android_power_settings_count_only_where_they_exist() {
        let native_power = Power { reduce_refresh: false, codec_performance: true, gpu_max_clocks: true };
        assert!(Preset::Native.matches(&Current { power: Some(native_power), ..native() }), "GPU clocks are Native's to leave");
        assert!(!Preset::Native.matches(&Current { power: Some(Power { reduce_refresh: true, ..native_power }), ..native() }));
        let saver = Power { reduce_refresh: true, codec_performance: false, gpu_max_clocks: false };
        assert!(Preset::BatterySaver.matches(&Current { power: Some(saver), ..battery() }));
        assert!(!Preset::BatterySaver.matches(&Current { power: Some(Power { gpu_max_clocks: true, ..saver }), ..battery() }));
        assert!(Preset::SmoothTv.matches(&Current { power: Some(native_power), ..smooth_tv() }));
        assert!(!Preset::WifiAway.matches(&Current { power: Some(saver), ..wifi_away() }));
    }

    #[test]
    fn frame_rates_follow_android() {
        assert_eq!(Preset::Native.fps(None), None);
        assert_eq!(Preset::BatterySaver.fps(None), Some(30));
        assert_eq!(Preset::Native.pacing().key(), "latency");
        assert_eq!(Preset::BatterySaver.pacing().key(), "cap-fps");
        assert_eq!(stream_frame_rate(119.88), 120);
        assert_eq!(stream_frame_rate(59.94), 60);
        assert_eq!(stream_frame_rate(0.0), 1);
        assert_eq!(stream_frame_rate(5000.0), 1000);
        assert_eq!(stream_frame_rate(f32::NAN), 1);
    }
}
