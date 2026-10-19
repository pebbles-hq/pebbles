//! Clipping (C11) and the GL MSAA-resolve scissor regression.

use super::*;

/// C11: a shadowed card half outside a clip is scissored — nothing below the clip edge.
#[test]
#[ignore = "gpu"]
fn shadow_respects_clip() {
    let Some((device, queue, mut renderer)) = setup("shadow_respects_clip") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &Rect::new(0.0, 0.0, 160.0, 100.0),
    );
    p.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        Rect::new(20.0, 54.0, 140.0, 154.0),
        Color::BLACK,
        8.0,
        8.0,
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(20.0, 50.0, 140.0, 150.0));
    p.pop_layer();

    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (160, 160), Color::WHITE);
    // Below the clip edge (y >= 100) everything is clipped away.
    for y in 100..160 {
        for x in 0..160 {
            assert!(is_white(pixel(&rgba, 160, x, y)), "pixel ({x},{y}) should be clipped");
        }
    }
    // Above the clip edge the shadow's blur is visible.
    let mut above = 0;
    for y in 0..100 {
        for x in 0..160 {
            if !is_white(pixel(&rgba, 160, x, y)) {
                above += 1;
            }
        }
    }
    assert!(above > 0, "expected shadow pixels above the clip edge");
}

/// C11: a clipped text run does not clip a later unclipped shadowed card (scoped clip).
#[test]
#[ignore = "gpu"]
fn clipped_text_then_shadow_elsewhere() {
    let Some((device, queue, mut renderer)) = setup("clipped_text_then_shadow_elsewhere") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);

    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &Rect::new(0.0, 0.0, 160.0, 40.0),
    );
    p.draw_glyphs(
        &font,
        24.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(Glyph { id: 36, x: 10.0, y: 30.0 }),
    );
    p.pop_layer();
    p.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        Rect::new(30.0, 64.0, 130.0, 124.0),
        Color::BLACK,
        8.0,
        8.0,
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(30.0, 60.0, 130.0, 120.0));

    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (160, 160), Color::WHITE);
    // The card's shadow (below the card) is not clipped by the text's clip.
    let halo = pixel(&rgba, 160, 80, 130);
    assert!(halo[0] < 240 && halo[1] < 240 && halo[2] < 240, "card shadow not visible: {halo:?}");
}

/// A frame whose LAST draw is clipped still resolves the whole target. wgpu's GL backend
/// resolves MSAA with `glBlitFramebuffer` under the current scissor, so without a full-target
/// scissor at the end of the pass every row outside the last clip stayed transparent black
/// (found running this suite with `WGPU_BACKEND=gl`).
#[test]
#[ignore = "gpu"]
fn msaa_resolve_ignores_last_clip() {
    let Some((device, queue, mut renderer)) = setup("msaa_resolve_ignores_last_clip") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &Rect::new(0.0, 0.0, 160.0, 100.0),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, 160.0, 160.0));
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (160, 160), Color::WHITE);
    assert_eq!(pixel(&rgba, 160, 80, 99), [0, 0, 0, 255], "inside the clip");
    assert_eq!(pixel(&rgba, 160, 80, 100), [255, 255, 255, 255], "outside the clip keeps the clear color");
}

/// R07 AC2: a 40×40 red image drawn at 2× under a zero-radius rounded-rect clip of 40×40 on
/// white — pixel (60, 60) (outside the clip, inside the image) stays white.
#[test]
#[ignore = "gpu"]
fn square_image_box_is_clipped() {
    let Some((device, queue, mut renderer)) = setup("square_image_box_is_clipped") else { return };
    let image = peniko::ImageData {
        data: peniko::Blob::from([255u8, 0, 0, 255].repeat(40 * 40)),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 40,
        height: 40,
    };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let clip = Rect::new(0.0, 0.0, 40.0, 40.0).to_rounded_rect(0.0);
    p.push_layer(Fill::NonZero, peniko::BlendMode::default(), 1.0, Affine::IDENTITY, &clip);
    p.draw_image(&peniko::ImageBrush::new(image), Affine::scale(2.0));
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (100, 100), Color::WHITE);
    assert_eq!(pixel(&rgba, 100, 60, 60), [255, 255, 255, 255], "outside the clip");
    assert_eq!(pixel(&rgba, 100, 20, 20), [255, 0, 0, 255], "inside the clip");
}

