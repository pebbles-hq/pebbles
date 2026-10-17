//! GPU integration tests (pebbles-gpu). Each is `#[ignore = "gpu"]` and skips gracefully
//! without an adapter (DR-15). V01's harness replaces the test-local helpers after C15.

use kurbo::{Affine, Rect, Stroke};
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Brush, Color, Fill};

use pebbles_gpu::{FrameParams, FrameTargets, GpuContext, GpuInit, RenderTarget, Renderer};

/// Acquire a headless device + renderer, printing a skip note and returning `None` without an adapter.
fn setup(name: &str) -> Option<(wgpu::Device, wgpu::Queue, Renderer)> {
    let mut ctx = pollster::block_on(GpuContext::new(&GpuInit::headless()));
    let id = match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("SKIP {name}: {e}");
            return None;
        }
    };
    let gpu = ctx.device(id).expect("device just requested");
    let device = gpu.device().clone();
    let queue = gpu.queue().clone();
    let renderer = Renderer::new(gpu, id);
    Some((device, queue, renderer))
}

/// Render `scene` into a fresh `size` target and read the pixels back.
fn render_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    scene: &Scene,
    size: (u32, u32),
    base: Color,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
    renderer.render(target, scene, &FrameParams::new(base)).expect("render");
    queue.submit([]);
    readback(device, queue, &texture)
}

fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

fn is_white(c: [u8; 4]) -> bool {
    (c[0] as i32 - 255).abs() <= 1 && (c[1] as i32 - 255).abs() <= 1 && (c[2] as i32 - 255).abs() <= 1
}

/// One `Renderer`, rendered alternately into an `Rgba8Unorm` and a `Bgra8Unorm` target:
/// every render is inside a `Validation` error scope, each pop is `None`, and the center
/// pixel of a red fill is red in both formats.
#[test]
#[ignore = "gpu"]
fn two_formats_alternate_without_validation_errors() {
    let mut ctx = pollster::block_on(GpuContext::new(&GpuInit::headless()));
    let id = match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("SKIP two_formats_alternate_without_validation_errors: {e}");
            return;
        }
    };
    let gpu = ctx.device(id).expect("device just requested");
    let device = gpu.device();
    let queue = gpu.queue();
    let mut renderer = Renderer::new(gpu, id);

    for format in
        [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Bgra8Unorm].into_iter().cycle().take(10)
    {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut scene = Scene::new();
        Painter::new(&mut scene).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &Rect::new(8.0, 8.0, 56.0, 56.0),
        );

        let guard = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut targets = FrameTargets::new();
        let target = RenderTarget::new(&view, format, &mut targets);
        renderer.render(target, &scene, &FrameParams::new(Color::WHITE)).expect("render");
        queue.submit([]);
        let err = pollster::block_on(guard.pop());
        assert!(err.is_none(), "validation error: {err:?}");

        let bytes = readback(device, queue, &texture);
        let px = &bytes[((32 * 64 + 32) * 4)..((32 * 64 + 32) * 4 + 4)];
        // The center pixel is RED in both targets; the byte order differs by format
        // (Bgra8Unorm stores B,G,R,A).
        let expected: [u8; 4] =
            if format == wgpu::TextureFormat::Bgra8Unorm { [0, 0, 255, 255] } else { [255, 0, 0, 255] };
        assert_eq!(px, expected, "center pixel {format:?}: {px:?}");
    }
}

/// D01 regression: a shadow recorded before its card must not cover the card's center.
#[test]
#[ignore = "gpu"]
fn shadow_does_not_cover_its_card() {
    let Some((device, queue, mut renderer)) = setup("shadow_does_not_cover_its_card") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_blurred_rounded_rect(Affine::IDENTITY, Rect::new(20.0, 24.0, 120.0, 84.0), Color::BLACK, 8.0, 8.0);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(20.0, 20.0, 120.0, 80.0));
    p.stroke(&Stroke::new(1.0), Affine::IDENTITY, Color::BLACK, None, &Rect::new(20.0, 20.0, 120.0, 80.0));

    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (140, 100), Color::WHITE);
    let c = pixel(&rgba, 140, 70, 50);
    assert!(is_white(c), "card center covered by shadow: {c:?}");
    // The shadow must render as a dark halo just below the card (blur + offset).
    let halo = pixel(&rgba, 140, 70, 82);
    assert!(halo[0] < 240 && halo[1] < 240 && halo[2] < 240, "shadow did not render: {halo:?}");
}

