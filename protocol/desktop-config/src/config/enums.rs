//! Desktop settings choice enums and localized label helpers.

use serde::{Deserialize, Serialize};

use super::labels;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodecPreference {
    Auto,
    H264,
    Hevc,
    Av1,
    PyroWave,
    PyroWave444,
}

impl CodecPreference {
    pub fn is_pyrowave(self) -> bool {
        matches!(self, Self::PyroWave | Self::PyroWave444)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioChannels {
    Stereo,
    Surround51,
    Surround71,
    /// Offered where the output plays it (`stream::offers_714`); asked for only from a PC that
    /// reports it.
    Surround714,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowMode {
    /// Exclusive fullscreen at the stream's mode when the display has it.
    Fullscreen,
    /// A borderless window covering the display.
    Borderless,
    Windowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseMode {
    /// The pointer is captured and moves relatively, for games.
    Capture,
    /// The local pointer is the host's pointer, for the desktop.
    Absolute,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VideoScaling {
    #[default]
    Fit,
    Fill,
    Stretch,
    Native,
}

impl VideoScaling {
    pub fn label(self) -> &'static str {
        match self {
            Self::Fit => labels::FIT,
            Self::Fill => labels::FILL_CROP,
            Self::Stretch => labels::STRETCH,
            Self::Native => labels::NATIVE_PIXELS,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalogScrolling {
    Off,
    #[default]
    Right,
    Left,
}

/// The controller chord that leaves the stream (V+ PC's gamepad quit combo, ported as is). The
/// alternatives are for controllers or games where Start+Select+LB+RB is taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamepadQuitCombo {
    #[default]
    StartSelectLbRb,
    SelectLbRbY,
    StartLbRbA,
    StartLbRbB,
    LbRbXY,
    LbRbAB,
}

impl GamepadQuitCombo {
    /// Their names follow the connected controller (`glyphs::Style::quit_combo`).
    pub const ALL: [Self; 6] = [
        Self::StartSelectLbRb,
        Self::SelectLbRbY,
        Self::StartLbRbA,
        Self::StartLbRbB,
        Self::LbRbXY,
        Self::LbRbAB,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DecoderPreference {
    Auto,
    Software,
}

/// Where PyroWave frames are reconstructed: on the CPU with the Rust decoder, or on the GPU
/// by the PyroWave library's Vulkan decoder (docs/pyrowave-decode.md). `Gpu` never falls back
/// to the CPU: when the library or the adapter cannot do it, PyroWave is not offered and the
/// reason is shown. The default is `Gpu` on Windows and Linux (the ABBAs in docs/pyrowave-decode.md:
/// lower `receive_to_present_ms` at p50 and p95 and no lower fps on every leg) and `Cpu` on macOS.
/// There `Gpu` decodes through the bundled KosmicKrisp Vulkan driver on Apple silicon with macOS 26
/// or later and falls back to `Cpu` with a notice elsewhere (`pyrowave_mode::mac_choice`); it stays
/// a choice until a Mac measures it. A Linux client
/// without `libpyrowave-shared.so` (a source build; `package-linux.sh -s` ships it) fails the probe
/// like a Windows one without the DLL: PyroWave is not offered, with the reason and the way out
/// (switch to `Cpu`); every other codec works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PyroWaveDecoder {
    #[cfg_attr(target_os = "macos", default)]
    Cpu,
    #[cfg_attr(not(target_os = "macos"), default)]
    Gpu,
}

impl PyroWaveDecoder {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => labels::CPU,
            Self::Gpu => labels::GPU,
        }
    }
}

/// What draws the window on Linux (Windows always draws with Direct3D 11). `Auto` starts on OpenGL,
/// where VAAPI pictures arrive zero-copy (`dmabuf`), unless a saved setting needs SDL's "gpu"
/// renderer, and moves to it when a stream needs it (`handheld::renderer_switch`); `OpenGl` never
/// leaves OpenGL, so such a stream is refused; `Gpu` draws with the GPU renderer from the start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Renderer {
    #[default]
    Auto,
    OpenGl,
    Gpu,
}

impl Renderer {
    pub const ALL: [Self; 3] = [Self::Auto, Self::OpenGl, Self::Gpu];

    /// The menu's name; the two renderers are named as their APIs, in every language.
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => labels::AUTOMATIC,
            Self::OpenGl => "OpenGL",
            Self::Gpu => "GPU (Vulkan)",
        }
    }
}

/// Where Nightjar shows itself on Linux (`kms`): `Automatic` opens a window on the desktop, or,
/// with no Wayland or X11 session (a console or kiosk box), draws straight to the display through
/// DRM KMS; `Desktop` and `Kms` choose one. `--kms` and `--no-kms` choose for one start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayOutput {
    #[default]
    Automatic,
    Desktop,
    Kms,
}

impl DisplayOutput {
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))] // the menu is drawn only on Linux
    pub const ALL: [Self; 3] = [Self::Automatic, Self::Desktop, Self::Kms];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => labels::AUTOMATIC,
            Self::Desktop => labels::DESKTOP,
            Self::Kms => labels::STRAIGHT_TO_DISPLAY,
        }
    }
}

