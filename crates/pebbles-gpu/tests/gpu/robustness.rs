//! S13 (device loss + rebuild), S20 (3 000-frame rotation) and S43 (exhaustion): the renderer
//! survives loss, stays byte-stable over long runs and stays inside its budgets under floods.

use pebbles_gpu::{CaptureError, GpuError, OffscreenRenderer, Unsupported};

use super::*;

fn offscreen(name: &str) -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new() {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("SKIP {name}: {e}");
            None
        }
    }
}

fn font() -> peniko::FontData {
    peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0)
}

fn glyph_run(p: &mut Painter<'_>, text_seed: u32, size: f32, at: (f64, f64)) {
    let glyphs: Vec<Glyph> = (0..12)
        .map(|k| Glyph { id: 36 + (k * 7 + text_seed) % 40, x: k as f32 * size * 0.6, y: 0.0 })
        .collect();
    p.draw_glyphs(
        &font(),
        size,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::translate(at),
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

fn image(w: u32, h: u32, seed: u32) -> peniko::ImageBrush {
    let px: Vec<u8> = (0..w * h)
        .flat_map(|k| [(k + seed).to_le_bytes()[0], (k / 7).to_le_bytes()[0], seed.to_le_bytes()[0], 255])
        .collect();
    peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(px),
        ..rgba_image(w, h, [0, 0, 0, 255])
    })
}

fn fixed_scene() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        Rect::new(10.0, 14.0, 90.0, 54.0),
        Color::from_rgba8(0, 0, 0, 80),
        8.0,
        4.0,
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(59, 130, 246),
        None,
        &Rect::new(10.0, 10.0, 90.0, 50.0),
    );
    glyph_run(&mut p, 3, 14.0, (12.0, 72.0));
    p.draw_image(&image(16, 16, 1), Affine::translate((100.0, 10.0)));
    s
}

/// S13 step 2.
#[test]
#[ignore = "gpu"]
fn destroy_fires_loss_and_rebuild_renders_identically() {
    let Some(mut r) = offscreen("destroy_fires_loss") else { return };
    let scene = fixed_scene();
    let a = r.render_rgba(&scene, (128, 96), Color::WHITE, 1.0).expect("render");
    r.device().destroy();
    let mut lost = false;
    for _ in 0..100 {
        let _ = r.device().poll(wgpu::PollType::Poll);
        if matches!(
            r.render_rgba(&scene, (128, 96), Color::WHITE, 1.0),
            Err(CaptureError::Gpu(GpuError::DeviceLost))
        ) {
            lost = true;
            break;
        }
    }
    assert!(lost, "a destroyed device reports DeviceLost");
    let Some(mut fresh) = offscreen("rebuild") else { return };
    let b = fresh.render_rgba(&scene, (128, 96), Color::WHITE, 1.0).expect("rebuilt render");
    assert_eq!(a, b, "the rebuilt stack renders identically");
}

/// S13 step 3.
#[test]
#[ignore = "gpu"]
fn old_context_loss_does_not_poison_new_one() {
    let (Some(a), Some(mut b)) = (offscreen("poison A"), offscreen("poison B")) else { return };
    a.device().destroy();
    for _ in 0..10 {
        let _ = a.device().poll(wgpu::PollType::Poll);
    }
    assert!(b.render_rgba(&fixed_scene(), (64, 48), Color::WHITE, 1.0).is_ok(), "B is untouched by A's loss");
}