/// D01 regression: page text under a scrim and an opaque dialog must be hidden inside the dialog.
#[test]
#[ignore = "gpu"]
fn dialog_scrim_hides_page_text() {
    let Some((device, queue, mut renderer)) = setup("dialog_scrim_hides_page_text") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);

    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_glyphs(
        &font,
        24.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(Glyph { id: 36, x: 20.0, y: 40.0 }),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 0, 128),
        None,
        &Rect::new(0.0, 0.0, 200.0, 120.0),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(60.0, 30.0, 180.0, 100.0));

    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (200, 120), Color::WHITE);
    for y in 32..98 {
        for x in 62..178 {
            let c = pixel(&rgba, 200, x, y);
            assert!(is_white(c), "dialog pixel ({x},{y}) not white: {c:?}");
        }
    }
}

/// C08: clearing `#808080` stores exactly 128 (sRGB-encoded, no linearization).
#[test]
#[ignore = "gpu"]
fn clear_is_exact_srgb() {
    let Some((device, queue, mut renderer)) = setup("clear_is_exact_srgb") else { return };
    let scene = Scene::new();
    let rgba = render_scene(
        &device,
        &queue,
        &mut renderer,
        &scene,
        (64, 64),
        Color::from_rgba8(0x80, 0x80, 0x80, 0xff),
    );
    for px in rgba.as_chunks::<4>().0 {
        assert!(
            (px[0] as i32 - 128).abs() <= 1
                && (px[1] as i32 - 128).abs() <= 1
                && (px[2] as i32 - 128).abs() <= 1,
            "pixel {px:?}"
        );
    }
}

/// C08: 50 % white over black blends in gamma space → 128 (linear would give ≈188).
#[test]
#[ignore = "gpu"]
fn half_alpha_white_over_black_is_gamma_space() {
    let Some((device, queue, mut renderer)) = setup("half_alpha_white_over_black_is_gamma_space") else {
        return;
    };
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 255, 255, 128),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::BLACK);
    let c = pixel(&rgba, 64, 32, 32);
    assert!(
        (c[0] as i32 - 128).abs() <= 1 && (c[1] as i32 - 128).abs() <= 1 && (c[2] as i32 - 128).abs() <= 1,
        "pixel {c:?}"
    );
}

/// C08: clearing `rgba(255,0,0,0.5)` premultiplies → readback `(128,0,0,128)`.
#[test]
#[ignore = "gpu"]
fn premultiplied_clear_with_alpha() {
    let Some((device, queue, mut renderer)) = setup("premultiplied_clear_with_alpha") else { return };
    let scene = Scene::new();
    let rgba =
        render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::from_rgba8(255, 0, 0, 128));
    let c = pixel(&rgba, 64, 32, 32);
    assert!(
        (c[0] as i32 - 128).abs() <= 1 && c[1] <= 1 && c[2] <= 1 && (c[3] as i32 - 128).abs() <= 1,
        "pixel {c:?}"
    );
}

/// C10: a 1-logical-px border under `scale_factor` 2 renders 2 device px wide (D04).
#[test]
#[ignore = "gpu"]
fn border_is_two_device_px_at_scale_2() {
    let Some((device, queue, mut renderer)) = setup("border_is_two_device_px_at_scale_2") else { return };
    let mut scene = Scene::new();
    Painter::new(&mut scene).stroke(
        &Stroke::new(1.0),
        Affine::scale(2.0),
        Color::BLACK,
        None,
        &kurbo::Line::new(kurbo::Point::new(0.0, 25.0), kurbo::Point::new(50.0, 25.0)),
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (100, 100), Color::WHITE);
    // The 1-logical-px stroke at scale 2 is a 2-device-px band centered on y=50; probe column x=50.
    let dark = (0..100).filter(|&y| pixel(&rgba, 100, 50, y)[0] < 128).count();
    assert!((2..=3).contains(&dark), "border should be ~2 device px, got {dark}");
}

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

/// C12: after warm-up, rendering the same scene (fills, text, a shadow) creates no buffers,
/// bind groups, textures or pipelines.
#[test]
#[ignore = "gpu"]
fn steady_state_frames_create_nothing() {
    let Some((device, _queue, mut renderer)) = setup("steady_state_frames_create_nothing") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_blurred_rounded_rect(Affine::IDENTITY, Rect::new(20.0, 24.0, 120.0, 84.0), Color::BLACK, 8.0, 8.0);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(20.0, 20.0, 120.0, 80.0));
    p.draw_image(
        &peniko::ImageBrush::new(rgba_image(2, 2, [0, 128, 255, 255])),
        Affine::translate((130.0, 10.0)),
    );
    p.draw_glyphs(
        &font,
        20.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        [36u32, 37, 38].into_iter().enumerate().map(|(i, id)| Glyph {
            id,
            x: 30.0 + 14.0 * i as f32,
            y: 56.0,
        }),
    );

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: 160, height: 120, depth_or_array_layers: 1 },
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
    for frame in 1..=20 {
        let stats = renderer
            .render(RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets), &scene, &params)
            .expect("render");
        if frame >= 3 {
            assert_eq!(
                (
                    stats.buffers_created,
                    stats.bind_groups_created,
                    stats.textures_created,
                    stats.pipelines_created
                ),
                (0, 0, 0, 0),
                "frame {frame} created resources: {stats:?}"
            );
        }
    }
}

