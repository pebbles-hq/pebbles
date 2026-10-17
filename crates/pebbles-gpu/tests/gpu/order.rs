//! Painter's order, steady state and target formats (C07, C12).

use super::*;

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

/// D01: page text drawn BEFORE a dialog stays under it even when the glyph sits inside the
/// dialog — a renderer that batches by kind (solids, then glyphs) would draw it on top.
#[test]
#[ignore = "gpu"]
fn text_under_dialog_stays_covered() {
    let Some((device, queue, mut renderer)) = setup("text_under_dialog_stays_covered") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_glyphs(
        &font,
        32.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(Glyph { id: 36, x: 100.0, y: 80.0 }),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(60.0, 30.0, 180.0, 100.0));
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (200, 120), Color::WHITE);
    for y in 32..98 {
        for x in 62..178 {
            let c = pixel(&rgba, 200, x, y);
            assert!(is_white(c), "page text shows through the dialog at ({x},{y}): {c:?}");
        }
    }
}