/// S20: 10 scenes in rotation, 3 000 frames at two alternating sizes, 50 strings and 20 images
/// in the working set: rotations 2 and 3 are byte-identical and the census settles.
#[test]
#[ignore = "gpu"]
fn rotating_scenes_are_byte_stable() {
    let Some(mut r) = offscreen("rotating_scenes_are_byte_stable") else { return };
    let images: Vec<_> = (0..20).map(|i| image(32, 32, i)).collect();
    let scenes: Vec<Scene> = (0..10u32)
        .map(|k| {
            let mut s = Scene::new();
            let mut p = Painter::new(&mut s);
            for j in 0..5u32 {
                glyph_run(
                    &mut p,
                    k * 5 + j,
                    12.0 + f32::from(u8::try_from(j).unwrap_or(0)) * 2.0,
                    (4.0, 16.0 + f64::from(j) * 18.0),
                );
            }
            for j in 0..2 {
                p.draw_image(
                    &images[((k * 2 + j) % 20) as usize],
                    Affine::translate((150.0 + f64::from(j) * 40.0, 10.0)),
                );
            }
            p.draw_blurred_rounded_rect(
                Affine::IDENTITY,
                Rect::new(150.0, 60.0, 230.0, 100.0),
                Color::from_rgba8(0, 0, 0, 60),
                6.0,
                3.0,
            );
            let g = peniko::Gradient::new_linear((0.0, 0.0), (240.0, 0.0))
                .with_stops([Color::from_rgb8(99, 102, 241), Color::from_rgb8(236, 72, 153)].as_slice());
            p.push_layer(
                Fill::NonZero,
                peniko::Mix::Normal,
                1.0,
                Affine::IDENTITY,
                &kurbo::RoundedRect::new(0.0, 110.0, 240.0, 150.0, 10.0),
            );
            p.fill(Fill::NonZero, Affine::IDENTITY, &g, None, &Rect::new(0.0, 110.0, 240.0, 150.0));
            p.pop_layer();
            s
        })
        .collect();
    let sizes = [(240, 160), (256, 176)];
    let mut rot2 = Vec::new();
    let mut census2: Option<pebbles_gpu::GpuCensus> = None;
    for frame in 0..3000usize {
        let k = frame % 10;
        let size = sizes[(frame / 10) % 2];
        let img = r.render_rgba(&scenes[k], size, Color::WHITE, 1.0).expect("render");
        let rotation = frame / 1000;
        // Same scene, same size: the first full cycle of 20 frames of rotations 2 and 3.
        if (1000..1020).contains(&frame) {
            rot2.push(img);
        } else if (2000..2020).contains(&frame) {
            assert_eq!(
                rot2[frame - 2000],
                img,
                "frame {frame} (rotation {rotation}) differs from rotation 2"
            );
        }
        #[cfg(debug_assertions)]
        if frame == 2000 {
            census2 = Some(r.census());
        }
    }
    #[cfg(debug_assertions)]
    assert_eq!(census2, Some(r.census()), "census at frame 2 000 == frame 3 000");
    let _ = &mut census2;
}

/// S43 step 1: 300 distinct 1024×1024 images over 30 frames stay within the image budget.
#[test]
#[ignore = "gpu"]
fn image_flood_stays_within_budget() {
    let Some((device, queue, mut renderer)) = setup("image_flood_stays_within_budget") else { return };
    let budget = pebbles_gpu::GpuBudgets::defaults(pebbles_gpu::BudgetPlatform::Native, &device.limits())
        .image_cache_bytes;
    for frame in 0..30u32 {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for i in 0..10u32 {
            p.draw_image(
                &image(1024, 1024, frame * 10 + i),
                Affine::scale(0.05) * Affine::translate((f64::from(i) * 1100.0, 0.0)),
            );
        }
        render_scene(&device, &queue, &mut renderer, &s, (600, 64), Color::WHITE);
        let st = renderer.last_frame_stats();
        // In-use images of this frame may overshoot (soft budget, 10 §6): at most this frame's 10.
        assert!(
            st.image_cache_bytes <= budget + 10 * 4 * 1024 * 1024,
            "frame {frame}: {} B",
            st.image_cache_bytes
        );
    }
}

/// S43 step 2: oversize and extreme-aspect images render, downscaled and recorded.
#[test]
#[ignore = "gpu"]
fn huge_images_downscale() {
    let Some((device, queue, mut renderer)) = setup("huge_images_downscale") else { return };
    for (w, h) in [(16384u32, 64u32), (64, 16384), (9000, 9000)] {
        let mut s = Scene::new();
        Painter::new(&mut s).draw_image(&image(w, h, 7), Affine::scale(64.0 / f64::from(w.max(h))));
        render_scene(&device, &queue, &mut renderer, &s, (64, 64), Color::WHITE);
        let st = renderer.last_frame_stats();
        assert!(
            st.unsupported[Unsupported::ImageTooLarge as usize] >= 1 || st.images_uploaded == 1,
            "{w}x{h}: {st:?}"
        );
    }
}

