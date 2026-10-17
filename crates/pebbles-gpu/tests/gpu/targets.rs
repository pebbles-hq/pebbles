//! Per-target textures (C14) and device validation routing (C16).

use super::*;

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

/// C16: on a pebbles-gpu device a validation error mentioning `Texture`/`create_view` is
/// reported (counted by `DeviceHealth`), never swallowed by the hybrid host's Vello filter.
#[test]
#[ignore = "gpu"]
fn validation_error_with_texture_text_is_reported() {
    let mut init = GpuInit::headless();
    init.strict_validation = false;
    let mut ctx = pollster::block_on(GpuContext::new(&init));
    let Ok(id) = pollster::block_on(ctx.request_device(None)) else {
        assert!(!std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1"), "no GPU adapter");
        return;
    };
    let gpu = ctx.device(id).expect("device just requested");
    let texture = target_texture(gpu.device(), 4, 4);
    let before = gpu.health().validation_errors();
    // A view of mip level 3 on a 1-mip texture: a Texture / create_view validation error.
    let _view = texture.create_view(&wgpu::TextureViewDescriptor { base_mip_level: 3, ..Default::default() });
    let _ = gpu.device().poll(wgpu::PollType::Poll);
    assert_eq!(gpu.health().validation_errors(), before + 1);
    assert!(!gpu.health().is_poisoned(), "validation errors never poison the device");
}
