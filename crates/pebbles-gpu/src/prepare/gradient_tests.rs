//! L31–L33: validation table, ramp encoding/baking and gradient-space transforms.

use super::*;
use peniko::Color;
use peniko::color::palette::css;

fn stops(s: &[(f32, Color)]) -> peniko::ColorStops {
    peniko::ColorStops(s.iter().map(|&(offset, c)| ColorStop { offset, color: c.into() }).collect())
}

fn linear(a: (f64, f64), b: (f64, f64), s: &[(f32, Color)]) -> Gradient {
    Gradient::new_linear(a, b).with_stops(stops(s).as_slice())
}

const BW: [(f32, Color); 2] = [(0.0, Color::BLACK), (1.0, Color::WHITE)];

#[test]
fn validation_table() {
    let id = Affine::IDENTITY;
    let green = css::LIME;
    let ok = linear((0.0, 0.0), (10.0, 0.0), &BW);
    assert_eq!(validate_gradient(&linear((0.0, 0.0), (10.0, 0.0), &[]), id), Err(css::BLACK), "no stops");
    assert_eq!(
        validate_gradient(&linear((0.0, 0.0), (10.0, 0.0), &[(0.5, green)]), id),
        Err(green),
        "one stop"
    );
    let first = Color::BLACK;
    let bad_offset = linear((0.0, 0.0), (10.0, 0.0), &[(0.0, first), (1.5, Color::WHITE)]);
    assert_eq!(validate_gradient(&bad_offset, id), Err(first), "offset 1.5");
    let decreasing = linear((0.0, 0.0), (10.0, 0.0), &[(0.6, first), (0.4, Color::WHITE)]);
    assert_eq!(validate_gradient(&decreasing, id), Err(first), "decreasing");
    assert_eq!(
        validate_gradient(&linear((3.0, 3.0), (3.0, 3.0), &BW), id),
        Err(first),
        "linear start == end"
    );
    let neg =
        Gradient::new_two_point_radial((0.0, 0.0), -1.0, (5.0, 0.0), 10.0).with_stops(stops(&BW).as_slice());
    assert_eq!(validate_gradient(&neg, id), Err(first), "negative radius");
    let sweep = Gradient::new_sweep((0.0, 0.0), 1.0, 1.0).with_stops(stops(&BW).as_slice());
    assert_eq!(validate_gradient(&sweep, id), Err(first), "sweep end <= start");
    let nan = Gradient::new_radial((f64::NAN, 0.0), 5.0).with_stops(stops(&BW).as_slice());
    assert_eq!(validate_gradient(&nan, id), Err(first), "NaN center");
    assert_eq!(validate_gradient(&ok, Affine::scale(0.0)), Err(first), "singular paint transform");
    assert_eq!(validate_gradient(&ok, id), Ok(()));
}

/// The ramp as 8-bit texels (`(v · 255 + 0.5)`), for value checks.
fn bake_of(g: &Gradient) -> (u32, Vec<[u8; 4]>) {
    let mut out = Vec::new();
    let len = RampKey::of(g).bake(&mut out);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let q = out.iter().map(|c| c.map(|v| (v * 255.0 + 0.5) as u8)).collect();
    (len, q)
}

#[test]
fn two_stop_black_white_256() {
    let (len, t) = bake_of(&linear((0.0, 0.0), (1.0, 0.0), &BW));
    assert_eq!(len, 256);
    assert_eq!(t.len(), 256);
    assert_eq!(t[0], [0, 0, 0, 255]);
    assert_eq!(t[255], [255, 255, 255, 255]);
    assert!((127..=129).contains(&t[128][0]), "{:?}", t[128]);
}

#[test]
fn premultiplied_vs_unpremultiplied_midpoint() {
    let s = [(0.0, Color::TRANSPARENT), (1.0, Color::WHITE)];
    let pre = linear((0.0, 0.0), (1.0, 0.0), &s);
    let (_, t) = bake_of(&pre);
    assert!(t[128].iter().all(|&v| (127..=129).contains(&v)), "{:?}", t[128]);
    let mut un = pre.clone();
    un.interpolation_alpha_space = InterpolationAlphaSpace::Unpremultiplied;
    let (_, t) = bake_of(&un);
    assert!(
        t[128][..3].iter().all(|&v| (63..=65).contains(&v)) && (127..=129).contains(&t[128][3]),
        "{:?}",
        t[128]
    );
}

#[test]
fn hard_stop_uses_2048() {
    let red = css::RED;
    let blue = css::BLUE;
    let (len, t) =
        bake_of(&linear((0.0, 0.0), (1.0, 0.0), &[(0.0, red), (0.5, red), (0.5, blue), (1.0, blue)]));
    assert_eq!(len, 2048);
    assert_eq!(t[1023], [255, 0, 0, 255]);
    assert_eq!(t[1024], [0, 0, 255, 255]);
}

