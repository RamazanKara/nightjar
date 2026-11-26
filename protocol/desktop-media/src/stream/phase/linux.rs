//! Linux display timing and non-blocking EGL V-Sync presents.
//!
//! `main.rs` forces EGL on Linux (`SDL_HINT_VIDEO_FORCE_EGL=1`) so VA-API DMA-BUFs can import via
//! `eglCreateImageKHR` on X11 as well as Wayland. Under EGL (and under the Vulkan GPU renderer),
//! `glXGetSyncValuesOML` is unavailable, so phase lock previously got no display timing on Linux
//! X11/gamescope or Wayland 1:1 streams.
//!
//! Display timing is resolved in order:
//! 1. `EGL_CHROMIUM_sync_control` (`eglGetSyncValuesCHROMIUM`, plus `eglGetMscRateANGLE` when
//!    advertised) on X11/gamescope EGL contexts.
//! 2. `GLX_OML_sync_control` (`glXGetSyncValuesOML` / `glXGetMscRateOML`) when a non-EGL GLX
//!    context is active (e.g. `SDL_VIDEO_FORCE_EGL=0`).
//! 3. Unprivileged DRM `DRM_IOCTL_CRTC_GET_SEQUENCE` on the window's active CRTC (non-master,
//!    cached per display mode), which works on both the OpenGL/EGL and GPU (Vulkan) renderers.

/// Which display clock supplied the phase-lock anchor on Linux.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub enum TimingSource {
    EglSync,
    GlxSync,
    DrmVblank,
    WaylandGlass,
    KmsFlip,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl TimingSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::EglSync => "EGL sync control",
            Self::GlxSync => "GLX sync control",
            Self::DrmVblank => "DRM vblank",
            Self::WaylandGlass => "Wayland presentation feedback",
            Self::KmsFlip => "KMS page flip",
        }
    }
}

/// Maximum past age of a display vblank timestamp (µs) accepted as a live phase anchor.
const MAX_PAST_US: i128 = 100_000;
/// Extended past age (µs) tolerated for EGL DRI3 sync timestamps, which only update on completed
/// swaps so brief network stalls or still frames would otherwise drop EGL sync and flap to DRM
/// vblank (Nightjar #175).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const EGL_STALE_GRACE_US: i128 = 500_000;
/// Maximum forward skew (µs) accepted for a slightly projected vblank timestamp.
const MAX_FUTURE_US: i128 = 5_000;
/// Bounds for a valid display refresh period (ns): ~24 Hz to 500 Hz, matching `Phase::sample`.
const MIN_PERIOD_NS: i64 = 2_000_000;
const MAX_PERIOD_NS: i64 = 41_700_000;

/// Age (`now - ust`, in µs) of a `CLOCK_MONOTONIC` UST timestamp returned by EGL or GLX sync
/// control within `max_past_us` of `now_ns`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn monotonic_ust_age_within_us(now_ns: i128, ust: u64, max_past_us: i128) -> Option<i64> {
    if ust == 0 {
        return None;
    }
    let ust = i128::from(ust);
    let age_us = now_ns / 1_000 - ust;
    if (-MAX_FUTURE_US..=max_past_us).contains(&age_us) {
        return Some(age_us as i64);
    }
    let age_ns = now_ns - ust;
    if (-MAX_FUTURE_US * 1_000..=max_past_us * 1_000).contains(&age_ns) {
        return Some((age_ns / 1_000) as i64);
    }
    None
}

/// Age (`now - ust`, in µs) of a `CLOCK_MONOTONIC` UST timestamp returned by EGL or GLX sync
/// control. Mesa's X11 DRI3 returns microseconds while some EGL implementations (such as ANGLE)
/// return nanoseconds; both are checked against `now_ns` and non-monotonic epochs or stale values
/// are rejected.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn monotonic_ust_age_us(now_ns: i128, ust: u64) -> Option<i64> {
    monotonic_ust_age_within_us(now_ns, ust, MAX_PAST_US)
}

