//! Per-stream internal state structs and pure timing, mode, and swapchain helpers.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use nightjar_core::http::AppRole;
use nightjar_core::session;
use rubylight_protocol::control::Reconfigure;
use sdl3_sys::everything::SDL_EventType;

use crate::config::Settings;

/// The packet size a reconnect carries from the running session ([`LaunchPlan::negotiated_packet_size`]):
/// the negotiated one, except a jumbo size once the network moved under the stream. Jumbo packets
/// passed their test on the path the stream started on. Another path (from docked Ethernet to
/// Wi-Fi) may drop them whole, and a reconnect cannot test it while the PC still holds the
/// session: 0 has it ask for normal packets, as every reconnect did before it carried a size.
pub(super) fn carried_packet_size(negotiated: i32, network_moved: bool) -> u32 {
    let size = negotiated.max(0) as u32;
    if network_moved && size > u32::from(rubylight_protocol::video_packet::STOCK_MAX_VIDEO_PACKET) { 0 } else { size }
}

/// The stream's rate, size and settings before a reconfigure was asked. A request the host does
/// not apply is undone from this one snapshot, so no field keeps the asked mode on its own.
#[derive(Clone, Copy)]
pub(super) struct ModeBefore {
    pub(super) fps_millihz: u32,
    pub(super) fps: i32,
    pub(super) settings_fps: u32,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) requested: Reconfigure,
}

impl ModeBefore {
    /// The stream as it runs, taken before a request changes any of it. The mode to go back to
    /// is the stream's own size and rate, never the `requested_mode` field: the menu's size
    /// editor writes the asked mode there before it sends it, and so does the request.
    pub(super) fn of((stream_width, stream_height, fps): (i32, i32, i32), fps_millihz: u32, settings: &Settings) -> Self {
        let side = |pixels: i32| u16::try_from(pixels).unwrap_or(0);
        Self {
            fps_millihz,
            fps,
            settings_fps: settings.fps,
            width: settings.width,
            height: settings.height,
            requested: Reconfigure { width: side(stream_width), height: side(stream_height), fps_millihz },
        }
    }

    /// Puts the previous rate and size back into the facts, the shared rate, the settings and
    /// the mode the menu and the launch plan read.
    pub(super) fn restore(&self, fps: &mut i32, shared_millihz: &std::sync::atomic::AtomicU32, settings: &mut Settings, requested: &mut Reconfigure) {
        *fps = self.fps;
        shared_millihz.store(self.fps_millihz, Ordering::Relaxed);
        settings.fps = self.settings_fps;
        settings.width = self.width;
        settings.height = self.height;
        *requested = self.requested;
    }
}

/// Pictures in a row whose RTP timestamps must show a requested rate before a reconfigure that
/// changes only the rate counts as applied.
pub(super) const FPS_EVIDENCE_FRAMES: u32 = 6;

/// Whether a picture's RTP interval (`ticks` of the 90 kHz clock) shows `fps_millihz`: within 15%
/// of one frame of it, which tells 60 from 120 apart however the timestamps round.
pub(super) fn shows_rate(ticks: u32, fps_millihz: u32) -> bool {
    if fps_millihz == 0 {
        return false;
    }
    let expected = 90_000_000.0 / f64::from(fps_millihz);
    (f64::from(ticks) - expected).abs() <= expected * 0.15
}

/// The timing CSV when `NIGHTJAR_TIMING_CSV` names a file or folder. Automatic reconnects append
/// to the existing file instead of truncating earlier rows.
pub(super) fn open_timing_csv(append: bool) -> Option<crate::timing_csv::TimingCsv> {
    let path = crate::timing_csv::path_from_env()?;
    let opened = if append {
        crate::timing_csv::TimingCsv::append(&path)
    } else {
        crate::timing_csv::TimingCsv::create(&path)
    };
    match opened {
        Ok(csv) => {
            eprintln!("nightjar: {} the timing CSV {}", if append { "appending to" } else { "writing" }, path.display());
            Some(csv)
        }
        Err(error) => {
            eprintln!("nightjar: cannot write the timing CSV {}: {error}", path.display());
            None
        }
    }
}

#[cfg(windows)]
#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(algorithm: *mut std::ffi::c_void, buffer: *mut u8, count: u32, flags: u32) -> i32;
}