/// Which GPU draws and decodes on a Windows PC with two (`gpu_adapters`), for the whole app:
/// Automatic is Windows' choice, unless YUV 4:4:4 is asked for and only the high-performance GPU
/// decodes it; the others are Windows' per-app preference, set for Nightjar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphicsCard {
    #[default]
    Automatic,
    PowerSaving,
    HighPerformance,
}

impl GraphicsCard {
    #[cfg_attr(not(windows), allow(dead_code))] // the menu is drawn only on Windows
    pub const ALL: [Self; 3] = [Self::Automatic, Self::PowerSaving, Self::HighPerformance];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => labels::AUTOMATIC,
            Self::PowerSaving => labels::POWER_SAVING,
            Self::HighPerformance => labels::HIGH_PERFORMANCE,
        }
    }
}

/// How Tear-free VRR (`vrr_tear_free`, Windows) waits for a scan under way, "Whole frames under
/// VRR" in Display & image: Automatic is the wait for the beam (`present::VrrSync::Scan`) that turns
/// to sync interval 1 on a display whose presents start no scan (`scan_sync::UNTRUSTED_MISSES`,
/// kept across starts); Scan keeps that wait and does not take that switch, VSync presents with
/// sync interval 1 from the start (`VrrSync::Present1`). Both waits present with sync interval 1
/// when waiting for the scan would cost the next picture its turn (`scan_sync::scan_wait`), shown
/// whole at the next refresh. `NIGHTJAR_VRR_SYNC` overrides it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum VrrWait {
    #[default]
    Automatic,
    Scan,
    VSync,
}

/// The name of a rumble feel in the settings.
pub fn rumble_feel_label(feel: nightjar_policy::rumble::RumbleFeel) -> &'static str {
    use nightjar_policy::rumble::RumbleFeel;
    match feel {
        RumbleFeel::Standard => labels::STANDARD,
        RumbleFeel::Strong => labels::STRONG,
        RumbleFeel::Subtle => labels::SUBTLE,
    }
}

impl VrrWait {
    #[cfg_attr(not(windows), allow(dead_code))] // the menu is drawn only on Windows
    pub const ALL: [Self; 3] = [Self::Automatic, Self::Scan, Self::VSync];

    pub fn label(self) -> &'static str {
        match self {
            Self::Automatic => labels::AUTOMATIC,
            Self::Scan => labels::NIGHTJARS_WAIT,
            Self::VSync => labels::WINDOWS_VSYNC,
        }
    }
}

/// Frame pacing without VRR, Android's `frame_pacing` (`FramePacingPolicy`): how decoded pictures
/// wait for the display. `stream::present` applies it; with VRR it does not apply, as on Android.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FramePacing {
    /// The pacing before this setting: the newest picture at each refresh, or each picture at once
    /// with V-Sync off. Adds no delay.
    #[default]
    Latency,
    /// V-Sync whatever the V-Sync setting says: the newest picture at each refresh, no tearing.
    Balanced,
    /// V-Sync, and no more pictures a second than the stream's frame rate: a picture that comes
    /// early waits for its slot (at most one frame) instead of replacing the one before it.
    CapFps,
    /// V-Sync, every picture in turn, one a refresh: a picture is skipped only once it is more
    /// than a frame old and a newer one is due.
    Smoothness,
}

