//! Start a stream from the command line, as game frontends (ES-DE, Pegasus, Playnite) do:
//!
//! `nightjar --host <name|uuid> [--app <name|uuid>] [--quit-on-exit]`
//!
//! The PC must be saved and paired. Without `--app` its app list opens; with it the app starts.
//! `--quit-on-exit` closes Nightjar when that stream ends, so the frontend gets its window back.
//! A PC or app that cannot be found, or a stream that cannot start, ends the process with an
//! error (exit code 1) instead of leaving the launcher open; a wrong command line exits with 64.
//! Cancelling the start closes Nightjar with exit code 0.
//!
//! moonlight-qt's form works too, for tools written for it (MoonDeck):
//! `nightjar [options] stream <host> <app>`, which closes Nightjar when the stream ends, with
//! moonlight-qt's stream options (`--resolution 1920x1080`, `--fps`, `--bitrate` in Kbps, `--hdr`,
//! `--no-vsync` and the others in [`StreamOptions`]). They change the settings of this run's
//! streams only and are never saved.
//!
//! `nightjar list <host> [--verbose]` prints the PC's apps, one name a line, as moonlight-qt's
//! `list` does (MoonDeck reads it), with its exit codes: 0, -1 for a PC that is not saved, not
//! paired or cannot be reached, and 1 when the app list cannot be read ([`run_list`]).
//!
//! `nightjar <pairing link>` is how the system opens a `rubylight:` link (see `link_channel`).

use std::ffi::OsString;
use nightjar_core::http::App;

use crate::config::{AudioChannels, CodecPreference, DecoderPreference, FramePacing, MouseMode, Settings, WindowMode};

mod launch;
pub use launch::{find_host, CliLaunch, HostEntry, Seen, Step};
#[cfg(test)]
use launch::{find_app, AppsSeen, HostSeen, LAUNCH_LIMIT, START_LIMIT};
#[cfg(test)]
use nightjar_core::http::AppControl;
#[cfg(test)]
use std::time::{Duration, Instant};

pub const USAGE: &str = "usage: nightjar [--kms | --no-kms] [--version | --host <name|uuid> [--app <name|uuid>] [--quit-on-exit] [stream options] | [stream options] stream <host> <app> | list <host> [--verbose] | <pairing link>]
stream options, as moonlight-qt's: --resolution <W>x<H>, --720, --1080, --1440, --4K, --fps <n>, --bitrate <Kbps>,
  --packet-size <bytes>, --video-codec auto|H.264|HEVC|AV1, --video-decoder auto|software|hardware, --audio-config stereo|5.1-surround|7.1-surround,
  --display-mode fullscreen|windowed|borderless, --capture-system-keys never|fullscreen|always, and these with their --no- forms:
  --hdr, --vsync, --frame-pacing, --performance-overlay, --quit-after, --yuv444, --audio-on-host, --absolute-mouse,
  --game-optimization, --multi-controller, --swap-gamepad-buttons, --background-gamepad
  --kms, --no-kms: on Linux, draw straight to the display (DRM/KMS) or through the desktop, whatever Display output says";

/// Not the link itself: it holds a PIN and passphrase, and this goes to the log.
const LINK_NOT_ALONE: &str = "a pairing link is the only argument";

/// moonlight-qt's form needs both.
const STREAM_NEEDS: &str = "stream needs a host and an app: stream <host> <app>";

/// moonlight-qt's exit code for a PC it cannot find, reach or use (`QCoreApplication::exit(-1)`):
/// 255 on Linux.
pub const LIST_NO_PC: i32 = -1;
/// moonlight-qt's exit code for an app list the PC did not give.
pub const LIST_FAILED: i32 = 1;

