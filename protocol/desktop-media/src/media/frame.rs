//! Decoded FFmpeg `Frame`, `Picture` presentation wrapper, pixel-format classification, and
//! Windows D3D11VA / HDR10+ / Dolby Vision frame metadata accessors.

#[cfg(windows)]
use std::ffi::c_void;
use std::ffi::{c_int, CStr};
use std::ptr::NonNull;

#[cfg(windows)]
use super::d3d11;
use super::ffi::*;
use super::{
    centred_chroma, wavelet_pool, FrameInfo, FRAME_D3D11, FRAME_D3D11_444, FRAME_NV12, FRAME_P010,
    FRAME_VIDEOTOOLBOX, FRAME_WAVELET, FRAME_YUV420P, FRAME_YUV420P10, FRAME_YUV444P,
    FRAME_YUV444P10, FRAME_YUV444P16,
};
// Linux and macOS (unix): DMA-BUF and Vulkan picture kind constants.
#[cfg(unix)]
use super::{FRAME_DMABUF, FRAME_VULKAN, FRAME_VULKAN444, FRAME_VULKAN_INTERLEAVED444};

/// The presenter's kind (`FRAME_*`) and 10-bit flag of a picture of `format`, whose hardware
/// frames context holds `sw_format` pictures (None without one); `dmabuf`: a VAAPI picture
/// exported as DMA-BUFs. Kind 4 is a picture nothing here shows.
pub(super) fn frame_kind(format: AVPixelFormat, sw_format: Option<AVPixelFormat>, dmabuf: bool) -> (c_int, c_int) {
    match format {
        AV_PIX_FMT_NV12 => (FRAME_NV12, 0),
        AV_PIX_FMT_P010LE => (FRAME_P010, 1),
        AV_PIX_FMT_YUV420P | AV_PIX_FMT_YUVJ420P => (FRAME_YUV420P, 0),
        AV_PIX_FMT_YUV444P | AV_PIX_FMT_YUVJ444P => (FRAME_YUV444P, 0),
        AV_PIX_FMT_YUV420P10LE => (FRAME_YUV420P10, 1),
        AV_PIX_FMT_YUV444P10LE => (FRAME_YUV444P10, 1),
        // NVDEC's 10-bit 4:4:4 copied to memory: its samples are in the high bits.
        AV_PIX_FMT_YUV444P16LE => (FRAME_YUV444P16, 1),
        // VideoToolbox: only NV12 and P010 surfaces copy into the presenter's textures; a 4:4:4
        // surface is a picture nothing here can show.
        AV_PIX_FMT_VIDEOTOOLBOX => match sw_format {
            Some(AV_PIX_FMT_NV12) => (FRAME_VIDEOTOOLBOX, 0),
            Some(AV_PIX_FMT_P010LE) => (FRAME_VIDEOTOOLBOX, 1),
            _ => (4, 0),
        },
        // NV12 and P010 surfaces copy into the presenter's textures. FFmpeg names the 4:4:4 ones
        // by their packing: AYUV is VUYX, Y410 XV30 and Y416 XV36 (XV48 for 16-bit samples).
        // Without a frames context, NV12.
        AV_PIX_FMT_D3D11 => match sw_format.unwrap_or(AV_PIX_FMT_NV12) {
            AV_PIX_FMT_NV12 => (FRAME_D3D11, 0),
            AV_PIX_FMT_P010LE => (FRAME_D3D11, 1),
            AV_PIX_FMT_VUYX | AV_PIX_FMT_VUYA => (FRAME_D3D11_444, 0),
            AV_PIX_FMT_XV30LE | AV_PIX_FMT_XV36LE => (FRAME_D3D11_444, 1),
            // Only FFmpeg builds as new as Windows' name XV48; D3D11 is Windows' alone.
            #[cfg(windows)]
            AV_PIX_FMT_XV48LE => (FRAME_D3D11_444, 1),
            _ => (4, 0),
        },
        // Linux and macOS (unix): Vulkan video and mapped VAAPI frame kinds.
        #[cfg(unix)]
        AV_PIX_FMT_VULKAN => match sw_format.unwrap_or(AV_PIX_FMT_NONE) {
            AV_PIX_FMT_NV12 => (FRAME_VULKAN, 0),
            AV_PIX_FMT_P010LE => (FRAME_VULKAN, 1),
            AV_PIX_FMT_NV24 => (FRAME_VULKAN444, 0),
            AV_PIX_FMT_P410LE => (FRAME_VULKAN444, 1),
            // A mapped 4:4:4 VAAPI picture (`vulkan_video::interleaved_444`).
            AV_PIX_FMT_RGBA => (FRAME_VULKAN_INTERLEAVED444, 0),
            AV_PIX_FMT_XV30LE => (FRAME_VULKAN_INTERLEAVED444, 1),
            // Not two planes the presenter copies (`vulkan_video::Planes`).
            _ => (4, 0),
        },
        // Linux and macOS (unix): VAAPI surface exported as DMA-BUFs.
        #[cfg(unix)]
        AV_PIX_FMT_VAAPI if dmabuf => (FRAME_DMABUF, i32::from(sw_format == Some(AV_PIX_FMT_P010LE))),
        _ => {
            let _ = dmabuf;
            (4, 0)
        }
    }
}

