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

// ---- R31: arbitrary image transforms ----

const QUAD_COLORS: [[u8; 4]; 4] = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 0, 255]];

/// A 4×4 image with one color per 2×2 quadrant: TL red, TR green, BL blue, BR yellow.
fn quadrants() -> peniko::ImageBrush {
    let px: Vec<u8> = (0..16u32)
        .flat_map(|i| {
            let (x, y) = (i % 4, i / 4);
            QUAD_COLORS[(usize::from(y >= 2) * 2) + usize::from(x >= 2)]
        })
        .collect();
    peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(px),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 4,
        height: 4,
    })
}

fn draw_quadrants(r: &mut (wgpu::Device, wgpu::Queue, Renderer), t: Affine, size: (u32, u32)) -> Vec<u8> {
    let mut s = Scene::new();
    Painter::new(&mut s).draw_image(&quadrants().with_quality(peniko::ImageQuality::Low), t);
    render_scene(&r.0, &r.1, &mut r.2, &s, size, Color::WHITE)
}

/// The color expected at device pixel center `(x, y)` for image transform `t`, or `None` near an
/// edge between quadrants / outside.
fn expected(t: Affine, x: u32, y: u32) -> Option<[u8; 4]> {
    let p = t.inverse() * kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
    let near = |v: f64| (v - 2.0).abs() < 0.3 || v < 0.3 || v > 3.7;
    if !(0.0..4.0).contains(&p.x) || !(0.0..4.0).contains(&p.y) || near(p.x) || near(p.y) {
        return None;
    }
    Some(QUAD_COLORS[usize::from(p.y >= 2.0) * 2 + usize::from(p.x >= 2.0)])
}

