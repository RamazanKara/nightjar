//! The stream's HUD: the statistics capsule, toasts and the centred prompts of a stream that
//! shows no picture (microphone only, Remote Input). They draw on `SURFACE_HUD_*`: a dark fill
//! and a hairline rim with no blur and no shadow, so they cost the stream nothing but their
//! shapes. Night-locked like the Control Center: the same inks in the night, day and OLED themes,
//! since video is under them; high contrast takes solid black and a 2 px rim.
//!
//! The capsule and the toast take no pointer and sit under the Control Center; the app decides
//! when each shows and what it says.

use egui::{Align2, Color32, Context, Id, Margin, Order, Rect, RichText, Shape, Stroke, StrokeKind, Ui, Vec2};

use super::design_tokens as tokens;
use crate::strings::tr;
use crate::theme::{regular, semibold, text};

/// The round of the statistics capsule and of a prompt.
pub const CAPSULE_RADIUS: f32 = 12.0;
/// The round of a toast.
pub const TOAST_RADIUS: f32 = 18.0;

/// The HUD's label ink and the quieter one of keys and notes. High contrast takes white for both.
pub fn inks() -> (Color32, Color32) {
    if crate::theme::high_contrast() {
        (Color32::WHITE, Color32::WHITE)
    } else {
        (tokens::SURFACE_HUD_LABEL, tokens::SURFACE_CONTROL_CENTER_LABEL_SECONDARY)
    }
}

/// The HUD surface at `rect`: the fill and its rim, nothing sampled and nothing cast.
pub fn surface(rect: Rect, radius: f32) -> Shape {
    let (fill, rim, width) = if crate::theme::high_contrast() {
        (tokens::GLASS_MENU_SOLID_FILL, tokens::GLASS_MENU_SOLID_RIM_TOP, tokens::GLASS_MENU_SOLID_RIM_WIDTH)
    } else {
        (tokens::SURFACE_HUD_FILL, tokens::SURFACE_HUD_RIM, 1.0)
    };
    Shape::Vec(vec![
        Shape::rect_filled(rect, radius, fill),
        Shape::rect_stroke(rect, radius, Stroke::new(width, rim), StrokeKind::Inside),
    ])
}

/// What `add` lays out on the HUD surface, painted under it once its size is known.
pub fn capsule<R>(ui: &mut Ui, margin: Margin, radius: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    let place = ui.painter().add(Shape::Noop);
    let frame = egui::Frame::new().inner_margin(margin).show(ui, add);
    ui.painter().set(place, surface(frame.response.rect, radius));
    frame.inner
}

/// What the statistics capsule shows.
#[derive(Clone, Copy, Debug)]
pub enum Stats<'a> {
    /// One line: the health mark in its colour, the line, and a notice under it.
    Compact { mark: &'a str, health: [u8; 3], text: &'a str, notice: Option<&'a str> },
    /// The stream's name over a grid of what is measured.
    Lines { title: &'a str, lines: &'a [(String, String)] },
}

/// The statistics capsule in its corner. It takes no pointer and sits under the Control Center.
pub fn stats(ctx: &Context, anchor: Align2, offset: Vec2, stats: Stats<'_>) -> Rect {
    let (ink, quiet) = inks();
    egui::Area::new(Id::unique("overlay"))
        .order(Order::Background)
        .interactable(false)
        .fade_in(false)
        .anchor(anchor, offset)
        .show(ctx, |ui| {
            capsule(ui, Margin::symmetric(14, 10), CAPSULE_RADIUS, |ui| match stats {
                Stats::Compact { mark, health: [r, g, b], text: line, notice } => {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(mark).font(semibold(text::LABEL)).color(Color32::from_rgb(r, g, b)));
                        let line = if line.is_empty() { tr("Waiting for video") } else { line };
                        ui.label(RichText::new(line).font(regular(text::LABEL)).color(ink));
                    });
                    if let Some(notice) = notice {
                        ui.label(RichText::new(notice).font(regular(text::CAPTION)).color(quiet));
                    }
                }
                Stats::Lines { title, lines } => {
                    ui.label(RichText::new(title).font(semibold(text::LABEL)).color(*crate::theme::MOON));
                    ui.add_space(2.0);
                    ui.spacing_mut().interact_size.y = 16.0;
                    egui::Grid::new("stats").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
                        if lines.is_empty() {
                            ui.label(RichText::new(tr("Measuring…")).color(quiet));
                            ui.end_row();
                        }
                        for (key, value) in lines {
                            ui.label(RichText::new(key).font(regular(text::CAPTION)).color(quiet));
                            ui.label(RichText::new(value).font(regular(text::CAPTION)).color(ink));
                            ui.end_row();
                        }
                    });
                }
            });
        })
        .response
        .rect
}

