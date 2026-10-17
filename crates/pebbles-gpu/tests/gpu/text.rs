//! Glyph rendering (C09, C04).

use super::*;

/// C09: a 40-glyph label whose last glyph changes every frame for 500 frames keeps its first
/// glyph (no disappearing text), and steady-state frames rasterize at most 2 glyphs.
#[test]
#[ignore = "gpu"]
fn glyphs_survive_500_frames() {
    let Some((device, queue, mut renderer)) = setup("glyphs_survive_500_frames") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let (w, h) = (640u32, 48u32);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let params = FrameParams::new(Color::WHITE);
    let mut targets = FrameTargets::new();
    // Ink of the first glyph's cell (x in 0..16).
    let first_ink = |rgba: &[u8]| {
        (0..h)
            .flat_map(|y| (0..16).map(move |x| (x, y)))
            .filter(|&(x, y)| !is_white(pixel(rgba, w, x, y)))
            .count()
    };
    let mut first = 0usize;
    for frame in 1..=500u32 {
        let mut scene = Scene::new();
        let ids = (0..39u32).map(|i| 36 + i % 26).chain(std::iter::once(36 + frame % 26));
        Painter::new(&mut scene).draw_glyphs(
            &font,
            16.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            ids.enumerate().map(|(i, id)| Glyph { id, x: 2.0 + 15.5 * i as f32, y: 30.0 }),
        );
        let stats = renderer
            .render(RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets), &scene, &params)
            .expect("render");
        if frame == 1 || frame == 500 {
            queue.submit([]);
            let ink = first_ink(&readback(&device, &queue, &texture));
            if frame == 1 {
                assert!(ink > 10, "first glyph has no ink in frame 1");
                first = ink;
            } else {
                let diff = (ink as f64 - first as f64).abs() / first as f64;
                assert!(diff <= 0.02, "first glyph ink {first} -> {ink}");
                assert!(stats.glyphs_rasterized <= 2, "frame 500 rasterized {}", stats.glyphs_rasterized);
            }
        }
    }
}

/// C04: weight travels only in normalized coords — "HHHH" at 32 px renders bolder at wght 700
/// than at wght 400 (bundled Inter Variable). Inter's 'H' advance barely varies with weight
/// (23.78 → 23.91 px at 32 px, per swash and skrifa), so the run is not > 2 px wider as the card
/// assumed; the assertion is on ink (thicker stems) and a non-narrower run instead.
#[test]
#[ignore = "gpu"]
fn bold_run_is_wider_than_regular() {
    let Some((device, queue, mut renderer)) = setup("bold_run_is_wider_than_regular") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let f = swash::FontRef::from_index(bytes, 0).expect("Inter parses");
    let h = f.charmap().map('H');
    let ink = |renderer: &mut Renderer, weight: f32| -> (u32, u64) {
        let coords: Vec<i16> =
            f.variations().normalized_coords([(swash::tag_from_str_lossy("wght"), weight)]).collect();
        let advance = f.glyph_metrics(&coords).scale(32.0).advance_width(h);
        let mut scene = Scene::new();
        Painter::new(&mut scene).draw_glyphs(
            &font,
            32.0,
            &coords,
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            (0..4).map(|i| Glyph { id: u32::from(h), x: 4.0 + advance * i as f32, y: 40.0 }),
        );
        let rgba = render_scene(&device, &queue, renderer, &scene, (200, 56), Color::WHITE);
        let cols: Vec<u32> =
            (0..200).filter(|&x| (0..56).any(|y| !is_white(pixel(&rgba, 200, x, y)))).collect();
        let width = cols.last().map_or(0, |last| last - cols[0] + 1);
        let coverage = rgba.as_chunks::<4>().0.iter().map(|p| u64::from(255 - p[0])).sum();
        (width, coverage)
    };
    let (regular_w, regular_ink) = ink(&mut renderer, 400.0);
    let (bold_w, bold_ink) = ink(&mut renderer, 700.0);
    assert!(bold_w >= regular_w, "bold run {bold_w} px narrower than regular {regular_w} px");
    assert!(bold_ink * 10 > regular_ink * 13, "bold ink {bold_ink} not > 1.3 × regular {regular_ink}");
}