/// moonlight-qt's `list <host>`: the PC's apps, one name a line.
#[derive(Debug, PartialEq, Eq)]
pub struct List {
    pub host: String,
    /// `--verbose`: moonlight-qt's progress lines before the list, on stdout.
    pub verbose: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Launch {
    pub host: String,
    pub app: Option<String>,
    pub quit_on_exit: bool,
    /// The game the PC runs for another device, joined by its id: the second screen of a first
    /// screen that joined it (`--second-screen-join`). The PC reports no running game to this
    /// device then, so `app` cannot find it.
    pub join: Option<i32>,
    /// moonlight-qt's stream options, for this run's streams from this PC.
    pub options: StreamOptions,
}

/// moonlight-qt's stream options (its `app/cli/commandlineparser.cpp`), as given: None leaves the
/// saved setting. They change the settings of one run's streams and are never saved. Options
/// Nightjar has no setting for (`--mouse-buttons-swap`, `--touchscreen-trackpad`,
/// `--mute-on-focus-loss`, `--reverse-scroll-direction`, `--keep-awake`) are taken and change
/// nothing, so a command line written for moonlight-qt still starts.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct StreamOptions {
    pub resolution: Option<(u32, u32)>,
    pub fps: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    /// `--packet-size`: the video packet size of this run's streams, including the encryption
    /// header, never above the PC's per-stream offer. A value above 1400 asks for jumbo packets
    /// and runs their pre-flight probe.
    pub packet_size: Option<u32>,
    pub codec: Option<CodecPreference>,
    pub decoder: Option<DecoderPreference>,
    pub audio: Option<AudioChannels>,
    pub window_mode: Option<WindowMode>,
    /// `--capture-system-keys`: whether they are captured, and whether only in full screen.
    pub system_keys: Option<(bool, bool)>,
    pub hdr: Option<bool>,
    pub vsync: Option<bool>,
    pub frame_pacing: Option<bool>,
    pub overlay: Option<bool>,
    pub yuv444: Option<bool>,
    pub audio_on_host: Option<bool>,
    pub absolute_mouse: Option<bool>,
    pub game_optimization: Option<bool>,
    pub multi_controller: Option<bool>,
    pub swap_face_buttons: Option<bool>,
    pub background_gamepad: Option<bool>,
    /// `--quit-after`: leaving the stream quits the game on the PC; `--no-quit-after`: it does
    /// not, whatever the game's own choice says.
    pub quit_after: Option<bool>,
}

/// moonlight-qt's on/off options, each with its `--no-` form; the last one given wins.
const TOGGLES: [&str; 17] = [
    "hdr",
    "vsync",
    "frame-pacing",
    "performance-overlay",
    "yuv444",
    "audio-on-host",
    "absolute-mouse",
    "game-optimization",
    "multi-controller",
    "swap-gamepad-buttons",
    "background-gamepad",
    "quit-after",
    // Taken, and nothing changes: Nightjar has no such setting.
    "mouse-buttons-swap",
    "touchscreen-trackpad",
    "mute-on-focus-loss",
    "reverse-scroll-direction",
    "keep-awake",
];

/// moonlight-qt's options with a value; the last one given wins.
const VALUES: [&str; 9] = [
    "resolution",
    "fps",
    "bitrate",
    "packet-size",
    "display-mode",
    "audio-config",
    "capture-system-keys",
    "video-codec",
    "video-decoder",
];

impl StreamOptions {
    fn any(&self) -> bool {
        *self != Self::default()
    }

    /// Changes `settings` for one stream as moonlight-qt does: a resolution or frame rate given
    /// without a bitrate picks the bitrate for them.
    pub fn apply(&self, settings: &mut Settings) {
        fn set(value: Option<bool>, field: &mut bool) {
            if let Some(value) = value {
                *field = value;
            }
        }
        if let Some((width, height)) = self.resolution {
            (settings.width, settings.height) = (width, height);
        }
        if let Some(fps) = self.fps {
            settings.fps = fps;
        }
        match self.bitrate_kbps {
            Some(kbps) => settings.bitrate_kbps = kbps,
            None if self.resolution.is_some() || self.fps.is_some() => settings.bitrate_kbps = 0,
            None => {}
        }
        if let Some(size) = self.packet_size {
            settings.packet_size = size;
        }
        if let Some(codec) = self.codec {
            settings.codec = codec;
        }
        if let Some(decoder) = self.decoder {
            settings.decoder = decoder;
        }
        if let Some(audio) = self.audio {
            settings.audio = audio;
        }
        if let Some(mode) = self.window_mode {
            settings.window_mode = mode;
        }
        if let Some((capture, fullscreen_only)) = self.system_keys {
            settings.capture_system_keys = capture;
            settings.system_keys_fullscreen_only = fullscreen_only;
        }
        set(self.hdr, &mut settings.hdr);
        set(self.vsync, &mut settings.vsync);
        // moonlight-qt's frame pacing shows every frame in turn, one a refresh: Smoothest video.
        // A pacing already chosen that waits for the refresh is kept.
        match self.frame_pacing {
            Some(true) if settings.frame_pacing == FramePacing::Latency => settings.frame_pacing = FramePacing::Smoothness,
            Some(false) => (settings.frame_pacing, settings.host_frame_times) = (FramePacing::Latency, false),
            _ => {}
        }
        set(self.overlay, &mut settings.overlay);
        if let Some(on) = self.yuv444 {
            // "Sharper colored text" is kept in both keys (`Settings::experimental_packed_444`).
            settings.experimental_packed_444 = on;
            settings.prefer_yuv444 = on;
        }
        set(self.audio_on_host, &mut settings.play_audio_on_host);
        if let Some(absolute) = self.absolute_mouse {
            settings.mouse_mode = if absolute { MouseMode::Absolute } else { MouseMode::Capture };
        }
        set(self.game_optimization, &mut settings.sops);
        set(self.multi_controller, &mut settings.multi_controller);
        set(self.swap_face_buttons, &mut settings.swap_face_buttons);
        set(self.background_gamepad, &mut settings.background_controllers);
    }