/// A toast over the lower middle of the picture, above everything else.
pub fn toast(ctx: &Context, text_: &str) -> Rect {
    egui::Area::new(Id::unique("toast"))
        .order(Order::Foreground)
        .interactable(false)
        .fade_in(false)
        .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -40.0))
        .show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 64.0).clamp(160.0, 720.0));
            capsule(ui, Margin::symmetric(18, 10), TOAST_RADIUS, |ui| {
                ui.label(RichText::new(text_).font(regular(text::LABEL)).color(inks().0));
            });
        })
        .response
        .rect
}

/// A prompt in the middle of a stream without a picture: `add` lays out its lines and buttons.
pub fn prompt<R>(ctx: &Context, id: &'static str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Area::new(Id::unique(id))
        .fade_in(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_max_width((ctx.content_rect().width() - 64.0).clamp(160.0, 560.0));
            capsule(ui, Margin::symmetric(24, 20), CAPSULE_RADIUS, add)
        })
        .inner
}

/// A prompt's heading, in amber.
pub fn heading(ui: &mut Ui, text_: &str) {
    ui.label(RichText::new(text_).font(semibold(text::TITLE)).color(*crate::theme::MOON));
}

/// A line of a prompt.
pub fn line(ui: &mut Ui, text_: &str) {
    ui.label(RichText::new(text_).font(regular(text::BODY)).color(inks().0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{self, Look};

    /// The stream's HUD over a white frame, as the app draws it: the capsule, a toast and a prompt.
    fn frame(look: Look, compact: bool) -> (Vec<(String, Color32)>, crate::shot::Canvas, [Rect; 3]) {
        theme::own_look(look);
        let ctx = Context::default();
        theme::apply(&ctx);
        theme::set_over_video(&ctx, true);
        let lines = vec![("Video".to_owned(), "2560x1440 · 120 FPS · HEVC".to_owned()), ("Video encoder".to_owned(), "62%".to_owned())];
        let mut rects = [Rect::NOTHING; 3];
        let (mut shapes, mut textures) = (Vec::new(), egui::TexturesDelta::default());
        for _ in 0..3 {
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0))), ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                ui.painter().rect_filled(ctx.content_rect(), 0, Color32::WHITE);
                let what = if compact {
                    Stats::Compact { mark: "●", health: [0x69, 0xdb, 0x7c], text: "120 FPS · 4 ms", notice: Some("The PC lowered the bitrate") }
                } else {
                    Stats::Lines { title: "Open World · Living Room PC", lines: &lines }
                };
                rects[0] = stats(&ctx, Align2::LEFT_TOP, egui::vec2(16.0, 16.0), what);
                rects[1] = toast(&ctx, "The game started at 1920×1080.");
                prompt(&ctx, "prompt-test", |ui| {
                    heading(ui, "Remote Input connected");
                    line(ui, "Keyboard, mouse and controllers control the PC.");
                    rects[2] = ui.min_rect();
                });
            });
            textures.append(output.textures_delta);
            shapes = output.shapes;
        }
        let mut inks = Vec::new();
        fn walk(shape: &Shape, out: &mut Vec<(String, Color32)>) {
            match shape {
                Shape::Vec(list) => list.iter().for_each(|shape| walk(shape, out)),
                Shape::Text(text) => {
                    let ink = text.override_text_color.or_else(|| text.galley.job.sections.first().map(|section| section.format.color)).unwrap_or(text.fallback_color);
                    out.push((text.galley.text().to_owned(), ink));
                }
                _ => {}
            }
        }
        shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut inks));
        let primitives = ctx.tessellate(shapes, 1.0);
        let mut canvas = crate::shot::Canvas::new(1280, 800);
        canvas.update_textures(&textures);
        canvas.paint(&primitives, 1.0);
        textures.clear();
        theme::own_palette();
        (inks, canvas, rects)
    }

    /// The pixel just inside a surface's top edge, at its middle: its fill over the white frame.
    fn fill(canvas: &crate::shot::Canvas, rect: Rect) -> [f32; 4] {
        canvas.pixel(rect.center().x as usize, rect.top() as usize + 4)
    }

    fn luminance(ink: Color32) -> f32 {
        (0.2126 * f32::from(ink.r()) + 0.7152 * f32::from(ink.g()) + 0.0722 * f32::from(ink.b())) / 255.0
    }

    /// The capsule, the toast and the prompt: dark with light text over a white frame in the
    /// night, day and OLED themes alike, never a light panel or dark text.
    #[test]
    fn every_hud_surface_is_night_in_every_theme() {
        for compact in [false, true] {
            let (night, night_canvas, rects) = frame(Look::Night, compact);
            assert!(night.len() >= 5, "the capsule, the toast and the prompt all drew: {night:?}");
            for (text, ink) in &night {
                assert!(luminance(*ink) > 0.45, "‘{text}’ is light on the dark HUD: {ink:?}");
            }
            for (name, rect) in ["capsule", "toast", "prompt"].into_iter().zip(rects) {
                let seen = fill(&night_canvas, rect);
                assert!(seen[..3].iter().all(|c| *c < 0.3), "the {name} is dark over a white frame: {seen:?}");
            }
            for look in [Look::Day, Look::Oled] {
                let (inks, canvas, same) = frame(look, compact);
                assert_eq!(inks, night, "{look:?}: the HUD keeps its night inks");
                for (name, rect) in ["capsule", "toast", "prompt"].into_iter().zip(same) {
                    let (a, b) = (fill(&canvas, rect), fill(&night_canvas, rect));
                    assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 0.004), "{look:?}: the {name} is night's: {a:?} / {b:?}");
                }
            }
        }
    }

    /// High contrast: solid black, white text and a 2 px rim on each HUD surface.
    #[test]
    fn every_hud_surface_is_solid_black_with_a_rim_in_high_contrast() {
        let (inks, canvas, rects) = frame(Look::HighContrast, false);
        assert!(inks.iter().filter(|(text, _)| text != "Open World · Living Room PC" && text != "Remote Input connected").all(|(_, ink)| *ink == Color32::WHITE), "{inks:?}");
        for (name, rect) in ["capsule", "toast", "prompt"].into_iter().zip(rects) {
            let inside = fill(&canvas, rect);
            assert!(inside[..3].iter().all(|c| *c < 0.004), "the {name} is solid black: {inside:?}");
            let rim = canvas.pixel(rect.center().x as usize, rect.top() as usize + 1);
            assert!(rim[..3].iter().all(|c| *c > 0.5), "the {name} has its rim: {rim:?}");
        }
    }

    /// The HUD is shapes only: nothing samples the picture and nothing is cast around it.
    #[test]
    fn the_hud_has_no_blur_and_no_shadow() {
        assert_eq!((tokens::SURFACE_HUD_BLUR, tokens::SURFACE_HUD_SHADOW_BLUR), (0.0, 0.0));
        let Shape::Vec(shapes) = surface(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 40.0)), CAPSULE_RADIUS) else { panic!("a fill and a rim") };
        assert_eq!(shapes.len(), 2);
        assert!(shapes.iter().all(|shape| matches!(shape, Shape::Rect(rect) if rect.blur_width == 0.0 && rect.brush.is_none())));
    }
}
