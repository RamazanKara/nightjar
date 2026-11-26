//! Unit tests for video texture colour/HDR paths, VRR pacing, skipped pictures, and `Pacer`.

use super::*;

mod pacer;
mod skipped;
mod texture;
mod vrr;

fn frame(trc: i32, space: i32, range: i32) -> FrameInfo {
    FrameInfo {
        width: 1920,
        height: 1080,
        kind: media::FRAME_D3D11,
        ten_bit: 1,
        pts: 0,
        data: [std::ptr::null_mut(); 3],
        linesize: [0; 3],
        color_space: space,
        color_range: range,
        color_trc: trc,
        centred_chroma: false,
    }
}

/// A video texture of `texture` size holding a decoded picture of that size from a `stream`
/// of that size, cropped as `pump` and `upload` crop it. The texture pointer is never
/// dereferenced.
fn holding(texture: (i32, i32), stream: (i32, i32)) -> VideoTexture {
    let mut video = VideoTexture::new(true, false);
    video.texture = std::ptr::NonNull::dangling().as_ptr();
    (video.width, video.height) = texture;
    video.format = SDL_PIXELFORMAT_NV12;
    video.visible = visible_part(nightjar_core::video::crop::visible_size(texture, &[stream]), texture);
    video
}

pub(crate) fn decoded(ready_us: u64) -> Decoded {
    let mut decoder =
        media::VideoDecoder::open(media::Codec::Hevc, media::Hardware::None, 1, false).unwrap();
    assert_eq!(
        decoder.send(
            include_bytes!("../../../../../app/src/main/assets/packed444/hevc-8.bin"),
            0
        ),
        0
    );
    let media::Received::Frame(frame) = decoder.receive() else {
        panic!("fixture did not decode");
    };
    Decoded {
        picture: media::Picture::Frame(frame),
        setup: nightjar_core::session::VideoSetup {
            format: nightjar_core::sdp::VIDEO_FORMAT_H265,
            width: 128,
            height: 128,
            fps: 60,
            chroma_packing: 1,
            logical_width: 128,
            logical_height: 64,
            color_space: 1,
            color_range: 1,
        },
        rtp: ready_us as u32,
        number: 0,
        receive_us: 0,
        enqueue_us: 0,
        input_us: 0,
        ready_us,
        host_latency: 0,
        early: 0,
        first_block_us: 0,
        race: Default::default(),
        bands: Vec::new(),
        race_seq: 0,
        visible: None,
    }
}
