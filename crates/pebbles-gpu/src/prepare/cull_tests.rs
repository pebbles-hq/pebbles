//! Culling and clamp tests (02 R04).

use super::*;
use crate::prepare::test_util::{cmd_kinds, prepare_scene_tessellated, solid_bbox};
use kurbo::{BezPath, Point};
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};

fn fill(p: &mut Painter<'_>, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &r);
}

#[test]
fn offscreen_rect_is_culled() {
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), Rect::new(10_000.0, 10.0, 10_050.0, 50.0));
    let (plan, _) = prepare_scene_tessellated(&scene, 100, 100);
    assert!(plan.cmds.is_empty());
    assert!(plan.solid_vertices.is_empty());
    assert_eq!(plan.stats.culled, 1);
    #[cfg(debug_assertions)]
    assert_eq!(plan.stats.culled_ops, 1);
}

#[test]
fn partially_visible_rect_is_kept() {
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), Rect::new(90.0, -20.0, 150.0, 20.0));
    let (plan, _) = prepare_scene_tessellated(&scene, 100, 100);
    assert_eq!(cmd_kinds(&plan), ["triangles"]);
}

#[test]
fn stroke_bounds_include_half_width() {
    let mut line = BezPath::new();
    line.move_to((-0.5, 10.0));
    line.line_to((-0.5, 90.0));
    let style = Stroke::new(2.0);
    let b = device_bounds(&Geometry::Path(line.clone()), Affine::IDENTITY, Some(&style));
    // Half-width (1.0) plus the AA margin (1.0) on each side.
    assert!((b.x0 - -2.5).abs() < 1e-9 && (b.x1 - 1.5).abs() < 1e-9, "{b:?}");
    let mut scene = Scene::new();
    Painter::new(&mut scene).stroke(&style, Affine::IDENTITY, Color::BLACK, None, &line);
    let (plan, _) = prepare_scene_tessellated(&scene, 100, 100);
    assert_eq!(cmd_kinds(&plan), ["triangles"]);
}

#[test]
fn culled_op_does_not_break_merging() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    fill(&mut p, Rect::new(10.0, 10.0, 20.0, 20.0));
    fill(&mut p, Rect::new(500.0, 10.0, 520.0, 20.0));
    fill(&mut p, Rect::new(30.0, 30.0, 40.0, 40.0));
    let (plan, _) = prepare_scene_tessellated(&scene, 100, 100);
    assert_eq!(cmd_kinds(&plan), ["triangles"]);
    assert_eq!(plan.stats.culled, 1);
}

#[test]
fn huge_rect_is_clamped() {
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), Rect::new(-1e9, -1e9, 1e9, 1e9));
    let (plan, _) = prepare_scene_tessellated(&scene, 100, 80);
    let b = solid_bbox(&plan);
    assert_eq!(cmd_kinds(&plan), ["triangles"]);
    assert!(b.x0 >= -2.0 && b.y0 >= -2.0 && b.x1 <= 102.0 && b.y1 <= 82.0, "{b:?}");
    assert!(b.x0 <= 0.0 && b.y0 <= 0.0 && b.x1 >= 100.0 && b.y1 >= 80.0, "covers the target: {b:?}");
}

#[test]
fn miter_join_bounds_are_conservative() {
    // A sharp spike: the miter tip extends far beyond half the width.
    let mut spike = BezPath::new();
    spike.move_to((200.0, 600.0));
    spike.line_to((500.0, 300.0));
    spike.line_to((520.0, 600.0));
    for xf in [Affine::IDENTITY, Affine::rotate(0.3).then_scale_non_uniform(1.5, 0.7)] {
        let style = Stroke::new(12.0).with_join(Join::Miter).with_miter_limit(10.0);
        let b = device_bounds(&Geometry::Path(spike.clone()), xf, Some(&style));
        let mut scene = Scene::new();
        Painter::new(&mut scene).stroke(&style, xf, Color::BLACK, None, &spike);
        let (plan, _) = prepare_scene_tessellated(&scene, 2000, 2000);
        let s = solid_bbox(&plan);
        assert!(!plan.solid_vertices.is_empty());
        assert!(b.contains(Point::new(s.x0, s.y0)) && b.contains(Point::new(s.x1, s.y1)), "{s:?} ⊄ {b:?}");
    }
}
