//! Non-blocking V-Sync presents (`DXGI_PRESENT_DO_NOT_WAIT` on Windows, `EGL_CHROMIUM_sync_control`
//! SBC tracking on Linux X11) with targeted VBlank sleeps.

use sdl3_sys::everything::*;

/// What a present that never waits behind the last picture did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
pub enum NoWait {
    Shown,
    /// The last picture has not reached the display: this one was not presented, and the back
    /// buffer still holds it.
    Refused,
    /// Something else went wrong. SDL's own present recovers from what can be recovered from.
    Failed,
    /// `SDL_RenderPresent` itself failed after committing to present (so no second present runs).
    PresentFailed,
}

/// How long [`present_without_waiting`] sleeps (ns) after `DXGI_ERROR_WAS_STILL_DRAWING`, given
/// the remaining hold budget, the display's refresh rate, the swap chain's last `SyncQPCTime`
/// and the current QPC counter/frequency. Sleeping straight to `next_vblank - 500 us` turns
/// 16-34 half-millisecond wakeups per frame into 1-2 wakeups without adding latency past the
/// vblank that frees the swap chain.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
pub(crate) fn no_wait_sleep_ns(
    remaining: std::time::Duration,
    refresh_hz: f32,
    sync_qpc: Option<i64>,
    now_qpc: u64,
    qpc_freq: u64,
) -> u64 {
    /// Fallback retry interval when DXGI frame statistics are unavailable (1 ms instead of 0.5 ms).
    const FALLBACK_NS: u64 = 1_000_000;
    /// Wake this far before the estimated next vblank so the retry catches the release cleanly.
    const LEAD_NS: u64 = 500_000;
    /// Keep retrying shortly after vblank while DWM or the GPU finishes retiring the last flip,
    /// instead of sleeping almost a whole refresh to the next vblank.
    const TRAIL_NS: u64 = 1_500_000;
    /// Floor for a calculated sleep so retries never tight-spin on sub-millisecond remainders.
    const MIN_NS: u64 = 500_000;

    let rem_ns = remaining.as_nanos().min(u64::MAX as u128) as u64;
    if rem_ns == 0 {
        return 0;
    }
    let fallback = FALLBACK_NS.min(rem_ns);
    let Some(sync) =
        sync_qpc.filter(|&t| t > 0 && qpc_freq > 0 && refresh_hz.is_finite() && refresh_hz > 1.0)
    else {
        return fallback;
    };
    let period_ns = (1_000_000_000.0 / f64::from(refresh_hz)) as u64;
    if period_ns == 0 {
        return fallback;
    }
    let sync = sync as u64;
    let (into_refresh, to_vblank) = if now_qpc < sync {
        let ahead_ns =
            u128::from(sync - now_qpc).saturating_mul(1_000_000_000) / u128::from(qpc_freq);
        if ahead_ns > u128::from(period_ns) {
            return fallback;
        }
        let ahead = ahead_ns as u64;
        (period_ns.saturating_sub(ahead), ahead)
    } else {
        let elapsed_ns =
            u128::from(now_qpc - sync).saturating_mul(1_000_000_000) / u128::from(qpc_freq);
        // Ignore stale frame statistics (older than 4 refreshes).
        if elapsed_ns > u128::from(period_ns) * 4 {
            return fallback;
        }
        let into = (elapsed_ns % u128::from(period_ns)) as u64;
        (into, period_ns.saturating_sub(into))
    };
    if into_refresh <= TRAIL_NS || to_vblank <= LEAD_NS + MIN_NS {
        MIN_NS.min(rem_ns)
    } else {
        (to_vblank - LEAD_NS).min(rem_ns)
    }
}

