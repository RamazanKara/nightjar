//! Pure stream display, pacing and bitrate math shared across UI, launch and stream modules.

use crate::config::{self, Settings};
use crate::strings::trf;

/// What the display offers, for "native" resolution and "display" refresh.
#[derive(Clone, Copy, Debug)]
pub struct Display {
    pub width: u32,
    pub height: u32,
    /// The refresh rate pacing uses: the display's, else 60 Hz.
    pub refresh_hz: f32,
    /// The display reported `refresh_hz`; otherwise it is the 60 Hz fallback.
    pub refresh_reported: bool,
}

impl Display {
    /// The refresh rate the overlay judges frames against: only a reported one.
    pub fn reported_refresh_hz(&self) -> Option<f32> {
        (self.refresh_reported && self.refresh_hz.is_finite() && self.refresh_hz > 0.0).then_some(self.refresh_hz)
    }

    /// The shortest interval between presents, for a display that reported its refresh rate.
    /// Without one there is no floor: guessing 60 Hz would cap every stream at 60 frames a second
    /// on a panel that runs faster, while a real 60 Hz display already spaces frames by its own
    /// refresh through V-Sync.
    pub fn refresh_interval_us(&self) -> Option<i64> {
        self.reported_refresh_hz().map(|hz| (1_000_000.0 / hz) as i64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacingMode {
    HostTimes,
    Immediate,
    /// Immediate, with the slices of a frame presented ahead of the beam (B10 step 4). Never
    /// chosen by `pacing`: the stream switches to it when the display's probe allows.
    BeamRace,
    Vsync,
}

impl PacingMode {
    pub fn label(self) -> &'static str {
        match self {
            PacingMode::HostTimes => "VRR, PC frame times",
            PacingMode::Immediate => "Immediate",
            PacingMode::BeamRace => "Beam race",
            PacingMode::Vsync => "V-Sync",
        }
    }
}

/// The pacing without VRR: V-Sync, or immediate when the settings turn V-Sync off and the frame
/// pacing is the lowest latency (every other frame pacing waits for the refresh, and so do the
/// PC's frame times, whose pictures go out for a refresh).
pub fn without_vrr(settings: &Settings) -> PacingMode {
    if settings.vsync || settings.frame_pacing.forces_vsync() || settings.host_frame_times {
        PacingMode::Vsync
    } else {
        PacingMode::Immediate
    }
}

/// Relative-to-Native scale steps offered in the desktop Resolution menu below full Native (100%):
/// nightjar-policy's `resolution_scale::SCALES` (85, 75, 67 and 50 %), the same steps as Android's.
pub const NATIVE_SCALES: [u32; 4] = {
    let s = nightjar_policy::resolution_scale::SCALES;
    [s[1], s[2], s[3], s[4]]
};

/// Normalizes a saved `native_scale` percentage: `0` and values `>= 100` mean full Native (100%),
/// smaller values stay within `25..=100`.
pub fn normalize_native_scale(scale_percent: u32) -> u32 {
    nightjar_policy::resolution_scale::normalize(scale_percent)
}

/// Even-aligned stream resolution for `scale_percent` of `display`'s native size.
/// Preserves the active display's aspect ratio while enforcing encoder-safe even dimensions
/// floored at 320x240; 67 % is exactly two thirds (1080p gives 1280x720).
pub fn scaled_native_resolution(display: Display, scale_percent: u32) -> (u32, u32) {
    nightjar_policy::resolution_scale::scaled(display.width, display.height, scale_percent)
}

/// The stream's resolution, rate and bitrate from the settings and display. The bitrate is the one
/// for the chosen codec (`start_bitrate`), PyroWave's own while PyroWave is chosen, as the settings
/// show it; a launch decides by what it offers the PC (`offers_pyrowave`).
pub fn stream_mode(settings: &Settings, display: Display) -> (u32, u32, u32, u32) {
    let (mut w, mut h) = if settings.width == 0 || settings.height == 0 {
        scaled_native_resolution(display, settings.native_scale)
    } else {
        (settings.width, settings.height)
    };
    w = (w.max(320) + 1) & !1;
    h = (h.max(240) + 1) & !1;
    let fps = if settings.fps == 0 {
        (display.refresh_hz.round() as u32).clamp(30, 480)
    } else {
        settings.fps
    };
    (w, h, fps, start_bitrate(settings, w, h, fps, settings.codec.is_pyrowave(), 0))
}

/// The bitrate a stream starts at: the chosen one, or the default for its size and frame rate. A
/// PyroWave stream (intra-only, it breaks up at the conventional default) starts at PyroWave's own
/// default instead, within `link_mbps` (the wired link toward the PC, 0 when unknown as in the
/// settings), or at the chosen one raised to PyroWave's floor (`nightjar_policy::pyrowave`).
pub fn start_bitrate(settings: &Settings, w: u32, h: u32, fps: u32, pyrowave: bool, link_mbps: u32) -> u32 {
    if pyrowave {
        nightjar_policy::pyrowave::start_kbps(settings.bitrate_kbps, settings.bitrate_kbps == 0, w, h, fps, link_mbps)
    } else if settings.bitrate_kbps == 0 {
        config::default_bitrate_kbps(w, h, fps)
    } else {
        settings.bitrate_kbps
    }
}

/// The bitrate setting's warning when PyroWave is chosen and the chosen bitrate is below the level
/// where the project's test scenes kept detail for the stream's `w`x`h` at `fps`, naming it. The
/// bitrate is kept as chosen; the automatic one never goes below that level.
pub fn pyrowave_bitrate_warning(settings: &Settings, w: u32, h: u32, fps: u32) -> Option<String> {
    if !settings.codec.is_pyrowave() || settings.bitrate_kbps == 0 {
        return None;
    }
    pyrowave_minimum_notice(settings.bitrate_kbps, w, h, fps)
}

/// The stream's start notice when the bitrate it actually sends (`kbps`, after the client's and the
/// host's caps) is below the level where the project's test scenes kept detail for `w`x`h` at
/// `fps`, naming it. `None` at or above it, and `None` for a stream that fell back to another
/// codec: the caller decides by the negotiated format.
pub fn pyrowave_minimum_notice(kbps: u32, w: u32, h: u32, fps: u32) -> Option<String> {
    let minimum = nightjar_policy::pyrowave::minimum_kbps(w, h, fps);
    (kbps < minimum).then(|| {
        trf(
            "Below the level where the project's test scenes kept detail for PyroWave: {} Mbps or more at {}x{}, {} fps. Text and textures may lose detail; raise the bitrate or use HEVC or AV1.",
            &[&minimum.div_ceil(1000), &w, &h, &fps],
        )
    })
}
