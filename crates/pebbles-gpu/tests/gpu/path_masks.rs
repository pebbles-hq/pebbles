//! R15: Tier B path masks live in the coverage atlas and are rasterized only on a miss.

use super::*;

/// An icon-sized five-point star (24 px), built inline rather than compiling the Tabler table
/// into this binary.
fn star(cx: f64, cy: f64) -> kurbo::BezPath {
    let mut p = kurbo::BezPath::new();
    for k in 0..10 {
        let r = if k % 2 == 0 { 11.0 } else { 4.5 };
        let a = std::f64::consts::PI * f64::from(k) / 5.0 - std::f64::consts::FRAC_PI_2;
        let pt = (cx + r * a.cos(), cy + r * a.sin());
        if k == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close_path();
    p
}

fn render_stats(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    r: &mut Renderer,
    scene: &Scene,
) -> pebbles_gpu::FrameStats {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.path_masks"),
        size: wgpu::Extent3d { width: 128, height: 128, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
    let stats = r.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render");
    queue.submit([]);
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    stats
}

#[test]
#[ignore = "gpu"]
fn icon_mask_cached_after_first_frame() {
    let Some((device, queue, mut r)) = setup("icon_mask_cached_after_first_frame") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let shape = star(0.0, 0.0);
    // 50 copies at 3 positions (distinct quarter-pixel fractions).
    let positions = [(20.0, 20.0), (60.25, 20.5), (100.75, 60.0)];
    for i in 0..50 {
        let (x, y) = positions[i % 3];
        p.fill(Fill::NonZero, Affine::translate((x, y)), Color::BLACK, None, &shape);
    }
    let first = render_stats(&device, &queue, &mut r, &scene);
    assert_eq!(first.tier_b_draws, 50);
    assert!(first.masks_rasterized <= 3 * 16, "frame 1 rasterized {}", first.masks_rasterized);
    assert_eq!(first.masks_rasterized, 3, "one mask per subpixel position");
    let second = render_stats(&device, &queue, &mut r, &scene);
    assert_eq!(second.masks_rasterized, 0, "frame 2 hits the atlas");
}
