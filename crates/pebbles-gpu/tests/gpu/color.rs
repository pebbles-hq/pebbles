//! The color contract and layer fallbacks (C08, C10, C17).

use super::*;

/// C08: clearing `#808080` stores exactly 128 (sRGB-encoded, no linearization).
#[test]
#[ignore = "gpu"]
fn clear_is_exact_srgb() {
    let Some((device, queue, mut renderer)) = setup("clear_is_exact_srgb") else { return };
    let scene = Scene::new();
    let rgba = render_scene(
        &device,
        &queue,
        &mut renderer,
        &scene,
        (64, 64),
        Color::from_rgba8(0x80, 0x80, 0x80, 0xff),
    );
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for px in rgba.as_chunks::<4>().0 {
        assert!(
            (px[0] as i32 - 128).abs() <= 1
                && (px[1] as i32 - 128).abs() <= 1
                && (px[2] as i32 - 128).abs() <= 1,
            "pixel {px:?}"
        );
    }
}

/// C08: 50 % white over black blends in gamma space → 128 (linear would give ≈188).
#[test]
#[ignore = "gpu"]
fn half_alpha_white_over_black_is_gamma_space() {
    let Some((device, queue, mut renderer)) = setup("half_alpha_white_over_black_is_gamma_space") else {
        return;
    };
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 255, 255, 128),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::BLACK);
    let c = pixel(&rgba, 64, 32, 32);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(
        (c[0] as i32 - 128).abs() <= 1 && (c[1] as i32 - 128).abs() <= 1 && (c[2] as i32 - 128).abs() <= 1,
        "pixel {c:?}"
    );
}

/// C08: clearing `rgba(255,0,0,0.5)` premultiplies → readback `(128,0,0,128)`.
#[test]
#[ignore = "gpu"]
fn premultiplied_clear_with_alpha() {
    let Some((device, queue, mut renderer)) = setup("premultiplied_clear_with_alpha") else { return };
    let scene = Scene::new();
    let rgba =
        render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::from_rgba8(255, 0, 0, 128));
    let c = pixel(&rgba, 64, 32, 32);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(
        (c[0] as i32 - 128).abs() <= 1 && c[1] <= 1 && c[2] <= 1 && (c[3] as i32 - 128).abs() <= 1,
        "pixel {c:?}"
    );
}

/// C10: a 1-logical-px border under `scale_factor` 2 renders 2 device px wide (D04).
#[test]
#[ignore = "gpu"]
fn border_is_two_device_px_at_scale_2() {
    let Some((device, queue, mut renderer)) = setup("border_is_two_device_px_at_scale_2") else { return };
    let mut scene = Scene::new();
    Painter::new(&mut scene).stroke(
        &Stroke::new(1.0),
        Affine::scale(2.0),
        Color::BLACK,
        None,
        &kurbo::Line::new(kurbo::Point::new(0.0, 25.0), kurbo::Point::new(50.0, 25.0)),
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (100, 100), Color::WHITE);
    // The 1-logical-px stroke at scale 2 is a 2-device-px band centered on y=50; probe column x=50.
    let dark = (0..100).filter(|&y| pixel(&rgba, 100, 50, y)[0] < 128).count();
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!((2..=3).contains(&dark), "border should be ~2 device px, got {dark}");
}

/// A `ColorFiltered`-style Multiply layer over a red child: exact W3C multiply (L23; this
/// replaced the C17 interim that skipped the layer).
#[test]
#[ignore = "gpu"]
fn color_filtered_multiplies_child() {
    let Some((device, queue, mut renderer)) = setup("color_filtered_multiplies_child") else { return };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(20.0, 20.0, 60.0, 60.0),
    );
    p.push_layer(
        Fill::NonZero,
        peniko::Mix::Multiply,
        1.0,
        Affine::IDENTITY,
        &Rect::new(0.0, 0.0, 80.0, 80.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 255, 255),
        None,
        &Rect::new(0.0, 0.0, 80.0, 80.0),
    );
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (80, 80), Color::WHITE);
    // W3C multiply (L23): red × blue = black over the child, white × blue = blue elsewhere.
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(pixel(&rgba, 80, 40, 40), [0, 0, 0, 255], "multiply over the red child");
    assert_eq!(pixel(&rgba, 80, 10, 10), [0, 0, 255, 255], "multiply over the white base");
    assert_eq!(renderer.last_frame_stats().unsupported[pebbles_gpu::Unsupported::LayerBlend as usize], 0);
    assert_eq!(renderer.last_frame_stats().unsupported_kinds(), [], "S50: no fallback key");
}
