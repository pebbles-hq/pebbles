//! Input-hygiene tests (02 R02, 11 S02): non-finite input is skipped and counted, empty and
//! singular input is skipped silently, and the layer stack stays balanced.

use super::*;
use crate::prepare::test_util::{cmd_kinds, prepare_scene};
use crate::stats::Unsupported;
use kurbo::{Affine, BezPath, Point, Rect, Stroke};
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};

const NAN: f64 = f64::NAN;

fn fill(p: &mut Painter<'_>, xf: Affine, shape: &impl kurbo::Shape) {
    p.fill(Fill::NonZero, xf, Color::BLACK, None, shape);
}

fn line() -> BezPath {
    BezPath::from_vec(vec![PathEl::MoveTo(Point::new(10.0, 50.0)), PathEl::LineTo(Point::new(90.0, 50.0))])
}

fn stroke_scene(style: &Stroke) -> Scene {
    let mut scene = Scene::new();
    Painter::new(&mut scene).stroke(style, Affine::IDENTITY, Color::BLACK, None, &line());
    scene
}

/// Assert the expected skip count (and the matching `non-finite-geometry` records) and kinds.
fn check(scene: &Scene, skipped: u32, kinds: &[&str]) {
    let (plan, stats) = prepare_scene(scene, 100, 100);
    assert_eq!(stats.non_finite_skipped, skipped);
    assert_eq!(stats.unsupported[Unsupported::NonFinite as usize], skipped);
    assert_eq!(cmd_kinds(&plan), kinds);
    assert!(plan.solid_vertices.iter().all(|v| v.pos.iter().all(|c| c.is_finite())));
}

#[test]
fn nan_rect_fill_is_skipped() {
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), Affine::IDENTITY, &Rect::new(NAN, 0.0, 10.0, 10.0));
    check(&scene, 1, &[]);
}

#[test]
fn inf_path_point_is_skipped() {
    let mut path = line();
    path.line_to(Point::new(f64::INFINITY, 20.0));
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), Affine::IDENTITY, &path);
    check(&scene, 1, &[]);
}

#[test]
fn nan_affine_fill_is_skipped() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    fill(&mut p, Affine::new([1.0, 0.0, 0.0, 1.0, NAN, 0.0]), &Rect::new(0.0, 0.0, 10.0, 10.0));
    fill(&mut p, Affine::IDENTITY, &Rect::new(0.0, 0.0, 10.0, 10.0));
    check(&scene, 1, &["triangles"]);
}

#[test]
fn nan_stroke_width_is_skipped() {
    check(&stroke_scene(&Stroke::new(NAN)), 1, &[]);
}

#[test]
fn negative_stroke_width_is_skipped() {
    check(&stroke_scene(&Stroke::new(-2.0)), 1, &[]);
}

#[test]
fn zero_width_stroke_draws_nothing() {
    check(&stroke_scene(&Stroke::new(0.0)), 0, &[]);
}

#[test]
fn zero_area_rect_draws_nothing() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    fill(&mut p, Affine::IDENTITY, &Rect::new(10.0, 10.0, 10.0, 50.0));
    fill(&mut p, Affine::IDENTITY, &Rect::new(10.0, 10.0, 50.0, 10.0));
    check(&scene, 0, &[]);
}

#[test]
fn nonfinite_clip_clips_everything_and_stays_balanced() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, NAN, 100.0));
    fill(&mut p, Affine::IDENTITY, &Rect::new(10.0, 10.0, 50.0, 50.0));
    p.pop_layer();
    fill(&mut p, Affine::IDENTITY, &Rect::new(10.0, 10.0, 50.0, 50.0));
    let (plan, _) = prepare_scene(&scene, 100, 100);
    // Only the fill after `pop_layer` produced triangles, under the full-target scissor.
    assert_eq!(plan.cmds.len(), 1);
    let crate::prepare::DrawCmd::Solid { clip, .. } = &plan.cmds[0] else { panic!("{:?}", plan.cmds) };
    assert_eq!(clip.scissor, crate::prepare::ScissorRect::full((100, 100)));
    check(&scene, 1, &["triangles"]);
}

#[test]
fn nonfinite_fragment_transform_skips_fragment() {
    let mut fragment = Scene::new();
    fill(&mut Painter::new(&mut fragment), Affine::IDENTITY, &Rect::new(10.0, 10.0, 50.0, 50.0));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.append(&fragment, Some(Affine::scale(f64::INFINITY)));
    p.append(&fragment, None);
    check(&scene, 1, &["triangles"]);
}

// S02 cases live in their own file (400-line rule) but keep the `prepare::hygiene::tests::` path.
include!("hygiene_s02_tests.rs");
