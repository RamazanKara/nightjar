//! Windows Direct3D 11 / DXGI low-latency swapchain depth and GPU thread priority settings.

use sdl3_sys::everything::*;

/// Overrides DXGI's maximum frame latency (`1..=16`, or `0` to leave DXGI's default unchanged).
pub const D3D11_MAX_LATENCY_ENV: &str = "NIGHTJAR_D3D11_MAX_LATENCY";
/// Set to `0` to skip requesting elevated GPU thread / Vulkan queue priority.
pub const GPU_PRIORITY_ENV: &str = "NIGHTJAR_GPU_PRIORITY";

pub const DEFAULT_D3D11_MAX_LATENCY: u32 = 1;
pub const MIN_D3D11_MAX_LATENCY: u32 = 1;
pub const MAX_D3D11_MAX_LATENCY: u32 = 16;
/// `IDXGIDevice1::SetGPUThreadPriority` maximum normal-app priority (`-7..=7`).
pub const D3D11_GPU_THREAD_PRIORITY: i32 = 7;

/// Parses `NIGHTJAR_D3D11_MAX_LATENCY`: `0` skips setting latency (`None`), positive integers are
/// clamped to `1..=16`, and unset/invalid values fall back to `Some(1)`.
pub fn parse_d3d11_max_latency(value: Option<&str>) -> Option<u32> {
    let Some(raw) = value.map(str::trim).filter(|s| !s.is_empty()) else {
        return Some(DEFAULT_D3D11_MAX_LATENCY);
    };
    match raw.parse::<u32>() {
        Ok(0) => None,
        Ok(n) => Some(n.clamp(MIN_D3D11_MAX_LATENCY, MAX_D3D11_MAX_LATENCY)),
        Err(_) => Some(DEFAULT_D3D11_MAX_LATENCY),
    }
}

/// Parses `NIGHTJAR_GPU_PRIORITY`: enabled unless `"0"` is set.
pub fn parse_gpu_priority(value: Option<&str>) -> bool {
    value.map(str::trim) != Some("0")
}

/// Reads `NIGHTJAR_D3D11_MAX_LATENCY` from the process environment.
pub fn d3d11_max_latency_from_env() -> Option<u32> {
    parse_d3d11_max_latency(std::env::var(D3D11_MAX_LATENCY_ENV).ok().as_deref())
}

/// Reads `NIGHTJAR_GPU_PRIORITY` from the process environment.
pub fn gpu_priority_from_env() -> bool {
    parse_gpu_priority(std::env::var(GPU_PRIORITY_ENV).ok().as_deref())
}

/// Configures DXGI max frame latency and GPU thread priority on a newly created Windows D3D11
/// renderer (`IDXGIDevice1` and waitable `IDXGISwapChain2`), best-effort.
pub fn apply_d3d11_latency(renderer: *mut SDL_Renderer) {
    #[cfg(windows)]
    apply_d3d11_latency_win(renderer);
    #[cfg(not(windows))]
    let _ = renderer;
}

