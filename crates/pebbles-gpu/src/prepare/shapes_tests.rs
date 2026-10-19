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
    let i = fill_instance(&Geometry::Rect(R), Affine::IDENTITY, C, false).expect("eligible");
    assert_eq!(i.outer, [10.0, 10.0, 50.0, 40.0]);
    assert_eq!(i.outer_radii, [0.0; 4]);
    assert_eq!(i.inner, [0.0; 4]);
}

#[test]
fn rrect_uniform_scale_is_sdf() {
    let i = fill_instance(&rrect(), Affine::scale(2.0).then_translate((5.0, 0.0).into()), C, false)
        .expect("eligible");
    assert_eq!(i.outer, [25.0, 20.0, 105.0, 80.0]);
    assert_eq!(i.outer_radii, [12.0; 4]);
}

#[test]
fn rrect_nonuniform_scale_is_tessellated() {
    assert!(fill_instance(&rrect(), Affine::scale_non_uniform(2.0, 1.0), C, false).is_none());
}

#[test]
fn rect_nonuniform_scale_is_sdf() {
    let i =
        fill_instance(&Geometry::Rect(R), Affine::scale_non_uniform(2.0, 0.5), C, false).expect("eligible");
    assert_eq!(i.outer, [20.0, 5.0, 100.0, 20.0]);
}

#[test]
fn rotated_rect_is_tessellated() {
    assert!(fill_instance(&Geometry::Rect(R), Affine::rotate(0.1), C, false).is_none());
    assert!(
        fill_instance(&Geometry::Rect(R), Affine::scale_non_uniform(-1.0, 1.0), C, false).is_none(),
        "mirror"
    );
}

#[test]
fn gradient_brush_takes_the_shape_cover() {
    let mut scene = Scene::new();
    let g = peniko::Gradient::new_linear((0.0, 0.0), (10.0, 0.0))
        .with_stops([Color::BLACK, Color::WHITE].as_slice());
    Painter::new(&mut scene).fill(Fill::NonZero, Affine::IDENTITY, &g, None, &R);
    let (plan, _) = prepare_scene(&scene, 64, 64);
    // 09 §6 brush × tier rule: a Tier A rect keeps its analytic coverage; the gradient paints it.
    assert!(matches!(
        plan.cmds[..],
        [crate::prepare::DrawCmd::Brush { cover: crate::prepare::plan::BrushCover::Shape(_), .. }]
    ));
    assert_eq!(plan.shapes[0].color, [1.0; 4], "white × alpha: coverage only");
}

#[test]
fn circle_instance_radii_equal_radius() {
    let i = fill_instance(&Geometry::Circle(Circle::new((32.0, 32.0), 10.0)), Affine::scale(2.0), C, false)
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
    // Volatile paths are tessellated (Tier C, R16); an Auto path is a coverage mask (Tier B).
    p.set_path_hint(pebbles_render::paint::display_list::PathHint::Volatile);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &triangle());
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &RoundedRect::from_rect(R, 6.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &triangle());
    p.set_path_hint(pebbles_render::paint::display_list::PathHint::Auto);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &triangle());
    let (plan, _) = prepare_scene(&scene, 64, 64);
    assert_eq!(cmd_kinds(&plan), ["triangles", "shapes", "triangles", "glyphs"]);
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

// ---- R11: strokes ----

use super::stroke_instance;
use kurbo::{Join, Stroke};

#[test]
fn uniform_rrect_border_is_sdf() {
    let i = stroke_instance(&rrect(), Affine::IDENTITY, &Stroke::new(2.0), C, false).expect("eligible");
    assert_eq!(i.outer, [9.0, 9.0, 51.0, 41.0]);
    assert_eq!(i.inner, [11.0, 11.0, 49.0, 39.0]);
    assert_eq!(i.outer_radii, [7.0; 4]);
    assert_eq!(i.inner_radii, [5.0; 4]);
}

#[test]
fn dashed_border_is_tessellated() {
    assert!(
        stroke_instance(&rrect(), Affine::IDENTITY, &Stroke::new(2.0).with_dashes(0.0, [4.0, 2.0]), C, false)
            .is_none()
    );
}

#[test]
fn bevel_join_zero_radius_is_tessellated() {
    let s = Stroke::new(2.0).with_join(Join::Bevel);
    assert!(stroke_instance(&Geometry::Rect(R), Affine::IDENTITY, &s, C, false).is_none());
}

#[test]
fn round_join_zero_radius_outer_radius_is_half_width() {
    let s = Stroke::new(4.0).with_join(Join::Round);
    let i = stroke_instance(&Geometry::Rect(R), Affine::IDENTITY, &s, C, false).expect("eligible");
    assert_eq!(i.outer_radii, [2.0; 4]);
    assert_eq!(i.inner_radii, [0.0; 4]);
}

#[test]
fn miter_join_zero_radius_outer_radius_is_zero() {
    let s = Stroke::new(4.0).with_join(Join::Miter);
    let i = stroke_instance(&Geometry::Rect(R), Affine::IDENTITY, &s, C, false).expect("eligible");
    assert_eq!(i.outer_radii, [0.0; 4]);
}

#[test]
fn thick_stroke_inner_empty_draws_filled() {
    // R is 40×30: a width of 30 consumes the shortest side.
    let i = stroke_instance(&Geometry::Rect(R), Affine::IDENTITY, &Stroke::new(30.0), C, false)
        .expect("eligible");
    assert_eq!(i.inner, [0.0; 4]);
}

#[test]
fn hairline_border_alpha_scaled() {
    let i = stroke_instance(
        &Geometry::Rect(R),
        Affine::IDENTITY,
        &Stroke::new(0.25),
        [1.0, 0.0, 0.0, 1.0],
        false,
    )
    .expect("eligible");
    assert_eq!(i.color, [0.25, 0.0, 0.0, 0.25]);
    assert_eq!(i.outer, [9.5, 9.5, 50.5, 40.5], "drawn 1 px wide");
}

#[test]
fn snapped_fill_edges_round_half_up_and_never_collapse() {
    let r = Geometry::Rect(Rect::new(10.25, 20.5, 30.75, 20.9));
    let i = fill_instance(&r, Affine::IDENTITY, C, true).expect("eligible");
    assert_eq!(
        i.outer,
        [10.0, 20.0, 31.0, 21.0],
        "x: 10.25→10, 30.75→31; y: 20.5 and 20.9 → 21 collapse → 1 px at 20"
    );
    let off = fill_instance(&r, Affine::IDENTITY, C, false).expect("eligible");
    assert_eq!(off.outer, [10.25, 20.5, 30.75, 20.9]);
}

#[test]
fn snapped_border_is_round_wd_whole_pixels() {
    for s in [1.0, 1.25, 1.5, 1.75, 2.0] {
        let i = stroke_instance(
            &Geometry::Rect(Rect::new(10.0, 10.0, 40.0, 30.0)),
            Affine::scale(s),
            &Stroke::new(1.0),
            C,
            true,
        )
        .expect("eligible");
        let w: f32 = if s < 1.5 { 1.0 } else { 2.0 }; // round_half_up(s)
        assert!(
            i.outer.iter().chain(&i.inner).all(|v| v.fract() == 0.0),
            "scale {s}: {:?} {:?}",
            i.outer,
            i.inner
        );
        assert_eq!(i.inner[0] - i.outer[0], w, "scale {s}");
    }
}
