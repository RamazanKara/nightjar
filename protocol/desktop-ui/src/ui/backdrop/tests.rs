use super::*;
use crate::ui::design_tokens as t;

fn frame(ctx: &Context, ambient: &mut Ambient, want: Want, time: f64) -> Vec<Shape> {
    let mut want = Some(want);
    let mut shapes = Vec::new();
    let output = ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            ..Default::default()
        },
        |ui| shapes = ambient.paint(ui.ctx(), want.take().expect("one pass")),
    );
    let mut delta = output.textures_delta;
    delta.clear();
    shapes
}

fn texture(ctx: &Context, value: u8) -> TextureHandle {
    ctx.load_texture(
        "t",
        egui::ColorImage::filled([2, 2], Color32::from_gray(value)),
        TextureOptions::LINEAR,
    )
}

fn art(key: &str, texture: &TextureHandle) -> Want {
    Want::Art {
        key: key.to_owned(),
        texture: texture.clone(),
        glow: Color32::from_rgba_premultiplied(60, 20, 20, 60),
    }
}

/// The stage's colour at a place in the window, in fractions of it, over a few texels so the bake's
/// noise does not count.
fn stage_at(baked: &egui::ColorImage, [x, y]: [f32; 2]) -> [f32; 3] {
    let span = 1.0 + 2.0 * OVERSCAN;
    let [w, h] = baked.size;
    let column = (((x + OVERSCAN) / span * w as f32) as usize).min(w - 9);
    let row = (((y + OVERSCAN) / span * h as f32) as usize).min(h - 1);
    let mut sum = [0.0f32; 3];
    for pixel in &baked.pixels[row * w + column..row * w + column + 8] {
        for (k, v) in [pixel.r(), pixel.g(), pixel.b()].into_iter().enumerate() {
            sum[k] += f32::from(v) / 8.0;
        }
    }
    sum
}

fn rgb(c: Color32) -> [f32; 3] {
    [c.r(), c.g(), c.b()].map(f32::from)
}

fn near(a: [f32; 3], b: [f32; 3], by: f32) -> bool {
    (0..3).all(|k| (a[k] - b[k]).abs() <= by)
}

/// A page without art stands on the stage in every look: the canvas, the stage's one layer past
/// the window's edges, and the page colour rising over it. Nothing else: no disc, no ring, no
/// speck.
#[test]
fn a_page_without_art_stands_on_the_stage_in_every_look() {
    for look in [Look::Night, Look::Day, Look::Oled] {
        crate::theme::own_look(look);
        let ctx = Context::default();
        let mut ambient = Ambient::default();
        frame(&ctx, &mut ambient, Want::Stage, 0.0);
        let shapes = frame(&ctx, &mut ambient, Want::Stage, 5.0);
        let screen = ctx.content_rect();
        assert_eq!(ambient.shows, Some(Shows::Stage), "{look:?}");
        assert_eq!(shapes.len(), 3, "{look:?}: the canvas, the stage, the page colour: {shapes:?}");
        let Shape::Rect(canvas) = &shapes[0] else {
            panic!("{look:?}: the canvas")
        };
        assert_eq!((canvas.rect, canvas.fill), (screen, shade::COLOR_CANVAS.get()), "{look:?}");
        let (Shape::Mesh(stage), Shape::Mesh(ground)) = (&shapes[1], &shapes[2]) else {
            panic!("{look:?}: two meshes")
        };
        let baked = ambient.stage.as_ref().expect("the stage is baked");
        assert_eq!((baked.0, stage.texture_id), (look, baked.1.id()), "{look:?}");
        let covers = stage.calc_bounds();
        assert!(covers.contains_rect(screen) && covers.width() > screen.width(), "{look:?}: {covers:?}");
        assert_eq!(ground.texture_id, TextureId::default(), "{look:?}");
        let foot = ground.vertices.last().expect("the ground's foot");
        assert_eq!((foot.pos.y, foot.color), (screen.bottom(), shade::TINT_FADE.get()), "{look:?}");
    }
}