/// C17: a child inside a blend layer (a `ColorFiltered`-style Multiply group): the filter rect
/// that the blend layer draws is skipped with the layer, so it no longer covers the child,
/// and the child drawn after the layer stays visible.
#[test]
#[ignore = "gpu"]
fn color_filtered_child_stays_visible() {
    let Some((device, queue, mut renderer)) = setup("color_filtered_child_stays_visible") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(20.0, 20.0, 60.0, 60.0),
    );
    p.push_layer(
        Fill::NonZero,
        peniko::Mix::Multiply,
        1.0,
        Affine::IDENTITY,
        &Rect::new(0.0, 0.0, 80.0, 80.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 255, 255),
        None,
        &Rect::new(0.0, 0.0, 80.0, 80.0),
    );
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (80, 80), Color::WHITE);
    assert_eq!(pixel(&rgba, 80, 40, 40), [255, 0, 0, 255], "child covered by the filter rect");
    assert_eq!(renderer.last_stats().unsupported[pebbles_gpu::Unsupported::LayerBlend as usize], 1);
}

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

/// A `w`×`h` straight-alpha RGBA image filled with `px`.
fn rgba_image(w: u32, h: u32, px: [u8; 4]) -> peniko::ImageData {
    peniko::ImageData {
        data: peniko::Blob::from(px.repeat((w * h) as usize)),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: w,
        height: h,
    }
}

fn image_scene(image: &peniko::ImageData, transform: Affine) -> Scene {
    let mut scene = Scene::new();
    Painter::new(&mut scene).draw_image(&peniko::ImageBrush::new(image.clone()), transform);
    scene
}

/// C13: the same image drawn in two frames is uploaded once; frame 2 creates no bind group.
#[test]
#[ignore = "gpu"]
fn same_image_two_frames_uploads_once() {
    let Some((device, queue, mut renderer)) = setup("same_image_two_frames_uploads_once") else { return };
    let image = rgba_image(4, 4, [255, 0, 0, 255]);
    let scene = image_scene(&image, Affine::translate((8.0, 8.0)));
    render_scene(&device, &queue, &mut renderer, &scene, (32, 32), Color::WHITE);
    let first = renderer.last_stats();
    render_scene(&device, &queue, &mut renderer, &scene, (32, 32), Color::WHITE);
    let second = renderer.last_stats();
    assert_eq!((first.images_uploaded, second.images_uploaded), (1, 0));
    assert_eq!(second.bind_groups_created, 0);
}

/// C13: the frame after the app drops its `ImageData` evicts the texture.
#[test]
#[ignore = "gpu"]
fn dropped_image_is_evicted_next_frame() {
    let Some((device, queue, mut renderer)) = setup("dropped_image_is_evicted_next_frame") else { return };
    let image = rgba_image(4, 4, [255, 0, 0, 255]);
    let scene = image_scene(&image, Affine::IDENTITY);
    render_scene(&device, &queue, &mut renderer, &scene, (16, 16), Color::WHITE);
    drop(scene);
    drop(image);
    render_scene(&device, &queue, &mut renderer, &Scene::new(), (16, 16), Color::WHITE);
    assert_eq!(renderer.last_stats().image_evictions, 1);
}

/// C13: a new image of the same size after a drop shows its own pixels (no ABA by pointer).
#[test]
#[ignore = "gpu"]
fn new_image_after_drop_shows_new_pixels() {
    let Some((device, queue, mut renderer)) = setup("new_image_after_drop_shows_new_pixels") else { return };
    let red = rgba_image(8, 8, [255, 0, 0, 255]);
    let scene = image_scene(&red, Affine::IDENTITY);
    render_scene(&device, &queue, &mut renderer, &scene, (8, 8), Color::WHITE);
    drop(scene);
    drop(red);
    let green = rgba_image(8, 8, [0, 255, 0, 255]);
    let rgba = render_scene(
        &device,
        &queue,
        &mut renderer,
        &image_scene(&green, Affine::IDENTITY),
        (8, 8),
        Color::WHITE,
    );
    assert_eq!(pixel(&rgba, 8, 4, 4), [0, 255, 0, 255]);
}

