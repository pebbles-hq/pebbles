//! The image-cache budget (10 §6, M02): upload once, liveness, LRU under budget with pinning,
//! no ABA, oversize downscale. Census-based (debug builds).
#![cfg(debug_assertions)]

use super::*;

const MIB: u64 = 1 << 20;

fn image(w: u32, h: u32, px: [u8; 4]) -> peniko::ImageBrush {
    peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(px.repeat((w * h) as usize)),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: w,
        height: h,
    })
}

fn draw(r: &mut (wgpu::Device, wgpu::Queue, Renderer), images: &[&peniko::ImageBrush]) -> Vec<u8> {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for img in images {
        p.draw_image(*img, Affine::scale(32.0 / f64::from(img.image.width.max(img.image.height))));
    }
    render_scene(&r.0, &r.1, &mut r.2, &s, (32, 32), Color::WHITE)
}

#[test]
#[ignore = "gpu"]
fn image_uploaded_once() {
    let Some(mut r) = setup("image_uploaded_once") else { return };
    let img = image(8, 8, [255, 0, 0, 255]);
    let mut uploads = 0;
    for _ in 0..100 {
        draw(&mut r, &[&img]);
        uploads += r.2.last_frame_stats().images_uploaded;
    }
    assert_eq!(uploads, 1);
}

#[test]
#[ignore = "gpu"]
fn dropped_image_evicted_next_frame() {
    let Some(mut r) = setup("dropped_image_evicted_next_frame") else { return };
    {
        let img = image(8, 8, [255, 0, 0, 255]);
        draw(&mut r, &[&img]);
        assert_eq!(r.2.census().image_entries, 1);
    }
    draw(&mut r, &[]);
    let c = r.2.census();
    assert_eq!((c.image_entries, c.image_bytes), (0, 0));
}

#[test]
#[ignore = "gpu"]
fn budget_enforced_lru() {
    let Some(mut r) = setup_with_budgets("budget_enforced_lru", |b| b.image_cache_bytes = 4 * MIB) else {
        return;
    };
    // 8 distinct 1 MiB images (512×512×4), one new image drawn per frame; all kept alive.
    let images: Vec<_> = (0..8u8).map(|i| image(512, 512, [i * 30, 0, 0, 255])).collect();
    for img in &images {
        draw(&mut r, &[img]);
        let bytes = r.2.census().image_bytes;
        assert!(bytes <= 4 * MIB + MIB, "{bytes} bytes after a frame");
    }
    assert!(r.2.census().image_entries <= 5);
}

/// Pinning (10 §6.3 P3): images drawn this frame survive even when together they exceed the
/// budget; the next frame without them evicts down to the budget. (Since S10 a single image is
/// downscaled below ¼ of the budget, so "over budget" takes several images.)
#[test]
#[ignore = "gpu"]
fn in_use_over_budget_survives_frame() {
    let Some(mut r) =
        setup_with_budgets("in_use_over_budget_survives_frame", |b| b.image_cache_bytes = 4 * MIB)
    else {
        return;
    };
    // Five ≈1 MiB images (511² × 4 B, just under ¼ of the budget: not downscaled) in one frame.
    let images: Vec<_> = (0..5u8).map(|i| image(511, 511, [0, 0, 200 + i, 255])).collect();
    let refs: Vec<&peniko::ImageBrush> = images.iter().collect();
    let px = draw(&mut r, &refs);
    assert_eq!(pixel(&px, 32, 16, 16), [0, 0, 204, 255], "the last image renders");
    let c = r.2.census();
    assert_eq!(c.image_entries, 5, "all pinned for the frame they were drawn");
    assert!(c.image_bytes > 4 * MIB);
    draw(&mut r, &[]);
    let c = r.2.census();
    assert!(c.image_bytes <= 4 * MIB && c.image_entries == 3, "evicted down to the budget: {c:?}");
}

#[test]
#[ignore = "gpu"]
fn no_aba_after_drop() {
    let Some(mut r) = setup("no_aba_after_drop") else { return };
    {
        let a = image(8, 8, [255, 0, 0, 255]);
        draw(&mut r, &[&a]);
    }
    let b = image(8, 8, [0, 0, 255, 255]);
    let px = draw(&mut r, &[&b]);
    assert_eq!(pixel(&px, 32, 16, 16), [0, 0, 255, 255]);
}

#[test]
#[ignore = "gpu"]
fn oversized_image_downscaled() {
    let Some(mut r) = setup("oversized_image_downscaled") else { return };
    let max = r.0.limits().max_texture_dimension_2d;
    let img = image(max + 10, 10, [0, 255, 0, 255]);
    let scope = r.0.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut s = Scene::new();
    Painter::new(&mut s).draw_image(&img, Affine::scale_non_uniform(64.0 / f64::from(max + 10), 0.8));
    let px = render_scene(&r.0, &r.1, &mut r.2, &s, (64, 8), Color::WHITE);
    assert!(pollster::block_on(scope.pop()).is_none(), "validation error");
    assert_eq!(pixel(&px, 64, 32, 4), [0, 255, 0, 255]);
}
