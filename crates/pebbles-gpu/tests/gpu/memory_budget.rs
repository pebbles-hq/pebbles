//! M17: the renderer's requested GPU bytes (10 §9.3) — the 1080p ceiling (B13) and the
//! worst case by construction under tight budgets (B14, §10.6).

use super::*;

const MIB: u64 = 1 << 20;

fn desktop_like() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(243, 244, 246),
        None,
        &Rect::new(0.0, 0.0, 1920.0, 1080.0),
    );
    for i in 0..24u32 {
        let (x, y) = (40.0 + f64::from(i % 6) * 310.0, 60.0 + f64::from(i / 6) * 250.0);
        let r = Rect::new(x, y, x + 280.0, y + 220.0);
        p.draw_blurred_rounded_rect(Affine::IDENTITY, r, Color::from_rgba8(0, 0, 0, 50), 12.0, 8.0);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::RoundedRect::from_rect(r, 12.0));
        let glyphs: Vec<Glyph> =
            (0..24).map(|k| Glyph { id: 36 + (k + i) % 26, x: k as f32 * 9.0, y: 0.0 }).collect();
        p.draw_glyphs(
            &font,
            16.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::translate((x + 16.0, y + 32.0)),
            None,
            Fill::NonZero,
            glyphs.into_iter(),
        );
    }
    s
}

fn render(
    device: &wgpu::Device,
    renderer: &mut Renderer,
    targets: &mut FrameTargets,
    size: (u32, u32),
    scene: &Scene,
) {
    let texture = target_texture(device, size.0, size.1);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, targets);
    renderer.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render");
}

#[test]
#[ignore = "gpu"]
fn memory_within_budget_at_1080p() {
    let Some((device, _queue, mut renderer)) = setup("memory_within_budget_at_1080p") else { return };
    let mut targets = FrameTargets::new();
    let s = desktop_like();
    for _ in 0..5 {
        render(&device, &mut renderer, &mut targets, (1920, 1080), &s);
    }
    let r = renderer.memory_report();
    println!("{r:#?}\ntotal = {:.2} MiB", r.total_bytes() as f64 / MIB as f64);
    assert!(r.total_bytes() <= 48 * MIB, "{r:?}");
}

#[test]
#[ignore = "gpu"]
fn memory_never_exceeds_ceiling() {
    let Some((device, _queue, mut renderer)) = setup_with_budgets("memory_never_exceeds_ceiling", |b| {
        b.image_cache_bytes = 32 * MIB;
        b.frame_targets_bytes = 64 * MIB;
    }) else {
        return;
    };
    let b = pebbles_gpu::GpuBudgets::defaults(pebbles_gpu::BudgetPlatform::Native, &device.limits());
    // §10.6 worst case by construction, from these budgets.
    let side = u64::from(b.glyph_atlas_max_side);
    let ceiling = 64 * MIB
        + side * side * u64::from(b.glyph_mask_max_layers)
        + side * side * u64::from(b.glyph_color_max_layers) * 4
        + 256 * u64::from(b.gradient_lut_max_rows) * 8
        + 32 * MIB
        + b.layer_pool_bytes
        + b.gpu_mask_bytes
        + 7 * b.arena_max_bytes
        + 64 * 1024;
    let mut ts = [FrameTargets::new(), FrameTargets::new(), FrameTargets::new()];
    let sizes = [(640, 480), (800, 600), (1024, 768)];
    for frame in 0..50u32 {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let mut in_use = 0;
        for k in 0..2u32 {
            // Two new 1024×1024 RGBA images (4 MiB each) per frame.
            let mut px = vec![0u8; 1024 * 1024 * 4];
            px[..8].copy_from_slice(&[frame.to_le_bytes(), k.to_le_bytes()].concat());
            let img = rgba_image(1, 1, [0, 0, 0, 255]);
            let img = peniko::ImageData { data: peniko::Blob::from(px), width: 1024, height: 1024, ..img };
            p.draw_image(
                &peniko::ImageBrush::new(img),
                Affine::scale(0.25) * Affine::translate((f64::from(k) * 1100.0, 0.0)),
            );
            in_use += 4 * MIB;
        }
        let i = (frame % 3) as usize;
        render(&device, &mut renderer, &mut ts[i], sizes[i], &s);
        let total = renderer.memory_report().total_bytes();
        assert!(total <= ceiling + in_use, "frame {frame}: {total} B > {} B", ceiling + in_use);
    }
}