/// What the stage is in each look: by night a faint cool light from the top centre on the canvas,
/// by day a light page that is lightest at the top, on black the same light fainter and black
/// below it. Always less to the sides than in the middle, and the page's own colour by the middle
/// of the window.
#[test]
fn the_stage_light_comes_from_the_top_centre_in_each_look() {
    let top = [0.5, 0.0];
    let side = [0.02, 0.0];
    let middle = [0.5, 0.5];
    crate::theme::own_look(Look::Night);
    let night = stage_bake(Look::Night);
    let lit = stage_at(&night, top);
    assert!(lit[2] > lit[0] + 6.0, "cool: {lit:?}");
    assert!(lit[1] > rgb(t::COLOR_CANVAS)[1] + 14.0, "it shows: {lit:?}");
    assert!(lit[1] < 60.0, "and stays a matte stage, no lamp: {lit:?}");
    assert!(stage_at(&night, side)[1] < lit[1] - 6.0, "less to the sides");
    assert!(near(stage_at(&night, middle), rgb(t::COLOR_CANVAS), 3.5), "{:?}", stage_at(&night, middle));

    crate::theme::own_look(Look::Oled);
    let oled = stage_bake(Look::Oled);
    let faint = stage_at(&oled, top);
    assert!(faint[1] > 3.0 && faint[1] < lit[1] * 0.6, "fainter on black: {faint:?} to {lit:?}");
    assert!(near(stage_at(&oled, [0.5, 0.75]), [0.0; 3], 0.5), "true black below it");
    assert!(near(stage_at(&oled, [0.02, 0.98]), [0.0; 3], 0.5), "and in its corners");

    crate::theme::own_look(Look::Day);
    let day = stage_bake(Look::Day);
    let page = rgb(shade::COLOR_CANVAS.get());
    assert!(page[1] > 200.0, "a light page: {page:?}");
    let lightest = stage_at(&day, top);
    assert!(lightest[1] >= page[1] + 4.0, "lightest at the top: {lightest:?}");
    assert!(stage_at(&day, side)[1] <= lightest[1] + 0.5, "never lighter at the sides");
    assert!(near(stage_at(&day, middle), page, 4.0), "{:?}", stage_at(&day, middle));
}

/// From one page without art to the next nothing changes behind them, a stage that is kept stays,
/// and a stage at rest asks for no frames.
#[test]
fn the_stage_is_one_for_every_page_and_rests() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    frame(&ctx, &mut ambient, Want::Stage, 0.0);
    frame(&ctx, &mut ambient, Want::Stage, 3.0);
    frame(&ctx, &mut ambient, Want::Stage, 4.0);
    let key = ambient.key;
    for (n, want) in [Want::Stage, Want::Keep, Want::Stage, Want::Keep].into_iter().enumerate() {
        let shapes = frame(&ctx, &mut ambient, want, 5.0 + n as f64);
        assert_eq!((ambient.key, shapes.len()), (key, 3), "frame {n}: the same stage, whole");
        assert!(!ctx.has_requested_repaint(), "frame {n}: the stage at rest asks for no frame");
        assert_eq!(painted(&ctx), Painted { along: 1.0, fading: false }, "frame {n}");
    }
}

#[test]
fn nothing_to_keep_shows_the_stage() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    frame(&ctx, &mut ambient, Want::Keep, 0.0);
    assert_eq!(ambient.shows, Some(Shows::Stage));
}

/// The stage is baked for its look: another look bakes it again and fades to it.
#[test]
fn another_look_bakes_the_stage_again_and_fades_to_it() {
    crate::theme::own_look(Look::Night);
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    frame(&ctx, &mut ambient, Want::Stage, 0.0);
    frame(&ctx, &mut ambient, Want::Stage, 5.0);
    let night = ambient.stage.as_ref().map(|(_, texture)| texture.id());
    crate::theme::own_look(Look::Day);
    let during = frame(&ctx, &mut ambient, Want::Keep, 6.0);
    assert_eq!(ambient.stage.as_ref().map(|(look, _)| *look), Some(Look::Day));
    assert_ne!(ambient.stage.as_ref().map(|(_, texture)| texture.id()), night);
    assert_eq!(during.len(), 4, "the night's stage under the day's: {during:?}");
    assert!(ctx.has_requested_repaint(), "the fade runs");
}

#[test]
fn the_glow_comes_with_the_ambient_and_rests_when_it_does() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    let a = texture(&ctx, 10);
    frame(&ctx, &mut ambient, Want::Stage, 0.0);
    assert_eq!(ambient.glow(), Color32::TRANSPARENT, "no cover on the stage");
    frame(&ctx, &mut ambient, Want::Stage, 5.0);
    frame(&ctx, &mut ambient, art("a", &a), 6.0);
    let midway = 6.0 + f64::from(t::MOTION_AMBIENT_MS) / 2000.0;
    frame(&ctx, &mut ambient, art("a", &a), midway);
    let half = ambient.glow();
    assert!(
        half.a() > 0 && half.a() < 60,
        "it fades in with it: {half:?}"
    );
    frame(&ctx, &mut ambient, art("a", &a), 10.0);
    frame(&ctx, &mut ambient, art("a", &a), 10.1);
    assert_eq!(ambient.glow().a(), 60);
    assert!(!ctx.has_requested_repaint(), "no idle repaint");
}

