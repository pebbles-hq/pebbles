//! R10 CPU tests: Tier A eligibility, device geometry, ordering and merging.

use kurbo::{Affine, BezPath, Circle, Rect, RoundedRect};
use pebbles_render::paint::display_list::Geometry;
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};

use super::fill_instance;
use crate::prepare::test_util::{cmd_kinds, prepare_scene};

const C: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
const R: Rect = Rect::new(10.0, 10.0, 50.0, 40.0);

fn rrect() -> Geometry {
    Geometry::RoundedRect(RoundedRect::from_rect(R, 6.0))
}

#[test]
fn rect_identity_is_sdf() {
    let i = fill_instance(&Geometry::Rect(R), Affine::IDENTITY, C).expect("eligible");
    assert_eq!(i.outer, [10.0, 10.0, 50.0, 40.0]);
    assert_eq!(i.outer_radii, [0.0; 4]);
    assert_eq!(i.inner, [0.0; 4]);
}

#[test]
fn rrect_uniform_scale_is_sdf() {
    let i =
        fill_instance(&rrect(), Affine::scale(2.0).then_translate((5.0, 0.0).into()), C).expect("eligible");
    assert_eq!(i.outer, [25.0, 20.0, 105.0, 80.0]);
    assert_eq!(i.outer_radii, [12.0; 4]);
}

#[test]
fn rrect_nonuniform_scale_is_tessellated() {
    assert!(fill_instance(&rrect(), Affine::scale_non_uniform(2.0, 1.0), C).is_none());
}

#[test]
fn rect_nonuniform_scale_is_sdf() {
    let i = fill_instance(&Geometry::Rect(R), Affine::scale_non_uniform(2.0, 0.5), C).expect("eligible");
    assert_eq!(i.outer, [20.0, 5.0, 100.0, 20.0]);
}

#[test]
fn rotated_rect_is_tessellated() {
    assert!(fill_instance(&Geometry::Rect(R), Affine::rotate(0.1), C).is_none());
    assert!(fill_instance(&Geometry::Rect(R), Affine::scale_non_uniform(-1.0, 1.0), C).is_none(), "mirror");
}

#[test]
fn gradient_brush_is_tessellated_or_fallback() {
    let mut scene = Scene::new();
    let g = peniko::Gradient::new_linear((0.0, 0.0), (10.0, 0.0))
        .with_stops([Color::BLACK, Color::WHITE].as_slice());
    Painter::new(&mut scene).fill(Fill::NonZero, Affine::IDENTITY, &g, None, &R);
    let (plan, _) = prepare_scene(&scene, 64, 64);
    assert!(plan.shapes.is_empty(), "a gradient never takes the solid Tier A path");
}

#[test]
fn circle_instance_radii_equal_radius() {
    let i = fill_instance(&Geometry::Circle(Circle::new((32.0, 32.0), 10.0)), Affine::scale(2.0), C)
        .expect("eligible");
    assert_eq!(i.outer, [44.0, 44.0, 84.0, 84.0]);
    assert_eq!(i.outer_radii, [20.0; 4]);
}

fn triangle() -> BezPath {
    let mut p = BezPath::new();
    p.move_to((0.0, 0.0));
    p.line_to((20.0, 0.0));
    p.line_to((10.0, 20.0));
    p.close_path();
    p
}

#[test]
fn order_shapes_interleave_with_triangles() {
    let mut scene = Scene::new();
    let p = &mut Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &triangle());
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &RoundedRect::from_rect(R, 6.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &triangle());
    let (plan, _) = prepare_scene(&scene, 64, 64);
    assert_eq!(cmd_kinds(&plan), ["triangles", "shapes", "triangles"]);
}

#[test]
fn consecutive_shapes_merge() {
    let mut scene = Scene::new();
    let p = &mut Painter::new(&mut scene);
    for i in 0..3 {
        let r = Rect::new(2.0 + 20.0 * f64::from(i), 2.0, 18.0 + 20.0 * f64::from(i), 18.0);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &RoundedRect::from_rect(r, 3.0));
    }
    let (plan, _) = prepare_scene(&scene, 64, 64);
    assert_eq!(cmd_kinds(&plan), ["shapes"]);
    assert_eq!(plan.shapes.len(), 3);
}

// ---- Tier A keeps the walk's properties (counterparts of the tessellated-tier tests) ----

#[test]
fn layer_alpha_folds_into_shape_color() {
    let mut scene = Scene::new();
    let p = &mut Painter::new(&mut scene);
    p.push_layer(Fill::NonZero, peniko::Mix::Normal, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(255, 0, 0, 255), None, &R);
    p.pop_layer();
    let (plan, _) = prepare_scene(&scene, 64, 64);
    assert_eq!(plan.shapes[0].color, [0.5, 0.0, 0.0, 0.5]);
}

#[test]
fn offscreen_shape_is_culled() {
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(500.0, 500.0, 600.0, 600.0),
    );
    let (plan, _) = prepare_scene(&scene, 64, 64);
    assert!(plan.shapes.is_empty() && plan.cmds.is_empty());
}

#[test]
fn shape_carries_its_clip_and_huge_rect_is_clamped() {
    let mut scene = Scene::new();
    let p = &mut Painter::new(&mut scene);
    p.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 32.0, 16.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(-1e9, -1e9, 1e9, 1e9));
    p.pop_layer();
    let (plan, _) = prepare_scene(&scene, 64, 64);
    let crate::prepare::DrawCmd::Shapes { clip, .. } = &plan.cmds[0] else {
        panic!("{:?}", cmd_kinds(&plan))
    };
    assert_eq!((clip.scissor.x1, clip.scissor.y1), (32, 16));
    assert_eq!(plan.shapes[0].outer, [-2.0, -2.0, 34.0, 18.0], "clamped to the scissor + 2 px");
}