/// S43 step 3: one run per font size 8..=2000 px (step 8) keeps the atlas within its caps.
#[test]
#[ignore = "gpu"]
fn glyph_size_flood() {
    let Some((device, queue, mut renderer)) = setup("glyph_size_flood") else { return };
    let caps = pebbles_gpu::GpuBudgets::defaults(pebbles_gpu::BudgetPlatform::Native, &device.limits());
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for size in (8..=2000).step_by(8) {
        #[allow(clippy::cast_precision_loss)]
        glyph_run(&mut p, size, size as f32, (10.0, 10.0 + f64::from(size % 200)));
    }
    render_scene(&device, &queue, &mut renderer, &s, (512, 512), Color::WHITE);
    #[cfg(debug_assertions)]
    {
        let c = renderer.census();
        assert!(
            c.glyph_mask_side <= caps.glyph_atlas_max_side
                && c.glyph_mask_layers <= caps.glyph_mask_max_layers.max(2),
            "{c:?}"
        );
        assert!(c.glyph_color_layers <= caps.glyph_color_max_layers.max(2), "{c:?}");
    }
    let _ = caps;
}

/// S43 step 4: 100 nested 0.9-opacity layers stay within `max_layer_targets`.
#[test]
#[ignore = "gpu"]
fn deep_layer_nesting() {
    let Some((device, queue, mut renderer)) = setup("deep_layer_nesting") else { return };
    let max = renderer.limits().max_layer_targets;
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for i in 0..100 {
        let r = Rect::new(f64::from(i), f64::from(i), 300.0 - f64::from(i), 300.0 - f64::from(i));
        p.push_layer(Fill::NonZero, peniko::Mix::Normal, 0.9, Affine::IDENTITY, &r);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(10, (i * 2) as u8, 200), None, &r);
    }
    for _ in 0..100 {
        p.pop_layer();
    }
    let mut targets = FrameTargets::new();
    let texture = target_texture(&device, 300, 300);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    renderer
        .render(
            RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets),
            &s,
            &FrameParams::new(Color::WHITE),
        )
        .expect("render");
    let _ = queue;
    assert!(
        u32::try_from(targets.layer_pool_targets()).unwrap_or(u32::MAX) <= max,
        "{} pooled > {max}",
        targets.layer_pool_targets()
    );
}

/// S56 AC2 (negative half): with `PEBBLES_INJECT_VK_VALIDATION_ERROR=1` (CI's dispatch-only
/// `inject_vk_validation_error` input on the lavapipe lane, `WGPU_VALIDATION=1`), make one raw
/// Vulkan call the Khronos validation layer rejects (`VUID-VkBufferCreateInfo-size-00912`: a
/// zero-sized buffer). The layer reports through wgpu-hal's debug messenger → `log::error!` →
/// the S56 bridge, so [`support::GpuTest`]'s drop check fails this test — proving a validation-
/// layer error turns the GPU lane red. Without the variable the test does nothing.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "gpu"]
fn injected_vulkan_validation_error_fails_the_test() {
    if std::env::var("PEBBLES_INJECT_VK_VALIDATION_ERROR").as_deref() != Ok("1") {
        return;
    }
    let Some(r) = support::gpu_or_skip("injected_vulkan_validation_error") else { return };
    // SAFETY: the hal device outlives this block; the buffer is destroyed before it ends, and the
    // invalid create-info is the point of the test (the driver tolerates size 0 on lavapipe).
    unsafe {
        let hal = r
            .device()
            .as_hal::<wgpu::hal::api::Vulkan>()
            .expect("PEBBLES_INJECT_VK_VALIDATION_ERROR needs the Vulkan backend");
        let raw = hal.raw_device();
        let info = ash::vk::BufferCreateInfo::default()
            .size(0)
            .usage(ash::vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(ash::vk::SharingMode::EXCLUSIVE);
        if let Ok(buffer) = raw.create_buffer(&info, None) {
            raw.destroy_buffer(buffer, None);
        }
    }
    eprintln!(
        "injected one zero-sized vkCreateBuffer; backend errors so far: {}",
        pebbles_gpu::backend_error_count()
    );
}
