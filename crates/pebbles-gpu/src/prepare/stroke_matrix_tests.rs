//! R06: the stroke geometry matrix — joins, caps, miter limit, dashes, transforms — asserted
//! on the device-space outline before tier routing (0.15 px tolerance).

use kurbo::{Affine, BezPath, Cap, Join, Point, Rect, Shape, Stroke};

use super::super::expand;

const TOL: f64 = 0.15;
const BIG: Rect = Rect::new(-1e4, -1e4, 1e4, 1e4);

/// The device-space outline of `path` stroked with `style` under `xf`, and the draw's alpha.
fn outline(path: &BezPath, style: &Stroke, xf: Affine) -> (BezPath, f32) {
    let mut ctx = kurbo::StrokeCtx::default();
    let out = expand(path, style, xf, BIG, 1.0, &mut ctx);
    let mut o = ctx.output().clone();
    o.apply_affine(xf);
    (o, out.alpha)
}

fn covers(o: &BezPath, p: (f64, f64)) -> bool {
    o.winding(Point::new(p.0, p.1)) != 0
}

fn polyline(pts: &[(f64, f64)]) -> BezPath {
    let mut b = BezPath::new();
    b.move_to(pts[0]);
    for p in &pts[1..] {
        b.line_to(*p);
    }
    b
}

fn hline() -> BezPath {
    polyline(&[(10.0, 10.0), (90.0, 10.0)])
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= TOL
}

/// Covered x-spans along y = 0 of a horizontal stroke from x = 0 (sampled every 0.01 px).
fn spans(o: &BezPath, until: f64) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut start = None;
    let mut x = 0.0;
    while x <= until {
        match (covers(o, (x, 0.0)), start) {
            (true, None) => start = Some(x),
            (false, Some(s)) => {
                out.push((s, x));
                start = None;
            }
            _ => {}
        }
        x += 0.01;
    }
    if let Some(s) = start {
        out.push((s, until));
    }
    out
}

fn cap(c: Cap) -> Rect {
    outline(&hline(), &Stroke::new(4.0).with_caps(c), Affine::IDENTITY).0.bounding_box()
}

#[test]
fn butt_cap_extent() {
    let b = cap(Cap::Butt);
    assert!(near(b.x0, 10.0) && near(b.x1, 90.0) && near(b.y0, 8.0) && near(b.y1, 12.0), "{b:?}");
}

#[test]
fn square_cap_extent() {
    let b = cap(Cap::Square);
    assert!(near(b.x0, 8.0) && near(b.x1, 92.0), "{b:?}");
}

#[test]
fn round_cap_extent() {
    let b = cap(Cap::Round);
    assert!(near(b.x0, 8.0) && near(b.x1, 92.0), "{b:?}");
    let o = outline(&hline(), &Stroke::new(4.0).with_caps(Cap::Round), Affine::IDENTITY).0;
    assert!(covers(&o, (91.0, 10.0)) && !covers(&o, (91.9, 11.9)));
}

fn corner(join: Join) -> BezPath {
    let s = Stroke::new(10.0).with_join(join);
    outline(&polyline(&[(10.0, 50.0), (50.0, 50.0), (50.0, 10.0)]), &s, Affine::IDENTITY).0
}

#[test]
fn miter_join_reaches_corner() {
    assert!(covers(&corner(Join::Miter), (54.5, 54.5)));
}

#[test]
fn bevel_join_cuts_corner() {
    assert!(!covers(&corner(Join::Bevel), (54.5, 54.5)));
}

#[test]
fn round_join_is_round() {
    let o = corner(Join::Round);
    assert!(covers(&o, (53.0, 53.0)) && !covers(&o, (54.5, 54.5)));
}

#[test]
fn miter_limit_falls_back_to_bevel() {
    // A 20° corner: the miter tip lies 5 / sin(10°) ≈ 28.8 px out along the outer bisector.
    let a = 160f64.to_radians();
    let path = polyline(&[(0.0, 50.0), (50.0, 50.0), (50.0 + 50.0 * a.cos(), 50.0 + 50.0 * a.sin())]);
    let tip = (50.0 + 15.0 * 0.985, 50.0 - 15.0 * 0.174);
    let miter = Stroke::new(10.0).with_join(Join::Miter).with_miter_limit(10.0);
    assert!(covers(&outline(&path, &miter, Affine::IDENTITY).0, tip), "a long miter is drawn under limit 10");
    let limited = miter.with_miter_limit(1.0);
    assert!(!covers(&outline(&path, &limited, Affine::IDENTITY).0, tip), "limit 1 bevels the corner");
}

fn dashed_line(pattern: &[f64], offset: f64, xf: Affine) -> BezPath {
    let s = Stroke::new(2.0).with_caps(Cap::Butt).with_dashes(offset, pattern.iter().copied());
    outline(&polyline(&[(0.0, 0.0), (60.0, 0.0)]), &s, xf).0
}

#[test]
fn dash_offset_shifts_first_dash() {
    let s = spans(&dashed_line(&[6.0, 3.0], 2.5, Affine::IDENTITY), 20.0);
    assert!(near(s[0].0, 0.0) && near(s[0].1, 3.5), "{s:?}");
}

#[test]
fn odd_dash_pattern_doubles() {
    let s = spans(&dashed_line(&[4.0], 0.0, Affine::IDENTITY), 20.0);
    assert!(near(s[0].0, 0.0) && near(s[0].1, 4.0) && near(s[1].0, 8.0) && near(s[1].1, 12.0), "{s:?}");
}

#[test]
fn closed_rect_has_no_cap() {
    let s = Stroke::new(4.0).with_caps(Cap::Square);
    let b = outline(&Rect::new(10.0, 10.0, 50.0, 50.0).to_path(0.1), &s, Affine::IDENTITY).0.bounding_box();
    assert!(near(b.x0, 8.0) && near(b.y0, 8.0) && near(b.x1, 52.0) && near(b.y1, 52.0), "{b:?}");
}

#[test]
fn stroke_scales_with_transform() {
    let b = outline(&hline(), &Stroke::new(1.0).with_caps(Cap::Butt), Affine::scale(2.0)).0.bounding_box();
    assert!(near(b.height(), 2.0), "{b:?}");
}

#[test]
fn stroke_scales_anisotropically() {
    let xf = Affine::scale_non_uniform(2.0, 0.5);
    let s = Stroke::new(4.0).with_caps(Cap::Butt);
    let h = outline(&hline(), &s, xf).0.bounding_box();
    assert!(near(h.height(), 2.0), "horizontal: {h:?}");
    let v = outline(&polyline(&[(10.0, 10.0), (10.0, 90.0)]), &s, xf).0.bounding_box();
    assert!(near(v.width(), 8.0), "vertical: {v:?}");
}

#[test]
fn dashes_scale_with_transform() {
    let s = spans(&dashed_line(&[6.0, 3.0], 0.0, Affine::scale(2.0)), 40.0);
    assert!(near(s[0].1 - s[0].0, 12.0), "{s:?}");
}

#[test]
fn hairline_is_one_px_with_scaled_alpha() {
    let (o, alpha) = outline(&hline(), &Stroke::new(0.25).with_caps(Cap::Butt), Affine::IDENTITY);
    assert!(near(o.bounding_box().height(), 1.0), "{:?}", o.bounding_box());
    assert!((alpha - 0.25).abs() < 1e-6, "alpha {alpha}");
}