/// Age (`now - timestamp_ns`, in µs) of a `CLOCK_MONOTONIC` nanosecond timestamp from the kernel
/// (`DRM_IOCTL_CRTC_GET_SEQUENCE.sequence_ns`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn monotonic_ns_age_us(now_ns: i128, timestamp_ns: i64) -> Option<i64> {
    if timestamp_ns <= 0 {
        return None;
    }
    let age_ns = now_ns - i128::from(timestamp_ns);
    if (-MAX_FUTURE_US * 1_000..=MAX_PAST_US * 1_000).contains(&age_ns) {
        return Some((age_ns / 1_000) as i64);
    }
    None
}

/// Refresh period (ns) from an integer `numerator / denominator` Hz rate (`eglGetMscRateANGLE` or
/// `glXGetMscRateOML`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn rate_period_ns(numerator: i32, denominator: i32) -> Option<i64> {
    if numerator <= 0 || denominator <= 0 {
        return None;
    }
    let ns = (1_000_000_000i128 * i128::from(denominator) / i128::from(numerator)) as i64;
    (MIN_PERIOD_NS..=MAX_PERIOD_NS).contains(&ns).then_some(ns)
}

/// Refresh period (ns) from a floating-point refresh rate in Hz.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn period_ns_from_hz(refresh_hz: f32) -> Option<i64> {
    if !refresh_hz.is_finite() || refresh_hz <= 1.0 {
        return None;
    }
    let ns = (1_000_000_000.0 / f64::from(refresh_hz)).round() as i64;
    (MIN_PERIOD_NS..=MAX_PERIOD_NS).contains(&ns).then_some(ns)
}

/// Picks the display refresh period (ns): prefers a measured period when valid and within 5% of
/// SDL's reported `refresh_hz` (filtering out synthetic 60 Hz Xwayland fallbacks on a 90 Hz panel),
/// otherwise uses `refresh_hz`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn choose_period_ns(measured_ns: Option<i64>, refresh_hz: f32) -> Option<i64> {
    let nominal = period_ns_from_hz(refresh_hz);
    if let Some(measured) = measured_ns.filter(|ns| (MIN_PERIOD_NS..=MAX_PERIOD_NS).contains(ns)) {
        let agrees = nominal.is_none_or(|nom| (measured - nom).abs() * 20 <= nom);
        if agrees {
            return Some(measured);
        }
    }
    nominal
}

/// Refresh period (ns) for DRM vblank timing: when SDL's reported panel `refresh_hz` (e.g.
/// 89.89 Hz on Steam Deck OLED) agrees within 2% of the kernel mode's nominal rate (90.00 Hz),
/// align DRM vblank with SDL's exact panel period so falling back between EGL sync and DRM vblank
/// never shifts the phase-lock period between 11.11 ms and 11.13 ms (Nightjar #175).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn drm_period_ns(drm_refresh_hz: f32, sdl_refresh_hz: f32) -> Option<i64> {
    let drm = period_ns_from_hz(drm_refresh_hz);
    let sdl = period_ns_from_hz(sdl_refresh_hz);
    match (drm, sdl) {
        (Some(d), Some(s)) if (d - s).abs() * 50 <= s => Some(s),
        _ => choose_period_ns(drm, sdl_refresh_hz),
    }
}

/// Consecutive healthy readings required before switching back to a preferred timing source after
/// a fallback, preventing rapid DRM↔EGL flapping during bursty dropouts (Nightjar #175).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const SOURCE_SWITCH_CONFIRM: u32 = 8;

