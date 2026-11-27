//! Unit tests for `FramePool` AVFrame recycling and mid-stream `VideoDecoder::flush_buffers`.

use std::sync::Arc;

use super::ffi::{av_frame_free, av_frame_get_buffer, AV_PIX_FMT_NV12};
use super::frame::{FramePool, POOLED_FRAMES};
use super::{Codec, Frame, Hardware, Received, VideoDecoder};

#[test]
fn a_dropped_frame_comes_back_empty_from_the_pool() {
    let pool = Arc::new(FramePool::new());
    let raw = pool.take().expect("a frame");
    {
        // The pool gets the frame back when the picture is done with, and keeps it unreferenced.
        let frame = Frame::pooled(&pool).expect("pooled frame");
        assert_ne!(frame.raw, raw);
        unsafe {
            (*frame.raw.as_ptr()).format = AV_PIX_FMT_NV12;
            (*frame.raw.as_ptr()).width = 64;
            (*frame.raw.as_ptr()).height = 32;
            assert!(av_frame_get_buffer(frame.raw.as_ptr(), 32) >= 0);
        }
    }
    assert_eq!(pool.idle(), 1, "the frame itself stays for reuse");
    let back = pool.take().expect("the returned frame");
    assert!(
        unsafe { (*back.as_ptr()).buf[0].is_null() && (*back.as_ptr()).data[0].is_null() },
        "no surface or buffer is held"
    );
    unsafe {
        av_frame_free(&mut raw.as_ptr());
        av_frame_free(&mut back.as_ptr());
    }
}

#[test]
fn the_pool_is_bounded_and_reuses_what_it_keeps() {
    let pool = FramePool::new();
    let mut frames = Vec::new();
    for _ in 0..=POOLED_FRAMES {
        frames.push(pool.take().expect("a frame"));
    }
    for raw in frames {
        pool.give(raw);
    }
    assert_eq!(pool.idle(), POOLED_FRAMES, "excess frames past the bound are freed");
    let reused = pool.take().expect("a kept frame");
    assert_eq!(pool.idle(), POOLED_FRAMES - 1);
    pool.give(reused);
}

#[test]
fn pictures_give_their_frame_structure_back_to_the_decoder() {
    let bytes = include_bytes!("../../../../app/src/main/assets/packed444/hevc-8.bin").as_slice();
    let mut decoder = VideoDecoder::open(Codec::Hevc, Hardware::None, 1, false).unwrap();
    for number in 0..20 {
        assert_eq!(decoder.send(bytes, i64::from(number)), 0, "picture {number}");
        let Received::Frame(picture) = decoder.receive() else {
            panic!("no decoded frame {number}");
        };
        // The presenter's held reference of the picture uses a structure of the same pool, and
        // its drop does not unref the picture's own buffers.
        let copy = picture.reference().expect("a second reference to the picture");
        assert_eq!(copy.info().pts, picture.info().pts);
        assert!(!picture.info().data[0].is_null());
        drop(copy);
        // The presenter lets the picture go after showing it: its structure returns to the pool.
        drop(picture);
        // The poll after a picture is what takes it back for the next one.
        assert!(matches!(decoder.receive(), Received::None));
    }
    let (made, reused) = decoder.frame_allocations();
    assert!(made <= 2, "{made} frame structures made for 20 pictures");
    assert!(reused >= 18, "{reused} of 20 pictures reused a structure");
}

#[test]
fn flush_buffers_decodes_the_next_key_frame_and_keeps_refused_state() {
    let fixture = include_bytes!("../../../../app/src/main/assets/packed444/hevc-8.bin").as_slice();
    let mut decoder = VideoDecoder::open(Codec::Hevc, Hardware::None, 1, false).unwrap();
    assert_eq!(decoder.send(fixture, 1), 0);
    assert!(matches!(decoder.receive(), Received::Frame(_)), "a first picture");
    decoder.refuse();
    decoder.flush_buffers();
    assert!(
        decoder.refused(),
        "mid-stream buffer flush must not clear hardware refusal state"
    );
    assert_eq!(decoder.send(fixture, 2), 0);
    let Received::Frame(picture) = decoder.receive() else {
        panic!("the key frame decodes after flush_buffers");
    };
    assert_eq!(picture.info().pts, 2);
}
