//! Dirty-rectangle tracking and atomic commit-mode rules for the KMS ARGB8888 overlay plane.
//!
//! Video and the stats/menu overlay live on separate KMS hardware planes, so an unchanged overlay
//! stays on screen with zero CPU work while video frames flip underneath it. When the overlay does
//! change (typically once a second), only the painted rectangle is cleared on the software canvas
//! and synced into the back dumb buffer, and an already-enabled overlay plane flips its `FB_ID`
//! without `DRM_MODE_ATOMIC_ALLOW_MODESET`.

/// Non-empty pixel rectangle `[x0..x1, y0..y1]` (`x0 < x1`, `y0 < y1`) within a surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirtyRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl DirtyRect {
    /// Clamps `[x0, y0, x1, y1]` to `0..width` and `0..height`, returning `None` when empty.
    pub fn clamped(bounds: [i32; 4], width: u32, height: u32) -> Option<Self> {
        let clamp_u32 = |v: i32, limit: u32| -> u32 {
            if v <= 0 {
                0
            } else {
                (v as u32).min(limit)
            }
        };
        let x0 = clamp_u32(bounds[0], width);
        let y0 = clamp_u32(bounds[1], height);
        let x1 = clamp_u32(bounds[2], width);
        let y1 = clamp_u32(bounds[3], height);
        (x0 < x1 && y0 < y1).then_some(Self { x0, y0, x1, y1 })
    }
}

/// Whether the KMS overlay plane already holds the current frame on screen with no pending hide/flip.
pub fn overlay_keeps_frame(engaged: bool, shown: bool, change: Option<bool>) -> bool {
    engaged && shown && change.is_none()
}

/// How an atomic commit updates the overlay plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayCommit {
    /// Overlay plane state and `FB_ID` stay as they are on screen.
    None,
    /// Overlay plane is already active on the CRTC at full size; only swap its `FB_ID`.
    FlipFb,
    /// Overlay plane is being enabled (`show: true`) or disabled (`show: false`), setting CRTC/rect.
    FullPlane { show: bool },
}

pub fn overlay_commit_mode(already_shown: bool, change: Option<bool>) -> OverlayCommit {
    match change {
        None => OverlayCommit::None,
        Some(true) if already_shown => OverlayCommit::FlipFb,
        Some(show) => OverlayCommit::FullPlane { show },
    }
}

/// ARGB8888 source canvas and write-combined destination dumb buffer.
pub struct OverlayBuffers {
    pub src: *const u8,
    pub src_pitch: usize,
    pub dst: *mut u8,
    pub dst_pitch: usize,
    pub width: u32,
    pub height: u32,
}

fn clamp_rect(rect: DirtyRect, width: u32, height: u32) -> Option<DirtyRect> {
    let r = DirtyRect {
        x0: rect.x0.min(width),
        y0: rect.y0.min(height),
        x1: rect.x1.min(width),
        y1: rect.y1.min(height),
    };
    (r.x0 < r.x1 && r.y0 < r.y1).then_some(r)
}

/// Zeroes `rect` (clamped to `width` x `height`) in an ARGB8888 buffer of stride `dst_pitch`.
///
/// # Safety
/// `dst` must be valid for `height` rows of `width * 4` bytes at stride `dst_pitch`.
pub unsafe fn clear_rect_bytes(
    dst: *mut u8,
    dst_pitch: usize,
    width: u32,
    height: u32,
    rect: DirtyRect,
) {
    let Some(r) = clamp_rect(rect, width, height) else { return };
    let start_byte = (r.x0 as usize) * 4;
    let len_bytes = ((r.x1 - r.x0) as usize) * 4;
    for row in r.y0 as usize..r.y1 as usize {
        // SAFETY: `r` is clamped to `width` x `height` and caller guarantees `dst` covers it.
        unsafe {
            std::ptr::write_bytes(dst.add(row * dst_pitch + start_byte), 0, len_bytes);
        }
    }
}