/// Presents at the next refresh unless the last picture still waits for one, trying again for
/// `hold`.
///
/// # Safety
/// `renderer` is the live D3D11 renderer of this thread.
#[cfg(windows)]
pub unsafe fn present_without_waiting(
    renderer: *mut SDL_Renderer,
    hold: std::time::Duration,
    refresh_hz: f32,
) -> NoWait {
    use windows::Win32::Graphics::Dxgi::{
        DXGI_ERROR_WAS_STILL_DRAWING, DXGI_FRAME_STATISTICS, DXGI_PRESENT_DO_NOT_WAIT,
        IDXGISwapChain,
    };
    use windows::core::Interface;
    // SAFETY: caller guarantees `renderer` is the live D3D11 renderer of this thread.
    unsafe {
        let chain = crate::render_device::live_renderer_pointer(
            renderer,
            SDL_PROP_RENDERER_D3D11_SWAPCHAIN_POINTER,
        );
        let Some(chain) = IDXGISwapChain::from_raw_borrowed(&chain) else {
            return NoWait::Failed;
        };
        if !SDL_FlushRenderer(renderer) {
            return NoWait::Failed;
        }
        let begun = std::time::Instant::now();
        let freq = SDL_GetPerformanceFrequency();
        let result = loop {
            let result = chain.Present(1, DXGI_PRESENT_DO_NOT_WAIT);
            let remaining = hold.saturating_sub(begun.elapsed());
            if result != DXGI_ERROR_WAS_STILL_DRAWING || remaining.is_zero() {
                break result;
            }
            // Sleep once until just before the next vblank that frees the swap chain.
            let mut stats = DXGI_FRAME_STATISTICS::default();
            let sync_qpc = chain
                .GetFrameStatistics(&mut stats)
                .ok()
                .map(|_| stats.SyncQPCTime);
            let sleep_ns = no_wait_sleep_ns(
                remaining,
                refresh_hz,
                sync_qpc,
                SDL_GetPerformanceCounter(),
                freq,
            );
            if sleep_ns > 0 {
                SDL_DelayNS(sleep_ns);
            }
        };
        // A present unbinds the back buffer's view: this makes SDL's next draw bind it again.
        SDL_FlushRenderer(renderer);
        if result == DXGI_ERROR_WAS_STILL_DRAWING {
            NoWait::Refused
        } else if result.is_ok() {
            NoWait::Shown
        } else {
            NoWait::Failed
        }
    }
}

/// Non-blocking V-Sync present on Linux X11 EGL: uses `EGL_CHROMIUM_sync_control`'s completed
/// swap buffer counter (`sbc`) to avoid queuing a second frame behind an unfinished swap in Mesa
/// DRI3 / Xwayland / gamescope. Falls back to `NoWait::Failed` (normal `SDL_RenderPresent`) on
/// Wayland, Vulkan, or drivers without live `sbc` progression.
///
/// # Safety
/// `renderer` is the live SDL renderer of this thread.
#[cfg(target_os = "linux")]
pub unsafe fn present_without_waiting(
    renderer: *mut SDL_Renderer,
    hold: std::time::Duration,
    refresh_hz: f32,
) -> NoWait {
    use crate::stream::phase;
    let Some(sample) = phase::egl_sync_sample_for_present(renderer, refresh_hz) else {
        return NoWait::Failed;
    };
    let Some(mut in_flight) = phase::egl_swap_check(sample.surface, sample.sbc) else {
        return NoWait::Failed;
    };
    // SAFETY: caller guarantees `renderer` is the live renderer of this thread.
    if !unsafe { SDL_FlushRenderer(renderer) } {
        return NoWait::Failed;
    }
    let begun = std::time::Instant::now();
    let mut last = sample;
    while in_flight {
        let remaining = hold.saturating_sub(begun.elapsed());
        if remaining.is_zero() {
            return NoWait::Refused;
        }
        let now_us = nightjar_core::session::now_us();
        let sync_us = last.ust_session_us.max(1);
        let sleep_ns = no_wait_sleep_ns(remaining, refresh_hz, Some(sync_us), now_us, 1_000_000);
        if sleep_ns > 0 {
            // SAFETY: plain timer sleep on the render thread.
            unsafe { SDL_DelayNS(sleep_ns) };
        }
        let Some(next) = phase::egl_sync_sample(renderer, refresh_hz) else {
            return NoWait::Failed;
        };
        last = next;
        let Some(busy) = phase::egl_swap_check(last.surface, last.sbc) else {
            return NoWait::Failed;
        };
        in_flight = busy;
    }
    // SAFETY: caller guarantees `renderer` is the live renderer of this thread.
    if unsafe { SDL_RenderPresent(renderer) } {
        phase::egl_swap_presented(last.surface, last.sbc);
        NoWait::Shown
    } else {
        NoWait::PresentFailed
    }
}

/// Upper bound on how long [`present_after_egl_swap`] waits for an in-flight DRI3 swap to retire
/// before presenting anyway (two refreshes, clamped to 20–100 ms so a stalled compositor cannot
/// hang the render loop).
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub(crate) fn egl_in_order_wait_cap(refresh_hz: f32) -> std::time::Duration {
    let us = if refresh_hz.is_finite() && refresh_hz > 1.0 {
        ((2_000_000.0 / f64::from(refresh_hz)).round() as u64).clamp(20_000, 100_000)
    } else {
        40_000
    };
    std::time::Duration::from_micros(us)
}

