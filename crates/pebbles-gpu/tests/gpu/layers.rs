//! Group opacity (09 §3.4–§3.7, L10–L14): folded single draws and exact offscreen layers.
//! 64×64 on white; every test runs under PEBBLES_GPU_STRICT (zero validation errors).

use super::*;
use peniko::Mix;

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const PINK: [u8; 3] = [255, 128, 128];

fn near(c: [u8; 4], want: [u8; 3], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(c[i]) - i32::from(want[i])).abs() <= tol)
}

fn layer(p: &mut Painter<'_>, alpha: f32, xf: Affine, clip: &impl kurbo::Shape) {
    p.push_layer(Fill::NonZero, Mix::Normal, alpha, xf, clip);
}

fn red(p: &mut Painter<'_>, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, RED, None, &r);
}

fn render(scene: &Scene, name: &str) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup(name)?;
    Some(render_scene(&device, &queue, &mut r, scene, (64, 64), Color::WHITE))
}

/// L12 AC5: one draw under opacity folds into its color (no layer target).
#[test]
#[ignore = "gpu"]
fn opacity_single_draw_is_folded() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(8.0, 8.0, 56.0, 56.0));
    p.pop_layer();
    let Some((device, queue, mut r)) = setup("opacity_single_draw_is_folded") else { return };
    let mut targets = FrameTargets::new();
    let (rgba, stats) = frame(&device, &queue, &mut r, &mut targets, &s);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!((stats.layers_folded, stats.layers_offscreen), (1, 0));
    let c = pixel(&rgba, 64, 32, 32);
    assert!(near(c, PINK, 2), "{c:?}");
    assert!(is_white(pixel(&rgba, 64, 60, 60)));
}

/// L13 AC5 / L14 AC6: two overlapping fills under alpha 0.5 composite once — the overlap is
/// the same pink as the single-covered area (no double blend).
#[test]
#[ignore = "gpu"]
fn opacity_group_does_not_double_blend() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(8.0, 8.0, 40.0, 40.0));
    red(&mut p, Rect::new(24.0, 24.0, 56.0, 56.0));
    p.pop_layer();
    let Some(rgba) = render(&s, "opacity_group_does_not_double_blend") else { return };
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for (x, y) in [(12, 12), (32, 32), (50, 50)] {
        let c = pixel(&rgba, 64, x, y);
        assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
    }
    assert!(is_white(pixel(&rgba, 64, 4, 4)));
    assert!(is_white(pixel(&rgba, 64, 50, 12)));
}

