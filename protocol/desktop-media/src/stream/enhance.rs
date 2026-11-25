//! AI enhancement, the "AI enhancement" upscaling mode: the GPU vendor's video super resolution
//! (NVIDIA RTX Video, Intel's video processor extension, AMD AMF's HQ Scaler) and NVIDIA's RTX
//! Video HDR, which turns an SDR stream into HDR on an HDR display (Direct3D 11 for NVIDIA/Intel;
//! AMD runs AMF `amf_scaler`). Ported from Moonlight V+ (qiin2333/moonlight-qt, `d3d11va.cpp`).
#![cfg_attr(not(windows), allow(dead_code))]

pub const VENDOR_AMD: u32 = 0x1002;
pub const VENDOR_INTEL: u32 = 0x8086;
pub const VENDOR_NVIDIA: u32 = 0x10DE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    AmdAmf,
    IntelVpe,
    NvidiaRtx,
}

impl Engine {
    pub fn label(self) -> &'static str {
        match self {
            Self::AmdAmf => "AMD Video Super Resolution",
            Self::IntelVpe => "Intel Video Super Resolution",
            Self::NvidiaRtx => "NVIDIA RTX Video Super Resolution",
        }
    }
}

/// What the GPU can do: its vendor's engine and which of the two features passed the probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    pub engine: Engine,
    pub super_resolution: bool,
    pub sdr_to_hdr: bool,
}

/// The vendor checks of the probe, each run only for its own vendor.
pub trait Probe {
    fn amf(&self) -> bool;
    fn intel_super_resolution(&self) -> bool;
    fn nvidia_super_resolution(&self) -> bool;
    fn nvidia_hdr(&self) -> bool;
}

/// The capability of a GPU of `vendor`, as V+'s `queryVideoEnhancement`: AMD has AMF's scaler and
/// no SDR to HDR, Intel its super resolution (V+ has no Intel SDR to HDR), NVIDIA both. None when
/// the GPU can do neither, so the mode is not offered.
pub fn capability(vendor: u32, probe: &impl Probe) -> Option<Capability> {
    let (engine, super_resolution, sdr_to_hdr) = match vendor {
        VENDOR_AMD => (Engine::AmdAmf, probe.amf(), false),
        VENDOR_INTEL => (Engine::IntelVpe, probe.intel_super_resolution(), false),
        VENDOR_NVIDIA => (Engine::NvidiaRtx, probe.nvidia_super_resolution(), probe.nvidia_hdr()),
        _ => return None,
    };
    (super_resolution || sdr_to_hdr).then_some(Capability { engine, super_resolution, sdr_to_hdr })
}

/// What runs for one kind of picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub super_resolution: bool,
    pub sdr_to_hdr: bool,
    /// The enhanced picture is PQ BT.2020 (an HDR picture, or SDR turned into HDR).
    pub hdr_output: bool,
}

/// V+'s choices per picture: NVIDIA's super resolution skips HDR (BT.2020) pictures, Intel's runs
/// only on them (`prepareEnhancedOutput`), AMF scales everything. RTX Video HDR runs on 8-bit SDR
/// streams, and here only when the display shows HDR (its PQ output needs it).
pub fn plan(capability: Capability, hdr_picture: bool, ten_bit: bool, hdr_display: bool) -> Plan {
    let super_resolution = capability.super_resolution && match capability.engine {
        Engine::AmdAmf => true,
        Engine::NvidiaRtx => !hdr_picture,
        Engine::IntelVpe => hdr_picture,
    };
    let sdr_to_hdr = capability.sdr_to_hdr && !hdr_picture && !ten_bit && hdr_display;
    Plan { super_resolution, sdr_to_hdr, hdr_output: hdr_picture || sdr_to_hdr }
}

/// Whether AI enhancement performs useful work for `input` displayed at `output`. Skips no-op
/// video processor passes (NVIDIA HDR, Intel SDR) and native/downscaled AMD AMF passes so direct
/// presentation stays active.
pub fn should_run(capability: Capability, hdr_picture: bool, ten_bit: bool, hdr_display: bool, input: (i32, i32), output: (i32, i32)) -> bool {
    let p = plan(capability, hdr_picture, ten_bit, hdr_display);
    (p.super_resolution || p.sdr_to_hdr) && !(capability.engine == Engine::AmdAmf && output.0 <= input.0 && output.1 <= input.1)
}

/// The stats line's name of what runs.
pub fn describe(capability: Capability, plan: Plan) -> String {
    let mut parts = Vec::new();
    if plan.super_resolution { parts.push(capability.engine.label()); }
    if plan.sdr_to_hdr { parts.push("RTX Video HDR"); }
    if parts.is_empty() { "AI enhancement (video processor only)".into() } else { format!("AI enhancement ({})", parts.join(", ")) }
}

