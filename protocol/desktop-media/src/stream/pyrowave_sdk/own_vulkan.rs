//! macOS: PyroWave's GPU decoder on a Vulkan device of its own, made through the Vulkan loader and
//! Mesa's KosmicKrisp driver (Vulkan on Metal 4: Apple silicon, macOS 26 or later) that Nightjar.app
//! bundles. The SDL renderer stays Metal, so the decoder reads its planes back into CPU buffers and
//! the presenter uploads them (`gpu_planes`, `planes_gpu`), as Linux does when it cannot share the
//! renderer's device.
//!
//! The device is made here, from the bundled loader opened by its path, and handed to the SDK with
//! `pyrowave_create_device`: the SDK's own device creation (`create_device_by_compat`) opens the
//! loader by name through volk, which would miss the bundle's and could pick up another one
//! installed on the Mac.

use std::ffi::CStr;
use std::path::PathBuf;
use std::sync::OnceLock;

use ash::vk;
use sdl3_sys::everything::*;

use super::zero_copy::{self, Chain, Features, SdlVulkan};

/// Overrides for a build outside a bundle (the CI smoke, a development build).
pub const LOADER_ENV: &str = "NIGHTJAR_VULKAN_LOADER";
pub const ICD_ENV: &str = "NIGHTJAR_VULKAN_ICD";

/// The bundled loader and manifest, when both are there (the environment's override first).
pub fn bundled() -> Result<(PathBuf, PathBuf), String> {
    let (loader, icd) = match (std::env::var_os(LOADER_ENV), std::env::var_os(ICD_ENV)) {
        (Some(loader), Some(icd)) => (PathBuf::from(loader), PathBuf::from(icd)),
        (Some(_), None) | (None, Some(_)) => return Err(format!("set both {LOADER_ENV} and {ICD_ENV}, or neither")),
        (None, None) => {
            let exe = std::env::current_exe().map_err(|e| format!("cannot locate the executable: {e}"))?;
            crate::stream::pyrowave_mode::bundle_paths(&exe).ok_or("the executable is not inside Nightjar.app")?
        }
    };
    for path in [&loader, &icd] {
        if !path.is_file() {
            return Err(format!("this Nightjar has no bundled Vulkan driver ({} is missing)", path.display()));
        }
    }
    Ok((loader, icd))
}

/// Points the Vulkan loader at the bundled driver only, before anything loads Vulkan: called from
/// `main` while the process has one thread. A `VK_DRIVER_FILES` or `VK_ICD_FILENAMES` already set
/// (a developer's) is left alone.
pub fn prepare_environment() {
    if std::env::var_os("VK_DRIVER_FILES").is_some() || std::env::var_os("VK_ICD_FILENAMES").is_some() {
        return;
    }
    if let Ok((_, icd)) = bundled() {
        // SAFETY: called from `main` while the process is single-threaded before Vulkan loads.
        unsafe { std::env::set_var("VK_DRIVER_FILES", icd) };
    }
}

/// The bundled loader, opened once by its path.
pub fn entry() -> Result<ash::Entry, String> {
    static ENTRY: OnceLock<Result<ash::Entry, String>> = OnceLock::new();
    ENTRY
        .get_or_init(|| {
            let (loader, _) = bundled()?;
            // SAFETY: Loading the Vulkan loader the bundle ships runs only its initializers.
            unsafe { ash::Entry::load_from(&loader) }.map_err(|e| format!("cannot load {}: {e}", loader.display()))
        })
        .clone()
}

/// A Vulkan 1.3 device with the decoder's features on the first device that has them, its
/// graphics queue and the lock the SDK takes around it. Dropped after the SDK device made on it.
pub struct OwnVulkan {
    entry: ash::Entry,
    instance: ash::Instance,
    device: ash::Device,
    physical_device: vk::PhysicalDevice,
    queue: vk::Queue,
    family: u32,
    lock: *mut SDL_Mutex,
    features: Features,
    pub name: String,
}

// The handles are used by one decoder at a time; the SDK takes `lock` around queue submissions.
unsafe impl Send for OwnVulkan {}

impl OwnVulkan {
    pub fn new() -> Result<Self, String> {
        let entry = entry()?;
        // SAFETY: Plain Vulkan calls on handles made here; every early return destroys what it made.
        unsafe {
            let version = entry.try_enumerate_instance_version().ok().flatten().unwrap_or(vk::API_VERSION_1_0);
            if version < vk::API_VERSION_1_3 {
                return Err(format!("the bundled Vulkan loader supports {}.{}, the decoder needs 1.3", vk::api_version_major(version), vk::api_version_minor(version)));
            }
            let portability = entry
                .enumerate_instance_extension_properties(None)
                .unwrap_or_default()
                .iter()
                .any(|e| e.extension_name_as_c_str() == Ok(ash::khr::portability_enumeration::NAME));
            let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
            let names = [ash::khr::portability_enumeration::NAME.as_ptr()];
            let mut info = vk::InstanceCreateInfo::default().application_info(&app);
            if portability {
                info = info.enabled_extension_names(&names).flags(vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR);
            }
            let instance = entry.create_instance(&info, None).map_err(|e| format!("the bundled Vulkan driver does not start ({e})"))?;
            match Self::on(&instance) {
                Ok((physical_device, family, features, name, device)) => {
                    let queue = device.get_device_queue(family, 0);
                    let lock = SDL_CreateMutex();
                    if lock.is_null() {
                        device.destroy_device(None);
                        instance.destroy_instance(None);
                        return Err(format!("cannot make the queue lock ({})", crate::sdl::error()));
                    }
                    Ok(Self { entry, instance, device, physical_device, queue, family, lock, features, name })
                }
                Err(e) => {
                    instance.destroy_instance(None);
                    Err(e)
                }
            }
        }
    }

