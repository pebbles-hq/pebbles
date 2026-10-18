use kurbo::{Affine, BezPath, Circle, Rect, Shape};
use peniko::Fill;

use super::{CoverageRasterizer, MaskBuf, MaskPlacement};
use crate::prepare::ScissorRect;

const CLIP: ScissorRect = ScissorRect { x0: 0, y0: 0, x1: 256, y1: 256 };

fn mask(path: &BezPath, rule: Fill, clip: ScissorRect) -> Option<(MaskPlacement, MaskBuf)> {
    let mut r = CoverageRasterizer::default();
    let mut out = MaskBuf::default();
    r.rasterize(path, Affine::IDENTITY, rule, clip, &mut out).map(|p| (p, out))
}

fn at(m: &MaskBuf, x: u32, y: u32) -> u8 {
    m.data[(y * m.width + x) as usize]
}

#[test]
fn unit_square_at_fractional_offsets() {
    let (p, m) = mask(&Rect::new(0.25, 0.25, 1.75, 1.75).to_path(0.1), Fill::NonZero, CLIP).expect("mask");
    assert_eq!(p, MaskPlacement { left: 0, top: 0, width: 2, height: 2 });
    let want = 0.5625 * 255.0;
    for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
        assert!((f64::from(at(&m, x, y)) - want).abs() <= 1.0, "({x},{y}) {}", at(&m, x, y));
    }
}

#[test]
fn circle_area_matches_pi_r2() {
    let (_, m) = mask(&Circle::new((100.0, 100.0), 40.0).to_path(0.01), Fill::NonZero, CLIP).expect("mask");
    let sum: f64 = m.data.iter().map(|&v| f64::from(v) / 255.0).sum();
    let area = std::f64::consts::PI * 1600.0;
    assert!(((sum - area) / area).abs() < 1e-3, "{sum} vs {area}");
}

fn two_squares(inner_reversed: bool) -> BezPath {
    let mut p = Rect::new(10.0, 10.0, 50.0, 50.0).to_path(0.1);
    let r = Rect::new(20.0, 20.0, 40.0, 40.0);
    if inner_reversed {
        p.move_to((r.x0, r.y0));
        p.line_to((r.x0, r.y1));
        p.line_to((r.x1, r.y1));
        p.line_to((r.x1, r.y0));
        p.close_path();
    } else {
        p.extend(r.to_path(0.1));
    }
    p
}

#[test]
fn evenodd_donut_has_hole() {
    let (p, m) = mask(&two_squares(false), Fill::EvenOdd, CLIP).expect("mask");
    assert_eq!((p.left, p.top), (10, 10));
    assert_eq!(at(&m, 20, 20), 0); // device (30, 30)
    assert_eq!(at(&m, 2, 2), 255);
}

#[test]
fn nonzero_overlapping_squares_saturate() {
    let (_, m) = mask(&two_squares(false), Fill::NonZero, CLIP).expect("mask");
    assert_eq!(at(&m, 20, 20), 255, "winding 2 is fully covered, never more");
    let (_, m) = mask(&two_squares(true), Fill::NonZero, CLIP).expect("mask");
    assert_eq!(at(&m, 20, 20), 0, "opposite winding cuts a hole");
}

#[test]
fn clip_limits_mask() {
    let clip = ScissorRect { x0: 30, y0: 0, x1: 40, y1: 35 };
    let (p, m) = mask(&Rect::new(10.0, 10.0, 50.0, 50.0).to_path(0.1), Fill::NonZero, clip).expect("mask");
    assert_eq!(p, MaskPlacement { left: 30, top: 10, width: 10, height: 25 });
    assert!(m.data.iter().all(|&v| v == 255));
}

#[test]
fn empty_path_is_none() {
    assert!(mask(&BezPath::new(), Fill::NonZero, CLIP).is_none());
    let mut line = BezPath::new();
    line.move_to((1.0, 1.0));
    line.line_to((9.0, 1.0));
    assert!(mask(&line, Fill::NonZero, CLIP).is_none(), "zero-height path has no area");
    let outside = ScissorRect { x0: 100, y0: 100, x1: 120, y1: 120 };
    assert!(mask(&Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.1), Fill::NonZero, outside).is_none());
    let mut nan = BezPath::new();
    nan.move_to((0.0, 0.0));
    nan.line_to((f64::NAN, 5.0));
    nan.line_to((5.0, 5.0));
    assert!(mask(&nan, Fill::NonZero, CLIP).is_none());
}

#[test]
fn no_allocation_after_warm_up() {
    let mut r = CoverageRasterizer::default();
    let mut out = MaskBuf::default();
    let path = Circle::new((60.0, 60.0), 50.0).to_path(0.1);
    let shift = |i: u32| Affine::translate((f64::from(i % 7) * 0.13, 0.0));
    for i in 0..7 {
        r.rasterize(&path, shift(i), Fill::NonZero, CLIP, &mut out);
    }
    let caps = (r.cmds_capacity(), out.data.capacity());
    for i in 0..100 {
        r.rasterize(&path, shift(i), Fill::NonZero, CLIP, &mut out);
    }
    assert_eq!((r.cmds_capacity(), out.data.capacity()), caps);
}
