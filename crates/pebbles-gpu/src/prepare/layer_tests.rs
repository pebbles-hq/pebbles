//! Layer scan, classification, fold and offscreen-emission tests (09 §3, L11/L12/L14).

use super::*;
use crate::prepare::plan::{DrawCmd, FramePlan, MaskClipId, ScissorRect, SdfClipId};
use crate::prepare::test_util::{cmd_kinds, prepare_scene, prepare_scene_tessellated};
use kurbo::{Affine, Circle, Rect};
use pebbles_render::paint::{Painter, Scene};
use peniko::{BlendMode, Color, Compose, Fill, Mix};

fn fill(p: &mut Painter<'_>, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &r);
}

fn push(p: &mut Painter<'_>, blend: impl Into<BlendMode>, alpha: f32, clip: &impl kurbo::Shape) {
    p.push_layer(Fill::NonZero, blend, alpha, Affine::IDENTITY, clip);
}

fn scan(scene: &Scene) -> Vec<LayerInfo> {
    let mut out = Vec::new();
    scan_layers(scene.ops(), &mut out);
    out
}

fn r(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect::new(x0, y0, x1, y1)
}

fn info(draw_ops: u8) -> LayerInfo {
    LayerInfo { draw_ops, ..LayerInfo::EMPTY }
}

#[test]
fn scan_counts_draw_ops_saturating() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 1.0, &r(0.0, 0.0, 64.0, 64.0));
    for i in 0..3 {
        fill(&mut p, r(f64::from(i), 0.0, 10.0, 10.0));
    }
    p.pop_layer();
    assert_eq!(scan(&s)[0].draw_ops, 2);
}

#[test]
fn scan_marks_direct_child_blend() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 1.0, &r(0.0, 0.0, 64.0, 64.0));
    push(&mut p, Mix::Multiply, 1.0, &r(0.0, 0.0, 64.0, 64.0));
    fill(&mut p, r(0.0, 0.0, 10.0, 10.0));
    p.pop_layer();
    p.pop_layer();
    let out = scan(&s);
    assert!(out[0].direct_child_non_default_blend);
    assert!(!out[0].nested_default_blend);
    assert!(!out[1].default_blend);
}

#[test]
fn scan_aligns_through_fragments() {
    let mut frag = Scene::new();
    {
        let mut p = Painter::new(&mut frag);
        push(&mut p, Mix::Normal, 0.5, &r(0.0, 0.0, 64.0, 64.0));
        fill(&mut p, r(0.0, 0.0, 10.0, 10.0));
        p.pop_layer();
    }
    let mut s = Scene::new();
    s.append(&frag, None);
    s.append(&frag, Some(Affine::translate((8.0, 0.0))));
    let out = scan(&s);
    assert_eq!(out, [info(1), info(1)]);
}

#[test]
fn classify_table() {
    let normal = BlendMode::default();
    let multiply = BlendMode::new(Mix::Multiply, Compose::SrcOver);
    let dest_in = BlendMode::new(Mix::Normal, Compose::DestIn);
    let with_child =
        LayerInfo { direct_child_non_default_blend: true, nested_default_blend: false, ..info(1) };
    let cases = [
        (info(1), normal, 0.0, LayerPlan::Skip),
        (info(0), normal, 0.5, LayerPlan::Skip),
        (info(1), normal, 1.0, LayerPlan::PureClip),
        (with_child, normal, 1.0, LayerPlan::Offscreen { promoted: true }),
        (info(1), normal, 0.5, LayerPlan::Fold { alpha: 0.5 }),
        (info(2), normal, 0.5, LayerPlan::Offscreen { promoted: false }),
        (info(0), dest_in, 0.0, LayerPlan::Offscreen { promoted: false }),
        (info(1), normal, f32::NAN, LayerPlan::Skip),
        (
            LayerInfo { default_blend: false, ..info(1) },
            multiply,
            1.5,
            LayerPlan::Offscreen { promoted: false },
        ),
    ];
    for (i, (info, blend, alpha, want)) in cases.into_iter().enumerate() {
        assert_eq!(classify_layer(info, blend, alpha), want, "case {i}");
    }
}