#[test]
fn padding_holds_end_colors() {
    let red = css::RED;
    let blue = css::BLUE;
    let (len, t) = bake_of(&linear((0.0, 0.0), (1.0, 0.0), &[(0.25, red), (0.75, blue)]));
    let len = len as usize;
    assert!(t[..len / 4].iter().all(|&c| c == [255, 0, 0, 255]));
    assert!(t[len * 3 / 4 + 1..].iter().all(|&c| c == [0, 0, 255, 255]));
}

#[test]
fn lut_len_rule() {
    let even = |n: usize| -> u32 {
        let s: Vec<(f32, Color)> = (0..n).map(|i| (i as f32 / (n - 1) as f32, Color::BLACK)).collect();
        bake_of(&linear((0.0, 0.0), (1.0, 0.0), &s)).0
    };
    assert_eq!(even(3), 512);
    assert_eq!(even(5), 1024);
    let close =
        linear((0.0, 0.0), (1.0, 0.0), &[(0.0, Color::BLACK), (0.001, Color::WHITE), (1.0, Color::BLACK)]);
    assert_eq!(bake_of(&close).0, 1024);
}

#[test]
fn oklab_red_blue_midpoint_differs_from_srgb() {
    let s = [(0.0, css::RED), (1.0, css::BLUE)];
    let srgb = linear((0.0, 0.0), (1.0, 0.0), &s);
    let mut ok = srgb.clone();
    ok.interpolation_cs = ColorSpaceTag::Oklab;
    let (_, a) = bake_of(&srgb);
    let (_, b) = bake_of(&ok);
    assert_ne!(a[128], b[128]);
    assert_eq!(b[0], [255, 0, 0, 255]);
}

#[test]
fn ramp_key_equality() {
    use std::hash::{BuildHasher, BuildHasherDefault};
    let g = linear((0.0, 0.0), (1.0, 0.0), &BW);
    let h =
        |k: &RampKey| BuildHasherDefault::<std::collections::hash_map::DefaultHasher>::default().hash_one(k);
    assert_eq!(RampKey::of(&g), RampKey::of(&g.clone()));
    assert_eq!(h(&RampKey::of(&g)), h(&RampKey::of(&g.clone())));
    let mut u = g.clone();
    u.interpolation_alpha_space = InterpolationAlphaSpace::Unpremultiplied;
    assert_ne!(RampKey::of(&g), RampKey::of(&u));
}

// ---- L33 ----

/// The §5.5 `t` on the CPU (f64), `None` where the shader reports invalid.
pub(crate) fn eval_t(geom: &GradientGeom, p: Point) -> Option<f64> {
    let m0 = geom.m0.map(f64::from);
    let m1 = geom.m1.map(f64::from);
    let g = Point::new(m0[0] * p.x + m0[1] * p.y + m0[2], m1[0] * p.x + m1[1] * p.y + m1[2]);
    let p0 = geom.p0.map(f64::from);
    match geom.kind {
        0 => Some(g.x + 1e-5),
        1 => Some(p0[0] + p0[1] * g.to_vec2().hypot()),
        4 => {
            if g.x.abs() < 1.0 / 4096.0 && g.y.abs() < 1.0 / 4096.0 {
                return Some(-p0[0] * p0[1]);
            }
            let mut a = g.y.atan2(g.x);
            if a < 0.0 {
                a += std::f64::consts::TAU;
            }
            Some((a - p0[0]) * p0[1])
        }
        _ => {
            // The gradient.wgsl two-circle quadratic, in f64.
            let (rho0, delta, a) = (p0[0], p0[1], p0[2]);
            let b = g.x + rho0 * delta;
            let c = g.to_vec2().hypot2() - rho0 * rho0;
            let disc = b * b - a * c;
            if disc < 0.0 {
                return None;
            }
            let k = b + if b >= 0.0 { 1.0 } else { -1.0 } * disc.sqrt();
            let mut roots = Vec::new();
            if k != 0.0 {
                roots.push(c / k);
            }
            if a != 0.0 {
                roots.push(k / a);
            }
            roots.into_iter().filter(|&w| rho0 + w * delta >= 0.0).reduce(f64::max)
        }
    }
}

#[test]
fn line_to_unit_maps_endpoints() {
    let m = line_to_unit(Point::new(10.0, 20.0), Point::new(50.0, 80.0));
    let a = m * Point::new(10.0, 20.0);
    let b = m * Point::new(50.0, 80.0);
    assert!(a.to_vec2().hypot() < 1e-9 && (b - Point::new(1.0, 0.0)).hypot() < 1e-9);
    assert!(m.determinant() > 0.0);
}

#[test]
fn linear_t_at_pixel_centers() {
    let g = linear((0.0, 0.0), (256.0, 0.0), &BW);
    let geom = gradient_geom(&g, Affine::IDENTITY);
    let t = eval_t(&geom, Point::new(128.5, 3.5)).unwrap();
    assert!((t - 0.50196).abs() < 1e-4, "{t}");
}