/// L14 AC6: the layer's own circle clip applies once, at the composite.
#[test]
#[ignore = "gpu"]
fn opacity_layer_with_circle_clip() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &kurbo::Circle::new((32.0, 32.0), 24.0));
    red(&mut p, Rect::new(0.0, 0.0, 40.0, 64.0));
    red(&mut p, Rect::new(24.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    let Some(rgba) = render(&s, "opacity_layer_with_circle_clip") else { return };
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for y in 0..64u32 {
        for x in 0..64u32 {
            let d = ((f64::from(x) + 0.5 - 32.0).powi(2) + (f64::from(y) + 0.5 - 32.0).powi(2)).sqrt();
            let c = pixel(&rgba, 64, x, y);
            if d < 23.0 {
                assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
            } else if d > 25.0 {
                assert!(is_white(c), "({x},{y}) {c:?}");
            }
        }
    }
}

/// L14 AC6: a layer inside a rotated (ancestor) clip: the clip applies at the composite only.
#[test]
#[ignore = "gpu"]
fn layer_inside_rotated_clip() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let rot = Affine::rotate_about(std::f64::consts::FRAC_PI_6, (32.0, 32.0));
    let clip = Rect::new(16.0, 16.0, 48.0, 48.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, rot, &clip);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(0.0, 0.0, 40.0, 64.0));
    red(&mut p, Rect::new(24.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    p.pop_layer();
    let Some(rgba) = render(&s, "layer_inside_rotated_clip") else { return };
    let inv = rot.inverse();
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for y in 0..64u32 {
        for x in 0..64u32 {
            let q = inv * kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let c = pixel(&rgba, 64, x, y);
            if clip.inset(-1.5).contains(q) {
                assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
            } else if !clip.inset(1.5).contains(q) {
                assert!(is_white(c), "({x},{y}) {c:?}");
            }
        }
    }
}

/// L13 AC6: a mask clip inside the layer target masks only the layer's content.
#[test]
#[ignore = "gpu"]
fn layer_with_mask_inside() {
    let mut star = kurbo::BezPath::new();
    star.move_to((32.0, 2.0));
    star.line_to((50.0, 60.0));
    star.line_to((4.0, 22.0));
    star.line_to((60.0, 22.0));
    star.line_to((14.0, 60.0));
    star.close_path();
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &star);
    red(&mut p, Rect::new(0.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    red(&mut p, Rect::new(0.0, 56.0, 8.0, 64.0)); // unmasked layer content
    p.pop_layer();
    let Some(rgba) = render(&s, "layer_with_mask_inside") else { return };
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(near(pixel(&rgba, 64, 32, 30), PINK, 2), "star body");
    assert!(near(pixel(&rgba, 64, 3, 60), PINK, 2), "unmasked fill");
    assert!(is_white(pixel(&rgba, 64, 4, 4)), "outside the star");
    assert!(is_white(pixel(&rgba, 64, 32, 62)), "between the star's feet");
}

/// Render `scene` into a fresh 64×64 texture through the persistent `targets`.
fn frame(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    r: &mut Renderer,
    targets: &mut FrameTargets,
    scene: &Scene,
) -> (Vec<u8>, pebbles_gpu::FrameStats) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.layers"),
        size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, targets);
    let stats = r.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render");
    (readback(device, queue, &texture), stats)
}

/// L10 AC3: opacity 0 draws nothing.
#[test]
#[ignore = "gpu"]
fn opacity_zero_draws_nothing() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(8.0, 8.0, 40.0, 40.0));
    red(&mut p, Rect::new(24.0, 24.0, 56.0, 56.0));
    p.pop_layer();
    let Some(rgba) = render(&s, "opacity_zero_draws_nothing") else { return };
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for y in (0..64).step_by(4) {
        for x in (0..64).step_by(4) {
            assert!(is_white(pixel(&rgba, 64, x, y)), "({x},{y})");
        }
    }
}

/// L10 AC4: 0.5 inside 0.5 around one red rect → alpha 0.25.
#[test]
#[ignore = "gpu"]
fn nested_opacity_multiplies() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(8.0, 8.0, 56.0, 56.0));
    p.pop_layer();
    p.pop_layer();
    let Some(rgba) = render(&s, "nested_opacity_multiplies") else { return };
    let c = pixel(&rgba, 64, 32, 32);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(near(c, [255, 191, 191], 2), "{c:?}");
}

/// L10 AC7: two sibling same-size opacity layers reuse one pooled target.
#[test]
#[ignore = "gpu"]
fn sibling_layers_share_one_pool_target() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for x in [0.0, 32.0] {
        layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(x, 0.0, x + 32.0, 64.0));
        red(&mut p, Rect::new(x + 4.0, 8.0, x + 20.0, 40.0));
        red(&mut p, Rect::new(x + 12.0, 24.0, x + 28.0, 56.0));
        p.pop_layer();
    }
    let Some((device, queue, mut r)) = setup("sibling_layers_share_one_pool_target") else { return };
    let mut targets = FrameTargets::new();
    let (rgba, stats) = frame(&device, &queue, &mut r, &mut targets, &s);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(stats.layers_offscreen, 2);
    assert_eq!(targets.layer_pool_targets(), 1);
    for (x, y) in [(16, 30), (48, 30)] {
        let c = pixel(&rgba, 64, x, y);
        assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
    }
}

