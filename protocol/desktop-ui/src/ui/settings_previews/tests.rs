//! The preview pictures' rules, without a window: where each value's numbers rest, that no two
//! values of a setting draw the same picture, and that high contrast draws lines only.

use super::*;

fn ink(high: bool) -> Ink {
    let amber = Color32::from_rgb(245, 176, 65);
    Ink {
        get: amber,
        wash: if high { Color32::TRANSPARENT } else { amber.gamma_multiply(0.2) },
        reference: Color32::from_rgb(160, 163, 177),
        quiet: Color32::from_rgb(63, 66, 80),
        high,
    }
}

fn room() -> Rect {
    Rect::from_min_size(pos2(20.0, 20.0), vec2(288.0, 100.0))
}

/// When each frame is shown, and whether it is.
fn paced(choice: FramePacingChoice) -> ([f32; pacing::FRAMES], [bool; pacing::FRAMES]) {
    let pose = pacing::pose(choice);
    (std::array::from_fn(|frame| pose[frame]), std::array::from_fn(|frame| pose[pacing::FRAMES + frame] > 0.5))
}

/// The times of the frames shown, in order.
fn shown(choice: FramePacingChoice) -> Vec<f32> {
    let (times, kept) = paced(choice);
    times.into_iter().zip(kept).filter(|(_, kept)| *kept).map(|(time, _)| time).collect()
}

fn on_a_refresh(time: f32) -> bool {
    time.fract() == 0.0
}

#[test]
fn every_pacing_shows_a_frame_after_it_arrives_and_in_order() {
    for choice in FramePacingChoice::ALL {
        let (times, kept) = paced(choice);
        for frame in 0..pacing::FRAMES {
            assert!(!kept[frame] || times[frame] >= pacing::ARRIVES[frame], "{choice:?} shows frame {frame} before it arrives");
        }
        assert!(shown(choice).windows(2).all(|pair| pair[1] > pair[0]), "{choice:?}: {:?}", shown(choice));
    }
}

#[test]
fn the_smooth_pacings_show_frames_evenly_and_lowest_latency_as_they_arrive() {
    for choice in [FramePacingChoice::Smoothness, FramePacingChoice::HostTimes] {
        let times = shown(choice);
        assert_eq!(times.len(), pacing::FRAMES, "{choice:?} shows every frame");
        assert!(times.windows(2).all(|pair| (pair[1] - pair[0] - 2.0).abs() < 1e-4), "{choice:?} is uneven: {times:?}");
    }
    let (times, kept) = paced(FramePacingChoice::Latency);
    assert!(kept.iter().all(|kept| *kept));
    assert!(times.iter().zip(pacing::ARRIVES).all(|(shown, arrives)| shown - arrives < 0.2), "{times:?}");
    assert!(!times.iter().all(|time| on_a_refresh(*time)), "it does not wait for the screen");
    let gaps: Vec<f32> = times.windows(2).map(|pair| pair[1] - pair[0]).collect();
    assert!(gaps.iter().cloned().fold(0.0, f32::max) > 2.0 * gaps.iter().cloned().fold(f32::MAX, f32::min), "and is as uneven as the network: {gaps:?}");
}

#[test]
fn balanced_waits_for_a_refresh_and_only_the_newest_frame_takes_it() {
    let (times, kept) = paced(FramePacingChoice::Balanced);
    assert!(times.iter().all(|time| on_a_refresh(*time)), "{times:?}");
    assert_eq!(kept.iter().filter(|kept| !**kept).count(), 1, "the late frame is passed over for the one behind it: {kept:?}");
    let (times, kept) = paced(FramePacingChoice::BalancedEvery);
    assert!(times.iter().all(|time| on_a_refresh(*time)) && kept.iter().all(|kept| *kept), "every frame, each on a refresh of its own: {times:?}");
    let capped = shown(FramePacingChoice::CapFps);
    assert_eq!(capped.len(), pacing::FRAMES);
    assert!(capped.windows(2).all(|pair| pair[1] - pair[0] >= 2.0), "the cap keeps a frame on screen for its whole turn: {capped:?}");
    assert!(capped.windows(2).any(|pair| pair[1] - pair[0] > 2.0), "and does not make up for a late one: {capped:?}");
}