#[test]
fn radial_concentric_t() {
    let g = Gradient::new_radial((128.0, 128.0), 100.0).with_stops(stops(&BW).as_slice());
    let geom = gradient_geom(&g, Affine::IDENTITY);
    assert_eq!(geom.kind, 1);
    let t = eval_t(&geom, Point::new(178.0, 128.0)).unwrap();
    assert!((t - 0.5).abs() < 1e-5, "{t}");
}

#[test]
fn strip_kind_selected_and_invalid_outside() {
    let g = Gradient::new_two_point_radial((0.0, 0.0), 10.0, (100.0, 0.0), 10.0)
        .with_stops(stops(&BW).as_slice());
    let geom = gradient_geom(&g, Affine::IDENTITY);
    assert_eq!(geom.kind, 2);
    assert!(eval_t(&geom, Point::new(50.0, 0.0)).is_some());
    assert!(eval_t(&geom, Point::new(50.0, 30.0)).is_none(), "outside the strip");
}

#[test]
fn two_point_conical_matches_brute_force() {
    let mut rng = crate::test_support::XorShift64::new(7);
    let (c0, r0, c1, r1) = (Point::new(40.0, 50.0), 5.0, Point::new(90.0, 60.0), 40.0);
    let g = Gradient::new_two_point_radial(c0, 5.0, c1, 40.0).with_stops(stops(&BW).as_slice());
    let geom = gradient_geom(&g, Affine::IDENTITY);
    assert_eq!(geom.kind, 3);
    for _ in 0..50 {
        let p = Point::new(rng.range_f64(0.0, 160.0), rng.range_f64(0.0, 120.0));
        // Largest t with |p − c(t)| = r(t), r(t) ≥ 0: solve the quadratic directly.
        let cd = c1 - c0;
        let dr = r1 - r0;
        let pd = p - c0;
        let a = cd.hypot2() - dr * dr;
        let b = -2.0 * (pd.dot(cd) + r0 * dr);
        let c = pd.hypot2() - r0 * r0;
        let disc = b * b - 4.0 * a * c;
        let want = if disc < 0.0 {
            None
        } else {
            let s = disc.sqrt();
            let roots = [(-b + s) / (2.0 * a), (-b - s) / (2.0 * a)];
            roots
                .into_iter()
                .filter(|&t| r0 + t * dr >= 0.0)
                .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.max(t))))
        };
        let got = eval_t(&geom, p);
        match (got, want) {
            (Some(g), Some(w)) => assert!((g - w).abs() < 1e-3, "p {p:?}: {g} vs {w}"),
            (None, None) => {}
            _ => panic!("p {p:?}: {got:?} vs {want:?}"),
        }
    }
}

#[test]
fn sweep_quadrants() {
    let g = Gradient::new_sweep((0.0, 0.0), 0.0, std::f32::consts::TAU).with_stops(stops(&BW).as_slice());
    let geom = gradient_geom(&g, Affine::IDENTITY);
    let t = |x: f64, y: f64| eval_t(&geom, Point::new(x, y)).unwrap();
    assert!(t(1.0, 0.0).abs() < 1e-6);
    assert!((t(0.0, 1.0) - 0.25).abs() < 1e-6);
    assert!((t(-1.0, 0.0) - 0.5).abs() < 1e-6);
}

#[test]
fn brush_transform_is_applied() {
    let g = linear((0.0, 0.0), (100.0, 0.0), &BW);
    let geom = gradient_geom(&g, Affine::rotate(std::f64::consts::FRAC_PI_2));
    let a = eval_t(&geom, Point::new(0.0, 10.0)).unwrap();
    let b = eval_t(&geom, Point::new(0.0, 60.0)).unwrap();
    let c = eval_t(&geom, Point::new(50.0, 10.0)).unwrap();
    assert!((b - a - 0.5).abs() < 1e-4, "varies along y");
    assert!((c - a).abs() < 1e-4, "constant along x");
}

#[test]
fn touching_circles_have_one_root() {
    // |c1 − c0| = 30 = r1 − r0: the start circle touches the end circle from inside, the
    // quadratic's leading coefficient is 0 and every valid pixel has exactly one root.
    let (c0, r0, c1, r1) = (Point::new(20.0, 50.0), 10.0, Point::new(50.0, 50.0), 40.0);
    let g = Gradient::new_two_point_radial(c0, 10.0, c1, 40.0).with_stops(stops(&BW).as_slice());
    let geom = gradient_geom(&g, Affine::IDENTITY);
    assert_eq!((geom.kind, geom.p0[2]), (3, 0.0));
    for p in [Point::new(60.0, 50.0), Point::new(50.0, 80.0), Point::new(85.0, 55.0)] {
        let t = eval_t(&geom, p).unwrap();
        let r = r0 + t * (r1 - r0);
        let on = (p - (c0 + (c1 - c0) * t)).hypot();
        assert!(r >= 0.0 && (on - r).abs() < 1e-3, "{p:?}: t {t}");
    }
    assert!(eval_t(&geom, Point::new(-5.0, 50.0)).is_none(), "behind the touching point");
}
