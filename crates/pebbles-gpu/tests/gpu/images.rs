//! The image cache (C13).

use super::*;

/// C13: the same image drawn in two frames is uploaded once; frame 2 creates no bind group.
#[test]
#[ignore = "gpu"]
fn same_image_two_frames_uploads_once() {
    let Some((device, queue, mut renderer)) = setup("same_image_two_frames_uploads_once") else { return };
    let image = rgba_image(4, 4, [255, 0, 0, 255]);
    let scene = image_scene(&image, Affine::translate((8.0, 8.0)));
    render_scene(&device, &queue, &mut renderer, &scene, (32, 32), Color::WHITE);
    let first = renderer.last_frame_stats();
    render_scene(&device, &queue, &mut renderer, &scene, (32, 32), Color::WHITE);
    let second = renderer.last_frame_stats();
    assert_eq!((first.images_uploaded, second.images_uploaded), (1, 0));
    assert_eq!(second.bind_groups_created, 0);
}

/// C13: the frame after the app drops its `ImageData` evicts the texture.
#[test]
#[ignore = "gpu"]
fn dropped_image_is_evicted_next_frame() {
    let Some((device, queue, mut renderer)) = setup("dropped_image_is_evicted_next_frame") else { return };
    let image = rgba_image(4, 4, [255, 0, 0, 255]);
    let scene = image_scene(&image, Affine::IDENTITY);
    render_scene(&device, &queue, &mut renderer, &scene, (16, 16), Color::WHITE);
    drop(scene);
    drop(image);
    render_scene(&device, &queue, &mut renderer, &Scene::new(), (16, 16), Color::WHITE);
    assert_eq!(renderer.last_frame_stats().image_evictions, 1);
}

/// C13: a new image of the same size after a drop shows its own pixels (no ABA by pointer).
#[test]
#[ignore = "gpu"]
fn new_image_after_drop_shows_new_pixels() {
    let Some((device, queue, mut renderer)) = setup("new_image_after_drop_shows_new_pixels") else { return };
    let red = rgba_image(8, 8, [255, 0, 0, 255]);
    let scene = image_scene(&red, Affine::IDENTITY);
    render_scene(&device, &queue, &mut renderer, &scene, (8, 8), Color::WHITE);
    drop(scene);
    drop(red);
    let green = rgba_image(8, 8, [0, 255, 0, 255]);
    let rgba = render_scene(
        &device,
        &queue,
        &mut renderer,
        &image_scene(&green, Affine::IDENTITY),
        (8, 8),
        Color::WHITE,
    );
    assert_eq!(pixel(&rgba, 8, 4, 4), [0, 255, 0, 255]);
}

/// C13: a straight-alpha edge [opaque red, transparent black] magnified ×64 over white has no
/// dark fringe — premultiplied at upload, the midpoint is (255, 128, 128) ±4 (straight-alpha
/// filtering gives R ≈ 191).
#[test]
#[ignore = "gpu"]
fn straight_alpha_edge_has_no_dark_fringe() {
    let Some((device, queue, mut renderer)) = setup("straight_alpha_edge_has_no_dark_fringe") else { return };
    let image = peniko::ImageData {
        data: peniko::Blob::from(vec![255, 0, 0, 255, 0, 0, 0, 0]),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 1,
    };
    let rgba = render_scene(
        &device,
        &queue,
        &mut renderer,
        &image_scene(&image, Affine::scale(64.0)),
        (128, 64),
        Color::WHITE,
    );
    let mid = pixel(&rgba, 128, 64, 32);
    let near = |v: u8, want: i32| (i32::from(v) - want).abs() <= 4;
    assert!(near(mid[0], 255) && near(mid[1], 128) && near(mid[2], 128), "midpoint {mid:?}");
}

/// C13: an image wider than the texture limit is downscaled and drawn, with one fallback record.
#[test]
#[ignore = "gpu"]
fn oversize_image_is_downscaled_not_rejected() {
    let Some((device, queue, mut renderer)) = setup("oversize_image_is_downscaled_not_rejected") else {
        return;
    };
    let w = device.limits().max_texture_dimension_2d + 1;
    let image = rgba_image(w, 1, [0, 0, 255, 255]);
    let scene = image_scene(&image, Affine::scale_non_uniform(64.0 / f64::from(w), 16.0));
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 16), Color::WHITE);
    assert_eq!(pixel(&rgba, 64, 32, 8), [0, 0, 255, 255]);
    assert_eq!(renderer.last_frame_stats().unsupported[pebbles_gpu::Unsupported::ImageTooLarge as usize], 1);
}

// ---- R30: sampler quality and extend ----

/// A 2×1 `[red, blue]` image scaled ×50 into a 100×50 target with sampler `s`.
fn two_texels(r: &mut (wgpu::Device, wgpu::Queue, Renderer), s: peniko::ImageSampler) -> Vec<u8> {
    let data = peniko::ImageData {
        data: peniko::Blob::from([[255u8, 0, 0, 255], [0, 0, 255, 255]].concat()),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 1,
    };
    let image = peniko::ImageBrush { image: data, sampler: s };
    let mut scene = Scene::new();
    Painter::new(&mut scene).draw_image(&image, Affine::scale(50.0));
    render_scene(&r.0, &r.1, &mut r.2, &scene, (100, 50), Color::WHITE)
}

fn mixed(px: [u8; 4]) -> bool {
    px[0] > 40 && px[2] > 40
}

#[test]
#[ignore = "gpu"]
fn quality_low_is_nearest() {
    let Some(mut r) = setup("quality_low_is_nearest") else { return };
    let s = peniko::ImageSampler { quality: peniko::ImageQuality::Low, ..Default::default() };
    let px = two_texels(&mut r, s);
    assert!((0..100).all(|x| !mixed(pixel(&px, 100, x, 25))), "Low must not blend");
}

#[test]
#[ignore = "gpu"]
fn quality_medium_blends() {
    let Some(mut r) = setup("quality_medium_blends") else { return };
    let px = two_texels(&mut r, peniko::ImageSampler::default());
    let n = (0..100).filter(|&x| mixed(pixel(&px, 100, x, 25))).count();
    assert!(n >= 10, "only {n} blended pixels");
}

#[test]
#[ignore = "gpu"]
fn extend_changes_edge_taps() {
    let Some(mut r) = setup("extend_changes_edge_taps") else { return };
    let pad = two_texels(&mut r, peniko::ImageSampler::default());
    let s = peniko::ImageSampler { x_extend: peniko::Extend::Repeat, ..Default::default() };
    let repeat = two_texels(&mut r, s);
    // Column 0: Pad taps red twice; Repeat wraps to blue.
    assert_ne!(pixel(&pad, 100, 0, 25), pixel(&repeat, 100, 0, 25));
}