pub(super) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    let filled = {
        #[cfg(windows)]
        {
            const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
            // SAFETY: valid buffer of `out.len()` bytes; system-preferred RNG needs no algorithm handle.
            unsafe {
                BCryptGenRandom(
                    std::ptr::null_mut(),
                    out.as_mut_ptr(),
                    u32::try_from(out.len()).unwrap_or(0),
                    BCRYPT_USE_SYSTEM_PREFERRED_RNG,
                ) >= 0
            }
        }
        // Linux and macOS (unix): read OS CSPRNG from `/dev/urandom`.
        #[cfg(unix)]
        {
            use std::io::Read;
            std::fs::File::open("/dev/urandom")
                .and_then(|mut file| file.read_exact(&mut out))
                .is_ok()
        }
        // Fallback when neither Windows BCrypt nor a POSIX `/dev/urandom` device exists (`not(unix)`).
        #[cfg(not(any(windows, unix)))]
        {
            false
        }
    };
    if !filled {
        use std::hash::{BuildHasher, Hasher};
        for chunk in out.chunks_mut(8) {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(session::now_us());
            let bytes = h.finish().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
    out
}

/// One-second statistics for the overlay.
#[derive(Default)]
pub(super) struct Window {
    pub(super) started: Option<Instant>,
    pub(super) shown: u32,
    pub(super) dropped_pacing: u32,
    pub(super) late: u32,
    /// Nightjar's own drawing of the frames shown (`render::frame_shares`).
    pub(super) render_us: u64,
    /// Their present calls, which are the system's.
    pub(super) present_us: u64,
    pub(super) hold_us: i64,
    pub(super) latency_us: i64,
    pub(super) latency_n: u32,
    /// The header's processing times (100 us units) and the frames that carried one, over this
    /// window and the one before it, filled in by `HostAverage` when the overlay is built.
    pub(super) host_latency: u64,
    pub(super) host_latency_n: u32,
    pub(super) network_us: u64,
    pub(super) network_n: u32,
    pub(super) timing_host_us: u64,
    pub(super) timing_encode_us: u64,
    pub(super) timing_n: u32,
}

/// Artemis and Moonlight average the header's processing time over the frames received in the last
/// two statistics windows, whether or not they were shown. This keeps the per-window deltas of the
/// receive-side counters and sums the previous window with the current one.
#[derive(Default)]
pub(super) struct HostAverage {
    seen: (u64, u64),
    previous: (u64, u64),
}

impl HostAverage {
    /// `total` and `frames` are the running counters; returns the two-window sum.
    pub(super) fn update(&mut self, total: u64, frames: u64) -> (u64, u32) {
        let current = (total.saturating_sub(self.seen.0), frames.saturating_sub(self.seen.1));
        self.seen = (total, frames);
        let sum = (self.previous.0 + current.0, self.previous.1 + current.1);
        self.previous = current;
        (sum.0, u32::try_from(sum.1).unwrap_or(u32::MAX))
    }
}

/// Unwraps 90 kHz RTP timestamps into host microseconds.
#[derive(Default)]
pub(super) struct RtpClock {
    last: Option<(u32, i64)>,
}

impl RtpClock {
    pub(super) fn host_us(&mut self, rtp: u32) -> i64 {
        let ticks = match self.last {
            Some((last, ticks)) => ticks + i64::from(rtp.wrapping_sub(last) as i32),
            None => i64::from(rtp),
        };
        self.last = Some((rtp, ticks));
        ticks * 100 / 9
    }
}

/// Why the stream ends after SDL's render device event `kind`.
pub(super) fn render_device_ended(kind: SDL_EventType) -> Option<&'static str> { crate::render_device::ended_reason(kind) }

/// [`Streaming::sends_controllers`] for `settings` in a session of `role`.
pub(super) fn sends_controllers(settings: &Settings, role: AppRole) -> bool {
    settings.controllers && role != AppRole::RemoteMonitor
}

/// How long an HDR stream's output has to stay without HDR, or with it again, before the picture's
/// drawing follows: a mode switch or a window moving between monitors flickers it.
pub(super) const HDR_OUTPUT_SETTLES: Duration = Duration::from_secs(1);

/// Whether an HDR stream is shown on HDR output, changed only by an answer that held for
/// [`HDR_OUTPUT_SETTLES`].
#[derive(Clone, Copy, Debug)]
pub(super) struct HdrOutput {
    pub(super) shown: bool,
    /// Since when the output has answered the other way.
    other_since: Option<Instant>,
}

impl HdrOutput {
    pub(super) fn new(shown: bool) -> Self { Self { shown, other_since: None } }

    /// The output's answer at `now`: the new answer once it has held for `HDR_OUTPUT_SETTLES`.
    pub(super) fn observe(&mut self, shown: bool, now: Instant) -> Option<bool> {
        if shown == self.shown {
            self.other_since = None;
            return None;
        }
        let since = *self.other_since.get_or_insert(now);
        if now.saturating_duration_since(since) < HDR_OUTPUT_SETTLES { return None; }
        self.shown = shown;
        self.other_since = None;
        Some(shown)
    }
}

/// The end-of-stream line for how the GPU renderer's frames reached the swapchain: `direct` of the
/// stream's `presents` were drawn straight into it, the rest through the back buffer and a copy
/// (the PQ present, a screenshot's read-back, a resize, or another renderer).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn window_target_line(direct: i64, presents: u64) -> String {
    let direct = u64::try_from(direct).unwrap_or(0).min(presents);
    format!("window target: {direct} of {presents} presents drawn straight into the swapchain, {} through the back buffer", presents - direct)
}