impl FramePacing {
    pub const ALL: [Self; 4] = [Self::Latency, Self::Balanced, Self::CapFps, Self::Smoothness];

    pub fn label(self) -> &'static str {
        match self {
            Self::Latency => labels::LOWEST_LATENCY,
            Self::Balanced => labels::BALANCED,
            Self::CapFps => labels::BALANCED_FPS_LIMIT,
            Self::Smoothness => labels::SMOOTHEST_VIDEO,
        }
    }

    /// Every mode but `Latency` waits for the refresh.
    pub fn forces_vsync(self) -> bool {
        self != Self::Latency
    }
}

/// Microphone echo cancellation, Android's `list_mic_echo_cancel` (`MicProcessing.Echo`):
/// automatic leaves it off only when this device plays through headphones or a headset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MicEcho {
    #[default]
    Auto,
    On,
    Off,
}

impl MicEcho {
    pub const ALL: [Self; 3] = [Self::Auto, Self::On, Self::Off];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => labels::AUTOMATIC,
            Self::On => labels::ON,
            Self::Off => labels::OFF,
        }
    }

    /// Whether echo cancellation runs: `private_output` is a playback device that keeps the
    /// stream's sound out of the microphone (headphones, a headset), when that is known.
    pub fn wanted(self, private_output: bool) -> bool {
        self == Self::On || (self == Self::Auto && !private_output)
    }
}

/// "Theme" (Android's `ThemeChoice`): System follows the system's light or dark setting, Light is
/// the day look even on a dark system, Dark is the night look, OLED black its black-surfaced
/// variant. High contrast wins over all four. Dark by default: the look desktop Nightjar had
/// before the choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Theme {
    System,
    #[default]
    Dark,
    Oled,
    /// Last, as in Android's `ThemeChoice`, so the stored values of the older choices stay.
    Light,
}

impl Theme {
    pub const ALL: [Self; 4] = [Self::System, Self::Dark, Self::Oled, Self::Light];

    /// Android's `theme_choice_names`.
    pub fn label(self) -> &'static str {
        match self {
            Self::System => labels::SYSTEM,
            Self::Dark => labels::DARK,
            Self::Oled => labels::OLED_BLACK,
            Self::Light => labels::LIGHT,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QualityMode {
    #[default]
    Off,
    Motion,
    Detail,
    Battery,
}

impl QualityMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => labels::OFF,
            Self::Motion => labels::PREFER_MOTION,
            Self::Detail => labels::PREFER_DETAIL,
            Self::Battery => labels::SAVE_POWER,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "StoredUpscaling")]
pub enum Upscaling {
    #[default]
    Bilinear,
    Fsr1,
    Sgsr1,
    /// The GPU vendor's video super resolution and, on NVIDIA, RTX Video HDR (`stream::enhance`);
    /// offered only where the probe finds them (Direct3D 11).
    Ai,
    /// macOS 13+ MetalFX spatial scaler, offered after a device probe.
    MetalFx,
}

/// How a stored upscaling reads: the removed Temporal (SGSR 2) choice, in the settings file and in every
/// PC, game and profile layer, becomes SGSR 1.
#[derive(Deserialize)]
enum StoredUpscaling {
    Bilinear,
    Fsr1,
    Sgsr1,
    Sgsr2,
    Ai,
    MetalFx,
}

impl From<StoredUpscaling> for Upscaling {
    fn from(stored: StoredUpscaling) -> Self {
        match stored {
            StoredUpscaling::Bilinear => Self::Bilinear,
            StoredUpscaling::Fsr1 => Self::Fsr1,
            StoredUpscaling::Sgsr1 | StoredUpscaling::Sgsr2 => Self::Sgsr1,
            StoredUpscaling::Ai => Self::Ai,
            StoredUpscaling::MetalFx => Self::MetalFx,
        }
    }
}