#[test]
fn a_new_hero_cross_fades_and_then_asks_for_no_more_frames() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    let (a, b) = (texture(&ctx, 10), texture(&ctx, 20));
    frame(&ctx, &mut ambient, art("a", &a), 0.0);
    frame(&ctx, &mut ambient, art("a", &a), 1.0);
    frame(&ctx, &mut ambient, art("a", &a), 1.1);
    assert!(
        !ctx.has_requested_repaint(),
        "an ambient at rest stays at rest"
    );
    let during = frame(&ctx, &mut ambient, art("b", &b), 2.0);
    assert!(ctx.has_requested_repaint(), "the fade runs");
    let done = 2.0 + f64::from(t::MOTION_AMBIENT_MS) / 1000.0 + 0.01;
    let after = frame(&ctx, &mut ambient, art("b", &b), done);
    assert_eq!(during.len(), after.len() + 1, "the old layer is let go");
}

#[test]
fn with_reduced_motion_the_ambient_cuts() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut style = (*ctx.global_style()).clone();
    style.animation_time = 0.0;
    ctx.set_global_style(style);
    let mut ambient = Ambient::default();
    let (a, b) = (texture(&ctx, 10), texture(&ctx, 20));
    frame(&ctx, &mut ambient, art("a", &a), 0.0);
    let cut = frame(&ctx, &mut ambient, art("b", &b), 0.1);
    assert_eq!(
        cut.len(),
        3,
        "the canvas, one layer, the page colour: {cut:?}"
    );
    let to_stage = frame(&ctx, &mut ambient, Want::Stage, 0.2);
    assert_eq!(to_stage.len(), 3, "and to the stage as well: {to_stage:?}");
    assert!(!ctx.has_requested_repaint(), "no fade to wait for");
}

#[test]
fn a_cover_on_its_way_keeps_what_shows() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    let a = texture(&ctx, 10);
    frame(&ctx, &mut ambient, art("a", &a), 0.0);
    let key = ambient.key;
    frame(&ctx, &mut ambient, Want::Keep, 0.1);
    assert_eq!(ambient.key, key);
    assert!(ambient.shows_art());
}

/// Art and stage cross-fade both ways: both layers show while the fade runs, the stage coming in
/// over the art and the art over the stage, the page colour over them holds still all along, and
/// each rests once it is whole.
#[test]
fn art_and_stage_cross_fade_both_ways_and_rest_afterwards() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    let a = texture(&ctx, 10);
    frame(&ctx, &mut ambient, art("a", &a), 0.0);
    let on_art = frame(&ctx, &mut ambient, art("a", &a), 0.9);
    let ground = |shapes: &[Shape]| match shapes.last() {
        Some(Shape::Mesh(mesh)) => mesh.vertices.iter().map(|v| (v.pos, v.color)).collect::<Vec<_>>(),
        other => panic!("the page colour last: {other:?}"),
    };
    let layers = |shapes: &[Shape]| -> Vec<(TextureId, u8)> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Mesh(mesh) if mesh.texture_id != TextureId::default() => {
                    Some((mesh.texture_id, mesh.vertices[0].color.a()))
                }
                _ => None,
            })
            .collect()
    };
    frame(&ctx, &mut ambient, Want::Stage, 1.0);
    assert!(ctx.has_requested_repaint(), "the stage fades in");
    let midway = f64::from(t::MOTION_AMBIENT_MS) / 2000.0;
    let during = frame(&ctx, &mut ambient, Want::Stage, 1.0 + midway);
    let stage = ambient.stage.as_ref().expect("the stage").1.id();
    let both = layers(&during);
    assert_eq!(both.len(), 2, "the art under the stage: {during:?}");
    assert_eq!((both[0], both[1].0), ((a.id(), 255), stage));
    assert!(both[1].1 > 40 && both[1].1 < 215, "the stage half in: {both:?}");
    assert!(painted(&ctx).fading);
    assert_eq!(ground(&during), ground(&on_art), "the page colour holds still");
    frame(&ctx, &mut ambient, Want::Stage, 2.0);
    let rests = frame(&ctx, &mut ambient, Want::Stage, 2.1);
    assert_eq!(layers(&rests), vec![(stage, 255)], "the stage alone");
    assert!(
        !ctx.has_requested_repaint(),
        "the stage at rest asks for nothing"
    );
    assert_eq!(ground(&rests), ground(&on_art));
    frame(&ctx, &mut ambient, art("a", &a), 3.0);
    assert!(ctx.has_requested_repaint(), "the art fades back");
    let back = layers(&frame(&ctx, &mut ambient, art("a", &a), 3.0 + midway));
    assert_eq!(back.len(), 2, "the stage under the art");
    assert_eq!((back[0], back[1].0), ((stage, 255), a.id()));
    frame(&ctx, &mut ambient, art("a", &a), 4.0);
    let rests = frame(&ctx, &mut ambient, art("a", &a), 4.1);
    assert_eq!(layers(&rests), vec![(a.id(), 255)], "the art alone");
    assert!(!ctx.has_requested_repaint(), "and rests");
}

