use kurbo::{Affine, BezPath, Circle, Rect, Shape, Stroke};
use pebbles_render::paint::display_list::{Geometry, Op, PathHint};
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};

use super::{MASK_MAX_SEGMENTS, Tier, choose_tier, mask_area, segment_count};

fn path_rect(w: f64, h: f64) -> BezPath {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((w, 0.0));
    p.line_to((w, h));
    p.line_to((0.0, h));
    p.close_path();
    p
}

fn fill_op(shape: &impl Shape, hint: PathHint) -> Op {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.set_path_hint(hint);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, shape);
    scene.ops()[0].clone()
}

fn stroke_op(shape: &impl Shape, style: &Stroke) -> Op {
    let mut scene = Scene::new();
    Painter::new(&mut scene).stroke(style, Affine::IDENTITY, Color::BLACK, None, shape);
    scene.ops()[0].clone()
}

#[test]
fn primitives_are_tier_a() {
    for shape in [Geometry::Rect(Rect::new(0., 0., 10., 10.)), Geometry::Circle(Circle::new((5., 5.), 4.))] {
        let op = match &shape {
            Geometry::Rect(r) => fill_op(r, PathHint::Auto),
            Geometry::Circle(c) => fill_op(c, PathHint::Auto),
            _ => unreachable!(),
        };
        assert_eq!(choose_tier(&op, Affine::IDENTITY), Tier::A);
    }
    let border = stroke_op(&Rect::new(0., 0., 10., 10.), &Stroke::new(2.0));
    assert_eq!(choose_tier(&border, Affine::IDENTITY), Tier::A);
}

#[test]
fn small_path_is_tier_b() {
    let star = path_rect(40.0, 30.0);
    assert_eq!(choose_tier(&fill_op(&star, PathHint::Auto), Affine::IDENTITY), Tier::B);
    let mut zig = BezPath::new();
    zig.move_to((0., 0.));
    zig.line_to((10., 20.));
    zig.line_to((20., 0.));
    assert_eq!(choose_tier(&stroke_op(&zig, &Stroke::new(3.0)), Affine::IDENTITY), Tier::B);
}

#[test]
fn volatile_path_is_tier_c() {
    let p = path_rect(40.0, 30.0);
    assert_eq!(choose_tier(&fill_op(&p, PathHint::Volatile), Affine::IDENTITY), Tier::C);
}

#[test]
fn area_boundary() {
    // Exactly 512² → B; one pixel more → C.
    let exact = fill_op(&path_rect(512.0, 512.0), PathHint::Auto);
    assert_eq!(mask_area(&Geometry::Path(path_rect(512.0, 512.0)), Affine::IDENTITY, None), 262_144);
    assert_eq!(choose_tier(&exact, Affine::IDENTITY), Tier::B);
    let over = fill_op(&path_rect(262_145.0, 1.0), PathHint::Auto);
    assert_eq!(mask_area(&Geometry::Path(path_rect(262_145.0, 1.0)), Affine::IDENTITY, None), 262_145);
    assert_eq!(choose_tier(&over, Affine::IDENTITY), Tier::C);
    // The device transform counts: a 300² path at 2× is 600² → C.
    assert_eq!(choose_tier(&fill_op(&path_rect(300.0, 300.0), PathHint::Auto), Affine::scale(2.0)), Tier::C);
}

#[test]
fn segment_boundary() {
    let poly = |n: u32| {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        for i in 1..n {
            p.line_to((f64::from(i % 2) * 0.5, f64::from(i) * 0.01));
        }
        p.close_path();
        p
    };
    let at = poly(MASK_MAX_SEGMENTS);
    assert_eq!(segment_count(&Geometry::Path(at.clone()), Affine::IDENTITY), MASK_MAX_SEGMENTS);
    assert_eq!(choose_tier(&fill_op(&at, PathHint::Auto), Affine::IDENTITY), Tier::B);
    let over = poly(MASK_MAX_SEGMENTS + 1);
    assert_eq!(choose_tier(&fill_op(&over, PathHint::Auto), Affine::IDENTITY), Tier::C);
}

#[test]
fn choice_is_pure() {
    let op = fill_op(&path_rect(40.0, 30.0), PathHint::Auto);
    let first = choose_tier(&op, Affine::translate((3.25, 1.5)));
    let _ = choose_tier(&fill_op(&path_rect(900.0, 900.0), PathHint::Auto), Affine::IDENTITY);
    assert_eq!(choose_tier(&op, Affine::translate((3.25, 1.5))), first);
}
