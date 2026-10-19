//! Layer scan, classification, fold and offscreen-emission tests (09 §3, L11/L12/L14).

use super::*;
use crate::prepare::PrepareScratch;
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
    // The promoted parent (alpha 1) and its Multiply child (backdrop path) are both offscreen.
    assert_eq!(plan.layers.len(), 2);
    assert_eq!(plan.layers[0].alpha, 1.0);
    assert_eq!(plan.layers[0].path, CompositePath::Over);
    assert_eq!(plan.layers[1].path, CompositePath::Backdrop);
    assert_eq!(plan.layers[1].parent, TargetRef::Layer(0));
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

// ---- L21 / L24: composite path selection (§4.2) ----

fn paths(build: impl FnOnce(&mut Painter<'_>)) -> FramePlan {
    let mut s = Scene::new();
    build(&mut Painter::new(&mut s));
    prepare_scene(&s, 64, 64).0
}

fn path_of(plan: &FramePlan, i: usize) -> CompositePath {
    plan.layers[i].path
}

use crate::prepare::plan::{CompositePath, TargetRef};

#[test]
fn dest_in_without_sdf_is_compose_path() {
    let plan = paths(|p| {
        push(p, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, &r(8.0, 8.0, 56.0, 56.0));
        fill(p, r(0.0, 0.0, 64.0, 64.0));
        p.pop_layer();
    });
    assert_eq!(path_of(&plan, 0), CompositePath::Compose);
    assert_eq!(plan.stats.composites_compose, 1);
    assert!(!plan.root_needs_backdrop);
}

#[test]
fn dest_in_under_rrect_clip_is_backdrop_path() {
    let plan = paths(|p| {
        push(
            p,
            BlendMode::new(Mix::Normal, Compose::DestIn),
            1.0,
            &kurbo::RoundedRect::new(8.0, 8.0, 56.0, 56.0, 8.0),
        );
        fill(p, r(0.0, 0.0, 64.0, 64.0));
        p.pop_layer();
    });
    assert_eq!(path_of(&plan, 0), CompositePath::Backdrop);
    assert_eq!(plan.stats.composites_backdrop, 1);
}

#[test]
fn multiply_layer_is_backdrop_path() {
    let plan = paths(|p| {
        push(p, Mix::Multiply, 1.0, &r(8.0, 8.0, 56.0, 56.0));
        fill(p, r(0.0, 0.0, 64.0, 64.0));
        p.pop_layer();
    });
    assert_eq!(path_of(&plan, 0), CompositePath::Backdrop);
    assert_eq!(plan.layers[0].parent, TargetRef::Root);
}

/// ColorFiltered (§4.7): a pure-clip parent, the child, then `push_layer(Multiply)` + fill:
/// the parent is promoted (Over) and the blend layer is the one Backdrop layer inside it.
#[test]
fn color_filter_pattern_plans_one_backdrop_layer() {
    let plan = paths(|p| {
        push(p, Mix::Normal, 1.0, &r(8.0, 8.0, 56.0, 56.0));
        fill(p, r(8.0, 8.0, 32.0, 56.0));
        push(p, Mix::Multiply, 1.0, &r(8.0, 8.0, 56.0, 56.0));
        fill(p, r(8.0, 8.0, 56.0, 56.0));
        p.pop_layer();
        p.pop_layer();
    });
    let backdrops: Vec<_> = plan.layers.iter().filter(|l| l.path == CompositePath::Backdrop).collect();
    assert_eq!(backdrops.len(), 1);
    assert_eq!(backdrops[0].parent, TargetRef::Layer(0));
    assert_eq!(path_of(&plan, 0), CompositePath::Over);
    assert_eq!(plan.stats.layers_promoted, 1);
    assert!(!plan.root_needs_backdrop, "the backdrop is the promoted layer, not the root");
}

/// ShaderMask (§4.7): `push_layer(Normal, 1)` child `push_layer((Normal, DestIn))` gradient
/// fill pop pop → a promoted outer (Over) and a Compose inner.
#[test]
fn shader_mask_plans_promoted_outer_and_compose_inner() {
    let plan = paths(|p| {
        push(p, Mix::Normal, 1.0, &r(0.0, 0.0, 64.0, 64.0));
        fill(p, r(8.0, 8.0, 56.0, 56.0));
        push(p, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, &r(0.0, 0.0, 64.0, 64.0));
        fill(p, r(0.0, 0.0, 32.0, 64.0));
        p.pop_layer();
        p.pop_layer();
    });
    assert_eq!(plan.layers.len(), 2);
    assert_eq!(path_of(&plan, 0), CompositePath::Over);
    assert_eq!(path_of(&plan, 1), CompositePath::Compose);
    assert_eq!(plan.stats.layers_promoted, 1);
}

#[test]
fn root_needs_backdrop_only_for_root_parented_backdrop_layers() {
    let nested = paths(|p| {
        push(p, Mix::Normal, 1.0, &r(0.0, 0.0, 64.0, 64.0));
        fill(p, r(8.0, 8.0, 56.0, 56.0));
        push(p, Mix::Screen, 1.0, &r(0.0, 0.0, 64.0, 64.0));
        fill(p, r(0.0, 0.0, 32.0, 64.0));
        p.pop_layer();
        p.pop_layer();
    });
    assert!(!nested.root_needs_backdrop);
    let root = paths(|p| {
        push(p, Mix::Screen, 1.0, &r(0.0, 0.0, 64.0, 64.0));
        fill(p, r(0.0, 0.0, 32.0, 64.0));
        p.pop_layer();
    });
    assert!(root.root_needs_backdrop);
}

// ---- S12: layer caps (11 §2.5) ----

#[test]
fn layer_target_never_exceeds_target_size() {
    let plan = paths(|p| {
        push(p, Mix::Normal, 0.5, &r(-500.0, -500.0, 5000.0, 5000.0));
        fill(p, r(0.0, 0.0, 64.0, 64.0));
        fill(p, r(10.0, 10.0, 30.0, 30.0));
        p.pop_layer();
    });
    assert_eq!(plan.layers[0].bounds, ScissorRect { x0: 0, y0: 0, x1: 64, y1: 64 });
}

fn nested_groups(depth: usize) -> FramePlan {
    paths(|p| {
        for _ in 0..depth {
            push(p, Mix::Normal, 0.9, &r(0.0, 0.0, 64.0, 64.0));
            fill(p, r(0.0, 0.0, 32.0, 32.0));
        }
        fill(p, r(16.0, 16.0, 48.0, 48.0));
        for _ in 0..depth {
            p.pop_layer();
        }
    })
}

#[test]
fn nesting_beyond_depth_cap_flattens() {
    // 40 nested groups: the live-target cap (16) flattens first; with it raised, the depth
    // cap (32) flattens the rest.
    let mut s = Scene::new();
    {
        let mut p = Painter::new(&mut s);
        for _ in 0..40 {
            push(&mut p, Mix::Normal, 0.9, &r(0.0, 0.0, 64.0, 64.0));
            fill(&mut p, r(0.0, 0.0, 32.0, 32.0));
        }
        fill(&mut p, r(16.0, 16.0, 48.0, 48.0));
        for _ in 0..40 {
            p.pop_layer();
        }
    }
    let mut limits = crate::test_support::webgl2_limits();
    limits.max_layer_targets = 64;
    let mut scratch = PrepareScratch::default();
    let mut stats = crate::stats::FrameStats::default();
    let input = crate::prepare::PrepareInput {
        target_size: (64, 64),
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
    };
    crate::prepare::prepare(&s, &input, &limits, &mut scratch, &mut stats);
    assert_eq!(scratch.plan.layers.len(), limits.max_layer_depth as usize);
    assert_eq!(stats.unsupported[crate::stats::Unsupported::LayerDepthExceeded as usize], 40 - 32);
    // The flattened layers' content still draws.
    assert!(scratch.plan.cmds.len() > 40);
}

#[test]
fn pool_exhaustion_flattens_deeper_layers() {
    let plan = nested_groups(20);
    assert_eq!(plan.layers.len(), crate::limits::MAX_LAYER_TARGETS as usize);
    assert_eq!(plan.stats.layers_offscreen, 16);
}

// ---- L61: effect layers ----

#[test]
fn reach_grows_bounds() {
    use pebbles_render::paint::display_list::{BlurEdge, LayerEffect};
    let plan = paths(|p| {
        let e = LayerEffect::Blur { sigma_x: 2.0, sigma_y: 1.0, edge: BlurEdge::Decal };
        p.push_effect_layer(e, Some(&r(20.0, 20.0, 40.0, 40.0)), Affine::IDENTITY);
        fill(p, r(0.0, 0.0, 64.0, 64.0));
        p.pop_layer();
    });
    // reach = ⌈3σ⌉ = (6, 3) around the clip's bbox.
    assert_eq!(plan.layers[0].bounds, ScissorRect { x0: 14, y0: 17, x1: 46, y1: 43 });
    assert!(plan.layers[0].effect.is_some());
    let DrawCmd::Composite { clip, .. } = plan.cmds[plan.cmds.len() - 1] else { panic!() };
    assert_eq!(clip.scissor, ScissorRect { x0: 20, y0: 20, x1: 40, y1: 40 }, "composited under the op clip");
}