/// The empty `AVFrame` structures a decoder and its presenter pass back and forth: the decoder
/// makes its pictures from them, and the presenter gives each back when it lets the picture go
/// instead of freeing it. Bounded, so a presenter that stops taking pictures cannot keep more
/// than a few.
pub(super) struct FramePool {
    free: std::sync::Mutex<Vec<NonNull<AVFrame>>>,
    /// The structures made and the ones taken back, for the tests that count allocations.
    #[cfg(test)]
    made: std::sync::atomic::AtomicU64,
    #[cfg(test)]
    reused: std::sync::atomic::AtomicU64,
}

/// The empty frame structures a decoder keeps for its stream's pictures: the one being decoded,
/// the presenter's queue (8 pictures at most, host times keeps every one) and the pacer's share,
/// with room for a stray one. Each structure is a few hundred bytes, none of them a surface.
pub(super) const POOLED_FRAMES: usize = 16;

// SAFETY: An `AVFrame` structure is allocated and freed by whichever thread FFmpeg's frame API is
// called on; a pooled one is unreferenced and empty, and only its former owner takes it again.
unsafe impl Send for FramePool {}
unsafe impl Sync for FramePool {}

impl FramePool {
    pub(super) fn new() -> Self {
        Self {
            free: std::sync::Mutex::default(),
            #[cfg(test)]
            made: std::sync::atomic::AtomicU64::new(0),
            #[cfg(test)]
            reused: std::sync::atomic::AtomicU64::new(0),
        }
    }

    /// An empty frame: one a picture gave back, or a new one.
    pub(super) fn take(&self) -> Option<NonNull<AVFrame>> {
        if let Some(raw) = self.free.lock().unwrap_or_else(|e| e.into_inner()).pop() {
            #[cfg(test)]
            self.reused.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Some(raw);
        }
        #[cfg(test)]
        self.made.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // SAFETY: An allocated structure, empty as FFmpeg makes it.
        NonNull::new(unsafe { av_frame_alloc() })
    }

    /// Takes an empty frame back for the next picture, or frees it when the pool is full.
    pub(super) fn give(&self, raw: NonNull<AVFrame>) {
        // SAFETY: The picture is being let go; what it held (a decoder surface, a mapping) is
        // unreferenced here, as `av_frame_free` did, before the structure is reused.
        unsafe { av_frame_unref(raw.as_ptr()) };
        let kept = {
            let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
            if free.len() < POOLED_FRAMES {
                free.push(raw);
                true
            } else {
                false
            }
        };
        if !kept {
            // SAFETY: An empty structure of `av_frame_alloc`, no longer pooled.
            unsafe { av_frame_free(&mut raw.as_ptr()) };
        }
    }

    #[cfg(test)]
    pub(super) fn counts(&self) -> (u64, u64) {
        (
            self.made.load(std::sync::atomic::Ordering::Relaxed),
            self.reused.load(std::sync::atomic::Ordering::Relaxed),
        )
    }