/// C13: a straight-alpha edge [opaque red, transparent black] magnified ×64 over white has no
/// dark fringe — premultiplied at upload, the midpoint is (255, 128, 128) ±4 (straight-alpha
/// filtering gives R ≈ 191).
#[test]
#[ignore = "gpu"]
fn straight_alpha_edge_has_no_dark_fringe() {
    let Some((device, queue, mut renderer)) = setup("straight_alpha_edge_has_no_dark_fringe") else { return };
    let image = peniko::ImageData {
        data: peniko::Blob::from(vec![255, 0, 0, 255, 0, 0, 0, 0]),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 1,
    };
    let rgba = render_scene(
        &device,
        &queue,
        &mut renderer,
        &image_scene(&image, Affine::scale(64.0)),
        (128, 64),
        Color::WHITE,
    );
    let mid = pixel(&rgba, 128, 64, 32);
    let near = |v: u8, want: i32| (i32::from(v) - want).abs() <= 4;
    assert!(near(mid[0], 255) && near(mid[1], 128) && near(mid[2], 128), "midpoint {mid:?}");
}

/// C13: an image wider than the texture limit is downscaled and drawn, with one fallback record.
#[test]
#[ignore = "gpu"]
fn oversize_image_is_downscaled_not_rejected() {
    let Some((device, queue, mut renderer)) = setup("oversize_image_is_downscaled_not_rejected") else {
        return;
    };
    let w = device.limits().max_texture_dimension_2d + 1;
    let image = rgba_image(w, 1, [0, 0, 255, 255]);
    let scene = image_scene(&image, Affine::scale_non_uniform(64.0 / f64::from(w), 16.0));
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 16), Color::WHITE);
    assert_eq!(pixel(&rgba, 64, 32, 8), [0, 0, 255, 255]);
    assert_eq!(renderer.last_stats().unsupported[pebbles_gpu::Unsupported::ImageTooLarge as usize], 1);
}

fn target_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// C14: one `Renderer`, a 300×200 and a 500×400 target, 20 alternations → exactly 2 MSAA
/// textures, and from frame 3 on no textures or views are created.
#[test]
#[ignore = "gpu"]
fn two_targets_alternating_allocate_msaa_twice() {
    let Some((device, _queue, mut renderer)) = setup("two_targets_alternating_allocate_msaa_twice") else {
        return;
    };

    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(0.0, 0.0, 50.0, 50.0),
    );
    let small = target_texture(&device, 300, 200);
    let large = target_texture(&device, 500, 400);
    let (small_view, large_view) =
        (small.create_view(&Default::default()), large.create_view(&Default::default()));
    let (mut small_targets, mut large_targets) = (FrameTargets::new(), FrameTargets::new());
    let params = FrameParams::new(Color::WHITE);
    let mut created = 0;
    for frame in 1..=40 {
        let target = if frame % 2 == 1 {
            RenderTarget::new(&small_view, wgpu::TextureFormat::Rgba8Unorm, &mut small_targets)
        } else {
            RenderTarget::new(&large_view, wgpu::TextureFormat::Rgba8Unorm, &mut large_targets)
        };
        let stats = renderer.render(target, &scene, &params).expect("render");
        created += stats.textures_created;
        if frame >= 3 {
            assert_eq!(
                (stats.textures_created, stats.texture_views_created),
                (0, 0),
                "frame {frame}: {stats:?}"
            );
        }
    }
    // The glyph atlas pages are created with the Renderer, so every texture counted here is MSAA.
    assert_eq!(created, 2);
}

/// C14: the render size comes from the target texture; rendering through it is valid.
#[test]
#[ignore = "gpu"]
fn render_target_size_comes_from_texture() {
    let Some((device, queue, mut renderer)) = setup("render_target_size_comes_from_texture") else { return };
    let texture = target_texture(&device, 200, 100);
    let view = texture.create_view(&Default::default());
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
    assert_eq!(target.size(), (200, 100));
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(150.0, 50.0, 200.0, 100.0),
    );
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    renderer.render(target, &scene, &FrameParams::new(Color::WHITE)).expect("render");
    assert!(pollster::block_on(scope.pop()).is_none(), "validation error");
    queue.submit([]);
    let rgba = readback(&device, &queue, &texture);
    assert_eq!(pixel(&rgba, 200, 190, 90), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, 200, 10, 10), [255, 255, 255, 255]);
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

/// Copy a texture's rows (padded to 256 bytes) into an unpadded byte buffer.
fn readback(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let width = texture.width();
    let height = texture.height();
    let padded = width * 4
        + (wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - (width * 4) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let size = padded * height;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.test.readback"),
        size: u64::from(size),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    while rx.try_recv().is_err() {
        let _ = device.poll(wgpu::PollType::Poll);
    }
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}