#[cfg(windows)]
pub use self::windows_impl::*;

#[cfg(not(windows))]
/// AI enhancement needs Direct3D 11: never offered elsewhere.
pub fn settings_probe(_device: *mut std::ffi::c_void) -> Option<Option<Capability>> {
    Some(None)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::ptr;

    use sdl3_sys::everything::*;
    use windows::core::{Interface, GUID};
    use windows::Win32::Foundation::{HMODULE, RECT};
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_UNKNOWN;
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::*;
    use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};

    use super::super::amf_scaler::{self, HqScaler};
    use super::super::direct::Transfer;
    use super::{capability, plan, Capability, Engine, Plan, Probe};

    const NVIDIA_PPE: GUID = GUID::from_values(0xd43ce1b3, 0x1f4b, 0x48ac, [0xba, 0xee, 0xc3, 0xc2, 0x53, 0x75, 0xe6, 0xf7]);
    const NVIDIA_TRUE_HDR: GUID = GUID::from_values(0xfdd62bb4, 0x620b, 0x4fd7, [0x9a, 0xb3, 0x1e, 0x59, 0xd0, 0xd5, 0x44, 0xb3]);
    const INTEL_VPE: GUID = GUID::from_values(0xedd1d4b9, 0x8659, 0x4cbc, [0xa4, 0xd6, 0x98, 0x31, 0xa2, 0x16, 0x3a, 0xc3]);

    /// Turns NVIDIA's super resolution on or off (`NvidiaStreamExt` v1, method 2).
    unsafe fn nvidia_super_resolution(context: &ID3D11VideoContext, processor: &ID3D11VideoProcessor, enable: bool) -> Result<(), String> {
        let data: [u32; 3] = [1, 2, u32::from(enable)];
        // SAFETY: caller guarantees `context` and `processor` are live D3D11 video objects.
        let hr = unsafe { context.VideoProcessorSetStreamExtension(processor, 0, &NVIDIA_PPE, 12, data.as_ptr().cast()) };
        if hr < 0 { Err(format!("NVIDIA RTX Video Super Resolution failed: {hr:#x}")) } else { Ok(()) }
    }

    /// Turns RTX Video HDR on or off (`NvidiaStreamExt` v4, method 3, the enable bit).
    unsafe fn nvidia_hdr(context: &ID3D11VideoContext, processor: &ID3D11VideoProcessor, enable: bool) -> Result<(), String> {
        let data: [u32; 3] = [4, 3, u32::from(enable)];
        // SAFETY: caller guarantees `context` and `processor` are live D3D11 video objects.
        let hr = unsafe { context.VideoProcessorSetStreamExtension(processor, 0, &NVIDIA_TRUE_HDR, 12, data.as_ptr().cast()) };
        if hr < 0 { Err(format!("NVIDIA RTX Video HDR failed: {hr:#x}")) } else { Ok(()) }
    }

    /// Intel's super resolution: interface version 3, then pre-processing mode and the scaling
    /// mode on the stream (`IntelVpeExt` is a function code and a pointer to its parameter).
    unsafe fn intel_super_resolution(context: &ID3D11VideoContext, processor: &ID3D11VideoProcessor, enable: bool) -> Result<(), String> {
        #[repr(C)]
        struct Ext { function: u32, param: *mut u32 }
        let size = std::mem::size_of::<Ext>() as u32;
        // SAFETY: caller guarantees `context` and `processor` are live D3D11 video objects.
        unsafe {
            let call = |function: u32, mut param: u32, stream: bool| {
                let ext = Ext { function, param: &mut param };
                if stream {
                    context.VideoProcessorSetStreamExtension(processor, 0, &INTEL_VPE, size, (&raw const ext).cast())
                } else {
                    context.VideoProcessorSetOutputExtension(processor, &INTEL_VPE, size, (&raw const ext).cast())
                }
            };
            let hr = call(0x01, 3, false);
            if hr < 0 { return Err(format!("Intel VPE version failed: {hr:#x}")); }
            let hr = call(0x20, u32::from(enable), false);
            if hr < 0 { return Err(format!("Intel VPE mode failed: {hr:#x}")); }
            let hr = call(0x37, if enable { 2 } else { 0 }, true);
            if hr < 0 { return Err(format!("Intel Video Super Resolution failed: {hr:#x}")); }
            Ok(())
        }
    }

    /// The adapter of the renderer's device, its vendor and name.
    unsafe fn adapter(device: &ID3D11Device) -> windows::core::Result<(IDXGIAdapter, u32, String)> {
        // SAFETY: caller guarantees `device` is a live D3D11 device.
        unsafe {
            let adapter = device.cast::<IDXGIDevice>()?.GetAdapter()?;
            let desc = adapter.GetDesc()?;
            let name = String::from_utf16_lossy(&desc.Description[..desc.Description.iter().position(|&c| c == 0).unwrap_or(desc.Description.len())]);
            Ok((adapter, desc.VendorId, name))
        }
    }

    unsafe fn processor(device: &ID3D11Device, input: (u32, u32), output: (u32, u32), frame_rate: u32)
        -> windows::core::Result<(ID3D11VideoDevice, ID3D11VideoContext1, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessor)> {
        // SAFETY: caller guarantees `device` is a live D3D11 device.
        unsafe {
            let video = device.cast::<ID3D11VideoDevice>()?;
            let context = device.GetImmediateContext()?.cast::<ID3D11VideoContext1>()?;
            let rate = DXGI_RATIONAL { Numerator: frame_rate.max(1), Denominator: 1 };
            let enumerator = video.CreateVideoProcessorEnumerator(&D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, InputFrameRate: rate, InputWidth: input.0, InputHeight: input.1,
                OutputFrameRate: rate, OutputWidth: output.0, OutputHeight: output.1, Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL })?;
            let processor = video.CreateVideoProcessor(&enumerator, 0)?;
            Ok((video, context, enumerator, processor))
        }
    }

    /// The probe on a device of its own on the renderer's adapter, as V+ probes on a temporary one.
    struct DeviceProbe {
        device: ID3D11Device,
        video: Option<(ID3D11VideoContext1, ID3D11VideoProcessor)>,
    }

    impl Probe for DeviceProbe {
        fn amf(&self) -> bool {
            let parameters = amf_scaler::Parameters { width: 1920, height: 1080, output_width: 3840, output_height: 2160,
                frame_rate: 60, ten_bit: false, sharpness: amf_scaler::sharpness(true, false) };
            // SAFETY: The probe's own live device.
            unsafe { HqScaler::new(&self.device, parameters) }.map_err(|e| eprintln!("nightjar: AI enhancement probe: {e}")).is_ok()
        }
        fn intel_super_resolution(&self) -> bool {
            // SAFETY: The probe's own video processor; turning the feature off changes nothing.
            self.video.as_ref().is_some_and(|(c, p)| unsafe { intel_super_resolution(c, p, false) }.map_err(|e| eprintln!("nightjar: AI enhancement probe: {e}")).is_ok())
        }
        fn nvidia_super_resolution(&self) -> bool {
            // SAFETY: As above.
            self.video.as_ref().is_some_and(|(c, p)| unsafe { nvidia_super_resolution(c, p, false) }.map_err(|e| eprintln!("nightjar: AI enhancement probe: {e}")).is_ok())
        }
        fn nvidia_hdr(&self) -> bool {
            // SAFETY: As above.
            self.video.as_ref().is_some_and(|(c, p)| unsafe { nvidia_hdr(c, p, false) }.map_err(|e| eprintln!("nightjar: AI enhancement probe: {e}")).is_ok())
        }
    }

    /// Probes the renderer's adapter. `device` is the renderer's `ID3D11Device`.
    pub fn probe(device: *mut c_void) -> Result<Option<Capability>, String> {
        // SAFETY: A live device pointer from the renderer, only borrowed here.
        let device = unsafe { ID3D11Device::from_raw_borrowed(&device) }.ok_or("AI enhancement needs the Direct3D 11 renderer")?;
        // SAFETY: Plain DXGI and D3D11 calls; the probe's device is its own.
        unsafe {
            let (adapter, vendor, name) = adapter(device).map_err(|e| format!("cannot read the GPU: {e}"))?;
            let mut own = None;
            D3D11CreateDevice(&adapter, D3D_DRIVER_TYPE_UNKNOWN, HMODULE::default(), D3D11_CREATE_DEVICE_VIDEO_SUPPORT, None,
                D3D11_SDK_VERSION, Some(&mut own), None, None).map_err(|e| format!("cannot make a probe device on {name}: {e}"))?;
            let device = own.ok_or("no probe device")?;
            let video = processor(&device, (1920, 1080), (3840, 2160), 60).ok().map(|(_, c, _, p)| (c, p));
            let result = capability(vendor, &DeviceProbe { device, video });
            eprintln!("nightjar: AI enhancement on {name} (vendor {vendor:#06x}): {result:?}");
            Ok(result)
        }
    }

    type ProbeState = std::sync::Mutex<Vec<(usize, Option<Option<Capability>>)>>;

    fn probe_state() -> &'static ProbeState {
        static STATE: ProbeState = std::sync::Mutex::new(Vec::new());
        &STATE
    }

    fn remember(device: usize, result: Option<Capability>) {
        let mut state = probe_state().lock().unwrap_or_else(|e| e.into_inner());
        match state.iter_mut().find(|(key, _)| *key == device) {
            Some(entry) => entry.1 = Some(result),
            None => state.push((device, Some(result))),
        }
    }

    /// `probe` for settings screens: runs once per renderer device on a thread of its own (it loads
    /// the AMF runtime) and is remembered. None while it runs; Some(None) when nothing is offered.
    pub fn settings_probe(device: *mut c_void) -> Option<Option<Capability>> {
        struct Owned(ID3D11Device);
        // SAFETY: D3D11 devices are free-threaded, and the probe thread holds its own reference,
        // so the renderer releasing its device cannot free it under the probe.
        unsafe impl Send for Owned {}
        if device.is_null() { return Some(None); }
        let mut state = probe_state().lock().unwrap_or_else(|e| e.into_inner());
        if let Some((_, done)) = state.iter().find(|(key, _)| *key == device as usize) { return *done; }
        // SAFETY: A live device pointer from the renderer, borrowed only for this clone, which
        // takes a reference of the probe thread's own: a device the renderer drops while the
        // probe runs stays alive until the probe is done with it.
        let Some(owned) = (unsafe { ID3D11Device::from_raw_borrowed(&device) }).cloned() else { return Some(None); };
        state.push((device as usize, None));
        let key = device as usize;
        let raw = Owned(owned);
        std::thread::spawn(move || {
            let raw = raw;
            let result = probe(raw.0.as_raw()).unwrap_or_else(|e| { eprintln!("nightjar: AI enhancement probe: {e}"); None });
            remember(key, result);
        });
        None
    }

    /// The stream's probe: the settings probe's result when there is one, else a probe now.
    pub fn stream_probe(device: *mut c_void) -> Result<Option<Capability>, String> {
        let known = probe_state().lock().unwrap_or_else(|e| e.into_inner()).iter().find(|(key, _)| *key == device as usize).and_then(|(_, done)| *done);
        if let Some(known) = known { return Ok(known); }
        let result = probe(device)?;
        remember(device as usize, result);
        Ok(result)
    }

    /// Whether AI enhancement should run for `source` at `output` (true on probe failure so
    /// `enhanced()` reports the failure visibly once).
    pub fn should_enhance(renderer: *mut SDL_Renderer, source: Source, output: (i32, i32)) -> bool {
        if output.0 <= 0 || output.1 <= 0 || output.0 > 16384 || output.1 > 16384 { return false; }
        let device = crate::render_device::live_renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_DEVICE_POINTER);
        match stream_probe(device) {
            Ok(Some(cap)) => super::should_run(cap, source.hdr(), source.ten_bit, crate::sdl::hdr_available(renderer), source.picture, output),
            _ => true,
        }
    }

    /// The part of a `texture`-sized video texture AMD's HQ Scaler reads: the picture, rounded up
    /// to the even size SDL makes NV12 and P010 textures at and never past the texture, so a
    /// decoded picture's alignment padding stays out.
    pub fn amf_input(picture: (i32, i32), texture: (u32, u32)) -> (i32, i32) {
        let even = |extent: i32, whole: u32| ((extent + 1) & !1).min(whole as i32);
        (even(picture.0, texture.0), even(picture.1, texture.1))
    }

    /// The video texture an enhancement reads: a hardware-decoded NV12 or P010 picture.
    #[derive(Clone, Copy, PartialEq)]
    pub struct Source {
        pub texture: *mut SDL_Texture,
        pub d3d11: *mut c_void,
        pub picture: (i32, i32),
        pub ten_bit: bool,
        pub colorspace: SDL_Colorspace,
        /// The picture's transfer, as the presenter decides it for every path
        /// (`present::picture_transfer`).
        pub transfer: Transfer,
    }

    impl Source {
        pub fn hdr(&self) -> bool {
            self.transfer != Transfer::Sdr
        }
    }

    /// The video processor's input and output colour spaces for a picture in `space`.
    pub fn colors(space: SDL_Colorspace, transfer: Transfer, hdr_output: bool) -> (DXGI_COLOR_SPACE_TYPE, DXGI_COLOR_SPACE_TYPE) {
        let input = match space {
            _ if transfer == Transfer::Pq => DXGI_COLOR_SPACE_YCBCR_STUDIO_G2084_LEFT_P2020,
            SDL_COLORSPACE_BT2020_FULL | SDL_COLORSPACE_BT709_FULL | SDL_COLORSPACE_BT601_FULL if transfer == Transfer::Hlg => DXGI_COLOR_SPACE_YCBCR_FULL_GHLG_TOPLEFT_P2020,
            _ if transfer == Transfer::Hlg => DXGI_COLOR_SPACE_YCBCR_STUDIO_GHLG_TOPLEFT_P2020,
            SDL_COLORSPACE_BT2020_LIMITED => DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P2020,
            SDL_COLORSPACE_BT2020_FULL => DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P2020,
            SDL_COLORSPACE_BT601_LIMITED => DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P601,
            SDL_COLORSPACE_BT601_FULL => DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P601,
            SDL_COLORSPACE_BT709_FULL => DXGI_COLOR_SPACE_YCBCR_FULL_G22_LEFT_P709,
            _ => DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
        };
        let output = if hdr_output { DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020 } else { DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 };
        (input, output)
    }

    /// The Direct3D 11 video processor from one texture into another, with a vendor's extensions.
    pub struct Processor {
        immediate: ID3D11DeviceContext,
        /// The picture is copied here first, as V+ copies into `m_EnhancedTexture`: the video
        /// processor reads only textures it may also render into, and SDL's video texture is not one.
        copy: ID3D11Texture2D,
        copy_size: (u32, u32),
        source: ID3D11Texture2D,
        context: ID3D11VideoContext1,
        processor: ID3D11VideoProcessor,
        input: ID3D11VideoProcessorInputView,
        output: ID3D11VideoProcessorOutputView,
    }

    impl Processor {
        /// `picture` is the part of `input` that holds the picture; `engine` None runs the plain
        /// processor (tests, and the base the vendors' extensions sit on), else it is the vendor's
        /// engine and what it runs for this picture.
        /// # Safety
        /// `device`, `input`, and `output` must be live D3D11 objects on this thread.
        pub unsafe fn new(device: &ID3D11Device, input: &ID3D11Texture2D, picture: (u32, u32), output: &ID3D11Texture2D,
            frame_rate: u32, space: (DXGI_COLOR_SPACE_TYPE, DXGI_COLOR_SPACE_TYPE), engine: Option<(Engine, Plan)>) -> Result<Self, String> {
            // SAFETY: caller guarantees `device`, `input`, and `output` are live D3D11 objects on this thread.
            unsafe {
                let mut out_desc = D3D11_TEXTURE2D_DESC::default();
                output.GetDesc(&mut out_desc);
                let size = (out_desc.Width, out_desc.Height);
                let (video, context, enumerator, processor) = processor(device, picture, size, frame_rate).map_err(|e| format!("cannot make the video processor: {e}"))?;
                let mut in_desc = D3D11_TEXTURE2D_DESC::default();
                input.GetDesc(&mut in_desc);
                let mut copy = None;
                device.CreateTexture2D(&D3D11_TEXTURE2D_DESC { BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                    Usage: D3D11_USAGE_DEFAULT, CPUAccessFlags: 0, MiscFlags: 0, ..in_desc }, None, Some(&mut copy))
                    .map_err(|e| format!("cannot make the video processor's input texture: {e}"))?;
                let copy = copy.unwrap();
                let mut input_view = None;
                video.CreateVideoProcessorInputView(&copy, &enumerator, &D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC { FourCC: 0, ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } } }, Some(&mut input_view))
                    .map_err(|e| format!("the video processor cannot read the picture: {e}"))?;
                let mut output_view = None;
                video.CreateVideoProcessorOutputView(output, &enumerator, &D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC { ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } } }, Some(&mut output_view))
                    .map_err(|e| format!("the video processor cannot write the enhanced picture: {e}"))?;
                let p = &processor;
                context.VideoProcessorSetStreamSourceRect(p, 0, true, Some(&RECT { left: 0, top: 0, right: picture.0 as i32, bottom: picture.1 as i32 }));
                context.VideoProcessorSetStreamDestRect(p, 0, true, Some(&RECT { left: 0, top: 0, right: size.0 as i32, bottom: size.1 as i32 }));
                context.VideoProcessorSetStreamFrameFormat(p, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
                context.VideoProcessorSetStreamColorSpace1(p, 0, space.0);
                context.VideoProcessorSetOutputTargetRect(p, true, Some(&RECT { left: 0, top: 0, right: size.0 as i32, bottom: size.1 as i32 }));
                context.VideoProcessorSetStreamOutputRate(p, 0, D3D11_VIDEO_PROCESSOR_OUTPUT_RATE_NORMAL, false, None);
                context.VideoProcessorSetOutputBackgroundColor(p, false, &D3D11_VIDEO_COLOR { Anonymous: D3D11_VIDEO_COLOR_0 { RGBA: D3D11_VIDEO_COLOR_RGBA { R: 0.0, G: 0.0, B: 0.0, A: 1.0 } } });
                context.VideoProcessorSetOutputColorSpace1(p, space.1);
                if let Some((engine, plan)) = engine {
                    // V+'s filters for both vendors with a video processor path, where the driver has them.
                    let mut caps = D3D11_VIDEO_PROCESSOR_CAPS::default();
                    if enumerator.GetVideoProcessorCaps(&mut caps).is_ok() {
                        if caps.FilterCaps & D3D11_VIDEO_PROCESSOR_FILTER_CAPS_NOISE_REDUCTION.0 as u32 != 0 {
                            context.VideoProcessorSetStreamFilter(p, 0, D3D11_VIDEO_PROCESSOR_FILTER_NOISE_REDUCTION, true, 30);
                        }
                        if caps.FilterCaps & D3D11_VIDEO_PROCESSOR_FILTER_CAPS_EDGE_ENHANCEMENT.0 as u32 != 0 {
                            context.VideoProcessorSetStreamFilter(p, 0, D3D11_VIDEO_PROCESSOR_FILTER_EDGE_ENHANCEMENT, true, 20);
                        }
                    }
                    match engine {
                        Engine::NvidiaRtx => {
                            nvidia_super_resolution(&context, p, plan.super_resolution)?;
                            if plan.sdr_to_hdr { nvidia_hdr(&context, p, true)?; }
                        }
                        Engine::IntelVpe => intel_super_resolution(&context, p, plan.super_resolution)?,
                        Engine::AmdAmf => return Err("AMD's super resolution runs in AMF, not the video processor".into()),
                    }
                }
                let immediate = device.GetImmediateContext().map_err(|e| e.to_string())?;
                Ok(Self { immediate, copy, copy_size: (in_desc.Width, in_desc.Height), source: input.clone(), context, processor, input: input_view.unwrap(), output: output_view.unwrap() })
            }
        }

        /// # Safety
        /// `self.copy`, `self.source`, and `self.processor` must be live on this thread.
        pub unsafe fn blt(&self) -> Result<(), String> {
            // SAFETY: Caller upholds live objects on this thread.
            unsafe { self.blt_from(None) }
        }

        /// Like `blt`, optionally reading directly from a decoder/bridge `(texture, slice)` to skip SDL copy.
        /// # Safety
        /// `self.copy`, `source` (if any), and `self.processor` must be live on this thread.
        pub unsafe fn blt_from(&self, source: Option<(&ID3D11Texture2D, u32)>) -> Result<(), String> {
            // SAFETY: caller guarantees `self.copy`, `source`, and `self.processor` are live on this thread.
            unsafe {
                match source {
                    Some((tex, slice)) => {
                        let region = D3D11_BOX { left: 0, top: 0, front: 0, right: self.copy_size.0, bottom: self.copy_size.1, back: 1 };
                        self.immediate.CopySubresourceRegion(&self.copy, 0, 0, 0, 0, tex, slice, Some(&region));
                    }
                    None => self.immediate.CopyResource(&self.copy, &self.source),
                }
                let stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: std::mem::ManuallyDrop::new(Some(self.input.clone())), ..Default::default() };
                let mut streams = [stream];
                let result = self.context.VideoProcessorBlt(&self.processor, &self.output, 0, &streams);
                std::mem::ManuallyDrop::drop(&mut streams[0].pInputSurface);
                result.map_err(|e| format!("the video processor failed: {e}"))
            }
        }
    }

    enum Pass {
        Processor(Processor),
        Amf {
            scaler: HqScaler,
            state: ID3DDeviceContextState,
            context: ID3D11DeviceContext1,
            immediate: ID3D11DeviceContext,
            input: ID3D11Texture2D,
            output: ID3D11Texture2D,
            last_bridge: Option<u64>,
        },
    }

    /// What an enhancer was made for; another picture shape or output size makes a new one.
    #[derive(Clone, Copy, PartialEq)]
    pub struct Key {
        pub source: Source,
        pub output: (i32, i32),
        pub hdr_display: bool,
    }

    pub struct Enhancer {
        key: Key,
        texture: *mut SDL_Texture,
        pass: Pass,
        pub label: String,
    }

    /// Copies the SDR white point and HDR headroom of `from` into texture creation `props`.
    unsafe fn copy_hdr_properties(from: *mut SDL_Texture, props: SDL_PropertiesID) {
        // SAFETY: caller guarantees `from` and `props` are live SDL handles on this thread.
        unsafe {
            let source = SDL_GetTextureProperties(from);
            let white = SDL_GetFloatProperty(source, SDL_PROP_TEXTURE_SDR_WHITE_POINT_FLOAT, 0.0);
            if white > 0.0 { SDL_SetFloatProperty(props, SDL_PROP_TEXTURE_CREATE_SDR_WHITE_POINT_FLOAT, white); }
            let headroom = SDL_GetFloatProperty(source, SDL_PROP_TEXTURE_HDR_HEADROOM_FLOAT, 0.0);
            if headroom > 0.0 { SDL_SetFloatProperty(props, SDL_PROP_TEXTURE_CREATE_HDR_HEADROOM_FLOAT, headroom); }
        }
    }

    unsafe fn create_texture(renderer: *mut SDL_Renderer, format: SDL_PixelFormat, access: SDL_TextureAccess, size: (i32, i32), space: SDL_Colorspace, hdr_from: Option<*mut SDL_Texture>) -> Result<(*mut SDL_Texture, ID3D11Texture2D), String> {
        // SAFETY: caller guarantees `renderer` and `hdr_from` are live SDL handles on this thread.
        unsafe {
            let props = SDL_CreateProperties();
            SDL_SetNumberProperty(props, SDL_PROP_TEXTURE_CREATE_FORMAT_NUMBER, i64::from(format.0));
            SDL_SetNumberProperty(props, SDL_PROP_TEXTURE_CREATE_ACCESS_NUMBER, i64::from(access.0));
            SDL_SetNumberProperty(props, SDL_PROP_TEXTURE_CREATE_WIDTH_NUMBER, i64::from(size.0));
            SDL_SetNumberProperty(props, SDL_PROP_TEXTURE_CREATE_HEIGHT_NUMBER, i64::from(size.1));
            SDL_SetNumberProperty(props, SDL_PROP_TEXTURE_CREATE_COLORSPACE_NUMBER, i64::from(space.0));
            if let Some(from) = hdr_from { copy_hdr_properties(from, props); }
            let texture = SDL_CreateTextureWithProperties(renderer, props);
            SDL_DestroyProperties(props);
            if texture.is_null() { return Err(format!("SDL could not make the {}x{} enhanced picture texture: {}", size.0, size.1, crate::sdl::error())); }
            SDL_SetTextureBlendMode(texture, SDL_BLENDMODE_NONE);
            SDL_SetTextureScaleMode(texture, SDL_SCALEMODE_LINEAR);
            let raw = SDL_GetPointerProperty(SDL_GetTextureProperties(texture), SDL_PROP_TEXTURE_D3D11_TEXTURE_POINTER, ptr::null_mut());
            match ID3D11Texture2D::from_raw_borrowed(&raw) {
                Some(d3d11) => Ok((texture, d3d11.clone())),
                None => { SDL_DestroyTexture(texture); Err("the enhanced picture texture is not a Direct3D 11 texture".into()) }
            }
        }
    }

    impl Enhancer {
        /// Sets up `capability`'s engine for `source` drawn at `output` on `renderer`.
        pub fn new(renderer: *mut SDL_Renderer, key: Key, capability: Capability, frame_rate: u32) -> Result<Self, String> {
            let source = key.source;
            let plan = plan(capability, source.hdr(), source.ten_bit, key.hdr_display);
            let label = super::describe(capability, plan);
            let raw = crate::render_device::live_renderer_pointer(renderer, SDL_PROP_RENDERER_D3D11_DEVICE_POINTER);
            // SAFETY: The renderer's live device and textures, used on the presenting thread.
            unsafe {
                let device = ID3D11Device::from_raw_borrowed(&raw).ok_or("AI enhancement needs the Direct3D 11 renderer")?;
                let input = ID3D11Texture2D::from_raw_borrowed(&source.d3d11).ok_or("the video texture is not a Direct3D 11 texture")?.clone();
                if capability.engine == Engine::AmdAmf {
                    // SDL rounds NV12 and P010 textures up to even sizes; AMF writes the whole texture.
                    let size = ((key.output.0 + 1) & !1, (key.output.1 + 1) & !1);
                    let format = if source.ten_bit { SDL_PIXELFORMAT_P010 } else { SDL_PIXELFORMAT_NV12 };
                    let (texture, output) = create_texture(renderer, format, SDL_TEXTUREACCESS_STATIC, size, source.colorspace, Some(source.texture))?;
                    let built = (|| {
                        let mut desc = D3D11_TEXTURE2D_DESC::default();
                        input.GetDesc(&mut desc);
                        let (width, height) = amf_input(source.picture, (desc.Width, desc.Height));
                        let scaler = HqScaler::new(device, amf_scaler::Parameters { width, height,
                            output_width: size.0, output_height: size.1, frame_rate: frame_rate.max(1) as i32, ten_bit: source.ten_bit,
                            sharpness: amf_scaler::sharpness(key.output != source.picture, source.hdr()) })?;
                        let mut state = None;
                        device.cast::<ID3D11Device1>().map_err(|e| e.to_string())?
                            .CreateDeviceContextState(0, &[device.GetFeatureLevel()], D3D11_SDK_VERSION, &ID3D11Device::IID, None, Some(&mut state))
                            .map_err(|e| format!("cannot make AMF's pipeline state: {e}"))?;
                        let immediate = device.GetImmediateContext().map_err(|e| e.to_string())?;
                        let context = immediate.cast::<ID3D11DeviceContext1>().map_err(|e| e.to_string())?;
                        Ok::<_, String>(Pass::Amf { scaler, state: state.unwrap(), context, immediate, input, output, last_bridge: None })
                    })();
                    return match built {
                        Ok(pass) => Ok(Self { key, texture, pass, label }),
                        Err(e) => { SDL_DestroyTexture(texture); Err(e) }
                    };
                }
                let space = if plan.hdr_output { SDL_COLORSPACE_HDR10 } else { SDL_COLORSPACE_SRGB };
                let (texture, output) = create_texture(renderer, SDL_PIXELFORMAT_ABGR2101010, SDL_TEXTUREACCESS_TARGET, key.output, space,
                    source.hdr().then_some(source.texture))?;
                let picture = (source.picture.0 as u32, source.picture.1 as u32);
                match Processor::new(device, &input, picture, &output, frame_rate, colors(source.colorspace, source.transfer, plan.hdr_output), Some((capability.engine, plan))) {
                    Ok(processor) => Ok(Self { key, texture, pass: Pass::Processor(processor), label }),
                    Err(e) => { SDL_DestroyTexture(texture); Err(e) }
                }
            }
        }

        pub fn matches(&self, key: &Key) -> bool {
            self.key == *key
        }

        /// Whether this enhancer can read `frame`'s D3D11 surface directly without `copy_held()`.
        pub fn can_read_held(&self, frame: &crate::media::Frame) -> bool {
            match &self.pass {
                Pass::Processor(_) => frame.d3d11_surface().is_some_and(|s| s.readable),
                Pass::Amf { .. } => frame.is_d3d11_bridge() && frame.d3d11_surface().is_some_and(|s| s.readable),
            }
        }

        /// Enhances `held` directly (skipping the SDL texture copy) or `self.source` if `None`.
        pub fn draw_frame(&mut self, renderer: *mut SDL_Renderer, held: Option<&crate::media::Frame>) -> Result<*mut SDL_Texture, String> {
            if let Some(frame) = held { frame.wait_d3d11_ready()?; }
            // SAFETY: Live renderer, device and textures on the presenting thread.
            unsafe {
                if !SDL_FlushRenderer(renderer) { return Err(format!("SDL could not flush: {}", crate::sdl::error())); }
                match (&mut self.pass, held) {
                    (Pass::Processor(processor), Some(frame)) => {
                        let surface = frame.d3d11_surface().ok_or("held frame has no D3D11 surface")?;
                        let src = ID3D11Texture2D::from_raw_borrowed(&surface.texture).ok_or("held frame has no texture")?;
                        let mut result = Err("held enhance did not run".into());
                        frame.with_decoder_lock(&mut || result = processor.blt_from(Some((src, surface.slice))));
                        result?;
                    }
                    (Pass::Processor(processor), None) => processor.blt()?,
                    (Pass::Amf { scaler, state, context, immediate, input, output, last_bridge }, held) => {
                        let next_bridge = held.and_then(crate::media::Frame::d3d11_bridge_id);
                        if *last_bridge != next_bridge {
                            if last_bridge.is_some() {
                                scaler.clear_surfaces();
                            }
                            *last_bridge = next_bridge;
                        }
                        let mut run = |src: &ID3D11Texture2D| {
                            let mut previous = None;
                            context.SwapDeviceContextState(&*state, Some(&mut previous));
                            let rendered = scaler.render(src, output, immediate);
                            context.SwapDeviceContextState(previous.as_ref(), None);
                            rendered
                        };
                        if let Some(frame) = held {
                            let surface = frame.d3d11_surface().ok_or("held frame has no D3D11 surface")?;
                            let src = ID3D11Texture2D::from_raw_borrowed(&surface.texture).ok_or("held frame has no texture")?;
                            let mut result = Err("held AMF enhance did not run".into());
                            frame.with_decoder_lock(&mut || result = run(src));
                            result?;
                        } else { run(input)?; }
                    }
                }
            }
            Ok(self.texture)
        }
    }

    impl Drop for Enhancer {
        fn drop(&mut self) {
            // SAFETY: The enhancer's own texture on a live renderer.
            unsafe { SDL_DestroyTexture(self.texture) };
        }
    }
}

#[cfg(test)]
mod tests;
