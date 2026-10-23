//! Opt-in GPU timestamps (M22): absent by default, present after a few frames when enabled
//! and supported (lavapipe supports them).

use super::*;

fn frame(
    device: &wgpu::Device,
    renderer: &mut Renderer,
    targets: &mut FrameTargets,
) -> pebbles_gpu::FrameStats {
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(0.0, 0.0, 32.0, 32.0),
    );
    let texture = target_texture(device, 64, 64);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, targets);
    renderer.render(target, &scene, &FrameParams::new(Color::WHITE)).expect("render")
}

#[test]
#[ignore = "gpu"]
fn timing_absent_is_none() {
    let Some((device, _queue, mut renderer)) = setup("timing_absent_is_none") else { return };
    let mut targets = FrameTargets::new();
    for _ in 0..5 {
        assert_eq!(frame(&device, &mut renderer, &mut targets).gpu_us, None);
    }
}

#[test]
#[ignore = "gpu"]
fn timing_present_when_supported() {
    let init = GpuInit { gpu_timing: true, ..GpuInit::headless() };
    let mut ctx = pollster::block_on(GpuContext::new(&init));
    let Ok(id) = pollster::block_on(ctx.request_device(None)) else { return };
    let gpu = ctx.device(id).expect("device");
    if !gpu.device().features().contains(wgpu::Features::TIMESTAMP_QUERY) {
        println!("skipped: no TIMESTAMP_QUERY");
        return;
    }
    let device = gpu.device().clone();
    let mut renderer = Renderer::new(gpu, id);
    let mut targets = FrameTargets::new();
    let mut last = None;
    for _ in 0..5 {
        last = frame(&device, &mut renderer, &mut targets).gpu_us;
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
    }
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(last.is_some(), "gpu_us after 5 frames");
}