    fn toggle(&mut self, name: &str, on: bool) {
        let slot = match name {
            "hdr" => &mut self.hdr,
            "vsync" => &mut self.vsync,
            "frame-pacing" => &mut self.frame_pacing,
            "performance-overlay" => &mut self.overlay,
            "yuv444" => &mut self.yuv444,
            "audio-on-host" => &mut self.audio_on_host,
            "absolute-mouse" => &mut self.absolute_mouse,
            "game-optimization" => &mut self.game_optimization,
            "multi-controller" => &mut self.multi_controller,
            "swap-gamepad-buttons" => &mut self.swap_face_buttons,
            "background-gamepad" => &mut self.background_gamepad,
            "quit-after" => &mut self.quit_after,
            _ => return,
        };
        *slot = Some(on);
    }

    fn value(&mut self, name: &str, value: &str) -> Result<(), String> {
        let wrong = || format!("--{name} cannot be {value}");
        let number = |n: &str| n.parse::<u32>().ok().filter(|n| *n > 0).ok_or_else(wrong);
        // moonlight-qt compares its choices ignoring case.
        let choice = value.to_ascii_lowercase();
        match name {
            "resolution" => {
                let (w, h) = choice.split_once('x').ok_or_else(wrong)?;
                self.resolution = Some((number(w)?, number(h)?));
            }
            "fps" => self.fps = Some(number(value)?),
            "bitrate" => self.bitrate_kbps = Some(number(value)?),
            "packet-size" => {
                let size = number(value)?;
                if !(256..=u32::from(rubylight_protocol::video_packet::MAX_VIDEO_PACKET)).contains(&size) {
                    return Err(format!("--packet-size must be 256 to {} bytes, not {value}", rubylight_protocol::video_packet::MAX_VIDEO_PACKET));
                }
                self.packet_size = Some(size);
            }
            "display-mode" => {
                self.window_mode = Some(match choice.as_str() {
                    "fullscreen" => WindowMode::Fullscreen,
                    "windowed" => WindowMode::Windowed,
                    "borderless" => WindowMode::Borderless,
                    _ => return Err(wrong()),
                })
            }
            "audio-config" => {
                self.audio = Some(match choice.as_str() {
                    "stereo" => AudioChannels::Stereo,
                    "5.1-surround" => AudioChannels::Surround51,
                    "7.1-surround" => AudioChannels::Surround71,
                    _ => return Err(wrong()),
                })
            }
            "capture-system-keys" => {
                self.system_keys = Some(match choice.as_str() {
                    "never" => (false, false),
                    "fullscreen" => (true, true),
                    "always" => (true, false),
                    _ => return Err(wrong()),
                })
            }
            "video-codec" => {
                self.codec = Some(match choice.as_str() {
                    "auto" => CodecPreference::Auto,
                    "h.264" => CodecPreference::H264,
                    "hevc" => CodecPreference::Hevc,
                    "av1" => CodecPreference::Av1,
                    _ => return Err(wrong()),
                })
            }
            "video-decoder" => {
                // Automatic decodes in hardware where this device can.
                self.decoder = Some(match choice.as_str() {
                    "auto" | "hardware" => DecoderPreference::Auto,
                    "software" => DecoderPreference::Software,
                    _ => return Err(wrong()),
                })
            }
            _ => return Err(format!("unknown argument --{name}")),
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    /// No arguments: the launcher as usual.
    Gui,
    Help,
    /// `--version`: print the version and exit, without a window (the packages' install checks).
    Version,
    Launch(Launch),
    /// moonlight-qt's `list <host>`: print the PC's apps and exit, without a window.
    List(List),
    /// A link the system opened with Nightjar, as it came: the window asks before it pairs, or
    /// refuses it. Text that is not valid is empty here, and refused the same way.
    Link(String),
}

/// Whether an argument is meant as a link. A browser or a QR reader may write the scheme in any
/// case, and any part after it may be wrong: that is for the window to refuse, with its own words.
pub(crate) fn is_link(arg: &str) -> bool {
    arg.get(.."rubylight:".len()).is_some_and(|scheme| scheme.eq_ignore_ascii_case("rubylight:"))
}

/// Reads the arguments after the program name. `--name value` and `--name=value` both work.
pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Result<Parsed, String> {
    let mut host = None;
    let mut app = None;
    let mut quit_on_exit = false;
    let mut options = StreamOptions::default();
    // moonlight-qt's `stream <host> <app>` or `list <host>`, as its words come.
    let mut words: Vec<String> = Vec::new();
    // `list`'s own options.
    let mut list_flags: Vec<String> = Vec::new();
    let mut first = true;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        // Alone, whatever else the command line holds: a link with a quote in it must not be able
        // to break out of the system's quoting and add arguments of its own.
        if is_link(&arg.to_string_lossy()) {
            if !first || args.next().is_some() {
                return Err(LINK_NOT_ALONE.into());
            }
            return Ok(Parsed::Link(arg.into_string().unwrap_or_default()));
        }
        first = false;
        let arg = arg
            .into_string()
            .map_err(|a| format!("{} is not valid text", a.to_string_lossy()))?;
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => {
                (flag.to_owned(), Some(value.to_owned()))
            }
            _ => (arg, None),
        };
        let mut value_of = |inline: Option<String>| -> Result<String, String> {
            let value = match inline {
                Some(value) => value,
                None => args
                    .next()
                    .and_then(|v| v.into_string().ok())
                    .filter(|v| !v.starts_with("--"))
                    .ok_or_else(|| format!("{flag} needs a value"))?,
            };
            let value = value.trim().to_owned();
            if value.is_empty() {
                return Err(format!("{flag} needs a value"));
            }
            Ok(value)
        };
        let name = flag.strip_prefix("--").unwrap_or_default();
        let toggle = |n: &str| TOGGLES.contains(&n);
        match flag.as_str() {
            "--help" | "-h" if inline.is_none() => return Ok(Parsed::Help),
            "--version" if inline.is_none() => return Ok(Parsed::Version),
            "--quit-on-exit" if inline.is_none() => quit_on_exit = true,
            "--verbose" | "--csv" if inline.is_none() => list_flags.push(flag.clone()),
            "--host" | "--app" => {
                let value = value_of(inline)?;
                let slot = if flag == "--host" {
                    &mut host
                } else {
                    &mut app
                };
                if slot.replace(value).is_some() {
                    return Err(format!("{flag} is given twice"));
                }
            }
            "--720" | "--1080" | "--1440" | "--4K" | "--4k" if inline.is_none() => {
                options.resolution = Some(match name {
                    "720" => (1280, 720),
                    "1080" => (1920, 1080),
                    "1440" => (2560, 1440),
                    _ => (3840, 2160),
                });
            }
            _ if inline.is_none() && toggle(name) => options.toggle(name, true),
            _ if inline.is_none() && name.strip_prefix("no-").is_some_and(toggle) => {
                options.toggle(&name[3..], false)
            }
            _ if VALUES.contains(&name) => {
                let value = value_of(inline)?;
                options.value(name, &value)?;
            }
            // moonlight-qt finds its action anywhere among the words.
            _ if !flag.starts_with('-')
                && (!words.is_empty() || flag.eq_ignore_ascii_case("stream") || flag.eq_ignore_ascii_case("list")) =>
            {
                words.push(flag.clone());
            }
            _ => return Err(format!("unknown argument {flag}")),
        }
    }
    if words.first().is_some_and(|w| w.eq_ignore_ascii_case("list")) {
        if host.is_some() || app.is_some() || quit_on_exit || options.any() {
            return Err("list takes only a host and --verbose: list <host> [--verbose]".into());
        }
        if list_flags.iter().any(|f| f == "--csv") {
            return Err("list --csv is not available; list prints one app name a line".into());
        }
        // moonlight-qt reads the host and leaves any word after it.
        let Some(host) = words.get(1).map(|h| h.trim().to_owned()).filter(|h| !h.is_empty()) else {
            return Err("list needs a host: list <host>".into());
        };
        return Ok(Parsed::List(List { host, verbose: !list_flags.is_empty() }));
    }
    if let Some(flag) = list_flags.first() {
        return Err(format!("unknown argument {flag}"));
    }
    if !words.is_empty() {
        let [_, stream_host, stream_app] = <[String; 3]>::try_from(words).map_err(|_| STREAM_NEEDS.to_owned())?;
        let (stream_host, stream_app) = (stream_host.trim().to_owned(), stream_app.trim().to_owned());
        if stream_host.is_empty() || stream_app.is_empty() {
            return Err(STREAM_NEEDS.into());
        }
        if host.is_some() || app.is_some() {
            return Err("give the PC and app either with stream or with --host and --app".into());
        }
        // moonlight-qt closes when the stream it started from the command line ends.
        (host, app, quit_on_exit) = (Some(stream_host), Some(stream_app), true);
    }
    match host {
        Some(host) => Ok(Parsed::Launch(Launch {
            host,
            app,
            quit_on_exit,
            join: None,
            options,
        })),
        None if app.is_some() => Err("--app needs --host".into()),
        None if quit_on_exit => Err("--quit-on-exit needs --host".into()),
        None if options.any() => Err("stream options need --host or stream <host> <app>".into()),
        None => Ok(Parsed::Gui),
    }
}