impl Upscaling {
    pub fn label(self) -> &'static str {
        match self {
            Self::Bilinear => "Bilinear",
            Self::Fsr1 => "FSR 1",
            Self::Sgsr1 => "SGSR 1",
            Self::Ai => labels::AI_ENHANCEMENT,
            Self::MetalFx => "MetalFX",
        }
    }

    /// The shader upscalers (`stream::upscale`), which take the sharpness setting.
    pub fn is_shader(self) -> bool {
        matches!(self, Self::Fsr1 | Self::Sgsr1)
    }
}

/// What the performance overlay shows while it is on, stored as Android's
/// `performance_overlay_mode` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayMode {
    /// Six lines of stream measurements, with the host's CPU and GPU when it offers them.
    #[serde(alias = "expanded")]
    Statistics,
    /// Every measurement and decoder diagnostic, with each GPU's readings in full.
    #[serde(alias = "advanced")]
    Debug,
    /// One line: rendered FPS, network round trip and frame loss behind a health mark. Also
    /// what an unknown value reads as, as Android's `normalizeMode`; keep it last: serde wants
    /// the catch-all there.
    #[default]
    #[serde(other)]
    Compact,
}

impl OverlayMode {
    pub const ALL: [Self; 3] = [Self::Compact, Self::Statistics, Self::Debug];

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => labels::COMPACT,
            Self::Statistics => labels::STATISTICS,
            Self::Debug => labels::DEBUG,
        }
    }
}

/// The corner the performance overlay sits in, stored as Android's
/// `performance_overlay_position` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayCorner {
    TopRight,
    BottomLeft,
    BottomRight,
    /// Also what an unknown value reads as; keep it last: serde wants the catch-all there.
    #[default]
    #[serde(other)]
    TopLeft,
}

impl OverlayCorner {
    pub const ALL: [Self; 4] = [Self::TopLeft, Self::TopRight, Self::BottomLeft, Self::BottomRight];

    pub fn label(self) -> &'static str {
        match self {
            Self::TopLeft => labels::TOP_LEFT,
            Self::TopRight => labels::TOP_RIGHT,
            Self::BottomLeft => labels::BOTTOM_LEFT,
            Self::BottomRight => labels::BOTTOM_RIGHT,
        }
    }
}

/// "When Nightjar opens": the setting behind `nightjar_policy::open_play::Opens`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenOnStart {
    #[default]
    Pcs,
    Last,
    Chosen,
}

impl OpenOnStart {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pcs => labels::SHOW_MY_PCS,
            Self::Last => labels::CONTINUE_LAST_GAME,
            Self::Chosen => labels::CHOSEN_GAME,
        }
    }

    pub fn opens(self) -> nightjar_policy::open_play::Opens {
        use nightjar_policy::open_play::Opens;
        match self {
            Self::Pcs => Opens::Pcs,
            Self::Last => Opens::Last,
            Self::Chosen => Opens::Chosen,
        }
    }
}

/// How Home lays out its library rows (Android's `home_view_layout`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HomeLayout {
    /// Every game of a row, line under line.
    Grid,
    /// Every game of a row as a line of its own: a small cover, the title, its store and when it
    /// was last played.
    List,
    /// One line per row, as many covers as fit and an "All N" tile for the rest. Also what an
    /// unknown value reads as; keep it last: serde wants the catch-all there.
    #[default]
    #[serde(other)]
    Rows,
}

impl HomeLayout {
    pub const ALL: [Self; 3] = [Self::Grid, Self::List, Self::Rows];

    pub fn label(self) -> &'static str {
        match self {
            Self::Grid => labels::GRID,
            Self::List => labels::LIST,
            Self::Rows => labels::ROWS,
        }
    }
}

/// Whether Home splits its library by store, by type or by collection (Android's `home_view_group`).
pub use nightjar_policy::console::{HomeGroup, HomeOrder, HomeSort};