    #[cfg(test)]
    pub(super) fn idle(&self) -> usize {
        self.free.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

impl Drop for FramePool {
    fn drop(&mut self) {
        // SAFETY: Empty structures of `av_frame_alloc`, and no picture holds them: it holds the
        // pool instead.
        for raw in self.free.get_mut().unwrap_or_else(|e| e.into_inner()).drain(..) {
            unsafe { av_frame_free(&mut raw.as_ptr()) };
        }
    }
}

pub struct Frame {
    pub(crate) raw: NonNull<AVFrame>,
    pub(super) wavelet: bool,
    /// The decoder's empty frames: a picture made from one goes back there when it is let go, so
    /// the stream allocates no frame struct per picture. None for a frame made elsewhere (the
    /// wavelet pass).
    pool: Option<std::sync::Arc<FramePool>>,
    /// The VAAPI surface's DMA-BUFs, for a presenter that imports them.
    // Linux and macOS (unix): DMA-BUF export attached to VAAPI frames.
    #[cfg(unix)]
    pub(super) dmabuf: Option<Box<crate::dmabuf::Export>>,
    /// Renderer-visible NT-handle shared texture exported from a separate D3D11 decode device.
    #[cfg(windows)]
    pub(super) shared: Option<d3d11::SharedFrame>,
    /// Picture metadata snapshotted before unreffing the decoder's `AVFrame` on bridge export.
    #[cfg(windows)]
    pub(super) exported: Option<Box<ExportedMeta>>,
}

#[cfg(windows)]
#[derive(Clone)]
pub(super) struct ExportedMeta {
    pub(super) info: FrameInfo,
    pub(super) hdr10_plus: Option<f32>,
    pub(super) dolby_vision: Option<DolbyVision>,
    pub(super) format_name: String,
    pub(super) corrupt: bool,
}

// An owned AVFrame reference can be handed from the decoder to the renderer.
unsafe impl Send for Frame {}

impl Frame {
    pub(super) fn new(raw: NonNull<AVFrame>) -> Self {
        Self {
            raw,
            wavelet: false,
            pool: None,
            // Linux and macOS (unix): no DMA-BUF export by default.
            #[cfg(unix)]
            dmabuf: None,
            #[cfg(windows)]
            shared: None,
            #[cfg(windows)]
            exported: None,
        }
    }

    /// An empty frame of `pool`, which takes its structure back when the picture is let go.
    pub(super) fn pooled(pool: &std::sync::Arc<FramePool>) -> Option<Self> {
        Some(Self {
            raw: pool.take()?,
            wavelet: false,
            pool: Some(pool.clone()),
            // Linux and macOS (unix): no DMA-BUF export by default.
            #[cfg(unix)]
            dmabuf: None,
            #[cfg(windows)]
            shared: None,
            #[cfg(windows)]
            exported: None,
        })
    }

    /// An empty frame of this picture's pool, or a new one when it has none.
    fn empty_like(&self) -> Option<Self> {
        match &self.pool {
            Some(pool) => Self::pooled(pool),
            None => Some(Self::new(NonNull::new(unsafe { av_frame_alloc() })?)),
        }
    }

    /// Another reference to the same picture (its surface, for a hardware one), without the
    /// export: what keeps an imported surface from going back to the decoder while it is shown.
    pub fn reference(&self) -> Option<Self> {
        #[cfg(windows)]
        if self.shared.is_some() {
            // Bridged frames already released FFmpeg's `AVFrame`; carry only the shared surface
            // lease and snapshotted metadata so callers hold no decoder surface across frames.
            let mut frame = self.empty_like()?;
            frame.wavelet = self.wavelet;
            frame.shared = self.shared.clone();
            frame.exported = self.exported.clone();
            return Some(frame);
        }
        let mut frame = self.empty_like()?;
        // SAFETY: a live frame; the reference is a new one to its buffers, in a frame of the pool.
        if unsafe { av_frame_ref(frame.raw.as_ptr(), self.raw.as_ptr()) } < 0 {
            return None;
        }
        frame.wavelet = self.wavelet;
        Some(frame)
    }

    #[cfg(all(windows, test))]
    pub(super) fn from_test_shared(shared: d3d11::SharedFrame, width: i32, height: i32, ten_bit: bool) -> Option<Self> {
        let mut frame = Self::new(NonNull::new(unsafe { av_frame_alloc() })?);
        frame.exported = Some(Box::new(ExportedMeta {
            info: FrameInfo {
                width, height, kind: FRAME_D3D11, ten_bit: i32::from(ten_bit), pts: 0,
                data: [std::ptr::null_mut(); 3], linesize: [0; 3],
                color_space: 1, color_range: 1, color_trc: 1, centred_chroma: false,
            },
            hdr10_plus: None,
            dolby_vision: None,
            format_name: if ten_bit { "p010le" } else { "nv12" }.into(),
            corrupt: false,
        }));
        frame.shared = Some(shared);
        Some(frame)
    }

    /// Notes that `Present` flushed the renderer queue, skipping a redundant `SlotLease::drop` flush.
    pub fn mark_d3d11_flushed(&self) {
        #[cfg(windows)]
        if let Some(shared) = &self.shared { shared.mark_renderer_flushed(); }
    }

    /// A VAAPI picture mapped for reading, not transferred: the driver's copy of the surface goes
    /// to the presenter's texture upload as it is, where a transfer copies it once more into a new
    /// frame first. The mapping holds the surface until the picture is dropped, as a D3D11
    /// picture does.
    pub fn map_to_memory(&self) -> Option<Self> {
        // SAFETY: a live frame; the mapping is a new frame of its own.
        unsafe {
            let memory = self.empty_like()?;
            (av_hwframe_map(memory.raw.as_ptr(), self.raw.as_ptr(), AV_HWFRAME_MAP_READ as c_int) >= 0
                && av_frame_copy_props(memory.raw.as_ptr(), self.raw.as_ptr()) >= 0)
                .then_some(memory)
        }
    }

    /// Whether FFmpeg says the picture is damaged: flagged corrupt (missing references, concealed
    /// slices) or with decode error flags.
    pub fn corrupt(&self) -> bool {
        #[cfg(windows)]
        if let Some(meta) = &self.exported {
            return meta.corrupt;
        }
        // SAFETY: a live frame.
        let frame = unsafe { self.raw.as_ref() };
        frame.flags & AV_FRAME_FLAG_CORRUPT as c_int != 0 || frame.decode_error_flags != 0
    }

    /// The exported DMA-BUFs of a `FRAME_DMABUF` picture.
    // Linux and macOS (unix): DMA-BUF export accessor.
    #[cfg(unix)]
    pub fn dmabuf(&self) -> Option<&crate::dmabuf::Export> {
        self.dmabuf.as_deref()
    }

    pub fn wavelet(width: usize, height: usize) -> Result<Self, String> {
        wavelet_pool::alloc(width, height)
    }

    pub fn info(&self) -> FrameInfo {
        #[cfg(windows)]
        if let Some(meta) = &self.exported {
            return meta.info;
        }
        unsafe {
            let frame = self.raw.as_ref();
            let (kind, ten_bit) = if self.wavelet {
                (FRAME_WAVELET, 0)
            } else {
                let sw_format = (!frame.hw_frames_ctx.is_null()).then(|| (*((*frame.hw_frames_ctx).data.cast::<AVHWFramesContext>())).sw_format);
                // Linux and macOS (unix): check whether this frame carries a DMA-BUF export.
                #[cfg(unix)]
                let dmabuf = self.dmabuf.is_some();
                // Non-Unix platforms (Windows): frames never carry DMA-BUF exports.
                #[cfg(not(unix))]
                let dmabuf = false;
                frame_kind(frame.format as AVPixelFormat, sw_format, dmabuf)
            };
            FrameInfo {
                width: frame.width,
                height: frame.height,
                kind,
                ten_bit,
                pts: frame.pts,
                data: [frame.data[0], frame.data[1], frame.data[2]],
                linesize: [frame.linesize[0], frame.linesize[1], frame.linesize[2]],
                color_space: frame.colorspace as i32,
                color_range: frame.color_range as i32,
                color_trc: frame.color_trc as i32,
                centred_chroma: centred_chroma(frame.chroma_location),
            }
        }
    }

    pub fn format_name(&self) -> String {
        #[cfg(windows)]
        if let Some(meta) = &self.exported {
            return meta.format_name.clone();
        }
        unsafe {
            let name = av_get_pix_fmt_name(self.raw.as_ref().format as AVPixelFormat);
            if name.is_null() {
                "unknown".into()
            } else {
                CStr::from_ptr(name).to_string_lossy().into_owned()
            }
        }
    }

    #[cfg(windows)]
    pub fn copy_to_d3d11(&self, texture: *mut c_void, width: i32, height: i32) -> bool {
        if let Some(shared) = &self.shared {
            // SAFETY: Caller guarantees `texture` is a live `ID3D11Texture2D` on the renderer device.
            return unsafe { shared.copy_to_renderer_texture(texture, width, height) };
        }
        // SAFETY: Caller guarantees `texture` is a live `ID3D11Texture2D`; `self.raw` is live.
        unsafe { d3d11::copy_shared_device_frame(self.raw.as_ref(), texture, width, height) }
    }
}

impl Frame {
    /// Whether the picture's 4:2:0 chroma sits midway between its luma columns, as the stream
    /// says (`AVFrame::chroma_location`), rather than on the left one.
    pub fn centred_chroma(&self) -> bool {
        #[cfg(windows)]
        if let Some(meta) = &self.exported {
            return meta.info.centred_chroma;
        }
        // SAFETY: A plain field of the live AVFrame.
        centred_chroma(unsafe { self.raw.as_ref() }.chroma_location)
    }
}

/// A D3D11VA picture: the decoder's texture array and the picture's slice in it.
#[cfg(windows)]
pub struct D3d11Surface {
    pub texture: *mut c_void,
    pub slice: u32,
    /// The textures were made with `D3D11_BIND_SHADER_RESOURCE` (`bind_frames`).
    pub readable: bool,
}

#[cfg(windows)]
impl Frame {
    pub fn d3d11_surface(&self) -> Option<D3d11Surface> {
        // SAFETY: `self.raw` is a live `AVFrame`.
        unsafe { d3d11::surface_of(self.raw.as_ref(), self.shared.as_ref()) }
    }

    pub fn wait_d3d11_ready(&self) -> Result<(), String> {
        self.shared.as_ref().map_or(Ok(()), d3d11::SharedFrame::wait_on_renderer)
    }

    pub fn is_d3d11_bridge(&self) -> bool { self.shared.is_some() }

    pub fn d3d11_bridge_id(&self) -> Option<u64> {
        self.shared.as_ref().map(d3d11::SharedFrame::bridge_id)
    }

    pub fn with_decoder_lock(&self, f: &mut dyn FnMut()) {
        // SAFETY: `self.raw` is a live `AVFrame`.
        unsafe { d3d11::with_lock(self.raw.as_ref(), self.shared.as_ref(), f) }
    }
    pub fn with_shared_device_lock(&self, f: &mut dyn FnMut()) {
        // SAFETY: `self.raw` is a live `AVFrame`.
        unsafe { d3d11::with_shared_device_lock(self.raw.as_ref(), self.shared.as_ref(), f) }
    }

    /// The frame's HDR10+ (SMPTE ST 2094-40) scene peak in nits: the largest MaxSCL of its first
    /// processing window, as libplacebo maps it (`10000 * maxscl`). None without HDR10+ or when
    /// the peak is 0.
    pub fn hdr10_plus_peak(&self) -> Option<f32> {
        #[cfg(windows)]
        if let Some(meta) = &self.exported {
            return meta.hdr10_plus;
        }
        // SAFETY: Side data of the live AVFrame, sized as FFmpeg made it.
        unsafe {
            let side = av_frame_get_side_data(self.raw.as_ptr(), AV_FRAME_DATA_DYNAMIC_HDR_PLUS);
            if side.is_null() || (*side).size < std::mem::size_of::<AVDynamicHDRPlus>() {
                return None;
            }
            let metadata = &*(*side).data.cast::<AVDynamicHDRPlus>();
            if metadata.num_windows == 0 {
                return None;
            }
            let peak = metadata.params[0]
                .maxscl
                .iter()
                .filter(|r| r.den != 0)
                .map(|r| 10000.0 * f64::from(r.num) / f64::from(r.den))
                .fold(0.0, f64::max);
            (peak > 0.0).then_some(peak as f32)
        }
    }

    /// What the frame's Dolby Vision RPU says, from the metadata FFmpeg's HEVC, AV1 and dav1d
    /// decoders attach to every frame with an RPU (`AV_FRAME_DATA_DOVI_METADATA`). None without one.
    pub fn dolby_vision(&self) -> Option<DolbyVision> {
        #[cfg(windows)]
        if let Some(meta) = &self.exported {
            return meta.dolby_vision;
        }
        // SAFETY: Side data of the live AVFrame, sized as FFmpeg made it; the offsets are FFmpeg's
        // and checked against that size.
        unsafe {
            let side = av_frame_get_side_data(self.raw.as_ptr(), AV_FRAME_DATA_DOVI_METADATA);
            if side.is_null() || (*side).size < std::mem::size_of::<AVDOVIMetadata>() {
                return None;
            }
            let metadata = &*(*side).data.cast::<AVDOVIMetadata>();
            let within = |offset: usize, size: usize| offset.checked_add(size).is_some_and(|end| end <= (*side).size);
            if !within(metadata.header_offset, std::mem::size_of::<AVDOVIRpuDataHeader>())
                || !within(metadata.color_offset, std::mem::size_of::<AVDOVIColorMetadata>())
            {
                return None;
            }
            let header = &*(*side).data.add(metadata.header_offset).cast::<AVDOVIRpuDataHeader>();
            let color = &*(*side).data.add(metadata.color_offset).cast::<AVDOVIColorMetadata>();
            let level1 = av_dovi_find_level(metadata, 1);
            let max_pq = if level1.is_null() { None } else { Some((*level1).__bindgen_anon_1.l1.max_pq) };
            Some(DolbyVision::of(header.vdr_rpu_profile, header.bl_video_full_range_flag, max_pq, color.source_max_pq))
        }
    }
}

/// The parts of a Dolby Vision RPU the tone map takes.
#[cfg(windows)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DolbyVision {
    /// The frame's peak in nits: L1's maximum PQ, else the source display's maximum.
    pub peak: Option<f32>,
    /// Profile 5: IPT pictures without an HDR10 base layer (FFmpeg's `ff_dovi_guess_profile_hevc`:
    /// RPU profile 0 with a full-range base layer).
    pub ipt: bool,
}

#[cfg(windows)]
impl DolbyVision {
    /// From the RPU's profile and base-layer range, and its L1 and source maximum in 12-bit PQ.
    pub fn of(vdr_rpu_profile: u8, full_range_base: u8, l1_max_pq: Option<u16>, source_max_pq: u16) -> Self {
        let nits = |pq: u16| (pq > 0).then(|| crate::stream::direct::pq_nits(f64::from(pq.min(4095)) / 4095.0) as f32);
        Self {
            peak: l1_max_pq.and_then(nits).or_else(|| nits(source_max_pq)),
            ipt: vdr_rpu_profile == 0 && full_range_base != 0,
        }
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        // Linux and macOS (unix): close DMA-BUF FDs before returning the VAAPI surface to FFmpeg's pool.
        #[cfg(unix)]
        {
            self.dmabuf = None;
        }
        match &self.pool {
            // The presenter gave the picture back: its structure becomes the decoder's next one.
            Some(pool) => pool.give(self.raw),
            None => unsafe { av_frame_free(&mut self.raw.as_ptr()) },
        }
    }
}

/// What the decoder hands the presenter: an FFmpeg frame, or (PyroWave decoded on the GPU) a set
/// of planes with the decode timeline/fence value that says when they are final (0 when already final).
pub enum Picture {
    Frame(Frame),
    GpuPlanes { lease: crate::stream::gpu_planes::Lease, fence_value: u64 },
    /// Frame already moved into `VideoTexture::held` without `av_frame_clone`.
    Taken,
}

impl Picture {
    /// The FFmpeg frame, for the pictures that are one.
    pub fn frame(&self) -> Option<&Frame> {
        match self { Self::Frame(frame) => Some(frame), _ => None }
    }

    /// Moves the FFmpeg frame out of `self` without cloning its buffer references.
    pub fn take_frame(&mut self) -> Option<Frame> {
        matches!(self, Self::Frame(_)).then(|| match std::mem::replace(self, Self::Taken) {
            Self::Frame(frame) => frame,
            _ => unreachable!(),
        })
    }

    pub fn is_taken(&self) -> bool {
        matches!(self, Self::Taken)
    }

    pub fn format_name(&self) -> String {
        match self {
            Self::Frame(frame) => frame.format_name(),
            Self::GpuPlanes { .. } => "PyroWave GPU planes".into(),
            Self::Taken => "taken".into(),
        }
    }
}
