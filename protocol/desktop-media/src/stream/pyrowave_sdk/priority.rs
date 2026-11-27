//! Vulkan global queue priority selection and unprivileged fallback for PyroWave SDK devices.

use std::ffi::CStr;

use ash::vk;

/// Vulkan KHR global priority device extension name (`VK_KHR_global_priority`).
pub const KHR_GLOBAL_PRIORITY: &CStr = ash::khr::global_priority::NAME;
/// Vulkan EXT global priority device extension name (`VK_EXT_global_priority`).
pub const EXT_GLOBAL_PRIORITY: &CStr = ash::ext::global_priority::NAME;

/// `VK_QUEUE_GLOBAL_PRIORITY_MEDIUM_KHR` (`256`), Vulkan's default queue priority.
pub const PRIORITY_MEDIUM: i32 = vk::QueueGlobalPriorityKHR::MEDIUM.as_raw();

/// Returns the compat queue priority passed to `pyrowave_create_device_by_compat`:
/// `PRIORITY_HIGH` (`512`) unless `NIGHTJAR_GPU_PRIORITY=0` disables elevated priority.
pub fn compat_queue_priority(gpu_priority_enabled: bool) -> i32 {
    if gpu_priority_enabled {
        super::PRIORITY_HIGH
    } else {
        PRIORITY_MEDIUM
    }
}

/// Selects the supported Vulkan global priority extension (`VK_KHR_global_priority` preferred over
/// `VK_EXT_global_priority`) when `gpu_priority_enabled` is true.
pub fn select_global_priority_extension<'a>(
    extensions: impl IntoIterator<Item = &'a CStr>,
    gpu_priority_enabled: bool,
) -> Option<&'static CStr> {
    if !gpu_priority_enabled {
        return None;
    }
    let mut has_ext = false;
    for ext in extensions {
        if ext == KHR_GLOBAL_PRIORITY {
            return Some(KHR_GLOBAL_PRIORITY);
        }
        if ext == EXT_GLOBAL_PRIORITY {
            has_ext = true;
        }
    }
    has_ext.then_some(EXT_GLOBAL_PRIORITY)
}

/// Selects the supported Vulkan global priority extension from enumerated `VkExtensionProperties`.
pub fn global_priority_extension_for(
    extensions: &[vk::ExtensionProperties],
    gpu_priority_enabled: bool,
) -> Option<&'static CStr> {
    select_global_priority_extension(
        extensions
            .iter()
            .filter_map(|e| e.extension_name_as_c_str().ok()),
        gpu_priority_enabled,
    )
}

/// Runs `try_create` with `priority_ext` and `VkDeviceQueueGlobalPriorityCreateInfoKHR`
/// (`VK_QUEUE_GLOBAL_PRIORITY_HIGH_KHR`, `512`) when `priority_ext` is `Some`; if that fails
/// (for instance `VK_ERROR_NOT_PERMITTED_KHR` on unprivileged drivers), retries `try_create`
/// without the priority extension and `pNext`.
pub fn create_device_with_priority_fallback<D, E: std::fmt::Display>(
    priority_ext: Option<&'static CStr>,
    mut try_create: impl FnMut(
        Option<&'static CStr>,
        Option<&vk::DeviceQueueGlobalPriorityCreateInfoKHR<'_>>,
    ) -> Result<D, E>,
) -> Result<(D, bool), E> {
    if let Some(ext) = priority_ext {
        let priority_info = vk::DeviceQueueGlobalPriorityCreateInfoKHR::default()
            .global_priority(vk::QueueGlobalPriorityKHR::HIGH);
        match try_create(Some(ext), Some(&priority_info)) {
            Ok(device) => return Ok((device, true)),
            Err(err) => {
                eprintln!(
                    "nightjar: Vulkan device creation with {} HIGH priority failed ({err}); retrying without global queue priority",
                    ext.to_string_lossy()
                );
            }
        }
    }
    try_create(None, None).map(|device| (device, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_selection_prefers_khr_falls_back_to_ext_and_respects_disable() {
        assert_eq!(
            select_global_priority_extension([c"VK_KHR_swapchain"], true),
            None
        );
        assert_eq!(
            select_global_priority_extension([EXT_GLOBAL_PRIORITY, KHR_GLOBAL_PRIORITY], true),
            Some(KHR_GLOBAL_PRIORITY)
        );
        assert_eq!(
            select_global_priority_extension([c"VK_KHR_swapchain", EXT_GLOBAL_PRIORITY], true),
            Some(EXT_GLOBAL_PRIORITY)
        );
        assert_eq!(
            select_global_priority_extension([KHR_GLOBAL_PRIORITY, EXT_GLOBAL_PRIORITY], false),
            None
        );
    }

    #[test]
    fn extension_properties_helper_matches_cstr_selection() {
        let ext_props = |name: &CStr| {
            let mut prop = vk::ExtensionProperties::default();
            let bytes = name.to_bytes_with_nul();
            for (dst, &src) in prop.extension_name.iter_mut().zip(bytes.iter()) {
                *dst = src as std::ffi::c_char;
            }
            prop
        };
        let props = [
            ext_props(c"VK_KHR_swapchain"),
            ext_props(EXT_GLOBAL_PRIORITY),
        ];
        assert_eq!(
            global_priority_extension_for(&props, true),
            Some(EXT_GLOBAL_PRIORITY)
        );
        assert_eq!(global_priority_extension_for(&props, false), None);
    }

    #[test]
    fn device_creation_attaches_high_priority_and_retries_without_it_on_failure() {
        let mut attempts = Vec::new();
        let (device, prioritized) =
            create_device_with_priority_fallback(Some(KHR_GLOBAL_PRIORITY), |ext, info| {
                attempts.push((ext, info.map(|i| i.global_priority.as_raw())));
                if ext.is_some() {
                    Err("VK_ERROR_NOT_PERMITTED_KHR")
                } else {
                    Ok("fallback-device")
                }
            })
            .unwrap();
        assert_eq!(device, "fallback-device");
        assert!(!prioritized);
        assert_eq!(
            attempts,
            vec![(Some(KHR_GLOBAL_PRIORITY), Some(512)), (None, None)]
        );
    }

    #[test]
    fn device_creation_keeps_priority_on_success_and_skips_when_unsupported() {
        let mut attempts = Vec::new();
        let (device, prioritized) = create_device_with_priority_fallback(
            Some(EXT_GLOBAL_PRIORITY),
            |ext, info| -> Result<_, &str> {
                attempts.push((ext, info.map(|i| i.global_priority.as_raw())));
                Ok("high-prio-device")
            },
        )
        .unwrap();
        assert_eq!(device, "high-prio-device");
        assert!(prioritized);
        assert_eq!(attempts, vec![(Some(EXT_GLOBAL_PRIORITY), Some(512))]);

        let mut no_ext_attempts = Vec::new();
        let (_, prioritized) =
            create_device_with_priority_fallback(None, |ext, info| -> Result<_, &str> {
                no_ext_attempts.push((ext, info.is_some()));
                Ok("plain-device")
            })
            .unwrap();
        assert!(!prioritized);
        assert_eq!(no_ext_attempts, vec![(None, false)]);
    }

    #[test]
    fn compat_priority_respects_gpu_priority_flag() {
        assert_eq!(compat_queue_priority(true), 512);
        assert_eq!(compat_queue_priority(false), 256);
    }
}
