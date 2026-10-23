//! The offscreen capture API (C15).

use super::*;

/// C15: a capture larger than the device limit is an `Err`, never a panic.
#[test]
#[ignore = "gpu"]
fn capture_larger_than_device_limit_is_err_not_panic() {
    let Some(mut o) = offscreen("capture_larger_than_device_limit_is_err_not_panic") else { return };
    let w = o.max_size() + 1;
    let result = o.render_rgba(&Scene::new(), (w, 16), Color::WHITE, 1.0);
    assert!(matches!(result, Err(pebbles_gpu::CaptureError::InvalidSize { .. })), "{result:?}");
}

/// C15: a zero-size capture is an `Err`.
#[test]
#[ignore = "gpu"]
fn capture_zero_size_is_err() {
    let Some(mut o) = offscreen("capture_zero_size_is_err") else { return };
    assert!(o.render_rgba(&Scene::new(), (0, 16), Color::WHITE, 1.0).is_err());
    assert!(o.render_rgba(&Scene::new(), (16, 0), Color::WHITE, 1.0).is_err());
}

/// G31: the boundary sizes 1 and `max` capture; `max + 1` and `u32::MAX` (either side) are
/// an `Err`. None of them panics. `max` is captured one row high to keep the readback small.
#[test]
#[ignore = "gpu"]
fn capture_boundary_sizes_never_panic() {
    let Some(mut o) = offscreen("capture_boundary_sizes_never_panic") else { return };
    let max = o.max_size();
    let img = o.render_rgba(&Scene::new(), (1, 1), Color::WHITE, 1.0).expect("1×1 capture");
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(img.pixels, [255, 255, 255, 255]);
    for size in [(max, 1), (1, max)] {
        let img = o.render_rgba(&Scene::new(), size, Color::WHITE, 1.0).expect("max-size capture");
        assert_eq!(img.pixels.len(), max as usize * 4, "{size:?}");
    }
    for size in [(max + 1, 1), (1, max + 1), (u32::MAX, 1), (1, u32::MAX), (u32::MAX, u32::MAX)] {
        let r = o.render_rgba(&Scene::new(), size, Color::WHITE, 1.0);
        assert!(matches!(r, Err(pebbles_gpu::CaptureError::InvalidSize { .. })), "{size:?}: {r:?}");
    }
}

/// C15: an unaligned width (17 px) has its row padding removed; pixel (16, 0) is correct.
#[test]
#[ignore = "gpu"]
fn readback_handles_unaligned_width() {
    let Some(mut o) = offscreen("readback_handles_unaligned_width") else { return };
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 255, 255),
        None,
        &Rect::new(16.0, 0.0, 17.0, 1.0),
    );
    let image = o.render_rgba(&scene, (17, 3), Color::WHITE, 1.0).expect("capture");
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(image.pixels.len(), 17 * 3 * 4);
    assert_eq!(pixel(&image.pixels, 17, 16, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&image.pixels, 17, 0, 1), [255, 255, 255, 255]);
}

/// C15: captures are unpremultiplied — a 50 % red fill over transparent reads (255, 0, 0, 128).
#[test]
#[ignore = "gpu"]
fn capture_is_unpremultiplied() {
    let Some(mut o) = offscreen("capture_is_unpremultiplied") else { return };
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 128),
        None,
        &Rect::new(0.0, 0.0, 8.0, 8.0),
    );
    let image = o.render_rgba(&scene, (8, 8), Color::TRANSPARENT, 1.0).expect("capture");
    let px = pixel(&image.pixels, 8, 4, 4);
    let near = |v: u8, want: i32| (i32::from(v) - want).abs() <= 1;
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(near(px[0], 255) && px[1] == 0 && px[2] == 0 && near(px[3], 128), "{px:?}");
}
