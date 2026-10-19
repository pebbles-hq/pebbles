//! Image brushes on fills and strokes (09 §6, L42): a 2×2 image (red, green / blue, white),
//! `ImageQuality::Low`, paint transform `scale(16)`, 64×64 on white.

use super::*;
use peniko::{Extend, ImageBrush, ImageQuality};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn quad_image() -> peniko::ImageData {
    let px: Vec<u8> = [RED, GREEN, BLUE, WHITE].concat();
    peniko::ImageData {
        data: peniko::Blob::from(px),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 2,
    }
}

fn brush(extend: Extend) -> ImageBrush {
    ImageBrush::new(quad_image()).with_quality(ImageQuality::Low).with_extend(extend)
}

fn fill(name: &str, b: &ImageBrush, xf: Affine) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup(name)?;
    let mut s = Scene::new();
    Painter::new(&mut s).fill(Fill::NonZero, Affine::IDENTITY, b, Some(xf), &Rect::new(0.0, 0.0, 64.0, 64.0));
    Some(render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE))
}

#[test]
#[ignore = "gpu"]
fn fill_pad() {
    let Some(px) = fill("image_brush_fill_pad", &brush(Extend::Pad), Affine::scale(16.0)) else { return };
    for ((x, y), want) in [
        ((8, 8), RED),
        ((24, 8), GREEN),
        ((8, 24), BLUE),
        ((24, 24), WHITE),
        ((56, 8), GREEN),
        ((8, 56), BLUE),
        ((56, 56), WHITE),
    ] {
        assert_eq!(pixel(&px, 64, x, y), want, "({x},{y})");
    }
}

#[test]
#[ignore = "gpu"]
fn fill_repeat() {
    let Some(px) = fill("image_brush_fill_repeat", &brush(Extend::Repeat), Affine::scale(16.0)) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 40, 8), RED);
    assert_eq!(pixel(&px, 64, 56, 8), GREEN);
}

#[test]
#[ignore = "gpu"]
fn fill_reflect() {
    let Some(px) = fill("image_brush_fill_reflect", &brush(Extend::Reflect), Affine::scale(16.0)) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 40, 8), GREEN);
    assert_eq!(pixel(&px, 64, 56, 8), RED);
}

#[test]
#[ignore = "gpu"]
fn alpha_half() {
    let b = brush(Extend::Pad).with_alpha(0.5);
    let Some(px) = fill("image_brush_alpha_half", &b, Affine::scale(16.0)) else { return };
    let c = pixel(&px, 64, 8, 8);
    assert!((0..3).all(|i| (i32::from(c[i]) - [255, 128, 128][i]).abs() <= 2), "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn stroke_ring() {
    let Some((device, queue, mut r)) = setup("image_brush_stroke_ring") else { return };
    let mut s = Scene::new();
    let rr = kurbo::RoundedRect::new(12.0, 12.0, 52.0, 52.0, 8.0);
    Painter::new(&mut s).stroke(
        &Stroke::new(16.0),
        Affine::IDENTITY,
        &brush(Extend::Pad),
        Some(Affine::scale(32.0)),
        &rr,
    );
    let px = render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE);
    // On the stroke near the top-left corner: the red cell (P = scale(32): cell (0,0) = red).
    assert_eq!(pixel(&px, 64, 14, 20), RED);
    assert!(is_white(pixel(&px, 64, 32, 32)), "inside the ring is unstroked");
}

#[test]
#[ignore = "gpu"]
fn rotated_brush_transform() {
    // P = translate(32, 0) · rotate(π/2) · scale(16): D maps device (x, y) to image
    // ((y) / 16, (32 − x) / 16), so the four 16-px cells left of x = 32 show the image rotated a
    // quarter turn clockwise: blue red / white green.
    let xf =
        Affine::translate((32.0, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2) * Affine::scale(16.0);
    let Some(px) = fill("image_brush_rotated_brush_transform", &brush(Extend::Pad), xf) else { return };
    for ((x, y), want) in [((8, 8), BLUE), ((24, 8), RED), ((8, 24), WHITE), ((24, 24), GREEN)] {
        assert_eq!(pixel(&px, 64, x, y), want, "({x},{y})");
    }
}