/// [`window_target_line`] for the stream's last renderer, `renderer` (SDL's name). Only the GPU
/// renderer counts its frames; any other one (OpenGL, Linux's default) has no such path, and its
/// "0 of N" would read as a copy that never happened.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn window_target_report(renderer: &str, direct: i64, presents: u64) -> String {
    if renderer == "gpu" {
        window_target_line(direct, presents)
    } else {
        format!("window target: not counted on the {renderer} renderer, which draws through its own back buffer ({presents} presents)")
    }
}

/// The stream's presents and how many of them the GPU renderer drew straight into the swapchain,
/// from its count (`sdl::window_target_frames`) read around each present. A renderer made again
/// mid-stream (a PQ or scRGB switch) counts from zero: its count starts afresh, the old one's kept.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct WindowTargetCount {
    renderer: usize,
    start: i64,
    last: i64,
    carried: i64,
    pub(super) presents: u64,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl WindowTargetCount {
    /// Before a present on `renderer`, whose count is `frames`. A count below the last one is
    /// another renderer at the old one's address.
    pub(super) fn before_present(&mut self, renderer: usize, frames: i64) {
        if self.presents == 0 || renderer != self.renderer || frames < self.last {
            if self.presents > 0 {
                self.carried += self.last - self.start;
            }
            self.renderer = renderer;
            self.start = frames;
        }
        self.last = frames;
        self.presents += 1;
    }

    /// After that present: its count now.
    pub(super) fn after_present(&mut self, frames: i64) {
        self.last = frames.max(self.last);
    }

    pub(super) fn direct(&self) -> i64 {
        self.carried + self.last - self.start
    }
}

/// What the HDR10 swapchain's metadata changes to at a present, if anything. `stream`: the
/// stream's own while it is HDR (`Some(None)` when what the host sent is no usable metadata);
/// `sent`: what was handed last (`None` before anything). Once HDR stops the swapchain gets
/// unknown metadata (`Some(None)`) instead of keeping the last stream's.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(super) fn swapchain_metadata_change<M: Copy + PartialEq>(stream: Option<Option<M>>, sent: Option<Option<M>>) -> Option<Option<M>> {
    stream.or(sent.map(|_| None)).filter(|wanted| sent != Some(*wanted))
}

/// How long a once-a-second stats update waits to piggyback on the next incoming video frame
/// before requesting a standalone redraw on a paused or static stream.
pub(super) const OVERLAY_IDLE_REDRAW: Duration = Duration::from_millis(100);

/// Whether `draw_overlay` in `app/overlay.rs` has egui meshes to draw over the picture this frame.
pub(super) fn draws_overlay_meshes(
    overlay_visible: bool,
    toast: bool,
    suspend_banner: bool,
    menu_visible: bool,
    curtain_down: bool,
    mic_only: bool,
    role: AppRole,
) -> bool {
    overlay_visible
        || toast
        || suspend_banner
        || menu_visible
        || curtain_down
        || mic_only
        || role == AppRole::InputOnly
}

/// Schedules a passive once-a-second overlay text update: video streams let the new text ride the
/// next picture (falling back to a standalone redraw after [`OVERLAY_IDLE_REDRAW`] if no picture
/// arrives), while pictureless sessions (`InputOnly`, `mic_only`) redraw immediately.
pub(super) fn schedule_overlay_redraw(
    overlay_visible: bool,
    immediate: bool,
    now: Instant,
    pending: &mut Option<Instant>,
) -> bool {
    if !overlay_visible {
        *pending = None;
        return false;
    }
    if immediate {
        *pending = None;
        return true;
    }
    pending.get_or_insert(now + OVERLAY_IDLE_REDRAW);
    false
}

/// Promotes a deferred overlay redraw to `needs_redraw` once [`OVERLAY_IDLE_REDRAW`] has elapsed
/// without a video frame presenting it.
pub(super) fn take_due_overlay_redraw(
    overlay_visible: bool,
    now: Instant,
    pending: &mut Option<Instant>,
) -> bool {
    if !overlay_visible {
        *pending = None;
        return false;
    }
    if pending.is_some_and(|due| now >= due) {
        *pending = None;
        return true;
    }
    false
}