/// In-order V-Sync present on Linux X11 EGL (Smoothness / FPS cap / Balanced-no-drop): waits for
/// the previous DRI3 swap (`sbc`) to complete before calling `SDL_RenderPresent` so Mesa DRI3 /
/// gamescope stays capped at 1 frame in flight (~11 ms instead of ~22 ms) without skipping any
/// picture (Nightjar #175). Returns `None` outside X11 EGL or when `sbc` is not advancing so the
/// caller falls through to `SDL_RenderPresent`.
///
/// # Safety
/// `renderer` is the live SDL renderer of this thread.
#[cfg(target_os = "linux")]
pub unsafe fn present_after_egl_swap(
    renderer: *mut SDL_Renderer,
    refresh_hz: f32,
) -> Option<bool> {
    use crate::stream::phase;
    let sample = phase::egl_sync_sample_for_present(renderer, refresh_hz)?;
    let mut in_flight = phase::egl_swap_check(sample.surface, sample.sbc)?;
    // SAFETY: caller guarantees `renderer` is the live renderer of this thread.
    if !unsafe { SDL_FlushRenderer(renderer) } {
        return None;
    }
    let cap = egl_in_order_wait_cap(refresh_hz);
    let begun = std::time::Instant::now();
    let mut last = sample;
    while in_flight {
        let remaining = cap.saturating_sub(begun.elapsed());
        if remaining.is_zero() {
            break;
        }
        let now_us = nightjar_core::session::now_us();
        let sync_us = last.ust_session_us.max(1);
        let sleep_ns = no_wait_sleep_ns(remaining, refresh_hz, Some(sync_us), now_us, 1_000_000);
        if sleep_ns > 0 {
            // SAFETY: plain timer sleep on the render thread.
            unsafe { SDL_DelayNS(sleep_ns) };
        }
        last = phase::egl_sync_sample(renderer, refresh_hz)?;
        in_flight = phase::egl_swap_check(last.surface, last.sbc)?;
    }
    // SAFETY: caller guarantees `renderer` is the live renderer of this thread.
    let ok = unsafe { SDL_RenderPresent(renderer) };
    if ok {
        phase::egl_swap_presented(last.surface, last.sbc);
    }
    Some(ok)
}

/// Only Linux X11 EGL has SBC-paced in-order presents.
///
/// # Safety
/// Nothing to uphold here; the signature matches the Linux one.
#[cfg(not(target_os = "linux"))]
pub unsafe fn present_after_egl_swap(
    _renderer: *mut SDL_Renderer,
    _refresh_hz: f32,
) -> Option<bool> {
    None
}

