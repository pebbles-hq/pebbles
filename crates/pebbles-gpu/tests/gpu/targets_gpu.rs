//! FrameTargets byte accounting and resize storms (M13, 10 §4.4). Solid-only scenes, so no
//! atlas texture is involved.

use super::*;

fn scene() -> Scene {
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(4.0, 4.0, 60.0, 40.0),
    );
    scene
}

fn render(
    device: &wgpu::Device,
    renderer: &mut Renderer,
    targets: &mut FrameTargets,
    size: (u32, u32),
    scene: &Scene,
) -> pebbles_gpu::FrameStats {
    let texture = target_texture(device, size.0, size.1);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, targets);
    renderer.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render")
}

#[test]
#[ignore = "gpu"]
fn two_targets_do_not_thrash() {
    let Some((device, _queue, mut renderer)) = setup("two_targets_do_not_thrash") else { return };
    let (mut a, mut b) = (FrameTargets::new(), FrameTargets::new());
    let s = scene();
    for frame in 1..=20 {
        let sa = render(&device, &mut renderer, &mut a, (300, 200), &s);
        let sb = render(&device, &mut renderer, &mut b, (500, 400), &s);
        if frame >= 3 {
            assert_eq!((sa.textures_created, sb.textures_created), (0, 0), "frame {frame}");
        }
    }
    #[cfg(debug_assertions)]
    assert_eq!(renderer.census().frame_target_entries, 2);
}

#[test]
#[ignore = "gpu"]
fn resize_storm_recreates_one_texture() {
    let Some((device, _queue, mut renderer)) = setup("resize_storm_recreates_one_texture") else { return };
    let mut t = FrameTargets::new();
    let s = scene();
    for i in 0..100u32 {
        let st = render(&device, &mut renderer, &mut t, (800 + 7 * i, 600 + 7 * i), &s);
        // R17: the main pass is single-sampled, so a resize creates no target texture at all.
        assert_eq!(st.textures_created, 0, "frame {i}");
        #[cfg(debug_assertions)]
        assert_eq!(renderer.census().frame_target_bytes, t.frame_target_bytes(), "frame {i}");
    }
}

#[test]
#[ignore = "gpu"]
fn dropped_target_leaves_the_book() {
    let Some((device, _queue, mut renderer)) = setup("dropped_target_leaves_the_book") else { return };
    let s = scene();
    {
        let mut a = FrameTargets::new();
        render(&device, &mut renderer, &mut a, (64, 48), &s);
    }
    let mut b = FrameTargets::new();
    for _ in 0..301 {
        render(&device, &mut renderer, &mut b, (64, 48), &s);
    }
    #[cfg(debug_assertions)]
    assert_eq!(renderer.census().frame_target_entries, 1);
}

#[test]
#[ignore = "gpu"]
fn over_budget_is_counted_not_refused() {
    let Some((device, _queue, mut renderer)) =
        setup_with_budgets("over_budget_is_counted_not_refused", |b| b.frame_targets_bytes = 0)
    else {
        return;
    };
    // A backdrop layer forces the root intermediate (a target-owned texture) into the book.
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, 32.0, 32.0));
    p.push_layer(
        Fill::NonZero,
        peniko::Mix::Multiply,
        1.0,
        Affine::IDENTITY,
        &Rect::new(8.0, 8.0, 24.0, 24.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(200, 30, 30),
        None,
        &Rect::new(0.0, 0.0, 32.0, 32.0),
    );
    p.pop_layer();
    let mut t = FrameTargets::new();
    let st = render(&device, &mut renderer, &mut t, (32, 32), &scene);
    assert!(t.frame_target_bytes() > 0, "the scene owns target textures");
    assert_eq!(st.frame_target_budget_exceeded, 1);
}
