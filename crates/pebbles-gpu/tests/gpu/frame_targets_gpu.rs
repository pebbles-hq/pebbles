//! R12: the negotiated MSAA sample count renders correctly either way.

use super::*;

const W: u32 = 64;

#[test]
#[ignore = "gpu"]
fn negotiated_count_renders() {
    let Some((device, queue, mut r)) = setup("negotiated_count_renders") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    // Exact coverage, not the pixel-grid policy (R18 snaps this edge under `Auto`).
    p.set_pixel_snap(pebbles_render::paint::display_list::PixelSnap::Off);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(10.5, 8.0, 30.0, 56.0));
    let px = render_scene(&device, &queue, &mut r, &s, (W, W), Color::WHITE);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!([1, 4].contains(&r.msaa_sample_count()), "count {}", r.msaa_sample_count());
    for y in 9..=54 {
        let edge = pixel(&px, W, 10, y);
        assert!((0..3).all(|i| (125..=130).contains(&edge[i])), "edge ({y}) {edge:?}");
    }
}

/// S17 (F20): 10 sizes × 20 cycles on one target, then 3 frames at one size: the book holds one
/// target and the last frames create nothing.
#[test]
#[ignore = "gpu"]
fn target_churn_returns_to_baseline() {
    let Some((device, _queue, mut renderer)) = setup("target_churn_returns_to_baseline") else { return };
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(2.0, 2.0, 30.0, 20.0),
    );
    let mut targets = FrameTargets::new();
    let mut render = |size: (u32, u32), targets: &mut FrameTargets| {
        let texture = target_texture(&device, size.0, size.1);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, targets);
        renderer.render(target, &scene, &FrameParams::new(Color::WHITE)).expect("render")
    };
    for _ in 0..20 {
        for k in 0..10u32 {
            render((64 + 37 * k, 48 + 29 * k), &mut targets);
        }
    }
    for i in 0..3 {
        let s = render((200, 150), &mut targets);
        assert_eq!((s.textures_created, s.texture_views_created), (0, 0), "settle frame {i}");
    }
    assert_eq!(targets.layer_pool_targets(), 0, "a 1-rect scene needs no pooled target");
    assert_eq!(targets.frame_target_bytes(), 0);
    #[cfg(debug_assertions)]
    assert_eq!(renderer.census().frame_target_entries, 1);
}