/// Whether an error that ends Nightjar is shown in a box too. A frontend start only writes it to
/// stderr and the log: a box could open behind the fullscreen frontend and keep Nightjar open.
pub fn shows_error_box(launch: Option<&Launch>) -> bool {
    launch.is_none()
}

/// Whether this build has a console for the usage text: a Windows release build has none.
pub const HAS_CONSOLE: bool = !cfg!(all(windows, not(debug_assertions)));

/// Lets a build without a console print into the console it was started from (cmd, PowerShell),
/// for `--version`. Output redirected into a file or a pipe keeps going there.
pub fn use_parent_console() {
    #[cfg(windows)]
    if !HAS_CONSOLE {
        use windows::core::w;
        use windows::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE};
        use windows::Win32::Storage::FileSystem::{
            CreateFileW, GetFileType, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK, FILE_TYPE_PIPE, OPEN_EXISTING,
        };
        use windows::Win32::System::Console::{AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
        // SAFETY: plain console and file calls on handles of this process; a failure leaves stdout
        // and stderr as they were, and printing then shows nothing, as before.
        unsafe {
            let is_redirected = |std_id| {
                GetStdHandle(std_id).is_ok_and(|handle| {
                    !handle.is_invalid()
                        && !handle.0.is_null()
                        && matches!(GetFileType(handle), FILE_TYPE_DISK | FILE_TYPE_PIPE)
                })
            };
            let out_redirected = is_redirected(STD_OUTPUT_HANDLE);
            let err_redirected = is_redirected(STD_ERROR_HANDLE);
            if out_redirected && err_redirected {
                return;
            }
            // A console parent hands a window program its own console's handles, which are no use
            // to it until it attaches; then CONOUT$ is that console's screen.
            if AttachConsole(ATTACH_PARENT_PROCESS).is_ok() {
                let conout = || {
                    CreateFileW(w!("CONOUT$"), (GENERIC_READ | GENERIC_WRITE).0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None)
                };
                if !out_redirected && let Ok(out) = conout() {
                    let _ = SetStdHandle(STD_OUTPUT_HANDLE, out);
                }
                if !err_redirected && let Ok(err) = conout() {
                    let _ = SetStdHandle(STD_ERROR_HANDLE, err);
                }
            }
        }
    }
}

