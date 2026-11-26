//! `AVBufferPool`-backed allocation for 64-bit-per-pixel PyroWave coefficient frames.

use std::ptr::{self, NonNull};
use std::sync::Mutex;

use super::ffi::{
    av_buffer_pool_get, av_buffer_pool_init, av_buffer_pool_uninit, av_frame_alloc, AVBufferPool,
    AV_PIX_FMT_RGBA64LE,
};
use super::Frame;

struct Pool {
    raw: *mut AVBufferPool,
    bytes: usize,
}

impl Drop for Pool {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: `self.raw` is a live `AVBufferPool` owned exclusively by this `Pool`.
            unsafe { av_buffer_pool_uninit(&mut self.raw) };
            self.bytes = 0;
        }
    }
}

// SAFETY: Access is serialized by `WAVELET_POOL` and `AVBufferPool` itself is refcounted/thread-safe.
unsafe impl Send for Pool {}

static WAVELET_POOL: Mutex<Pool> = Mutex::new(Pool {
    raw: ptr::null_mut(),
    bytes: 0,
});

/// Releases the pooled FFmpeg wavelet buffers when a stream closes without a reconnect.
pub(crate) fn clear() {
    let mut pool = WAVELET_POOL.lock().unwrap_or_else(|e| e.into_inner());
    if !pool.raw.is_null() {
        // SAFETY: `pool.raw` is owned by `WAVELET_POOL`; live `AVFrame` refs keep their own buffer refs.
        unsafe { av_buffer_pool_uninit(&mut pool.raw) };
        pool.bytes = 0;
    }
}

fn alloc_from(pool: &mut Pool, width: usize, height: usize) -> Result<Frame, String> {
    if width == 0 || height == 0 {
        return Err("Wavelet frame dimensions must be non-zero".into());
    }
    let linesize = width
        .checked_mul(8)
        .and_then(|b| b.checked_next_multiple_of(32))
        .ok_or("Wavelet row size overflow")?;
    let bytes = linesize.checked_mul(height).ok_or("Wavelet frame size overflow")?;
    if width > i32::MAX as usize || height > i32::MAX as usize || linesize > i32::MAX as usize {
        return Err("Wavelet frame dimensions exceed FFmpeg limits".into());
    }
    // SAFETY: `pool.raw` is either null or an active `AVBufferPool` owned by this `Pool`.
    let buffer = unsafe {
        if pool.raw.is_null() || pool.bytes != bytes {
            av_buffer_pool_uninit(&mut pool.raw);
            pool.bytes = 0;
            pool.raw = av_buffer_pool_init(bytes, None);
            if pool.raw.is_null() {
                return Err("Could not create wavelet buffer pool".into());
            }
            pool.bytes = bytes;
        }
        av_buffer_pool_get(pool.raw)
    };
    if buffer.is_null() {
        return Err("Could not allocate wavelet coefficients".into());
    }
    // SAFETY: `buffer` is a valid `AVBufferRef` of `bytes` bytes from `av_buffer_pool_get`.
    unsafe {
        let Some(raw_ptr) = NonNull::new(av_frame_alloc()) else {
            let mut buffer = buffer;
            super::ffi::av_buffer_unref(&mut buffer);
            return Err("Could not allocate wavelet frame".into());
        };
        let mut frame = Frame::new(raw_ptr);
        frame.wavelet = true;
        let raw = frame.raw.as_mut();
        raw.width = width as i32;
        raw.height = height as i32;
        raw.format = AV_PIX_FMT_RGBA64LE;
        raw.buf[0] = buffer;
        raw.data[0] = (*buffer).data;
        raw.linesize[0] = linesize as i32;
        raw.extended_data = raw.data.as_mut_ptr();
        Ok(frame)
    }
}

/// Allocates a `width` x `height` `AV_PIX_FMT_RGBA64LE` frame backed by a persistent `AVBufferPool`,
/// reusing committed pages across frames instead of faulting fresh ~64 MiB allocations per frame.
pub(super) fn alloc(width: usize, height: usize) -> Result<Frame, String> {
    let mut pool = WAVELET_POOL.lock().unwrap_or_else(|e| e.into_inner());
    alloc_from(&mut pool, width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wavelet_frames_reuse_pooled_buffers_across_allocations() {
        // Isolated pool prevents parallel wavelet tests from resetting the size mid-assertion.
        let mut pool = Pool { raw: ptr::null_mut(), bytes: 0 };
        let first = alloc_from(&mut pool, 64, 32).expect("first wavelet frame");
        let first_info = first.info();
        assert_eq!((first_info.width, first_info.height), (64, 32));
        assert_eq!(first_info.linesize[0], 64 * 8);
        let first_ptr = first_info.data[0] as usize;
        assert_ne!(first_ptr, 0);
        drop(first);

        let second = alloc_from(&mut pool, 64, 32).expect("second wavelet frame");
        assert_eq!(second.info().data[0] as usize, first_ptr, "buffer returns to pool on drop");
        drop(second);

        let resized = alloc_from(&mut pool, 80, 40).expect("resized wavelet frame");
        assert_eq!((resized.info().width, resized.info().height), (80, 40));
        assert_eq!(resized.info().linesize[0], (80 * 8usize).next_multiple_of(32) as i32);
    }
}