/// An integer rect clip is a hard scissor on every edge: nothing at row/column 19 of
/// [20, 100) (render-check clip/rect_scissor against the golden model).
#[test]
#[ignore = "gpu"]
fn integer_clip_is_hard_on_every_edge() {
    let Some((device, queue, mut renderer)) = setup("integer_clip_is_hard_on_every_edge") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &Rect::new(20.0, 20.0, 100.0, 100.0),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, 128.0, 128.0));
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (128, 128), Color::WHITE);
    for i in 0..128 {
        for (x, y) in [(19, i), (i, 19), (100, i), (i, 100)] {
            assert!(is_white(pixel(&rgba, 128, x, y)), "({x},{y}) leaked: {:?}", pixel(&rgba, 128, x, y));
        }
    }
    assert_eq!(pixel(&rgba, 128, 20, 20)[..3], [0, 0, 0]);
}

/// L04: a circle clip is analytic — the edge pixel is partially covered, interior full,
/// outside untouched.
#[test]
#[ignore = "gpu"]
fn circle_clip_edge_is_analytic() {
    let Some((device, queue, mut r)) = setup("circle_clip_edge_is_analytic") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &kurbo::Circle::new((32.0, 32.0), 24.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.pop_layer();
    let px = render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE);
    // Every pixel within 1 px of the edge has the exact pixel area inside the circle (±3; the
    // clip uses the exact-area coverage of Tier A fills): red stays 255, green = 255 · (1 − area).
    let area = |x: u32, y: u32| -> f64 {
        let n = 16;
        let mut inside = 0;
        for j in 0..n {
            for i in 0..n {
                let sx = f64::from(x) + (f64::from(i) + 0.5) / f64::from(n);
                let sy = f64::from(y) + (f64::from(j) + 0.5) / f64::from(n);
                inside += u32::from((sx - 32.0).hypot(sy - 32.0) <= 24.0);
            }
        }
        f64::from(inside) / f64::from(n * n)
    };
    let mut partial = 0;
    for y in 0..64u32 {
        for x in 0..64u32 {
            let d = (f64::from(x) + 0.5 - 32.0).hypot(f64::from(y) + 0.5 - 32.0);
            if (d - 24.0).abs() > 1.0 {
                continue;
            }
            let cov = area(x, y);
            let want = (255.0 * (1.0 - cov)).round() as u8;
            let got = pixel(&px, 64, x, y);
            assert_eq!(got[0], 255, "({x},{y})");
            assert!(got[1].abs_diff(want) <= 3, "({x},{y}) d {d:.3}: green {} want {want}", got[1]);
            partial += u32::from((80..=176).contains(&got[1]));
        }
    }
    assert!(partial >= 8, "only {partial} half-covered edge pixels");
    assert_eq!(pixel(&px, 64, 32, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&px, 64, 32, 4), [255, 255, 255, 255]);
}

/// L09: a 1920×1080 rotated clip is a GPU mask (4× MSAA where available): its edge has
/// fractional coverage, not just 0 / 255.
#[test]
#[ignore = "gpu"]
fn gpu_mask_edges_are_antialiased() {
    let Some((device, queue, mut renderer)) = setup("gpu_mask_edges_are_antialiased") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let rot = Affine::rotate_about(std::f64::consts::FRAC_PI_6, (960.0, 540.0));
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        rot,
        &Rect::new(160.0, 90.0, 1760.0, 990.0),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, 1920.0, 1080.0));
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (1920, 1080), Color::WHITE);
    // Center inside, corner outside.
    assert_eq!(pixel(&rgba, 1920, 960, 540)[0], 0);
    assert!(is_white(pixel(&rgba, 1920, 2, 2)));
    // Along row 540, the left edge of the rotated rect crosses at a fractional x.
    let row: Vec<u8> = (0..1920).map(|x| pixel(&rgba, 1920, x, 540)[0]).collect();
    assert!(row.iter().any(|&v| v > 10 && v < 245), "no fractional edge pixel on row 540");
}

/// L09: a GPU mask inside a Tier B circle mask multiplies its parent: outside the circle stays
/// untouched even where the GPU mask covers.
#[test]
#[ignore = "gpu"]
fn nested_gpu_mask_multiplies_parent() {
    let Some((device, queue, mut renderer)) = setup("nested_gpu_mask_multiplies_parent") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let rot = Affine::rotate_about(0.3, (32.0, 32.0));
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        rot,
        &kurbo::Ellipse::new((32.0, 32.0), (20.0, 20.0), 0.0),
    ); // a rotated circle: Tier B mask
    p.set_path_hint(pebbles_render::paint::display_list::PathHint::Volatile);
    p.push_layer(Fill::NonZero, peniko::BlendMode::default(), 1.0, rot, &Rect::new(-10.0, 24.0, 80.0, 60.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.pop_layer();
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::WHITE);
    assert_eq!(pixel(&rgba, 64, 32, 40), [255, 0, 0, 255], "inside both");
    assert!(is_white(pixel(&rgba, 64, 2, 40)), "inside the GPU mask, outside the circle");
    assert!(is_white(pixel(&rgba, 64, 32, 16)), "inside the circle, outside the GPU mask");
}