/// Whether the usage text (`--help`, or a wrong command line) is shown in a box too: only by a
/// build without a console, where printing it shows nothing, and never for a command line that
/// names a PC (`--host`, or moonlight-qt's `stream` and `list`): that is a frontend's start, where a box
/// could open behind it and keep Nightjar open (see [`shows_error_box`]).
pub fn shows_usage_box<S: AsRef<std::ffi::OsStr>>(has_console: bool, args: &[S]) -> bool {
    let names_a_host = args.iter().any(|arg| {
        let arg = arg.as_ref().to_string_lossy();
        arg == "--host" || arg.starts_with("--host=") || arg.eq_ignore_ascii_case("stream") || arg.eq_ignore_ascii_case("list")
    });
    !has_console && !names_a_host
}

/// `list`'s output: the name of each app to stream, one a line, as moonlight-qt prints it. Host
/// actions (disconnect, terminate) are no apps to stream and are left out.
pub fn app_names(apps: &[App]) -> String {
    apps.iter().filter(|app| !app.is_control_action()).map(|app| format!("{}\n", app.name)).collect()
}

/// What `list` says and exits with when the app list could not be read.
fn list_error(error: &nightjar_core::http::Error, host: &str) -> (String, i32) {
    use nightjar_core::http::Error;
    match error {
        Error::Io(_) => (format!("Failed to connect to {host}"), LIST_NO_PC),
        Error::NotPaired | Error::Status { code: 401, .. } => (
            format!("Computer {host} has not been paired. Please open Nightjar to pair before retrieving games list."),
            LIST_NO_PC,
        ),
        other => (crate::hosts::describe(other), LIST_FAILED),
    }
}