#[test]
fn skip_layer_emits_nothing() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 0.0, &r(0.0, 0.0, 64.0, 64.0));
    for i in 0..3 {
        fill(&mut p, r(f64::from(i), 0.0, 10.0, 10.0));
    }
    p.pop_layer();
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert!(plan.cmds.is_empty(), "{:?}", cmd_kinds(&plan));
}

// ---- L12: the alpha fold (§3.4) ----

/// Every emitted alpha of `plan` (vertex colors, instance colors).
fn alphas(plan: &FramePlan) -> Vec<f32> {
    let mut out: Vec<f32> = plan.solid_vertices.iter().map(|v| v.color[3]).collect();
    out.extend(plan.shapes.iter().map(|s| s.color[3]));
    out
}

fn folded(alpha: f32, draw: impl FnOnce(&mut Painter<'_>)) -> FramePlan {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, alpha, &r(0.0, 0.0, 64.0, 64.0));
    draw(&mut p);
    p.pop_layer();
    prepare_scene(&s, 64, 64).0
}

#[test]
fn fold_scales_solid_vertex_color() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 0.5, &r(0.0, 0.0, 64.0, 64.0));
    fill(&mut p, r(4.0, 4.0, 40.0, 40.0));
    p.pop_layer();
    let plan = prepare_scene_tessellated(&s, 64, 64).0;
    assert!(!plan.solid_vertices.is_empty());
    for v in &plan.solid_vertices {
        assert_eq!(v.color, [0.0, 0.0, 0.0, 0.5]);
    }
    assert!(!plan.cmds.iter().any(|c| matches!(c, DrawCmd::PushLayer { .. })));
}

#[test]
fn fold_scales_every_draw_kind() {
    let half = |a: f32| (a - 0.5).abs() < 1e-6;
    // rrect SDF
    let plan = folded(0.5, |p| {
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &kurbo::RoundedRect::new(4.0, 4.0, 40.0, 40.0, 6.0),
        )
    });
    assert!(!plan.shapes.is_empty() && plan.shapes.iter().all(|s| half(s.color[3])), "rrect");
    // glyph run
    let plan = folded(0.5, |p| {
        p.draw_glyphs(
            &crate::test_support::bundled_font(),
            16.0,
            &[],
            &peniko::Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            [pebbles_render::paint::Glyph { id: 36, x: 10.0, y: 20.0 }].into_iter(),
        )
    });
    assert!(!plan.glyphs.is_empty() && plan.glyphs.iter().all(|g| half(g.color[3])), "glyphs");
    // image
    let plan =
        folded(0.5, |p| p.draw_image(&crate::test_support::rgba_image(8, 8, [255; 4]), Affine::IDENTITY));
    assert!(!plan.images.is_empty() && plan.images.iter().all(|i| half(i.quad.color[3])), "image");
    // shadow
    let plan = folded(0.5, |p| {
        p.draw_blurred_rounded_rect(Affine::IDENTITY, r(8.0, 8.0, 40.0, 40.0), Color::BLACK, 4.0, 3.0)
    });
    assert!(!plan.shadows.is_empty() && plan.shadows.iter().all(|s| half(s.color[3])), "shadow");
}

#[test]
fn nested_folds_multiply() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 0.5, &r(0.0, 0.0, 64.0, 64.0));
    push(&mut p, Mix::Normal, 0.5, &r(0.0, 0.0, 64.0, 64.0));
    fill(&mut p, r(4.0, 4.0, 20.0, 20.0));
    p.pop_layer();
    p.pop_layer();
    let (plan, _) = prepare_scene(&s, 64, 64);
    let a = alphas(&plan);
    assert!(!a.is_empty() && a.iter().all(|&a| (a - 0.25).abs() < 1e-6), "{a:?}");
}

#[test]
fn fold_resets_after_pop() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 0.5, &r(0.0, 0.0, 64.0, 64.0));
    fill(&mut p, r(4.0, 4.0, 20.0, 20.0));
    p.pop_layer();
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &r(30.0, 30.0, 40.0, 40.0));
    let (plan, _) = prepare_scene(&s, 64, 64);
    let a = alphas(&plan);
    assert_eq!(a.last().copied(), Some(1.0), "{a:?}");
    assert!(a.contains(&0.5), "{a:?}");
}