fn assert_quadrants(px: &[u8], w: u32, h: u32, t: Affine) {
    let mut checked = 0;
    for y in 0..h {
        for x in 0..w {
            if let Some(want) = expected(t, x, y) {
                let got = pixel(px, w, x, y);
                assert!(
                    got.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2),
                    "({x},{y}) {got:?} want {want:?}"
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "only {checked} pixels checked");
}

#[test]
#[ignore = "gpu"]
fn image_translate_scale() {
    let Some(mut r) = setup("image_translate_scale") else { return };
    let t = Affine::translate((8.0, 8.0)) * Affine::scale(16.0);
    assert_quadrants(&draw_quadrants(&mut r, t, (80, 80)), 80, 80, t);
}

#[test]
#[ignore = "gpu"]
fn image_nonuniform_scale() {
    let Some(mut r) = setup("image_nonuniform_scale") else { return };
    let t = Affine::scale_non_uniform(16.0, 8.0);
    assert_quadrants(&draw_quadrants(&mut r, t, (64, 32)), 64, 32, t);
}

#[test]
#[ignore = "gpu"]
fn image_mirrored() {
    let Some(mut r) = setup("image_mirrored") else { return };
    let t = Affine::translate((64.0, 0.0)) * Affine::scale_non_uniform(-16.0, 16.0);
    let px = draw_quadrants(&mut r, t, (64, 64));
    assert_quadrants(&px, 64, 64, t);
    // Left/right swapped: the top-left device quadrant is green.
    assert_eq!(pixel(&px, 64, 16, 16), [0, 255, 0, 255]);
}

#[test]
#[ignore = "gpu"]
fn image_rotated_30() {
    let Some(mut r) = setup("image_rotated_30") else { return };
    let t = Affine::rotate_about(std::f64::consts::FRAC_PI_6, kurbo::Point::new(48.0, 48.0))
        * Affine::translate((16.0, 16.0))
        * Affine::scale(16.0);
    let px = draw_quadrants(&mut r, t, (96, 96));
    assert_quadrants(&px, 96, 96, t);
    // Ink bbox = the transformed corners' bbox ± 1 px; outside the rotated quad: white.
    let corners = [(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (4.0, 4.0)].map(|c| t * kurbo::Point::from(c));
    let (x0, x1) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
    let (y0, y1) = corners.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
    let ink: Vec<(u32, u32)> = (0..96u32)
        .flat_map(|y| (0..96u32).map(move |x| (x, y)))
        .filter(|&(x, y)| !is_white(pixel(&px, 96, x, y)))
        .collect();
    let ix0 = ink.iter().map(|p| p.0).min().unwrap();
    let ix1 = ink.iter().map(|p| p.0).max().unwrap() + 1;
    let iy0 = ink.iter().map(|p| p.1).min().unwrap();
    let iy1 = ink.iter().map(|p| p.1).max().unwrap() + 1;
    assert!(
        (f64::from(ix0) - x0).abs() <= 1.0 && (f64::from(ix1) - x1).abs() <= 1.0,
        "x {ix0}..{ix1} vs {x0}..{x1}"
    );
    assert!(
        (f64::from(iy0) - y0).abs() <= 1.0 && (f64::from(iy1) - y1).abs() <= 1.0,
        "y {iy0}..{iy1} vs {y0}..{y1}"
    );
    let inv = t.inverse();
    for &(x, y) in &ink {
        let q = inv * kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
        assert!(
            (-0.2..=4.2).contains(&q.x) && (-0.2..=4.2).contains(&q.y),
            "ink outside the quad at ({x},{y})"
        );
    }
}

#[test]
#[ignore = "gpu"]
fn image_skewed() {
    let Some(mut r) = setup("image_skewed") else { return };
    let t = Affine::translate((4.0, 4.0)) * Affine::skew(0.5, 0.0) * Affine::scale(16.0);
    let px = draw_quadrants(&mut r, t, (112, 72));
    assert_quadrants(&px, 112, 72, t);
}

// ---- R32: ImageSampler::alpha ----

fn solid_image(px: [u8; 4], alpha_type: peniko::ImageAlphaType) -> peniko::ImageBrush {
    peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(px.repeat(16)),
        format: peniko::ImageFormat::Rgba8,
        alpha_type,
        width: 4,
        height: 4,
    })
}

fn center_of(r: &mut (wgpu::Device, wgpu::Queue, Renderer), image: &peniko::ImageBrush) -> [u8; 4] {
    let mut s = Scene::new();
    Painter::new(&mut s).draw_image(image, Affine::translate((4.0, 4.0)) * Affine::scale(4.0));
    let px = render_scene(&r.0, &r.1, &mut r.2, &s, (24, 24), Color::WHITE);
    pixel(&px, 24, 12, 12)
}

fn near(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
}

#[test]
#[ignore = "gpu"]
fn image_alpha_half() {
    let Some(mut r) = setup("image_alpha_half") else { return };
    let img = solid_image([255, 0, 0, 255], peniko::ImageAlphaType::Alpha).with_alpha(0.5);
    let c = center_of(&mut r, &img);
    assert!(near(c, [255, 127, 127, 255]), "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn image_alpha_zero_draws_nothing() {
    let Some(mut r) = setup("image_alpha_zero_draws_nothing") else { return };
    let img = solid_image([255, 0, 0, 255], peniko::ImageAlphaType::Alpha).with_alpha(0.0);
    assert_eq!(center_of(&mut r, &img), [255, 255, 255, 255]);
    assert_eq!(r.2.last_frame_stats().draw_calls, 0);
}

#[test]
#[ignore = "gpu"]
fn premultiplied_image_alpha_half() {
    let Some(mut r) = setup("premultiplied_image_alpha_half") else { return };
    // Premultiplied (100, 50, 20, 128) × 0.5 = (50, 25, 10, 64) over white:
    // out = src + 255 · (1 − 64/255).
    let img = solid_image([100, 50, 20, 128], peniko::ImageAlphaType::AlphaPremultiplied).with_alpha(0.5);
    let c = center_of(&mut r, &img);
    let over = |s: f64| (s + 255.0 * (1.0 - 64.0 / 255.0)).round() as u8;
    let want = [over(50.0), over(25.0), over(10.0), 255];
    assert!(near(c, want), "{c:?} want {want:?}");
}

// ---- R35: mipmapped minification ----

/// A 512² one-pixel black/white checkerboard drawn at 64² with `High`: every pixel is the
/// average gray 128 ±3 (no moiré from sampling level 0).
#[test]
#[ignore = "gpu"]
fn downscaled_checkerboard_has_no_moire() {
    let Some(mut r) = setup("downscaled_checkerboard_has_no_moire") else { return };
    let px: Vec<u8> = (0..512u32 * 512)
        .flat_map(|i| if (i % 512 + i / 512) % 2 == 0 { [0u8, 0, 0, 255] } else { [255u8, 255, 255, 255] })
        .collect();
    let image = peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(px),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 512,
        height: 512,
    })
    .with_quality(peniko::ImageQuality::High);
    let mut s = Scene::new();
    Painter::new(&mut s).draw_image(&image, Affine::scale(0.125));
    let out = render_scene(&r.0, &r.1, &mut r.2, &s, (64, 64), Color::WHITE);
    for y in 0..64 {
        for x in 0..64 {
            let p = pixel(&out, 64, x, y);
            assert!(p[..3].iter().all(|c| c.abs_diff(128) <= 3), "({x},{y}) {p:?}");
        }
    }
}