#[cfg(windows)]
fn apply_d3d11_latency_win(renderer: *mut SDL_Renderer) {
    use std::sync::Once;
    use windows::core::Interface;
    use windows::Win32::Foundation::E_ACCESSDENIED;
    use windows::Win32::Graphics::Direct3D11::ID3D11Device;
    use windows::Win32::Graphics::Dxgi::{
        IDXGIDevice1, IDXGISwapChain, IDXGISwapChain2, DXGI_ERROR_INVALID_CALL,
    };

    if renderer.is_null() {
        return;
    }
    let d3d11 = super::renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_DEVICE_POINTER);
    if d3d11.is_null() {
        return;
    }
    // SAFETY: `renderer` is live and `d3d11` is SDL's `ID3D11Device` pointer borrowed here.
    let Some(device) = (unsafe { ID3D11Device::from_raw_borrowed(&d3d11) }) else {
        return;
    };
    let Ok(dxgi_device) = device.cast::<IDXGIDevice1>() else {
        return;
    };

    if let Some(max_latency) = d3d11_max_latency_from_env() {
        // SAFETY: `dxgi_device` is a live `IDXGIDevice1` COM interface.
        if let Err(err) = unsafe { dxgi_device.SetMaximumFrameLatency(max_latency) } {
            static LOGGED_DEVICE_LATENCY: Once = Once::new();
            LOGGED_DEVICE_LATENCY.call_once(|| {
                eprintln!(
                    "nightjar: IDXGIDevice1::SetMaximumFrameLatency({max_latency}) failed: {err}"
                );
            });
        }
        let swapchain =
            super::renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_SWAPCHAIN_POINTER);
        if !swapchain.is_null() {
            // SAFETY: `swapchain` is SDL's `IDXGISwapChain` pointer borrowed here.
            if let Some(chain2) = unsafe { IDXGISwapChain::from_raw_borrowed(&swapchain) }
                .and_then(|chain| chain.cast::<IDXGISwapChain2>().ok())
            {
                // SAFETY: `chain2` is a live `IDXGISwapChain2`; non-waitable swapchains return
                // `DXGI_ERROR_INVALID_CALL`, which we ignore silently.
                if let Err(err) = unsafe { chain2.SetMaximumFrameLatency(max_latency) } {
                    match err.code() {
                        DXGI_ERROR_INVALID_CALL => {}
                        _ => {
                            static LOGGED_CHAIN_LATENCY: Once = Once::new();
                            LOGGED_CHAIN_LATENCY.call_once(|| {
                                eprintln!(
                                    "nightjar: IDXGISwapChain2::SetMaximumFrameLatency({max_latency}) failed: {err}"
                                );
                            });
                        }
                    }
                }
            }
        }
    }

    if gpu_priority_from_env() {
        // SAFETY: `dxgi_device` is a live `IDXGIDevice1` COM interface.
        if let Err(err) = unsafe { dxgi_device.SetGPUThreadPriority(D3D11_GPU_THREAD_PRIORITY) } {
            static LOGGED_GPU_PRIORITY: Once = Once::new();
            LOGGED_GPU_PRIORITY.call_once(|| {
                if err.code() == E_ACCESSDENIED {
                    eprintln!("nightjar: IDXGIDevice1::SetGPUThreadPriority({D3D11_GPU_THREAD_PRIORITY}) denied (E_ACCESSDENIED); continuing at default priority");
                } else {
                    eprintln!("nightjar: IDXGIDevice1::SetGPUThreadPriority({D3D11_GPU_THREAD_PRIORITY}) failed: {err}");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_latency_defaults_skips_zero_and_clamps_valid_values() {
        assert_eq!(parse_d3d11_max_latency(None), Some(1));
        assert_eq!(parse_d3d11_max_latency(Some("")), Some(1));
        assert_eq!(parse_d3d11_max_latency(Some("   ")), Some(1));
        assert_eq!(parse_d3d11_max_latency(Some("0")), None);
        assert_eq!(parse_d3d11_max_latency(Some(" 0 ")), None);
        assert_eq!(parse_d3d11_max_latency(Some("1")), Some(1));
        assert_eq!(parse_d3d11_max_latency(Some("3")), Some(3));
        assert_eq!(parse_d3d11_max_latency(Some("16")), Some(16));
        assert_eq!(parse_d3d11_max_latency(Some("17")), Some(16));
        assert_eq!(parse_d3d11_max_latency(Some("999")), Some(16));
        assert_eq!(parse_d3d11_max_latency(Some("-1")), Some(1));
        assert_eq!(parse_d3d11_max_latency(Some("invalid")), Some(1));
    }

    #[test]
    fn gpu_priority_is_enabled_unless_zero() {
        assert!(parse_gpu_priority(None));
        assert!(parse_gpu_priority(Some("")));
        assert!(parse_gpu_priority(Some("1")));
        assert!(parse_gpu_priority(Some("high")));
        assert!(!parse_gpu_priority(Some("0")));
        assert!(!parse_gpu_priority(Some(" 0 ")));
    }
}
