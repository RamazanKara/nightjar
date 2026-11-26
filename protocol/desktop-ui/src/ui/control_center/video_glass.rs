//! What the stream's presenter holds for the Control Center's glass, as a machine with no
//! renderer in it: while the dock is open on `Surface::Blurred` the presenter puts each video
//! frame through its blur chain; in every other frame it holds nothing, and what it held goes on
//! the first frame after the dock closed. A closed Control Center therefore costs a stream no
//! blur pass, no render target and no frame capture, which the tests below count on a fake stage.

use nightjar_policy::control_center::Surface;

/// The presenter's side of the glass.
pub trait VideoStage {
    /// Blurs the video frame on screen into the texture the dock samples, making its render
    /// targets when it has none.
    fn capture(&mut self) -> Result<(), String>;
    /// Frees the render targets.
    fn release(&mut self);
}

/// Whether the presenter holds render targets, and whether its blur failed in this stream.
#[derive(Debug, Default)]
pub struct VideoGlass {
    held: bool,
    failed: bool,
}

impl VideoGlass {
    /// Render targets are held: only ever while the dock is open on blur.
    pub fn holds(&self) -> bool {
        self.held
    }

    /// A capture failed in this stream: the dock keeps its baked fill from then on, and says so
    /// once in the log instead of trying every frame.
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// One presented frame. `surface` is what `nightjar_policy::control_center::surface` gave: None
    /// while the Control Center is closed. Answers what the dock is drawn on in this frame.
    pub fn frame(&mut self, stage: &mut dyn VideoStage, surface: Option<Surface>) -> Option<Surface> {
        if matches!(surface, Some(Surface::Blurred)) && !self.failed {
            match stage.capture() {
                Ok(()) => {
                    self.held = true;
                    return surface;
                }
                Err(why) => {
                    eprintln!("nightjar: control center: no live blur over this stream, its fill is baked: {why}");
                    self.failed = true;
                }
            }
        }
        if self.held {
            stage.release();
            self.held = false;
        }
        surface.map(|surface| if matches!(surface, Surface::Blurred) { Surface::Baked } else { surface })
    }

    /// The stream ended or its renderer goes: whatever is held is freed.
    pub fn end(&mut self, stage: &mut dyn VideoStage) {
        if self.held {
            stage.release();
            self.held = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nightjar_policy::control_center::{surface, Live};
    use nightjar_policy::glass_probe::Tier;

    #[derive(Default)]
    struct Fake {
        captures: u32,
        releases: u32,
        targets: u32,
        fail: bool,
    }

    impl VideoStage for Fake {
        fn capture(&mut self) -> Result<(), String> {
            if self.fail {
                return Err("no render target".into());
            }
            self.captures += 1;
            self.targets = 1;
            Ok(())
        }

        fn release(&mut self) {
            self.releases += 1;
            self.targets = 0;
        }
    }

    fn live() -> Live {
        Live { tier: Tier::Live, blur_available: true, power_save: false, thermal_severe: false, high_contrast: false }
    }

    /// A stream that never opens the Control Center: no blur pass, no render target, no capture.
    #[test]
    fn closed_over_a_stream_it_costs_nothing() {
        let (mut glass, mut stage) = (VideoGlass::default(), Fake::default());
        for _ in 0..600 {
            assert_eq!(glass.frame(&mut stage, surface(false, &live())), None);
        }
        assert_eq!((stage.captures, stage.releases, stage.targets), (0, 0, 0));
        assert!(!glass.holds());
    }

    /// Open on a live tier every frame goes through the chain; the first frame after closing
    /// frees the targets, and the frames after that touch nothing.
    #[test]
    fn open_it_blurs_each_frame_and_frees_on_the_first_frame_after() {
        let (mut glass, mut stage) = (VideoGlass::default(), Fake::default());
        for _ in 0..3 {
            assert_eq!(glass.frame(&mut stage, surface(true, &live())), Some(Surface::Blurred));
        }
        assert_eq!((stage.captures, stage.targets), (3, 1));
        assert!(glass.holds());
        assert_eq!(glass.frame(&mut stage, surface(false, &live())), None);
        assert_eq!((stage.releases, stage.targets), (1, 0), "freed on the first frame after close");
        for _ in 0..10 {
            glass.frame(&mut stage, surface(false, &live()));
        }
        assert_eq!((stage.captures, stage.releases), (3, 1), "and nothing after it");
    }

    /// Baked and solid surfaces never reach the presenter.
    #[test]
    fn a_baked_or_solid_dock_takes_no_frame() {
        let (mut glass, mut stage) = (VideoGlass::default(), Fake::default());
        let baked = Live { blur_available: false, ..live() };
        let solid = Live { high_contrast: true, ..live() };
        let saving = Live { power_save: true, ..live() };
        assert_eq!(glass.frame(&mut stage, surface(true, &baked)), Some(Surface::Baked));
        assert_eq!(glass.frame(&mut stage, surface(true, &solid)), Some(Surface::Solid));
        assert_eq!(glass.frame(&mut stage, surface(true, &saving)), Some(Surface::Baked));
        assert_eq!((stage.captures, stage.releases), (0, 0));
        // Blur going away while open (the picture turned HDR) frees what was held.
        glass.frame(&mut stage, surface(true, &live()));
        assert_eq!(glass.frame(&mut stage, surface(true, &baked)), Some(Surface::Baked));
        assert_eq!((stage.releases, stage.targets), (1, 0));
    }

    /// A chain that fails is not tried again every frame: the dock shows on its baked fill.
    #[test]
    fn a_failed_capture_falls_back_visibly_and_once() {
        let (mut glass, mut stage) = (VideoGlass::default(), Fake { fail: true, ..Default::default() });
        assert_eq!(glass.frame(&mut stage, surface(true, &live())), Some(Surface::Baked));
        assert!(glass.failed());
        stage.fail = false;
        assert_eq!(glass.frame(&mut stage, surface(true, &live())), Some(Surface::Baked));
        assert_eq!(stage.captures, 0, "not tried again in this stream");
    }

    #[test]
    fn the_end_of_the_stream_frees_what_is_held() {
        let (mut glass, mut stage) = (VideoGlass::default(), Fake::default());
        glass.frame(&mut stage, surface(true, &live()));
        glass.end(&mut stage);
        glass.end(&mut stage);
        assert_eq!((stage.releases, stage.targets), (1, 0));
    }
}
