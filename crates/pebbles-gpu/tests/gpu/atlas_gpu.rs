//! M01: the coverage atlas grows (side doubling, array layers) without losing glyphs, stays
//! stable across frames, and counts overflow instead of failing.

use super::*;

/// One shared Inter `FontData` (a fresh `Blob` per call would copy the ~800 KB font and mint
/// a new font identity every time).
fn inter() -> peniko::FontData {
    static FONT: std::sync::OnceLock<peniko::FontData> = std::sync::OnceLock::new();
    FONT.get_or_init(|| {
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0)
    })
    .clone()
}

/// One run of `ids` at `size` px, pens `step` px apart starting at `(x, y)`.
fn run(p: &mut Painter<'_>, size: f32, ids: impl Iterator<Item = u32>, x: f32, y: f32, step: f32) {
    let glyphs: Vec<Glyph> = ids.enumerate().map(|(i, id)| Glyph { id, x: x + i as f32 * step, y }).collect();
    p.draw_glyphs(
        &inter(),
        size,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

fn ink(px: &[u8], w: u32, region: (u32, u32, u32, u32)) -> usize {
    let (x0, y0, x1, y1) = region;
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| !is_white(pixel(px, w, x, y)))
        .count()
}

#[test]
#[ignore = "gpu"]
fn atlas_is_stable_across_frames() {
    let Some((device, queue, mut r)) = setup("atlas_is_stable_across_frames") else { return };
    let mut s = Scene::new();
    run(&mut Painter::new(&mut s), 16.0, (36..76).map(|i| i as u32), 4.0, 30.0, 11.0);
    let first = render_scene(&device, &queue, &mut r, &s, (480, 48), Color::WHITE);
    let first_ink = ink(&first, 480, (0, 0, 480, 48));
    for frame in 2..=300 {
        let px = render_scene(&device, &queue, &mut r, &s, (480, 48), Color::WHITE);
        assert_eq!(r.last_frame_stats().glyphs_rasterized, 0, "frame {frame}");
        if frame == 300 {
            assert_eq!(ink(&px, 480, (0, 0, 480, 48)), first_ink);
        }
    }
}

#[test]
#[ignore = "gpu"]
fn atlas_grows_without_losing_glyphs() {
    let Some((device, queue, mut r)) =
        setup_with_budgets("atlas_grows_without_losing_glyphs", |b| b.glyph_atlas_max_side = 2048)
    else {
        return;
    };
    let reference = |p: &mut Painter<'_>| run(p, 24.0, [43u32, 44, 45].into_iter(), 8.0, 40.0, 20.0);
    let mut s = Scene::new();
    reference(&mut Painter::new(&mut s));
    let before = render_scene(&device, &queue, &mut r, &s, (512, 1400), Color::WHITE);
    // The same reference glyphs plus ~2 MPx of distinct 200-px glyphs: the 1024² mask page must
    // double its side within this frame.
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    reference(&mut p);
    // Clip the big glyphs below the reference region (accented capitals at 200 px reach far up);
    // they still enter the atlas, which is what forces the growth.
    p.push_layer(
        Fill::NonZero,
        peniko::Mix::Normal,
        1.0,
        Affine::IDENTITY,
        &Rect::new(0.0, 100.0, 512.0, 1400.0),
    );
    for row in 0..5 {
        run(
            &mut p,
            200.0,
            (36 + row * 10..46 + row * 10).map(|i| i as u32),
            0.0,
            120.0 + row as f32 * 70.0,
            50.0,
        );
    }
    let after = render_scene(&device, &queue, &mut r, &s, (512, 1400), Color::WHITE);
    let st = r.last_frame_stats();
    assert!(st.atlas_grows >= 1, "{st}");
    let region = (0, 0, 80, 60);
    for y in region.1..region.3 {
        for x in region.0..region.2 {
            assert_eq!(
                pixel(&before, 512, x, y),
                pixel(&after, 512, x, y),
                "reference glyph changed at ({x},{y})"
            );
        }
    }
}

#[test]
#[ignore = "gpu"]
fn glyphs_on_layer_1_render() {
    let Some((device, queue, mut r)) = setup_with_budgets("glyphs_on_layer_1_render", |b| {
        b.glyph_atlas_max_side = 1024;
        b.glyph_mask_max_layers = 2;
    }) else {
        return;
    };
    // ~40 distinct 200-px glyphs exceed one 1024² layer, so the later ones land on layer 1.
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for row in 0..8u32 {
        run(&mut p, 200.0, 36 + row * 5..41 + row * 5, 0.0, 180.0 + row as f32 * 200.0, 200.0);
    }
    let px = render_scene(&device, &queue, &mut r, &s, (1000, 1700), Color::WHITE);
    let st = r.last_frame_stats();
    assert!(st.atlas_grows >= 1 && st.glyph_overflow == 0, "{st}");
    // The last row (allocated last, on layer 1) shows ink.
    assert!(ink(&px, 1000, (0, 1500, 1000, 1700)) > 1000, "layer-1 glyphs have ink");
}

#[test]
#[ignore = "gpu"]
fn overflow_is_counted_not_fatal() {
    let Some((device, queue, mut r)) = setup_with_budgets("overflow_is_counted_not_fatal", |b| {
        b.glyph_atlas_max_side = 512;
        b.glyph_mask_max_layers = 2;
    }) else {
        return;
    };
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    // 10 000 distinct glyph sizes of one glyph (each its own atlas entry). Sizes step by 1/64 px
    // from 30 px, so ~10 000 bitmaps of ~25×25 px (padded) — far beyond 2 × 512².
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for i in 0..10_000u32 {
        let size = 30.0 + i as f32 / 64.0;
        run(&mut p, size, std::iter::once(44), (i % 100) as f32 * 9.0, 20.0 + (i / 100) as f32 * 9.0, 0.0);
    }
    let _ = render_scene(&device, &queue, &mut r, &s, (900, 920), Color::WHITE);
    let st = r.last_frame_stats();
    assert!(st.glyph_overflow > 0, "{st}");
    let err = pollster::block_on(scope.pop());
    assert!(err.is_none(), "validation error: {err:?}");
}