/// Updates `buffers.dst` so it contains `next_src` from `buffers.src` and zeros any stale pixels
/// left in `buffers.dst` from `prev_dst` outside `next_src`, touching no bytes outside
/// `prev_dst ∪ next_src` and writing each touched byte at most once in ascending address order.
///
/// # Safety
/// `buffers.src` and `buffers.dst` must be valid for `buffers.height` rows of `buffers.width * 4`
/// bytes at `buffers.src_pitch` and `buffers.dst_pitch` respectively, and must not overlap.
pub unsafe fn sync_overlay_rect(
    buffers: OverlayBuffers,
    prev_dst: Option<DirtyRect>,
    next_src: Option<DirtyRect>,
) {
    let OverlayBuffers { src, src_pitch, dst, dst_pitch, width, height } = buffers;
    let old = prev_dst.and_then(|r| clamp_rect(r, width, height));
    let new = next_src.and_then(|r| clamp_rect(r, width, height));
    match (old, new) {
        (None, None) => {}
        (Some(old), None) => {
            // SAFETY: caller guarantees `dst` covers `width` x `height` at `dst_pitch`.
            unsafe { clear_rect_bytes(dst, dst_pitch, width, height, old) };
        }
        (None, Some(new)) => {
            let start = (new.x0 as usize) * 4;
            let bytes = ((new.x1 - new.x0) as usize) * 4;
            for row in new.y0 as usize..new.y1 as usize {
                // SAFETY: `new` is clamped to `width` x `height`; `src` and `dst` do not overlap.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        src.add(row * src_pitch + start),
                        dst.add(row * dst_pitch + start),
                        bytes,
                    );
                }
            }
        }
        (Some(old), Some(new)) => {
            let zero_span = |row: u32, x0: u32, x1: u32| {
                if x0 < x1 {
                    let start = (x0 as usize) * 4;
                    let bytes = ((x1 - x0) as usize) * 4;
                    // SAFETY: `x0..x1 <= width` and `row < height`.
                    unsafe { std::ptr::write_bytes(dst.add(row as usize * dst_pitch + start), 0, bytes) };
                }
            };
            let copy_span = |row: u32, x0: u32, x1: u32| {
                if x0 < x1 {
                    let start = (x0 as usize) * 4;
                    let bytes = ((x1 - x0) as usize) * 4;
                    // SAFETY: `x0..x1 <= width` and `row < height`; `src` and `dst` do not overlap.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            src.add(row as usize * src_pitch + start),
                            dst.add(row as usize * dst_pitch + start),
                            bytes,
                        );
                    }
                }
            };
            for row in old.y0..old.y1.min(new.y0) {
                zero_span(row, old.x0, old.x1);
            }
            for row in new.y0..new.y1 {
                if old.y0 <= row && row < old.y1 {
                    if old.x0 < new.x0 {
                        zero_span(row, old.x0, old.x1.min(new.x0));
                    }
                    copy_span(row, new.x0, new.x1);
                    if old.x1 > new.x1 {
                        zero_span(row, old.x0.max(new.x1), old.x1);
                    }
                } else {
                    copy_span(row, new.x0, new.x1);
                }
            }
            for row in old.y0.max(new.y1)..old.y1 {
                zero_span(row, old.x0, old.x1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_mode_flips_fb_only_when_already_shown_and_uses_full_plane_on_edges() {
        assert_eq!(overlay_commit_mode(false, None), OverlayCommit::None);
        assert_eq!(overlay_commit_mode(true, None), OverlayCommit::None);
        assert_eq!(overlay_commit_mode(false, Some(true)), OverlayCommit::FullPlane { show: true });
        assert_eq!(overlay_commit_mode(true, Some(true)), OverlayCommit::FlipFb);
        assert_eq!(overlay_commit_mode(true, Some(false)), OverlayCommit::FullPlane { show: false });
        assert_eq!(overlay_commit_mode(false, Some(false)), OverlayCommit::FullPlane { show: false });

        assert!(overlay_keeps_frame(true, true, None));
        assert!(!overlay_keeps_frame(false, true, None));
        assert!(!overlay_keeps_frame(true, false, None));
        assert!(!overlay_keeps_frame(true, true, Some(false)));
    }

    #[test]
    fn disjoint_corner_move_across_double_buffers_clears_old_corner_without_touching_middle() {
        let (w, h) = (8u32, 8u32);
        let pitch = (w as usize) * 4;
        let mut canvas = vec![0u8; pitch * h as usize];
        let mut buf_a = vec![0xAAu8; pitch * h as usize];
        let mut buf_b = vec![0xAAu8; pitch * h as usize];

        let top_left = DirtyRect::clamped([0, 0, 2, 2], w, h).unwrap();
        let bottom_right = DirtyRect::clamped([6, 6, 8, 8], w, h).unwrap();

        // Paint top-left in canvas and sync into buf_b (prev = None).
        for y in 0..2usize {
            for x in 0..2usize {
                canvas[y * pitch + x * 4..y * pitch + x * 4 + 4].copy_from_slice(&[1, 2, 3, 255]);
            }
        }
        unsafe {
            sync_overlay_rect(
                OverlayBuffers { src: canvas.as_ptr(), src_pitch: pitch, dst: buf_b.as_mut_ptr(), dst_pitch: pitch, width: w, height: h },
                None,
                Some(top_left),
            );
        }
        assert_eq!(&buf_b[0..4], &[1, 2, 3, 255]);
        assert_eq!(&buf_b[3 * pitch + 3 * 4..3 * pitch + 3 * 4 + 4], &[0xAA; 4], "middle stays untouched");

        // Clear top-left on canvas, paint bottom-right, sync into buf_a (prev = None), then into buf_b (prev = top_left).
        unsafe { clear_rect_bytes(canvas.as_mut_ptr(), pitch, w, h, top_left) };
        for y in 6..8usize {
            for x in 6..8usize {
                canvas[y * pitch + x * 4..y * pitch + x * 4 + 4].copy_from_slice(&[9, 8, 7, 255]);
            }
        }
        unsafe {
            sync_overlay_rect(
                OverlayBuffers { src: canvas.as_ptr(), src_pitch: pitch, dst: buf_a.as_mut_ptr(), dst_pitch: pitch, width: w, height: h },
                None,
                Some(bottom_right),
            );
            sync_overlay_rect(
                OverlayBuffers { src: canvas.as_ptr(), src_pitch: pitch, dst: buf_b.as_mut_ptr(), dst_pitch: pitch, width: w, height: h },
                Some(top_left),
                Some(bottom_right),
            );
        }
        // Top-left in buf_b was zeroed; bottom-right has the new pixels; middle row 3 still has 0xAA!
        for y in 0..2usize {
            for x in 0..2usize {
                assert_eq!(&buf_b[y * pitch + x * 4..y * pitch + x * 4 + 4], &[0, 0, 0, 0]);
            }
        }
        for y in 6..8usize {
            for x in 6..8usize {
                assert_eq!(&buf_b[y * pitch + x * 4..y * pitch + x * 4 + 4], &[9, 8, 7, 255]);
            }
        }
        assert_eq!(&buf_b[3 * pitch..4 * pitch], &vec![0xAAu8; pitch], "rows between disjoint corners are never written");
    }

    #[test]
    fn sync_overlay_rect_matches_full_surface_reference_for_all_relative_positions() {
        let (w, h) = (6u32, 5u32);
        let pitch = (w as usize) * 4;
        let rects = [
            None,
            DirtyRect::clamped([0, 0, 2, 2], w, h),
            DirtyRect::clamped([1, 1, 4, 3], w, h),
            DirtyRect::clamped([2, 0, 6, 5], w, h),
            DirtyRect::clamped([4, 3, 6, 5], w, h),
            DirtyRect::clamped([0, 2, 6, 4], w, h),
            DirtyRect::clamped([2, 1, 3, 4], w, h),
        ];
        for &old in &rects {
            for &new in &rects {
                let mut dst = vec![0u8; pitch * h as usize];
                if let Some(r) = old {
                    for y in r.y0 as usize..r.y1 as usize {
                        for x in r.x0 as usize..r.x1 as usize {
                            dst[y * pitch + x * 4..y * pitch + x * 4 + 4].copy_from_slice(&[0xCC; 4]);
                        }
                    }
                }
                let mut src = vec![0u8; pitch * h as usize];
                if let Some(r) = new {
                    for y in r.y0 as usize..r.y1 as usize {
                        for x in r.x0 as usize..r.x1 as usize {
                            let px = [(x as u8) + 1, (y as u8) + 10, 0x55, 0xFF];
                            src[y * pitch + x * 4..y * pitch + x * 4 + 4].copy_from_slice(&px);
                        }
                    }
                }
                unsafe {
                    sync_overlay_rect(
                        OverlayBuffers { src: src.as_ptr(), src_pitch: pitch, dst: dst.as_mut_ptr(), dst_pitch: pitch, width: w, height: h },
                        old,
                        new,
                    );
                }
                assert_eq!(dst, src, "mismatch for old={old:?}, new={new:?}");
            }
        }
    }
}
