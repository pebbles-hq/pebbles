//! R17: Tier C runs render offscreen (MSAA when negotiated) and composite in painter's order
//! into a single-sampled main pass.

use super::*;

const W: u32 = 64;

fn volatile_square(p: &mut Painter<'_>, r: Rect, c: Color) {
    let mut path = kurbo::BezPath::new();
    path.move_to((r.x0, r.y0));
    path.line_to((r.x1, r.y0));
    path.line_to((r.x1, r.y1));
    path.line_to((r.x0, r.y1));
    path.close_path();
    p.set_path_hint(pebbles_render::paint::display_list::PathHint::Volatile);
    p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &path);
    p.set_path_hint(pebbles_render::paint::display_list::PathHint::Auto);
}

#[test]
#[ignore = "gpu"]
fn tier_c_run_composites_in_order() {
    let Some((device, queue, mut r)) = setup("tier_c_run_composites_in_order") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let red = Color::from_rgba8(255, 0, 0, 255);
    let blue = Color::from_rgba8(0, 0, 255, 255);
    p.fill(Fill::NonZero, Affine::IDENTITY, red, None, &Rect::new(0.0, 0.0, 40.0, 40.0));
    volatile_square(&mut p, Rect::new(20.0, 20.0, 60.0, 60.0), Color::BLACK);
    p.fill(Fill::NonZero, Affine::IDENTITY, blue, None, &Rect::new(30.0, 30.0, 50.0, 50.0));
    let px = render_scene(&device, &queue, &mut r, &s, (W, W), Color::WHITE);
    assert_eq!(r.last_stats().tier_c_draws, 1);
    assert_eq!(pixel(&px, W, 10, 10)[..3], [255, 0, 0], "fill below");
    assert_eq!(pixel(&px, W, 25, 25)[..3], [0, 0, 0], "Tier C over the first fill");
    assert_eq!(pixel(&px, W, 40, 40)[..3], [0, 0, 255], "last fill over Tier C");
    assert_eq!(pixel(&px, W, 55, 55)[..3], [0, 0, 0]);
    assert_eq!(pixel(&px, W, 62, 62)[..3], [255, 255, 255]);
    // Edge at x = 20 is pixel-aligned: no bleed outside the run.
    assert_eq!(pixel(&px, W, 19, 50)[..3], [255, 255, 255]);
}

#[test]
#[ignore = "gpu"]
fn no_offscreen_target_without_tier_c() {
    let Some((device, _queue, mut r)) = setup("no_offscreen_target_without_tier_c") else { return };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.census"),
        size: wgpu::Extent3d { width: 1920, height: 1080, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    let mut s = Scene::new();
    Painter::new(&mut s).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(10.0, 10.0, 900.0, 500.0),
    );
    r.render(
        RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets),
        &s,
        &FrameParams::new(Color::WHITE),
    )
    .expect("render");
    let color = 1920u64 * 1080 * 4;
    assert!(targets.frame_target_bytes() * 10 <= color, "{} bytes", targets.frame_target_bytes());
    assert_eq!(targets.frame_target_bytes(), 0);
}