// ---- S10: oversize downscale at upload ----

/// S10 AC4: a 10000×300 image renders with no validation error (run under PEBBLES_GPU_STRICT=1).
#[test]
#[ignore = "gpu"]
fn huge_image_renders_without_validation_error() {
    let Some((device, queue, mut r)) = setup("huge_image_renders_without_validation_error") else { return };
    let image = peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from([255u8, 0, 0, 255].repeat(10_000 * 300)),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 10_000,
        height: 300,
    });
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut s = Scene::new();
    Painter::new(&mut s).draw_image(&image, Affine::scale(0.1));
    let px = render_scene(&device, &queue, &mut r, &s, (1000, 30), Color::WHITE);
    assert!(pollster::block_on(scope.pop()).is_none(), "validation error");
    assert_eq!(pixel(&px, 1000, 500, 15), [255, 0, 0, 255]);
}

// ---- R36: nine-slice ----

/// R36: drawn at integer positions, the four corner texels of a nine-slice are exact (R19) and
/// the center stretches.
#[test]
#[ignore = "gpu"]
fn nine_slice_corners_are_exact() {
    let Some(mut r) = setup("nine_slice_corners_are_exact") else { return };
    // 8×8 image: 2-px corners red/green/blue/yellow, border gray, center white.
    let px: Vec<u8> = (0..64u32)
        .flat_map(|i| {
            let (x, y) = (i % 8, i / 8);
            match (x < 2, x >= 6, y < 2, y >= 6) {
                (true, _, true, _) => [255, 0, 0, 255],
                (_, true, true, _) => [0, 255, 0, 255],
                (true, _, _, true) => [0, 0, 255, 255],
                (_, true, _, true) => [255, 255, 0, 255],
                (false, false, false, false) => [255, 255, 255, 255],
                _ => [128, 128, 128, 255],
            }
        })
        .collect();
    let image = peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(px),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 8,
        height: 8,
    });
    let mut s = Scene::new();
    Painter::new(&mut s).draw_image_nine(
        &image,
        Rect::new(2.0, 2.0, 6.0, 6.0),
        Rect::new(4.0, 4.0, 60.0, 28.0),
        Affine::IDENTITY,
    );
    let out = render_scene(&r.0, &r.1, &mut r.2, &s, (64, 32), Color::from_rgba8(0, 0, 0, 255));
    assert_eq!(pixel(&out, 64, 4, 4), [255, 0, 0, 255]);
    assert_eq!(pixel(&out, 64, 59, 4), [0, 255, 0, 255]);
    assert_eq!(pixel(&out, 64, 4, 27), [0, 0, 255, 255]);
    assert_eq!(pixel(&out, 64, 59, 27), [255, 255, 0, 255]);
    assert_eq!(pixel(&out, 64, 32, 16), [255, 255, 255, 255], "stretched center");
    assert_eq!(pixel(&out, 64, 3, 4), [0, 0, 0, 255], "nothing outside dst");
}