    /// The first device the decoder runs on, its graphics queue family, its features, its name
    /// and the device made on it.
    unsafe fn on(instance: &ash::Instance) -> Result<(vk::PhysicalDevice, u32, Features, String, ash::Device), String> {
        // SAFETY: Caller guarantees `instance` is a live Vulkan instance created in `Self::new`.
        unsafe {
            let devices = instance.enumerate_physical_devices().map_err(|e| format!("Vulkan lists no devices ({e})"))?;
            if devices.is_empty() {
                return Err("the bundled Vulkan driver finds no GPU it supports".into());
            }
            let mut refused = Vec::new();
            for physical_device in devices {
                let caps = zero_copy::caps_of(instance, physical_device);
                let features = match zero_copy::plan(std::slice::from_ref(&caps)) {
                    Ok(features) => features,
                    Err(e) => {
                        refused.push(e);
                        continue;
                    }
                };
                let family = instance
                    .get_physical_device_queue_family_properties(physical_device)
                    .iter()
                    .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE));
                let Some(family) = family else {
                    refused.push(format!("{} has no graphics and compute queue", caps.name));
                    continue;
                };
                // A partial Vulkan (VK_KHR_portability_subset, such as MoltenVK) would have to be enabled
                // and told to the SDK, whose shaders were never checked against its limits: refused.
                let subset = c"VK_KHR_portability_subset";
                let extensions = instance.enumerate_device_extension_properties(physical_device).unwrap_or_default();
                if extensions.iter().any(|e| e.extension_name_as_c_str() == Ok(subset)) {
                    refused.push(format!("{} is a partial Vulkan (VK_KHR_portability_subset)", caps.name));
                    continue;
                }
                let mut chain = Chain::new();
                chain.enable(features);
                let priority_ext = super::priority::global_priority_extension_for(
                    &extensions,
                    crate::sdl::gpu_priority_from_env(),
                );
                let result = super::priority::create_device_with_priority_fallback(priority_ext, |ext, queue_priority| {
                    let priority = [1.0f32];
                    let mut queue_info = vk::DeviceQueueCreateInfo::default()
                        .queue_family_index(family as u32)
                        .queue_priorities(&priority);
                    if let Some(queue_priority) = queue_priority {
                        queue_info.p_next = (queue_priority as *const vk::DeviceQueueGlobalPriorityCreateInfoKHR<'_>).cast();
                    }
                    let queues = [queue_info];
                    let ext_ptrs = ext.map(|name| [name.as_ptr()]);
                    let mut info = vk::DeviceCreateInfo::default().queue_create_infos(&queues);
                    if let Some(names) = &ext_ptrs {
                        info = info.enabled_extension_names(names);
                    }
                    info.p_next = (&raw const chain.pdf2).cast();
                    instance.create_device(physical_device, &info, None)
                });
                match result {
                    Ok((device, _)) => return Ok((physical_device, family as u32, features, caps.name, device)),
                    Err(e) => refused.push(format!("{} refused a device ({e})", caps.name)),
                }
            }
            Err(refused.join("; "))
        }
    }

    /// The device as the SDK's shared-device creation takes it (`ShareInfo`), no compute queue.
    pub fn as_sdl(&self) -> SdlVulkan {
        SdlVulkan {
            instance: self.instance.handle(),
            physical_device: self.physical_device,
            device: self.device.handle(),
            queue: self.queue,
            family: self.family,
            lock: self.lock,
            features: self.features,
            compute: super::queue::RendererCompute::NotAsked,
        }
    }

    pub fn get_instance_proc_addr(&self) -> vk::PFN_vkGetInstanceProcAddr {
        self.entry.static_fn().get_instance_proc_addr
    }

    /// The driver's own name for the record (KosmicKrisp, or whichever answered).
    pub fn driver(&self) -> String {
        let mut driver = vk::PhysicalDeviceDriverProperties::default();
        let mut properties = vk::PhysicalDeviceProperties2::default().push_next(&mut driver);
        // SAFETY: A query on the live instance and device.
        unsafe { self.instance.get_physical_device_properties2(self.physical_device, &mut properties) };
        let name = driver.driver_name_as_c_str().map(CStr::to_string_lossy).unwrap_or_default().into_owned();
        let info = driver.driver_info_as_c_str().map(CStr::to_string_lossy).unwrap_or_default().into_owned();
        format!("{name} {info}").trim().to_owned()
    }
}

impl Drop for OwnVulkan {
    fn drop(&mut self) {
        // SAFETY: The SDK device made on these handles is gone (`GpuDecoder`'s Drop runs first).
        unsafe {
            let _ = self.device.device_wait_idle();
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
            SDL_DestroyMutex(self.lock);
        }
    }
}