#[test]
fn no_two_values_of_a_setting_draw_the_same_picture() {
    let differ = |poses: Vec<Pose>, what: &str| {
        for (at, pose) in poses.iter().enumerate() {
            assert!(!poses[..at].contains(pose), "{what}: value {at} draws what an earlier one does");
        }
    };
    differ(FramePacingChoice::ALL.map(|choice| Picture::Pacing(choice).pose()).to_vec(), "Frame pacing");
    differ([false, true].map(|hdr| Picture::Tone { hdr }.pose()).to_vec(), "HDR");
    differ([VideoScaling::Fit, VideoScaling::Fill, VideoScaling::Stretch, VideoScaling::Native].map(|scaling| Picture::Screen { scaling, top: false }.pose()).to_vec(), "Video size");
    differ(
        [(Upscaling::Bilinear, 0.5), (Upscaling::Fsr1, 0.0), (Upscaling::Fsr1, 0.5), (Upscaling::Fsr1, 1.0)].map(|(upscaling, sharpness)| Picture::Edge { upscaling, sharpness }.pose()).to_vec(),
        "Upscaling",
    );
}

/// A number the picture reads has a spring; the rest of the pose is never set.
#[test]
fn every_number_a_picture_reads_has_a_spring() {
    let pictures = [
        Picture::Pacing(FramePacingChoice::Balanced),
        Picture::Tone { hdr: true },
        Picture::Edge { upscaling: Upscaling::Fsr1, sharpness: 1.0 },
        Picture::Screen { scaling: VideoScaling::Fit, top: true },
    ];
    for picture in pictures {
        assert!(picture.pose()[picture.numbers()..].iter().all(|number| *number == 0.0), "{picture:?}");
        assert!(picture.pose()[..picture.numbers()].iter().any(|number| *number != 0.0), "{picture:?}");
    }
}

#[test]
fn hdr_keeps_the_highlights_that_sdr_cuts_off() {
    let (off, on) = (tone::pose(false)[0], tone::pose(true)[0]);
    assert!((tone::shown(1.0, off) - tone::CLIP).abs() < 1e-4, "without HDR the brightest is no brighter than where it is cut off");
    assert!((tone::shown(1.0, on) - 1.0).abs() < 1e-4, "with HDR the ramp is whole");
    for step in 0..=20 {
        let x = step as f32 / 20.0;
        assert!((tone::shown(x, on) - x).abs() < 1e-4, "with HDR nothing bends at {x}");
        assert!(tone::shown(x, off) <= x + 1e-4 && tone::shown(x, off) <= tone::CLIP + 1e-4);
        assert!(step == 0 || tone::shown(x, off) >= tone::shown(x - 0.05, off) - 1e-4, "the curve never falls");
    }
    assert!((tone::shown(0.3, off) - 0.3).abs() < 1e-4, "the dark half is the same either way");
}

#[test]
fn the_edge_sharpens_with_the_upscaler_and_its_sharpness() {
    let edge = |upscaling, sharpness| edge::pose(upscaling, sharpness);
    assert_eq!(edge(Upscaling::Bilinear, 0.0)[0], edge::SOFT);
    assert_eq!(edge(Upscaling::Bilinear, 1.0), edge(Upscaling::Bilinear, 0.0), "the plain enlargement has no sharpness to set");
    for upscaling in [Upscaling::Fsr1, Upscaling::Sgsr1] {
        let widths: Vec<f32> = [0.0, 0.25, 0.5, 0.75, 1.0].map(|sharpness| edge(upscaling, sharpness)[0]).to_vec();
        assert!(widths[0] < edge::SOFT, "{upscaling:?} is sharper than the plain enlargement at any sharpness");
        assert!(widths.windows(2).all(|pair| pair[1] < pair[0]), "{upscaling:?}: {widths:?}");
        assert!((widths[4] - edge::CRISP).abs() < 1e-4);
        assert_eq!(edge(upscaling, 0.5)[1], 0.0, "no halo at a moderate sharpness");
        assert!(edge(upscaling, 1.0)[1] > 0.9, "a halo at the most");
    }
}