/// Runs moonlight-qt's `list` against the saved PCs: the app names go to stdout, an error to
/// stderr, and the exit code is moonlight-qt's.
pub fn run_list(list: &List, store: &crate::config::Store) -> i32 {
    use std::io::Write;
    let host_store = nightjar_core::hosts::HostStore::open(store.dir()).with_read_only(store.is_read_only());
    let records: Vec<crate::config::HostRecord> = host_store.load_records(crate::config::keep_readable_settings_value);
    let entries = records.iter().enumerate().map(|(index, record)| HostEntry {
        id: index as u64,
        unique_id: &record.unique_id,
        name: &record.name,
        alias: record.alias.as_deref().filter(|a| !a.is_empty()),
        paired: record.server_certificate.is_some(),
    });
    let record = match find_host(entries, &list.host) {
        Ok(index) => &records[index as usize],
        Err(why) => {
            eprintln!("{why}");
            return LIST_NO_PC;
        }
    };
    let identity = match nightjar_core::hosts::IdentityStore::open(store.dir())
        .with_read_only(store.is_read_only())
        .load_existing_client()
    {
        Ok(Some(identity)) => std::sync::Arc::new(identity),
        Ok(None) => {
            eprintln!("this device has no pairing key yet; pair the PC in Nightjar first");
            return LIST_NO_PC;
        }
        Err(e) => {
            eprintln!("this device's pairing key cannot be read ({e}); open Nightjar once");
            return LIST_NO_PC;
        }
    };
    let mut out = std::io::stdout();
    if list.verbose {
        let _ = out.write_all(b"Establishing connection to PC...\n");
    }
    let addresses = crate::hosts::poll_addresses(record);
    let mut announced_loading = false;
    match list_apps_from_addresses(&addresses, &record.address, |address| {
        let client = crate::hosts::client_for(&identity, record, address).map_err(|why| (why, LIST_NO_PC))?;
        if list.verbose && !announced_loading {
            announced_loading = true;
            let _ = out.write_all(b"Loading app list...\n");
        }
        client.app_list(&nightjar_core::http::HostCapabilities::UNKNOWN).map_err(|e| list_error(&e, &list.host))
    }) {
        Ok(apps) => {
            let _ = out.write_all(app_names(&apps).as_bytes());
            let _ = out.flush();
            0
        }
        Err((why, code)) => {
            eprintln!("{why}");
            code
        }
    }
}

fn list_apps_from_addresses(
    addresses: &[String],
    fallback: &str,
    mut try_addr: impl FnMut(&str) -> Result<Vec<nightjar_core::http::App>, (String, i32)>,
) -> Result<Vec<nightjar_core::http::App>, (String, i32)> {
    if addresses.is_empty() {
        return try_addr(fallback);
    }
    let mut last_err = None;
    for address in addresses {
        match try_addr(address) {
            Ok(apps) => return Ok(apps),
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| ("this PC has no address".to_owned(), LIST_NO_PC)))
}

#[cfg(test)]
mod tests;