/// Sticky Linux display-timing source selector with hysteresis.
#[derive(Debug, Default)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct SourceHysteresis {
    active: Option<TimingSource>,
    pending: Option<(TimingSource, u32)>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl SourceHysteresis {
    /// Picks between `preferred` (EGL/GLX sync) and `fallback` (DRM vblank). Switches immediately
    /// when the currently active source is unavailable, but requires `SOURCE_SWITCH_CONFIRM`
    /// consecutive healthy samples before switching from an active fallback back to `preferred`.
    pub(crate) fn pick(
        &mut self,
        preferred: Option<((i64, i64), TimingSource)>,
        fallback: Option<((i64, i64), TimingSource)>,
    ) -> Option<((i64, i64), TimingSource)> {
        match (self.active, preferred, fallback) {
            (Some(active), Some((t, src)), Some(_)) if active == src => {
                self.pending = None;
                Some((t, src))
            }
            (Some(active), Some((pref_t, pref_src)), Some((fb_t, fb_src))) if active == fb_src => {
                let count = match self.pending {
                    Some((src, n)) if src == pref_src => n + 1,
                    _ => 1,
                };
                if count >= SOURCE_SWITCH_CONFIRM {
                    self.active = Some(pref_src);
                    self.pending = None;
                    Some((pref_t, pref_src))
                } else {
                    self.pending = Some((pref_src, count));
                    Some((fb_t, fb_src))
                }
            }
            (_, Some((t, src)), _) => {
                self.active = Some(src);
                self.pending = None;
                Some((t, src))
            }
            (_, None, Some((t, src))) => {
                self.active = Some(src);
                self.pending = None;
                Some((t, src))
            }
            (_, None, None) => {
                self.pending = None;
                None
            }
        }
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Exact whitespace-delimited extension token check (matching `dmabuf.rs`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn has_extension(extensions: &str, name: &str) -> bool {
    extensions.split_ascii_whitespace().any(|token| token == name)
}

/// Phase-lock latch and period from a recent Wayland `wp_presentation` or KMS flip timestamp on the
/// session clock (µs).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn glass_phase_timing(
    last_glass_us: Option<u64>,
    last_refresh_ns: Option<u32>,
    now_us: u64,
    refresh_hz: f32,
) -> Option<(i64, i64)> {
    let glass_us = last_glass_us?;
    let age_us = i128::from(now_us) - i128::from(glass_us);
    if !(-MAX_FUTURE_US..=MAX_PAST_US).contains(&age_us) {
        return None;
    }
    let period_ns = choose_period_ns(last_refresh_ns.map(i64::from), refresh_hz)?;
    Some((glass_us as i64, period_ns))
}

/// Presents before giving up on a driver whose `sbc` counter never advances.
const UNVERIFIED_LIMIT: u32 = 8;

/// Tracks submitted vs completed EGL swap buffer counts (`sbc`) for non-blocking V-Sync on X11
/// DRI3. Never refuses a present until `completed_sbc` has actually been observed advancing after a
/// swap; if `completed_sbc` stays stuck, marks itself broken so callers fall back to normal
/// `SDL_RenderPresent`.
#[derive(Debug, Default)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct EglSwapTracker {
    surface: usize,
    submitted_sbc: u64,
    baseline_sbc: Option<u64>,
    verified: bool,
    unverified_presents: u32,
    broken: bool,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl EglSwapTracker {
    pub(crate) fn bind(&mut self, surface: usize) {
        if self.surface != surface {
            *self = Self { surface, ..Default::default() };
        }
    }

    /// Whether a previously submitted swap is still in flight (`Some(true)`), the swapchain is
    /// clear to present immediately (`Some(false)`), or `sbc` is not advancing on this driver
    /// (`None`).
    pub(crate) fn should_wait(&mut self, completed_sbc: u64) -> Option<bool> {
        if self.broken {
            return None;
        }
        if !self.verified
            && let Some(base) = self.baseline_sbc
        {
            if completed_sbc > base {
                self.verified = true;
            } else if self.unverified_presents >= UNVERIFIED_LIMIT {
                self.broken = true;
                return None;
            }
        }
        if completed_sbc > self.submitted_sbc {
            self.submitted_sbc = completed_sbc;
        }
        if !self.verified {
            return Some(false);
        }
        Some(self.submitted_sbc > completed_sbc)
    }

    /// Records a swap submitted by `present_without_waiting` when `completed_sbc_before` was last
    /// observed before the swap.
    pub(crate) fn note_presented(&mut self, completed_sbc_before: u64) {
        if self.baseline_sbc.is_none() {
            self.baseline_sbc = Some(completed_sbc_before);
        }
        if !self.verified {
            self.unverified_presents = self.unverified_presents.saturating_add(1);
        }
        self.submitted_sbc = self.submitted_sbc.max(completed_sbc_before).saturating_add(1);
    }

    /// Clears the expected-submitted watermark after an external `SDL_RenderPresent` (e.g. a
    /// redraw, overlay frame, or waiting fallback) so the next no-wait check resyncs cleanly.
    pub(crate) fn note_external_present(&mut self) {
        self.submitted_sbc = 0;
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::cell::RefCell;
    use std::ffi::{c_char, c_int, c_void, CStr};
    use std::fs::File;
    use std::os::fd::AsRawFd;

    use sdl3_sys::everything::*;

    use super::{
        choose_period_ns, drm_period_ns, has_extension, monotonic_ns_age_us, monotonic_ust_age_us,
        monotonic_ust_age_within_us, rate_period_ns, EglSwapTracker, SourceHysteresis,
        TimingSource, EGL_STALE_GRACE_US,
    };

    const EGL_EXTENSIONS: c_int = 0x3055;

    type EglQueryString = unsafe extern "C" fn(*mut c_void, c_int) -> *const c_char;
    type EglGetSyncValues =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut u64, *mut u64, *mut u64) -> u32;
    type EglGetMscRate =
        unsafe extern "C" fn(*mut c_void, *mut c_void, *mut i32, *mut i32) -> u32;
    type GlxGetSyncValues =
        unsafe extern "C" fn(*mut c_void, libc::c_ulong, *mut i64, *mut i64, *mut i64) -> i32;
    type GlxGetMscRate =
        unsafe extern "C" fn(*mut c_void, libc::c_ulong, *mut i32, *mut i32) -> i32;

    #[derive(Clone, Copy)]
    struct EglProcs {
        get_sync_values: EglGetSyncValues,
        get_msc_rate: Option<EglGetMscRate>,
    }

    #[derive(Clone, Copy)]
    struct GlxProcs {
        get_sync_values: GlxGetSyncValues,
        get_msc_rate: Option<GlxGetMscRate>,
    }

    /// One valid `eglGetSyncValuesCHROMIUM` reading on the current EGL surface.
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct EglSyncSample {
        pub(crate) surface: usize,
        pub(crate) ust_session_us: i64,
        pub(crate) msc: u64,
        pub(crate) sbc: u64,
        pub(crate) period_ns: Option<i64>,
    }

    /// `struct drm_crtc_get_sequence` (`include/uapi/drm/drm.h`).
    #[repr(C)]
    #[derive(Default)]
    struct CrtcGetSequence {
        crtc_id: u32,
        active: u32,
        sequence: u64,
        sequence_ns: i64,
    }

    const _: () = assert!(size_of::<CrtcGetSequence>() == 24);

    /// `DRM_IOCTL_CRTC_GET_SEQUENCE = DRM_IOWR(0x3b, struct drm_crtc_get_sequence)`.
    const CRTC_GET_SEQUENCE: u32 =
        (3 << 30) | ((size_of::<CrtcGetSequence>() as u32) << 16) | (0x64 << 8) | 0x3b;

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct DisplayKey {
        id: u32,
        width: u32,
        height: u32,
        refresh_milli: u32,
    }

    struct DrmVblank {
        file: File,
        crtc_id: u32,
        refresh_hz: f64,
    }

    struct DrmCache {
        key: Option<DisplayKey>,
        vblank: Option<DrmVblank>,
    }

    thread_local! {
        static EGL_CACHE: RefCell<Option<(usize, Option<EglProcs>)>> = const { RefCell::new(None) };
        static GLX_CACHE: RefCell<Option<Option<GlxProcs>>> = const { RefCell::new(None) };
        static DRM_CACHE: RefCell<DrmCache> = const { RefCell::new(DrmCache { key: None, vblank: None }) };
        static SWAP_TRACKER: RefCell<EglSwapTracker> = RefCell::new(EglSwapTracker::default());
        static SOURCE_HYSTERESIS: RefCell<SourceHysteresis> = RefCell::new(SourceHysteresis::default());
    }

    fn clock_monotonic_ns() -> Option<i128> {
        let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        // SAFETY: `now` is a valid out pointer.
        if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) } != 0 {
            return None;
        }
        Some(i128::from(now.tv_sec) * 1_000_000_000 + i128::from(now.tv_nsec))
    }

    unsafe fn fn_ptr<T>(pointer: SDL_FunctionPointer) -> Option<T> {
        // SAFETY: Caller ensures `T` matches the C ABI signature of `pointer`.
        pointer.map(|f| unsafe { std::mem::transmute_copy(&f) })
    }

    unsafe fn egl_procs(display: *mut c_void) -> Option<EglProcs> {
        let key = display as usize;
        EGL_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some((cached, procs)) = *cache
                && cached == key
            {
                return procs;
            }
            // SAFETY: `display` is a live current EGL display on the render thread.
            let procs = unsafe {
                let query: Option<EglQueryString> =
                    fn_ptr(SDL_EGL_GetProcAddress(c"eglQueryString".as_ptr()));
                let raw = query.map_or(std::ptr::null(), |q| q(display, EGL_EXTENSIONS));
                let extensions = if raw.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(raw).to_string_lossy().into_owned()
                };
                if !has_extension(&extensions, "EGL_CHROMIUM_sync_control") {
                    None
                } else {
                    fn_ptr::<EglGetSyncValues>(SDL_EGL_GetProcAddress(
                        c"eglGetSyncValuesCHROMIUM".as_ptr(),
                    ))
                    .map(|get_sync_values| {
                        let get_msc_rate = has_extension(&extensions, "EGL_ANGLE_sync_control_rate")
                            .then(|| {
                                fn_ptr::<EglGetMscRate>(SDL_EGL_GetProcAddress(
                                    c"eglGetMscRateANGLE".as_ptr(),
                                ))
                            })
                            .flatten();
                        EglProcs { get_sync_values, get_msc_rate }
                    })
                }
            };
            *cache = Some((key, procs));
            procs
        })
    }

    pub(crate) fn egl_sync_sample(
        renderer: *mut SDL_Renderer,
        refresh_hz: f32,
    ) -> Option<EglSyncSample> {
        // SAFETY: SDL EGL/window queries on the live renderer's thread.
        unsafe {
            let display = SDL_EGL_GetCurrentDisplay();
            if display.is_null() {
                return None;
            }
            let window = SDL_GetRenderWindow(renderer);
            if window.is_null() {
                return None;
            }
            let surface = SDL_EGL_GetWindowSurface(window);
            if surface.is_null() {
                return None;
            }
            let procs = egl_procs(display)?;
            let (mut ust, mut msc, mut sbc) = (0u64, 0u64, 0u64);
            if (procs.get_sync_values)(display, surface, &mut ust, &mut msc, &mut sbc) == 0 {
                return None;
            }
            let now_ns = clock_monotonic_ns()?;
            let age_us = monotonic_ust_age_within_us(now_ns, ust, EGL_STALE_GRACE_US)?;
            let measured_period = procs.get_msc_rate.and_then(|rate| {
                let (mut num, mut den) = (0i32, 0i32);
                (rate(display, surface, &mut num, &mut den) != 0)
                    .then(|| rate_period_ns(num, den))
                    .flatten()
            });
            let period_ns = choose_period_ns(measured_period, refresh_hz);
            let ust_session_us = nightjar_core::session::now_us() as i64 - age_us;
            let sample = EglSyncSample { surface: surface as usize, ust_session_us, msc, sbc, period_ns };
            FRAME_EGL_SYNC.with(|cached| *cached.borrow_mut() = Some(sample));
            Some(sample)
        }
    }

    thread_local! {
        static FRAME_EGL_SYNC: RefCell<Option<EglSyncSample>> = const { RefCell::new(None) };
    }

    pub(crate) fn clear_frame_egl_sync() {
        FRAME_EGL_SYNC.with(|cached| *cached.borrow_mut() = None);
    }

    pub(crate) fn egl_sync_sample_for_present(
        renderer: *mut SDL_Renderer,
        refresh_hz: f32,
    ) -> Option<EglSyncSample> {
        let cached = FRAME_EGL_SYNC.with(|cell| *cell.borrow());
        if let Some(sample) = cached {
            let idle = SWAP_TRACKER.with(|tracker| {
                let tracker = tracker.borrow();
                tracker.surface == sample.surface
                    && tracker.verified
                    && !tracker.broken
                    && tracker.submitted_sbc <= sample.sbc
            });
            if idle {
                return Some(sample);
            }
        }
        egl_sync_sample(renderer, refresh_hz)
    }

    fn glx_timing(renderer: *mut SDL_Renderer, refresh_hz: f32) -> Option<(i64, i64)> {
        // SAFETY: SDL GL/X11 queries on the live renderer's thread.
        unsafe {
            if !SDL_EGL_GetCurrentDisplay().is_null() || SDL_GL_GetCurrentContext().is_null() {
                return None;
            }
            let props = SDL_GetWindowProperties(SDL_GetRenderWindow(renderer));
            let display =
                SDL_GetPointerProperty(props, SDL_PROP_WINDOW_X11_DISPLAY_POINTER, std::ptr::null_mut());
            let drawable =
                SDL_GetNumberProperty(props, SDL_PROP_WINDOW_X11_WINDOW_NUMBER, 0) as libc::c_ulong;
            if display.is_null() || drawable == 0 {
                return None;
            }
            let procs = GLX_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                if let Some(procs) = *cache {
                    return procs;
                }
                let procs = fn_ptr::<GlxGetSyncValues>(SDL_GL_GetProcAddress(
                    c"glXGetSyncValuesOML".as_ptr(),
                ))
                .map(|get_sync_values| GlxProcs {
                    get_sync_values,
                    get_msc_rate: fn_ptr::<GlxGetMscRate>(SDL_GL_GetProcAddress(
                        c"glXGetMscRateOML".as_ptr(),
                    )),
                });
                *cache = Some(procs);
                procs
            })?;
            let (mut ust, mut msc, mut sbc) = (0i64, 0i64, 0i64);
            if (procs.get_sync_values)(display, drawable, &mut ust, &mut msc, &mut sbc) == 0
                || ust <= 0
            {
                return None;
            }
            let now_ns = clock_monotonic_ns()?;
            let age_us = monotonic_ust_age_us(now_ns, ust as u64)?;
            let measured_period = procs.get_msc_rate.and_then(|rate| {
                let (mut num, mut den) = (0i32, 0i32);
                (rate(display, drawable, &mut num, &mut den) != 0)
                    .then(|| rate_period_ns(num, den))
                    .flatten()
            });
            let period_ns = choose_period_ns(measured_period, refresh_hz)?;
            Some((nightjar_core::session::now_us() as i64 - age_us, period_ns))
        }
    }

    fn query_crtc_sequence(file: &File, crtc_id: u32) -> std::io::Result<CrtcGetSequence> {
        let mut req = CrtcGetSequence { crtc_id, ..Default::default() };
        loop {
            // SAFETY: `CRTC_GET_SEQUENCE` matches `CrtcGetSequence` (24 bytes, `DRM_IOWR(0x3b)`).
            if unsafe { libc::ioctl(file.as_raw_fd(), CRTC_GET_SEQUENCE as _, &mut req) } == 0 {
                return Ok(req);
            }
            let err = std::io::Error::last_os_error();
            if !matches!(err.raw_os_error(), Some(libc::EINTR | libc::EAGAIN)) {
                return Err(err);
            }
        }
    }

    fn drm_timing(renderer: *mut SDL_Renderer, refresh_hz: f32) -> Option<(i64, i64)> {
        // SAFETY: live renderer on the render thread.
        let window = unsafe { SDL_GetRenderWindow(renderer) };
        if window.is_null() {
            return None;
        }
        // SAFETY: live window on the render thread.
        let id = unsafe { SDL_GetDisplayForWindow(window) }.0;
        let display = super::super::super::drm_vrr::describe_display(id);
        let key = DisplayKey {
            id,
            width: display.width,
            height: display.height,
            refresh_milli: (display.refresh_hz.max(0.0) * 1000.0).round() as u32,
        };
        DRM_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.key != Some(key) {
                cache.key = Some(key);
                cache.vblank = super::super::super::drm_vrr::outputs().ok().and_then(|outputs| {
                    let output = super::super::super::drm_vrr::select_for_vblank(
                        &outputs,
                        &display,
                        crate::handheld::gamescope(),
                    )
                    .ok()?;
                    let file = super::super::super::drm_vrr::open_card_non_master(&output.card).ok()?;
                    Some(DrmVblank {
                        file,
                        crtc_id: output.crtc_id,
                        refresh_hz: output.refresh_hz,
                    })
                });
            }
            let vblank = cache.vblank.as_ref()?;
            let seq = match query_crtc_sequence(&vblank.file, vblank.crtc_id) {
                Ok(seq) => seq,
                Err(_) => {
                    // Drop the cached handle on hard ioctl errors so we don't retry every frame.
                    cache.vblank = None;
                    return None;
                }
            };
            if seq.active == 0 {
                return None;
            }
            let now_ns = clock_monotonic_ns()?;
            let age_us = monotonic_ns_age_us(now_ns, seq.sequence_ns)?;
            let period_ns = drm_period_ns(vblank.refresh_hz as f32, refresh_hz)?;
            Some((nightjar_core::session::now_us() as i64 - age_us, period_ns))
        })
    }

    pub(crate) fn timing_with_source(
        renderer: *mut SDL_Renderer,
        refresh_hz: f32,
    ) -> Option<((i64, i64), TimingSource)> {
        let preferred = egl_sync_sample(renderer, refresh_hz)
            .and_then(|s| s.period_ns.map(|p| ((s.ust_session_us, p), TimingSource::EglSync)))
            .or_else(|| glx_timing(renderer, refresh_hz).map(|t| (t, TimingSource::GlxSync)));
        SOURCE_HYSTERESIS.with(|h| {
            let mut h = h.borrow_mut();
            let need_fallback = preferred.is_none() || h.active == Some(TimingSource::DrmVblank);
            let fallback = if need_fallback {
                drm_timing(renderer, refresh_hz).map(|t| (t, TimingSource::DrmVblank))
            } else {
                None
            };
            h.pick(preferred, fallback)
        })
    }

    pub(crate) fn present_stats(renderer: *mut SDL_Renderer) -> crate::timing_csv::Flip {
        let Some(sample) = egl_sync_sample(renderer, 0.0) else {
            return Default::default();
        };
        let submitted = SWAP_TRACKER.with(|tracker| {
            let tracker = tracker.borrow();
            if tracker.surface == sample.surface {
                tracker.submitted_sbc as u32
            } else {
                0
            }
        });
        crate::timing_csv::Flip {
            present_count: submitted,
            stat_present_count: sample.sbc as u32,
            // EGL sync control has no separate PresentRefreshCount; leaving it 0 keeps MissWatch
            // from false-triggering while still recording SBC/MSC/UST in the timing CSV.
            stat_present_refresh: 0,
            stat_sync_refresh: sample.msc as u32,
            stat_sync_us: sample.ust_session_us.max(0) as u64,
            ..Default::default()
        }
    }

    pub(crate) fn egl_swap_check(surface: usize, completed_sbc: u64) -> Option<bool> {
        SWAP_TRACKER.with(|tracker| {
            let mut tracker = tracker.borrow_mut();
            tracker.bind(surface);
            tracker.should_wait(completed_sbc)
        })
    }

    pub(crate) fn egl_swap_presented(surface: usize, completed_sbc_before: u64) {
        clear_frame_egl_sync();
        SWAP_TRACKER.with(|tracker| {
            let mut tracker = tracker.borrow_mut();
            tracker.bind(surface);
            tracker.note_presented(completed_sbc_before);
        });
    }

    pub(crate) fn note_external_present() {
        clear_frame_egl_sync();
        SWAP_TRACKER.with(|tracker| tracker.borrow_mut().note_external_present());
    }

    pub fn reset_display_timing() {
        clear_frame_egl_sync();
        DRM_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.key = None;
            cache.vblank = None;
        });
        EGL_CACHE.with(|cache| *cache.borrow_mut() = None);
        SWAP_TRACKER.with(|tracker| tracker.borrow_mut().reset());
        SOURCE_HYSTERESIS.with(|h| h.borrow_mut().reset());
    }
}

#[cfg(target_os = "linux")]
pub use os::reset_display_timing;
#[cfg(target_os = "linux")]
pub(crate) use os::{
    clear_frame_egl_sync, egl_swap_check, egl_swap_presented, egl_sync_sample,
    egl_sync_sample_for_present, note_external_present, present_stats, timing_with_source,
};

#[cfg(not(target_os = "linux"))]
pub fn reset_display_timing() {}

#[cfg(not(target_os = "linux"))]
pub(crate) fn clear_frame_egl_sync() {}

#[cfg(not(target_os = "linux"))]
#[allow(dead_code)]
pub(crate) fn note_external_present() {}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