/// Desktop localized `.label()` helper for the shared `HomeGroup`, `HomeSort`, and `HomeOrder` enums.
pub trait HomeEnumLabel: Copy {
    fn label(self) -> &'static str;
}

impl HomeEnumLabel for HomeGroup {
    fn label(self) -> &'static str {
        match self {
            Self::Store => labels::STORE,
            Self::Type => labels::TYPE,
            Self::Collection => labels::COLLECTION,
            Self::Installation => labels::INSTALLATION,
            Self::None => labels::NONE,
        }
    }
}

impl HomeEnumLabel for HomeSort {
    fn label(self) -> &'static str {
        match self {
            Self::Title => labels::TITLE,
            Self::LastPlayed => labels::LAST_PLAYED,
            Self::Playtime => labels::PLAY_TIME,
        }
    }
}

impl HomeEnumLabel for HomeOrder {
    fn label(self) -> &'static str {
        match self {
            Self::Ascending => labels::ASCENDING,
            Self::Descending => labels::DESCENDING,
        }
    }
}

/// The size of Home's covers (Android's `home_view_size`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardSize {
    Small,
    Large,
    /// Also what an unknown value reads as; keep it last: serde wants the catch-all there.
    #[default]
    #[serde(other)]
    Medium,
}

impl CardSize {
    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => labels::SMALL,
            Self::Medium => labels::MEDIUM,
            Self::Large => labels::LARGE,
        }
    }

    /// A cover's widest against the medium one's, as on Android.
    pub fn scale(self) -> f32 {
        match self {
            Self::Small => 0.75,
            Self::Medium => 1.0,
            Self::Large => 1.3,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReduceMotion {
    Reduce,
    Full,
    /// Reduce when the operating system asks for it (Windows: animations off). A value written
    /// by a newer Nightjar reads as this too, instead of dropping every setting. Keep it last:
    /// serde wants the catch-all there.
    #[default]
    #[serde(other)]
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    English,
    German,
    Spanish,
    French,
    Italian,
    Japanese,
    Korean,
    Polish,
    PortugueseBrazil,
    Russian,
    Turkish,
    Ukrainian,
    Vietnamese,
    ChineseSimplified,
    ChineseTraditional,
    /// The system's language when Nightjar has it, else English; also what an unknown value
    /// from a newer Nightjar reads as. Keep it last: serde wants the catch-all there.
    #[default]
    #[serde(other)]
    System,
}

impl Language {
    /// The choices in the settings, System default first.
    pub const ALL: [Language; 16] = [
        Language::System,
        Language::English,
        Language::German,
        Language::Spanish,
        Language::French,
        Language::Italian,
        Language::Japanese,
        Language::Korean,
        Language::Polish,
        Language::PortugueseBrazil,
        Language::Russian,
        Language::Turkish,
        Language::Ukrainian,
        Language::Vietnamese,
        Language::ChineseSimplified,
        Language::ChineseTraditional,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostDisplay {
    #[default]
    HostDefault,
    Exclusive,
    ExtendedPrimary,
    Extended,
    ExtendedIsolated,
    ExtendedPrimaryIsolated,
    Physical,
}

/// The video FEC this client asks for (`rlFecPercentage` from Rubylight, `x-ml-video.fecPercentage`
/// from Foundation Sunshine, at most 100 %): the PC's own, automatic from this device's loss
/// reports, or a fixed share. Other PCs keep their own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCorrection {
    #[default]
    Host,
    Auto,
    Percent(u8),
}

impl ErrorCorrection {
    /// The fixed shares the settings offer; a hand-edited file may hold any 0-255.
    pub const PERCENTAGES: [u8; 4] = nightjar_policy::settings_catalog_gen::ERROR_CORRECTION_PERCENTAGES;
}

impl From<ErrorCorrection> for rubylight_protocol::fec_report::FecChoice {
    fn from(value: ErrorCorrection) -> Self {
        match value {
            ErrorCorrection::Host => Self::Host,
            ErrorCorrection::Auto => Self::Auto,
            ErrorCorrection::Percent(percentage) => Self::Fixed(percentage),
        }
    }
}