/// By day a game's ambient, baked dark, takes the page's light wash; the stage is the page's light
/// colour already and takes none, and between them the wash comes and goes with the art.
#[test]
fn by_day_the_wash_lies_over_art_and_never_over_the_stage() {
    crate::theme::own_look(Look::Day);
    let ctx = Context::default();
    let mut ambient = Ambient::default();
    let a = texture(&ctx, 10);
    let wash = |shapes: &[Shape]| {
        shapes.iter().skip(1).find_map(|shape| match shape {
            Shape::Rect(rect) => Some(rect.fill.a()),
            _ => None,
        })
    };
    frame(&ctx, &mut ambient, Want::Stage, 0.0);
    assert_eq!(wash(&frame(&ctx, &mut ambient, Want::Stage, 1.0)), None, "none on the stage");
    frame(&ctx, &mut ambient, art("a", &a), 2.0);
    let midway = 2.0 + f64::from(t::MOTION_AMBIENT_MS) / 2000.0;
    let half = wash(&frame(&ctx, &mut ambient, art("a", &a), midway)).expect("it comes with the art");
    let whole = wash(&frame(&ctx, &mut ambient, art("a", &a), 3.0)).expect("over the art");
    assert_eq!(whole, (DAY_WASH * 255.0).round() as u8);
    assert!(half > 20 && half < whole - 20, "{half} of {whole}");
}

fn veil_of(ctx: &Context, ambient: &Ambient, scrolled: f32) -> Vec<Shape> {
    let screen = ctx.content_rect();
    let brand = Rect::from_min_size(pos2(48.0, 29.0), vec2(118.0, 30.0));
    ambient.veil(ctx, screen, brand, scrolled)
}

/// The veil is the backdrop itself in its own place, over art and over the stage alike: the one
/// layer under the page again, each vertex sampling what the backdrop shows right there, so where
/// nothing passes beneath it there is nothing to see. Whole behind the chrome, nothing at its
/// foot, and none at all with the page at its top.
#[test]
fn the_veil_is_the_backdrop_itself_in_its_own_place_and_nothing_at_the_top() {
    crate::theme::own_palette();
    let ctx = Context::default();
    let a = texture(&ctx, 40);
    for stage in [false, true] {
        let what = if stage { "the stage" } else { "art" };
        let want = || if stage { Want::Stage } else { art("a", &a) };
        let mut ambient = Ambient::default();
        let from = if stage { 10.0 } else { 0.0 };
        frame(&ctx, &mut ambient, want(), from);
        let shapes = frame(&ctx, &mut ambient, want(), from + 5.0);
        let Shape::Mesh(layer) = &shapes[1] else {
            panic!("{what}: the layer")
        };
        let shown = if stage { ambient.stage.as_ref().expect("the stage").1.id() } else { a.id() };
        assert_eq!(layer.texture_id, shown, "{what}");
        let first = layer.vertices[0];
        let last = layer.vertices[layer.vertices.len() - 1];
        let uv = |p: Pos2| {
            pos2(
                egui::remap(p.x, first.pos.x..=last.pos.x, first.uv.x..=last.uv.x),
                egui::remap(p.y, first.pos.y..=last.pos.y, first.uv.y..=last.uv.y),
            )
        };
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(veil_of(ui.ctx(), &ambient, 0.0).is_empty(), "{what}: at the top");
            let veil = veil_of(ui.ctx(), &ambient, 200.0);
            assert_eq!(veil.len(), 1, "{what}: one layer, no dark fill: {veil:?}");
            let Shape::Mesh(mesh) = &veil[0] else {
                panic!("{what}: a mesh")
            };
            assert_eq!(mesh.texture_id, shown, "{what}");
            let screen = ui.ctx().content_rect();
            let foot = mesh.vertices.iter().map(|v| v.pos.y).fold(0.0, f32::max);
            let mut whole = 0;
            for v in &mesh.vertices {
                let want = uv(v.pos);
                assert!((v.uv - want).length() < 1e-3, "{what}: {v:?} wants {want:?}");
                if v.pos.y >= foot - 0.01 {
                    assert_eq!(v.color.a(), 0, "{what}: no edge at its foot: {v:?}");
                }
                if v.pos.y <= band_foot(screen) + 0.01 {
                    assert_eq!(v.color.a(), 255, "{what}: whole behind the chrome: {v:?}");
                    whole += 1;
                }
            }
            assert!(whole > 20, "{what}: the band behind the chrome has its rows");
        });
        let mut delta = output.textures_delta;
        delta.clear();
    }
}