// ---- R37: dynamic images ----

fn draw_dynamic(
    r: &mut (wgpu::Device, wgpu::Queue, Renderer),
    img: &pebbles_render::paint::DynamicImage,
) -> Vec<u8> {
    let mut s = Scene::new();
    Painter::new(&mut s).draw_dynamic_image(img, peniko::ImageQuality::Low, 1.0, Affine::IDENTITY);
    render_scene(&r.0, &r.1, &mut r.2, &s, (img.width(), img.height()), Color::WHITE)
}

/// R37: a new generation with 10 dirty rows uploads exactly those rows.
#[test]
#[ignore = "gpu"]
fn dynamic_update_uploads_only_dirty_rows() {
    let Some(mut r) = setup("dynamic_update_uploads_only_dirty_rows") else { return };
    let mut img = pebbles_render::paint::DynamicImage::new(256, 256);
    draw_dynamic(&mut r, &img);
    let mut px = vec![0u8; 256 * 256 * 4];
    for row in 100..110 {
        for x in 0..256 {
            px[(row * 256 + x) * 4..][..4].copy_from_slice(&[0, 0, 255, 255]);
        }
    }
    img.update(peniko::Blob::from(px), Some(pebbles_render::paint::IRect { x: 0, y: 100, w: 256, h: 10 }));
    let out = draw_dynamic(&mut r, &img);
    assert_eq!(r.2.last_frame_stats().texture_bytes_uploaded, 256 * 4 * 10);
    assert_eq!(pixel(&out, 256, 50, 105), [0, 0, 255, 255]);
    assert_eq!(pixel(&out, 256, 50, 50), [255, 255, 255, 255], "transparent elsewhere");
}

/// R37: 60 updates of one dynamic image reuse one texture.
#[test]
#[ignore = "gpu"]
fn dynamic_image_reuses_one_texture() {
    let Some(mut r) = setup("dynamic_image_reuses_one_texture") else { return };
    let mut img = pebbles_render::paint::DynamicImage::new(32, 32);
    let mut created = 0;
    for i in 0..60u8 {
        img.update(peniko::Blob::from([i, 0, 0, 255].repeat(32 * 32)), None);
        let out = draw_dynamic(&mut r, &img);
        created += r.2.last_frame_stats().textures_created;
        assert_eq!(pixel(&out, 32, 16, 16), [i, 0, 0, 255], "frame {i} shows its own pixels");
    }
    assert_eq!(created, 1);
}

// ---- R63 meshes / R64 sprite batches ----

#[test]
#[ignore = "gpu"]
fn vertex_colors_interpolate() {
    use pebbles_render::paint::{Mesh, VertexColorMode, VertexMode};
    let Some((device, queue, mut r)) = setup("vertex_colors_interpolate") else { return };
    let mesh = std::sync::Arc::new(
        Mesh::new(
            VertexMode::Triangles,
            vec![[0.0, 0.0], [63.0, 0.0], [0.0, 63.0]],
            Some(vec![
                Color::from_rgba8(255, 0, 0, 255),
                Color::from_rgba8(0, 255, 0, 255),
                Color::from_rgba8(0, 0, 255, 255),
            ]),
            None,
            None,
        )
        .unwrap(),
    );
    let mut s = Scene::new();
    Painter::new(&mut s).draw_vertices(&mesh, Color::WHITE, VertexColorMode::Modulate, Affine::IDENTITY);
    let px = render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE);
    // Pixel (21, 21)'s center (21.5, 21.5), next to the centroid: barycentric weights.
    let c = pixel(&px, 64, 21, 21);
    let (u, v) = (21.5 / 63.0, 21.5 / 63.0);
    let want = [255.0 * (1.0 - u - v), 255.0 * u, 255.0 * v];
    assert!((0..3).all(|i| (f64::from(c[i]) - want[i]).abs() <= 3.0), "centroid {c:?} vs {want:?}");
    assert!(is_white(pixel(&px, 64, 60, 60)), "outside the triangle");
}

