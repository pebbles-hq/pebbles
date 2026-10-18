//! Tessellation tests (02 R03, 11 S03): explicit fill rules, direct path events, error
//! discard and tessellator reuse.

use super::*;
use crate::prepare::test_util::{covers, prepare_scene_tessellated as prepare_scene};
use kurbo::{BezPath, Point, Rect, Shape};
use lyon_tessellation::FillRule;
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};

fn square(path: &mut BezPath, r: Rect) {
    path.extend(r.path_elements(0.1));
}

fn fill_scene(path: &BezPath, rule: Fill) -> FramePlan {
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(rule, Affine::IDENTITY, Color::BLACK, None, path);
    prepare_scene(&scene, 100, 100).0
}

#[test]
fn even_odd_donut_has_a_hole() {
    // Both subpaths come from `Rect::path_elements`, so they share the same winding.
    let mut donut = BezPath::new();
    square(&mut donut, Rect::new(10.0, 10.0, 90.0, 90.0));
    square(&mut donut, Rect::new(30.0, 30.0, 70.0, 70.0));
    let center = Point::new(50.0, 50.0);
    let ring = Point::new(20.0, 50.0);
    let even_odd = fill_scene(&donut, Fill::EvenOdd);
    assert!(!covers(&even_odd, center));
    assert!(covers(&even_odd, ring));
    let non_zero = fill_scene(&donut, Fill::NonZero);
    assert!(covers(&non_zero, center));
}

#[test]
fn fill_rule_is_explicit() {
    assert_eq!(fill_options(Fill::EvenOdd).fill_rule, FillRule::EvenOdd);
    assert_eq!(fill_options(Fill::NonZero).fill_rule, FillRule::NonZero);
    assert_eq!(fill_options(Fill::NonZero).tolerance, FILL_TOLERANCE_PX);
}

#[test]
fn open_subpath_fills_closed() {
    let mut tri = BezPath::new();
    tri.move_to((10.0, 10.0));
    tri.line_to((90.0, 10.0));
    tri.line_to((50.0, 90.0));
    let plan = fill_scene(&tri, Fill::NonZero);
    assert!(covers(&plan, Point::new(50.0, 110.0 / 3.0)));
}

#[test]
fn two_subpaths_both_fill() {
    let mut two = BezPath::new();
    square(&mut two, Rect::new(10.0, 10.0, 30.0, 30.0));
    square(&mut two, Rect::new(60.0, 60.0, 80.0, 80.0));
    let plan = fill_scene(&two, Fill::NonZero);
    assert!(covers(&plan, Point::new(20.0, 20.0)));
    assert!(covers(&plan, Point::new(70.0, 70.0)));
    assert!(!covers(&plan, Point::new(45.0, 45.0)));
}

/// A NaN start point: lyon's sweep reaches it first and returns `PositionIsNaN`. (A NaN
/// reached later trips a lyon `debug_assert!` instead — one more reason R02 filters first.)
fn nan_triangle() -> [PathEl; 3] {
    [
        PathEl::MoveTo(Point::new(f64::NAN, f64::NAN)),
        PathEl::LineTo(Point::new(10.0, 10.0)),
        PathEl::LineTo(Point::new(0.0, 10.0)),
    ]
}

/// A plan holding one good fill, then the fill entry driven with a NaN point (bypassing R02).
fn fill_after_good(stats: &mut FrameStats) -> (FramePlan, Option<Range<u32>>, (usize, usize)) {
    let mut plan = FramePlan::default();
    let mut tess = PathTessellator::default();
    let good = Rect::new(0.0, 0.0, 10.0, 10.0).path_elements(0.1);
    tess.fill(&mut plan, stats, good, Affine::IDENTITY, Fill::NonZero, [1.0; 4]).expect("good fill");
    let before = (plan.solid_vertices.len(), plan.solid_indices.len());
    let bad =
        tess.fill(&mut plan, stats, nan_triangle().into_iter(), Affine::IDENTITY, Fill::NonZero, [1.0; 4]);
    (plan, bad, before)
}

#[test]
fn tessellation_error_truncates() {
    let (plan, bad, before) = fill_after_good(&mut FrameStats::default());
    assert!(bad.is_none());
    assert_eq!((plan.solid_vertices.len(), plan.solid_indices.len()), before);
    assert!(plan.cmds.is_empty());
}

#[test]
fn lyon_error_records_tessellation_error() {
    let mut stats = FrameStats::default();
    let (plan, bad, before) = fill_after_good(&mut stats);
    assert!(bad.is_none());
    assert_eq!((plan.solid_vertices.len(), plan.solid_indices.len()), before);
    assert!(plan.cmds.is_empty());
    assert_eq!(stats.unsupported[Unsupported::TessError as usize], 1);
    assert_eq!(stats.fallbacks, 1);
}

#[test]
fn tessellator_is_reused() {
    let mut plan = FramePlan::default();
    let mut tess = PathTessellator::default();
    let mut stats = FrameStats::default();
    let addr: *const FillTessellator = &tess.fill;
    for i in 0..1000 {
        plan.clear();
        let r = Rect::new(0.0, 0.0, 10.0 + f64::from(i % 7), 10.0).path_elements(0.1);
        tess.fill(&mut plan, &mut stats, r, Affine::IDENTITY, Fill::NonZero, [1.0; 4]).expect("fill");
    }
    assert!(std::ptr::eq(addr, &tess.fill));
    assert_eq!(stats.fallbacks, 0);
}
