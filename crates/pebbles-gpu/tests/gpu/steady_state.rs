//! Steady-state counters (10 §9.1, M04): FrameStats counts real work.

use super::*;

/// M04 AC2: a 100×100 grid of 8×8 rects in two alternating colors (the §11.1 recipe).
#[test]
#[ignore = "gpu"]
fn stats_count_draws_and_passes() {
    let Some((device, queue, mut r)) = setup("stats_count_draws_and_passes") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for y in 0..100u32 {
        for x in 0..100u32 {
            let c = if (x + y) % 2 == 0 { Color::BLACK } else { Color::from_rgb8(37, 99, 235) };
            let (fx, fy) = (f64::from(x) * 8.0, f64::from(y) * 8.0);
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(fx, fy, fx + 8.0, fy + 8.0));
        }
    }
    let _ = render_scene(&device, &queue, &mut r, &s, (800, 800), Color::WHITE);
    let st = r.last_frame_stats();
    assert_eq!(st.render_passes, 1, "{st}");
    assert!(st.draw_calls >= 1, "{st}");
    assert!(st.buffer_bytes_uploaded > 0, "{st}");
    assert_eq!(st.instances, 10_000, "{st}");
}

/// M09 AC1: alternating Rgba8Unorm and Bgra8Unorm targets for 6 frames creates each prewarm
/// pipeline once per format, nothing from frame 3 on, and no validation error.
#[test]
#[ignore = "gpu"]
fn two_formats_create_pipelines_once() {
    let Some((device, queue, mut r)) = setup("two_formats_create_pipelines_once") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(4.0, 4.0, 20.0, 20.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &kurbo::RoundedRect::new(24.0, 4.0, 40.0, 20.0, 4.0),
    );
    let formats = [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Bgra8Unorm];
    let textures = formats.map(|f| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.formats"),
            size: wgpu::Extent3d { width: 64, height: 32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: f,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    });
    let mut targets = [FrameTargets::new(), FrameTargets::new()];
    let prewarm_set = 5; // solid, sdf, shadow, glyph, image (K3)
    let mut total = 0;
    for frame in 1..=6usize {
        let i = frame % 2;
        let view = textures[i].create_view(&wgpu::TextureViewDescriptor::default());
        let st = r
            .render(
                RenderTarget::new(&view, formats[i], &mut targets[i]),
                &s,
                &FrameParams::new(Color::WHITE),
            )
            .expect("render");
        total += st.pipelines_created;
        if frame >= 3 {
            assert_eq!(st.pipelines_created, 0, "frame {frame}: {st}");
        }
    }
    queue.submit([]);
    assert_eq!(total, 2 * prewarm_set);
}