// ---- L14: offscreen Over layers (§3.5) ----

fn two_fills(clip: &impl kurbo::Shape, alpha: f32) -> FramePlan {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, alpha, clip);
    fill(&mut p, r(4.0, 4.0, 40.0, 40.0));
    fill(&mut p, r(20.0, 20.0, 60.0, 60.0));
    p.pop_layer();
    prepare_scene(&s, 64, 64).0
}

#[test]
fn opacity_two_fills_goes_offscreen() {
    let plan = two_fills(&r(0.0, 0.0, 48.0, 48.0), 0.5);
    // The two fills share a clip and merge into one instanced Shapes draw (C13).
    assert_eq!(cmd_kinds(&plan), ["push_layer", "shapes", "pop_layer", "composite"]);
    assert_eq!(plan.layers.len(), 1);
    assert_eq!(plan.layers[0].alpha, 0.5);
    assert_eq!(plan.layers[0].bounds, ScissorRect { x0: 0, y0: 0, x1: 48, y1: 48 });
}

#[test]
fn offscreen_own_circle_clip_is_sdf_at_composite() {
    let plan = two_fills(&Circle::new((32.0, 32.0), 20.0), 0.5);
    assert_eq!(plan.cmds.len(), 4);
    let DrawCmd::Composite { clip, .. } = plan.cmds[3] else { panic!("{:?}", cmd_kinds(&plan)) };
    assert_ne!(clip.sdf, SdfClipId::NONE);
}

#[test]
fn offscreen_own_path_clip_masks_composite() {
    let mut star = kurbo::BezPath::new();
    star.move_to((32.0, 2.0));
    star.line_to((50.0, 60.0));
    star.line_to((4.0, 22.0));
    star.line_to((60.0, 22.0));
    star.line_to((14.0, 60.0));
    star.close_path();
    let plan = two_fills(&star, 0.5);
    assert_eq!(plan.cmds.len(), 4, "{:?}", cmd_kinds(&plan));
    let DrawCmd::Composite { clip, .. } = plan.cmds[3] else { panic!("{:?}", cmd_kinds(&plan)) };
    assert_ne!(clip.mask, MaskClipId::NONE);
}

#[test]
fn promoted_clip_goes_offscreen_with_alpha_1() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    push(&mut p, Mix::Normal, 1.0, &r(0.0, 0.0, 48.0, 48.0));
    push(&mut p, Mix::Multiply, 1.0, &r(0.0, 0.0, 48.0, 48.0));
    fill(&mut p, r(4.0, 4.0, 40.0, 40.0));
    p.pop_layer();
    p.pop_layer();
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.layers.len(), 1);
    assert_eq!(plan.layers[0].alpha, 1.0);
    assert_eq!(plan.stats.layers_promoted, 1);
}

#[test]
fn layer_content_uses_fresh_clip_state() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    // A parent SDF clip, then an offscreen layer inside it.
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Circle::new((32.0, 32.0), 30.0));
    push(&mut p, Mix::Normal, 0.5, &r(8.0, 8.0, 56.0, 56.0));
    fill(&mut p, r(4.0, 4.0, 40.0, 40.0));
    fill(&mut p, r(20.0, 20.0, 60.0, 60.0));
    p.pop_layer();
    p.pop_layer();
    let (plan, _) = prepare_scene(&s, 64, 64);
    let bounds = plan.layers[0].bounds;
    let inner: Vec<_> = plan
        .cmds
        .iter()
        .filter_map(|c| match c {
            DrawCmd::Shapes { clip, .. } | DrawCmd::Solid { clip, .. } => Some(*clip),
            _ => None,
        })
        .collect();
    assert!(!inner.is_empty());
    for clip in inner {
        assert_eq!(clip.scissor, bounds);
        assert_eq!(clip.sdf, SdfClipId::NONE);
        assert_eq!(clip.mask, MaskClipId::NONE);
    }
    let Some(DrawCmd::Composite { clip, .. }) = plan.cmds.last() else { panic!() };
    assert_ne!(clip.sdf, SdfClipId::NONE);
}
