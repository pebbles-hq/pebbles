//! Gradient fills and strokes (09 §5, L35–L37) on the GPU.

use super::*;
use peniko::{Extend, Gradient, InterpolationAlphaSpace};

fn bw() -> [Color; 2] {
    [Color::BLACK, Color::WHITE]
}

fn draw(name: &str, size: (u32, u32), base: Color, build: impl FnOnce(&mut Painter<'_>)) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup(name)?;
    let mut s = Scene::new();
    build(&mut Painter::new(&mut s));
    Some(render_scene(&device, &queue, &mut r, &s, size, base))
}

fn full(p: &mut Painter<'_>, g: &Gradient, size: (f64, f64)) {
    p.fill(Fill::NonZero, Affine::IDENTITY, g, None, &Rect::new(0.0, 0.0, size.0, size.1));
}

fn within(v: u8, want: f64, tol: f64) -> bool {
    (f64::from(v) - want).abs() <= tol
}

#[test]
#[ignore = "gpu"]
fn linear_ramp_midpoint() {
    let g = Gradient::new_linear((0.0, 0.0), (256.0, 0.0)).with_stops(bw().as_slice());
    let Some(px) = draw("linear_ramp_midpoint", (256, 8), Color::WHITE, |p| full(p, &g, (256.0, 8.0))) else {
        return;
    };
    let mid = pixel(&px, 256, 128, 4);
    assert!(mid[..3].iter().all(|&v| (126..=130).contains(&v)), "{mid:?}");
    assert!(pixel(&px, 256, 0, 4)[..3].iter().all(|&v| v <= 2));
    assert!(pixel(&px, 256, 255, 4)[..3].iter().all(|&v| v >= 253));
}

#[test]
#[ignore = "gpu"]
fn extend_repeat_and_reflect() {
    for (extend, want) in [(Extend::Repeat, 64.0), (Extend::Reflect, 191.0)] {
        let g = Gradient::new_linear((0.0, 0.0), (64.0, 0.0)).with_stops(bw().as_slice()).with_extend(extend);
        let Some(px) =
            draw("extend_repeat_and_reflect", (256, 8), Color::WHITE, |p| full(p, &g, (256.0, 8.0)))
        else {
            return;
        };
        let c = pixel(&px, 256, 80, 4);
        assert!(within(c[0], want, 3.0), "{extend:?}: {c:?}");
    }
}

#[test]
#[ignore = "gpu"]
fn radial_center_and_edge() {
    // Centered on pixel (32, 32)'s center, so that pixel is exactly the first stop.
    let g = Gradient::new_radial((32.5, 32.5), 32.0).with_stops(bw().as_slice());
    let Some(px) = draw("radial_center_and_edge", (64, 64), Color::WHITE, |p| full(p, &g, (64.0, 64.0)))
    else {
        return;
    };
    assert!(pixel(&px, 64, 32, 32)[0] <= 2, "{:?}", pixel(&px, 64, 32, 32));
    // Pixel (32, 1) has its center at distance 31 → t ≈ 0.97 (near the edge).
    let c = pixel(&px, 64, 32, 1);
    assert!(within(c[0], 31.0 / 32.0 * 255.0, 3.0), "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn two_point_conical_matches_cpu_eval() {
    let (c0, r0, c1, r1) = (kurbo::Point::new(40.0, 50.0), 5.0f64, kurbo::Point::new(90.0, 60.0), 40.0f64);
    let g = Gradient::new_two_point_radial(c0, 5.0, c1, 40.0).with_stops(bw().as_slice());
    let Some(px) =
        draw("two_point_conical_matches_cpu_eval", (160, 120), Color::from_rgba8(255, 0, 0, 255), |p| {
            full(p, &g, (160.0, 120.0))
        })
    else {
        return;
    };
    for (x, y) in [(60u32, 55u32), (90, 60), (110, 70), (75, 40), (100, 90)] {
        let p = kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
        let (cd, dr, pd) = (c1 - c0, r1 - r0, p - c0);
        let a = cd.hypot2() - dr * dr;
        let b = -2.0 * (pd.dot(cd) + r0 * dr);
        let c = pd.hypot2() - r0 * r0;
        let s = (b * b - 4.0 * a * c).sqrt();
        let t = [(-b + s) / (2.0 * a), (-b - s) / (2.0 * a)]
            .into_iter()
            .filter(|&t| r0 + t * dr >= 0.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .clamp(0.0, 1.0);
        let got = pixel(&px, 160, x, y);
        assert!(within(got[0], t * 255.0, 3.0) && got[1] == got[0], "({x},{y}) t {t}: {got:?}");
    }
}

#[test]
#[ignore = "gpu"]
fn sweep_quadrants() {
    let g = Gradient::new_sweep((32.0, 32.0), 0.0, std::f32::consts::TAU).with_stops(bw().as_slice());
    let Some(px) = draw("sweep_quadrants", (64, 64), Color::WHITE, |p| full(p, &g, (64.0, 64.0))) else {
        return;
    };
    // Directly below the center (y-down 90°) → t = 0.25.
    let c = pixel(&px, 64, 32, 52);
    assert!(within(c[0], 0.25 * 255.0, 4.0), "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn alpha_space_midpoints() {
    for (space, want) in
        [(InterpolationAlphaSpace::Premultiplied, 128.0), (InterpolationAlphaSpace::Unpremultiplied, 64.0)]
    {
        let mut g = Gradient::new_linear((0.0, 0.0), (256.0, 0.0))
            .with_stops([Color::TRANSPARENT, Color::WHITE].as_slice());
        g.interpolation_alpha_space = space;
        let Some(px) = draw("alpha_space_midpoints", (256, 8), Color::BLACK, |p| full(p, &g, (256.0, 8.0)))
        else {
            return;
        };
        let c = pixel(&px, 256, 128, 4);
        assert!(within(c[0], want, 2.0), "{space:?}: {c:?}");
    }
}

#[test]
#[ignore = "gpu"]
fn brush_transform_rotates_gradient() {
    let g = Gradient::new_linear((0.0, 0.0), (64.0, 0.0)).with_stops(bw().as_slice());
    let Some(px) = draw("brush_transform_rotates_gradient", (64, 64), Color::WHITE, |p| {
        let rot = Affine::rotate(std::f64::consts::FRAC_PI_2);
        p.fill(Fill::NonZero, Affine::IDENTITY, &g, Some(rot), &Rect::new(0.0, 0.0, 64.0, 64.0));
    }) else {
        return;
    };
    // Rotated a quarter turn: the ramp runs down the y axis.
    assert!(pixel(&px, 64, 32, 2)[0] < 16 && pixel(&px, 64, 32, 61)[0] > 235);
    assert!((i32::from(pixel(&px, 64, 2, 32)[0]) - i32::from(pixel(&px, 64, 61, 32)[0])).abs() <= 2);
}

#[test]
#[ignore = "gpu"]
fn gradient_stroke_follows_outline() {
    let g = Gradient::new_linear((0.0, 0.0), (64.0, 0.0))
        .with_stops([Color::from_rgba8(255, 0, 0, 255), Color::from_rgba8(0, 0, 255, 255)].as_slice());
    let Some(px) = draw("gradient_stroke_follows_outline", (64, 64), Color::WHITE, |p| {
        p.stroke(&Stroke::new(6.0), Affine::IDENTITY, &g, None, &kurbo::Circle::new((32.0, 32.0), 24.0));
    }) else {
        return;
    };
    let left = pixel(&px, 64, 8, 32);
    let right = pixel(&px, 64, 55, 32);
    assert!(left[0] > 180 && left[2] < 80, "left of the ring is red-ish: {left:?}");
    assert!(right[2] > 180 && right[0] < 80, "right of the ring is blue-ish: {right:?}");
    assert!(is_white(pixel(&px, 64, 32, 32)), "the ring's center is untouched");
}

#[test]
#[ignore = "gpu"]
fn degenerate_gradients_render_fallback_color() {
    let green = Color::from_rgba8(0, 200, 0, 255);
    let g = Gradient::new_linear((10.0, 10.0), (10.0, 10.0)).with_stops([green, Color::WHITE].as_slice());
    let Some(px) = draw("degenerate_gradients_render_fallback_color", (32, 32), Color::WHITE, |p| {
        full(p, &g, (32.0, 32.0))
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 32, 16, 16), [0, 200, 0, 255]);
}