#[test]
fn the_video_keeps_its_shape_except_stretched() {
    let pose = |scaling, top| screen::pose(scaling, top);
    let [w, h, up, rx, ry] = [0, 1, 2, 3, 4].map(|at| pose(VideoScaling::Fit, false)[at]);
    assert!(w == 1.0 && h < 1.0 && up == 0.0 && rx == ry, "Fit: as wide as the screen with bars above and below");
    let [w, h, _, rx, ry] = [0, 1, 2, 3, 4].map(|at| pose(VideoScaling::Fill, false)[at]);
    assert!(w > 1.0 && h == 1.0 && rx == ry, "Fill: the screen's height, its sides cut off");
    let [w, h, _, rx, ry] = [0, 1, 2, 3, 4].map(|at| pose(VideoScaling::Stretch, false)[at]);
    assert!(w == 1.0 && h == 1.0 && rx < ry, "Stretch: the whole screen, the circle squeezed");
    let [w, h, _, rx, ry] = [0, 1, 2, 3, 4].map(|at| pose(VideoScaling::Native, false)[at]);
    assert!(w < 1.0 && h < 1.0 && rx == ry, "1:1 pixels: its own size");
    // Video position: Top moves a video with room above it to the screen's top edge.
    let top = pose(VideoScaling::Fit, true);
    assert!((top[2] - top[1] / 2.0 + 0.5).abs() < 1e-4, "{top:?}");
    assert_eq!(pose(VideoScaling::Fill, true)[2], 0.0, "a video as tall as the screen has nowhere to go");
}

fn each(shapes: &[Shape], visit: &mut dyn FnMut(&Shape)) {
    for shape in shapes {
        match shape {
            Shape::Vec(inner) => each(inner, visit),
            other => visit(other),
        }
    }
}

fn pictures() -> Vec<Picture> {
    let mut all: Vec<Picture> = FramePacingChoice::ALL.map(Picture::Pacing).to_vec();
    all.extend([false, true].map(|hdr| Picture::Tone { hdr }));
    all.extend([(Upscaling::Bilinear, 0.5), (Upscaling::Fsr1, 0.2), (Upscaling::Sgsr1, 1.0)].map(|(upscaling, sharpness)| Picture::Edge { upscaling, sharpness }));
    all.extend([VideoScaling::Fit, VideoScaling::Fill, VideoScaling::Stretch, VideoScaling::Native].map(|scaling| Picture::Screen { scaling, top: false }));
    all
}

/// High contrast: no gradient and nothing translucent, a fill is a solid dot or it is not there.
#[test]
fn high_contrast_draws_lines_only() {
    for picture in pictures() {
        let mut shapes = Vec::new();
        picture.paint(&picture.pose(), room(), &ink(true), &mut shapes);
        assert!(!shapes.is_empty(), "{picture:?}");
        each(&shapes, &mut |shape| {
            let fill = match shape {
                Shape::Mesh(_) => panic!("{picture:?} draws a gradient in high contrast"),
                Shape::Rect(rect) => rect.fill,
                Shape::Circle(circle) => circle.fill,
                Shape::Ellipse(ellipse) => ellipse.fill,
                Shape::Path(path) => path.fill,
                _ => Color32::TRANSPARENT,
            };
            assert!(fill == Color32::TRANSPARENT || fill.is_opaque(), "{picture:?} fills with {fill:?} in high contrast");
            if let Shape::LineSegment { stroke, .. } = shape {
                assert!(stroke.color.is_opaque(), "{picture:?} draws a faint line in high contrast: {:?}", stroke.color);
            }
        });
    }
}

/// Every picture draws what the player gets in amber, in every look.
#[test]
fn every_picture_marks_what_the_player_gets_in_amber() {
    for high in [false, true] {
        let ink = ink(high);
        for picture in pictures() {
            let mut shapes = Vec::new();
            picture.paint(&picture.pose(), room(), &ink, &mut shapes);
            let mut amber = false;
            each(&shapes, &mut |shape| {
                amber |= match shape {
                    Shape::Rect(rect) => rect.fill == ink.get || rect.stroke.color == ink.get,
                    Shape::LineSegment { stroke, .. } => stroke.color == ink.get,
                    Shape::Ellipse(ellipse) => ellipse.stroke.color == ink.get,
                    Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(ink.get),
                    Shape::Mesh(mesh) => mesh.vertices.iter().any(|vertex| vertex.color.a() > 0),
                    _ => false,
                };
            });
            assert!(amber, "{picture:?} (high contrast: {high})");
        }
    }
}