/// Only Windows and Linux X11 EGL have this present: SDL's own takes over elsewhere.
///
/// # Safety
/// Nothing to uphold here; the signature is the Windows one's.
#[cfg(not(any(windows, target_os = "linux")))]
pub unsafe fn present_without_waiting(
    _renderer: *mut SDL_Renderer,
    _hold: std::time::Duration,
    _refresh_hz: f32,
) -> NoWait {
    NoWait::Failed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn no_wait_present_sleeps_once_until_just_before_next_vblank() {
        // 10 MHz QPC (Windows standard), 100 Hz display (10 ms period).
        let freq = 10_000_000u64;
        let sync = 1_000_000i64;
        // 2 ms after last vblank -> 8 ms left -> sleeps 7.5 ms (500 us lead before vblank).
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(sync), 1_020_000, freq),
            7_500_000
        );
        // Clamped to remaining hold budget.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(3), 100.0, Some(sync), 1_020_000, freq),
            3_000_000
        );
        // Within 1 ms of vblank -> 500 us floor so we never spin.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(sync), 1_093_000, freq),
            500_000
        );
        // Missing or stale stats (>4 refreshes old) fall back to 1 ms instead of 0.5 ms polling.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, None, 1_020_000, freq),
            1_000_000
        );
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(sync), 1_500_000, freq),
            1_000_000
        );
        // Zero remaining budget sleeps nothing.
        assert_eq!(
            no_wait_sleep_ns(Duration::ZERO, 100.0, Some(sync), 1_020_000, freq),
            0
        );
    }

    #[test]
    fn no_wait_present_retries_shortly_after_vblank_instead_of_sleeping_a_refresh() {
        // 10 MHz QPC, 100 Hz display (10 ms period = 100_000 ticks).
        let freq = 10_000_000u64;
        let sync = 1_000_000i64;
        // Right at vblank and up through the 1.5 ms post-vblank window -> 500 us retry, not 9.4 ms.
        for ticks_after in [0u64, 1_000, 10_000, 15_000] {
            assert_eq!(
                no_wait_sleep_ns(
                    Duration::from_millis(10),
                    100.0,
                    Some(sync),
                    sync as u64 + ticks_after,
                    freq,
                ),
                500_000,
                "ticks_after={ticks_after}"
            );
        }
        // Same after wrapping across a refresh boundary (10.1 ms after last reported sync).
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(sync), 1_101_000, freq),
            500_000
        );
        // Clamped to remaining budget inside the post-vblank window.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_micros(200), 100.0, Some(sync), 1_001_000, freq),
            200_000
        );
        // Past the 1.5 ms window (1.6 ms into refresh) -> sleeps once to next_vblank - 500 us.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(sync), 1_016_000, freq),
            7_900_000
        );
    }

    #[test]
    fn no_wait_present_handles_future_sync_qpc_without_sleeping_a_refresh() {
        // 10 MHz QPC, 100 Hz display (10 ms period = 100_000 ticks).
        let freq = 10_000_000u64;
        // Sync 4 ms ahead -> sleeps 3.5 ms (500 us lead).
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(1_040_000), 1_000_000, freq),
            3_500_000
        );
        // Sync 0.3 ms ahead -> 500 us floor.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(1_003_000), 1_000_000, freq),
            500_000
        );
        // Sync 9.5 ms ahead (0.5 ms after the previous vblank) -> post-vblank 500 us retry.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(1_095_000), 1_000_000, freq),
            500_000
        );
        // Sync more than a refresh ahead -> 1 ms fallback.
        assert_eq!(
            no_wait_sleep_ns(Duration::from_millis(10), 100.0, Some(1_200_000), 1_000_000, freq),
            1_000_000
        );
    }

    /// Alternating refused and shown frames never hit `REFUSED_IN_A_ROW`: once refusals keep
    /// coming past a whole refresh on a 1:1 stream, Balanced falls back to waiting presents
    /// instead of staying trapped at half frame rate, and probes no-wait again after a clean
    /// window.
    #[test]
    fn alternating_refusals_past_a_refresh_wait_for_turns_and_probe_again() {
        let (frame, refresh) = (8_333, 8_333);
        let mut skipped = super::super::SkippedPicture::new(super::super::REFUSED_HOLD);
        for _ in 0..200 {
            skipped.skipped(0, frame, refresh);
            if skipped.waits() {
                break;
            }
            assert!(!skipped.shown());
        }
        assert_eq!(skipped.hold_us, 8_500);
        assert!(skipped.waits(), "sustained alternating refusals past a refresh enter waiting mode");
        assert!(skipped.must_wait());
        // First shown() resets in_a_row but waiting mode stays active through PICTURES_SHORTEN - 1.
        for _ in 0..super::super::PICTURES_SHORTEN - 1 {
            assert!(!skipped.shown());
            assert!(skipped.waits());
            assert!(skipped.must_wait());
        }
        assert!(skipped.shown(), "probes no-wait presents again after a clean window");
        assert!(!skipped.waits());
        assert!(!skipped.must_wait());
        assert_eq!(skipped.hold_us, 8_500, "keeps the longest hold on the first probe back");
        // Another clean window shortens the hold as usual.
        for _ in 0..super::super::PICTURES_SHORTEN {
            assert!(!skipped.shown());
        }
        assert_eq!(skipped.hold_us, 8_000);
        // Display change clears waiting mode too.
        while !skipped.waits() {
            skipped.skipped(0, frame, refresh);
            if !skipped.waits() {
                skipped.shown();
            }
        }
        skipped.restart();
        assert!(!skipped.waits());
        assert!(!skipped.must_wait());
        // A stream faster than the display is refused by design and never enters waiting mode.
        let mut faster = super::super::SkippedPicture::new(super::super::REFUSED_HOLD);
        for _ in 0..500 {
            faster.skipped(0, 8_333, 16_666);
            assert!(!faster.shown());
        }
        assert!(!faster.waits());
    }
}
