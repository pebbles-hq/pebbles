//! V07: the same scene renders the same bytes into Rgba8Unorm, Bgra8Unorm and an sRGB texture
//! drawn through its Unorm view (DR-05) — the swapchain path offscreen capture cannot see.

use super::*;

fn scene_x() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(0x80, 0x80, 0x80),
        None,
        &Rect::new(0.0, 0.0, 40.0, 40.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 255, 128),
        None,
        &Rect::new(44.0, 4.0, 84.0, 44.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 128),
        None,
        &Rect::new(60.0, 20.0, 100.0, 60.0),
    );
    p.stroke(&Stroke::new(1.0), Affine::IDENTITY, Color::BLACK, None, &Rect::new(104.5, 4.5, 124.5, 59.5));
    let g = peniko::Gradient::new_linear((0.0, 0.0), (128.0, 0.0))
        .with_stops([Color::WHITE, Color::BLACK].as_slice());
    p.fill(Fill::NonZero, Affine::IDENTITY, &g, None, &Rect::new(0.0, 56.0, 128.0, 64.0));
    s
}

fn render_into(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    format: wgpu::TextureFormat,
    view_format: Option<wgpu::TextureFormat>,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.format"),
        size: wgpu::Extent3d { width: 128, height: 64, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: view_format.as_slice(),
    });
    let view =
        texture.create_view(&wgpu::TextureViewDescriptor { format: view_format, ..Default::default() });
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, view_format.unwrap_or(format), &mut targets);
    renderer.render(target, &scene_x(), &FrameParams::new(Color::WHITE)).expect("render");
    readback(device, queue, &texture)
}

fn near(a: &[u8], b: &[u8], tol: u8, what: &str) {
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        assert!(x.abs_diff(*y) <= tol, "{what}: byte {i} (px {}): {x} vs {y}", i / 4);
    }
}

#[test]
#[ignore = "gpu"]
fn formats_agree_rgba_bgra_srgb_view() {
    let Some((device, queue, mut renderer)) = setup("formats_agree_rgba_bgra_srgb_view") else { return };
    let a = render_into(&device, &queue, &mut renderer, wgpu::TextureFormat::Rgba8Unorm, None);
    let mut b = render_into(&device, &queue, &mut renderer, wgpu::TextureFormat::Bgra8Unorm, None);
    for px in b.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    near(&a, &b, 1, "Bgra8Unorm");
    let gray = pixel(&a, 128, 10, 10);
    near(&gray, &[0x80, 0x80, 0x80, 255], 1, "50% gray is not linearized");
    let gl = device.adapter_info().backend == wgpu::Backend::Gl;
    if gl {
        eprintln!("SKIP sRGB view case: no view_formats on GL (DR-05)");
        return;
    }
    let c = render_into(
        &device,
        &queue,
        &mut renderer,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        Some(wgpu::TextureFormat::Rgba8Unorm),
    );
    near(&a, &c, 1, "Rgba8UnormSrgb via Unorm view");
}