/// L10 AC8: the same layered scene for 200 frames: the pool and the census stop changing.
#[test]
#[ignore = "gpu"]
fn layer_pool_steady_state_200_frames() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &kurbo::Circle::new((32.0, 32.0), 24.0));
    red(&mut p, Rect::new(8.0, 8.0, 40.0, 40.0));
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(16.0, 16.0, 60.0, 60.0));
    red(&mut p, Rect::new(24.0, 24.0, 56.0, 56.0));
    red(&mut p, Rect::new(30.0, 10.0, 50.0, 50.0));
    p.pop_layer();
    p.pop_layer();
    let Some((device, queue, mut r)) = setup("layer_pool_steady_state_200_frames") else { return };
    let mut targets = FrameTargets::new();
    let mut at10 = None;
    for i in 1..=200 {
        let _ = frame(&device, &queue, &mut r, &mut targets, &s);
        // The census is debug-only (M05); release runs still compare the pool and target bytes.
        #[cfg(debug_assertions)]
        let census = r.census();
        #[cfg(not(debug_assertions))]
        let census = ();
        let snap = (census, targets.layer_pool_targets(), targets.frame_target_bytes());
        if i == 10 {
            at10 = Some(snap);
        } else if i == 200 {
            assert_eq!(Some(snap), at10);
        }
    }
}

/// M16: 20 nested 512×512 opacity layers under a 16 MiB pool budget. In-use targets are never
/// refused (the frame renders; `layer_budget_exceeded` counts it), and after every frame the
/// pool is trimmed back under the budget.
#[test]
#[ignore = "gpu"]
fn layer_pool_respects_budget() {
    const MIB: u64 = 1024 * 1024;
    let Some((device, _queue, mut r)) =
        setup_with_budgets("layer_pool_respects_budget", |b| b.layer_pool_bytes = 16 * MIB)
    else {
        return;
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.layer_budget"),
        size: wgpu::Extent3d { width: 512, height: 512, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    for depth in [20usize, 12, 20, 4] {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for _ in 0..depth {
            layer(&mut p, 0.9, Affine::IDENTITY, &Rect::new(0.0, 0.0, 512.0, 512.0));
            red(&mut p, Rect::new(0.0, 0.0, 300.0, 300.0));
        }
        red(&mut p, Rect::new(200.0, 200.0, 512.0, 512.0));
        for _ in 0..depth {
            p.pop_layer();
        }
        let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
        let stats = r.render(target, &s, &FrameParams::new(Color::WHITE)).expect("render");
        // S12: at most `max_layer_targets` (16) live layers go offscreen; deeper ones flatten.
        let live = depth.min(16);
        let in_use = live as u64 * 512 * 512 * 4 * 2;
        assert_eq!(stats.layers_offscreen as usize, live);
        assert!(stats.layer_pool_bytes <= 16 * MIB, "depth {depth}: {} bytes", stats.layer_pool_bytes);
        assert_eq!(stats.layer_budget_exceeded, u32::from(in_use > 16 * MIB), "depth {depth}");
    }
}

/// L52 AC2: an Over layer splits the root into 3 passes and copies nothing; a Multiply layer
/// copies its backdrop once.
#[test]
#[ignore = "gpu"]
fn frame_stats_count_passes_and_copies() {
    let Some((device, queue, mut r)) = setup("frame_stats_count_passes_and_copies") else { return };
    let scene = |blend: peniko::BlendMode| {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.push_layer(Fill::NonZero, blend, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
        red(&mut p, Rect::new(8.0, 8.0, 40.0, 40.0));
        red(&mut p, Rect::new(24.0, 24.0, 56.0, 56.0));
        p.pop_layer();
        s
    };
    let mut targets = FrameTargets::new();
    let (_, over) = frame(&device, &queue, &mut r, &mut targets, &scene(peniko::BlendMode::default()));
    assert_eq!((over.render_passes, over.backdrop_copies), (3, 0));
    let (_, mul) = frame(&device, &queue, &mut r, &mut targets, &scene(Mix::Multiply.into()));
    assert_eq!(mul.backdrop_copies, 1);
}