/// The audit's flaw: a settings row and a cover read between the capsules and behind the logo.
/// Behind the chrome the band is whole, then it eases away with no step anywhere.
#[test]
fn the_band_behind_the_chrome_is_whole_and_then_eases_away() {
    for height in [600.0, 800.0, 1080.0] {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, height));
        let brand = Rect::from_min_size(pos2(48.0, 29.0), vec2(118.0, 30.0));
        let at = |x: f32, y: f32| veil_at(screen, brand, pos2(x, y));
        let bar_bottom = super::super::chrome::BAR_TOP + super::super::chrome::BAR_H;
        for y in 0..=bar_bottom as i32 {
            for x in [0.0, 400.0, 800.0, 1279.0] {
                assert_eq!(at(x, y as f32), 1.0, "{height}: nothing shows at {x}, {y}");
            }
        }
        let mut last = at(800.0, 0.0);
        for y in 1..250 {
            let here = at(800.0, y as f32);
            assert!(here <= last && last - here < 0.035, "{height} at {y}: {last} to {here}");
            last = here;
        }
        assert_eq!(at(800.0, veil_height(screen)), 0.0);
        assert!(band_foot(screen) + 40.0 <= veil_height(screen), "{height}: an ease, not an edge");
    }
}

#[test]
fn the_veil_ends_above_where_the_page_colour_starts() {
    for height in [600.0, 800.0, 1080.0, 1440.0] {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1280.0, height));
        let foot = veil_height(screen);
        assert!(foot <= height * FADE_FROM, "{height}: {foot}");
    }
}

#[test]
fn with_high_contrast_the_veil_is_plain_black() {
    crate::theme::own_look(Look::HighContrast);
    let ctx = Context::default();
    let ambient = Ambient::default();
    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let veil = veil_of(ui.ctx(), &ambient, 200.0);
        assert_eq!(veil.len(), 1);
        let Shape::Mesh(mesh) = &veil[0] else {
            panic!("a mesh")
        };
        assert_eq!(mesh.texture_id, TextureId::default());
        assert_eq!(mesh.vertices[0].color, Color32::BLACK);
    });
    let mut delta = output.textures_delta;
    delta.clear();
}

/// High contrast paints solid black whatever the page wants: no ambient, no stage light, nothing
/// baked and nothing for glass to sample.
#[test]
fn high_contrast_paints_solid_black_with_no_light_and_no_backdrop_to_sample() {
    crate::theme::own_look(Look::HighContrast);
    let ctx = Context::default();
    let a = texture(&ctx, 200);
    for (n, want) in [Want::Stage, Want::Keep, art("a", &a), Want::Tile { title: "Game".into() }].into_iter().enumerate() {
        let mut ambient = Ambient::default();
        let from = n as f64 * 10.0;
        frame(&ctx, &mut ambient, want, from);
        frame(&ctx, &mut ambient, Want::Stage, from + 0.5);
        let shapes = frame(&ctx, &mut ambient, Want::Stage, from + 1.0);
        let screen = ctx.content_rect();
        assert_eq!(shapes.len(), 1, "want {n}: {shapes:?}");
        let Shape::Rect(black) = &shapes[0] else {
            panic!("want {n}: a rect")
        };
        assert_eq!((black.rect, black.fill), (screen, Color32::BLACK), "want {n}");
        assert!(ambient.stage.is_none(), "want {n}: no light is baked");
        let behind = glass::backdrop_of(&ctx);
        assert!(behind.base.is_none() && behind.top.is_none(), "want {n}: nothing to sample");
        assert_eq!(ambient.glow(), Color32::TRANSPARENT, "want {n}");
        assert!(!ctx.has_requested_repaint(), "want {n}: and it rests");
    }
}