#[test]
#[ignore = "gpu"]
fn textured_mesh_modulates() {
    use pebbles_render::paint::{Mesh, VertexColorMode, VertexMode};
    let Some((device, queue, mut r)) = setup("textured_mesh_modulates") else { return };
    let img =
        peniko::ImageBrush::new(rgba_image(2, 2, [0, 255, 0, 255])).with_quality(peniko::ImageQuality::Low);
    let mesh = std::sync::Arc::new(
        Mesh::new(
            VertexMode::TriangleStrip,
            vec![[0.0, 0.0], [64.0, 0.0], [0.0, 64.0], [64.0, 64.0]],
            Some(vec![Color::from_rgba8(255, 255, 255, 255); 4]),
            Some(vec![[0.0, 0.0], [2.0, 0.0], [0.0, 2.0], [2.0, 2.0]]),
            None,
        )
        .unwrap(),
    );
    let mut s = Scene::new();
    let half = img.clone().with_alpha(0.5);
    Painter::new(&mut s).draw_vertices(&mesh, &half, VertexColorMode::Modulate, Affine::IDENTITY);
    let px = render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE);
    let c = pixel(&px, 64, 32, 32);
    assert!((i32::from(c[0]) - 128).abs() <= 2 && c[1] == 255 && (i32::from(c[2]) - 128).abs() <= 2, "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn ten_thousand_sprites_one_draw() {
    use pebbles_render::paint::Sprite;
    let Some((device, queue, mut r)) = setup("ten_thousand_sprites_one_draw") else { return };
    let img = peniko::ImageBrush::new(rgba_image(4, 4, [255, 0, 0, 255]));
    let sprites: std::sync::Arc<[Sprite]> = (0..10_000)
        .map(|i| {
            Sprite::from_components(
                Rect::new(0.0, 0.0, 4.0, 4.0),
                0.0,
                0.5,
                (0.0, 0.0),
                ((i % 100) as f32 * 2.0, (i / 100) as f32 * 2.0),
            )
        })
        .collect();
    let mut s = Scene::new();
    Painter::new(&mut s).draw_atlas(&img, &sprites, Affine::IDENTITY);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.sprites"),
        size: wgpu::Extent3d { width: 200, height: 200, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
    let stats = r.render(target, &s, &FrameParams::new(Color::WHITE)).expect("render");
    assert_eq!(stats.draw_calls, 1, "one instanced draw for the batch");
    let px = readback(&device, &queue, &texture);
    assert_eq!(pixel(&px, 200, 101, 101), [255, 0, 0, 255]);
}

#[test]
#[ignore = "gpu"]
fn sprite_color_modulates() {
    use pebbles_render::paint::Sprite;
    let Some((device, queue, mut r)) = setup("sprite_color_modulates") else { return };
    let img = peniko::ImageBrush::new(rgba_image(8, 8, [255, 255, 255, 255]));
    let mut sp = Sprite::from_components(Rect::new(0.0, 0.0, 8.0, 8.0), 0.0, 4.0, (0.0, 0.0), (16.0, 16.0));
    sp.color = Some(Color::from_rgba8(0, 0, 255, 255));
    let sprites: std::sync::Arc<[Sprite]> = vec![sp].into();
    let mut s = Scene::new();
    Painter::new(&mut s).draw_atlas(&img, &sprites, Affine::IDENTITY);
    let px = render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE);
    assert_eq!(pixel(&px, 64, 32, 32), [0, 0, 255, 255]);
    assert!(is_white(pixel(&px, 64, 8, 8)));
}
