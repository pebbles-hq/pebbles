//! L50: fragment transform composition (09 §7.1) and per-fragment balance (§7.2).

use super::*;
use crate::prepare::test_util::{prepare_scene, prepare_scene_tessellated};
use pebbles_render::paint::Painter;
use peniko::{Color, Fill};

fn one_fill() -> Scene {
    let mut f = Scene::new();
    Painter::new(&mut f).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0),
    );
    f
}

#[test]
fn fragment_appended_twice_translates_geometry() {
    let f = one_fill();
    let mut s = Scene::new();
    s.append(&f, Some(Affine::translate((10.0, 0.0))));
    s.append(&f, Some(Affine::translate((50.0, 0.0))));
    let (plan, _) = prepare_scene_tessellated(&s, 100, 64);
    let n = plan.solid_vertices.len() / 2;
    assert!(n > 0 && plan.solid_vertices.len() == 2 * n);
    for i in 0..n {
        let (a, b) = (plan.solid_vertices[i].pos, plan.solid_vertices[n + i].pos);
        assert_eq!((b[0] - a[0], b[1] - a[1]), (40.0, 0.0), "vertex {i}");
    }
}

#[test]
fn fragment_clip_uses_composed_transform() {
    let mut f = Scene::new();
    {
        let mut p = Painter::new(&mut f);
        p.push_layer(
            Fill::NonZero,
            peniko::Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &kurbo::Rect::new(10.0, 10.0, 40.0, 40.0),
        );
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
        p.pop_layer();
    }
    let mut s = Scene::new();
    s.append(&f, Some(Affine::rotate_about(std::f64::consts::FRAC_PI_6, (25.0, 25.0))));
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.stats.clips_mask, 1, "a rect under rotate(30°) is a mask clip (row 8)");
}

#[test]
fn fragment_gradient_paint_transform_composes() {
    let mut f = Scene::new();
    let g = peniko::Gradient::new_linear((0.0, 0.0), (10.0, 0.0))
        .with_stops([Color::BLACK, Color::WHITE].as_slice());
    Painter::new(&mut f).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &g,
        None,
        &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
    );
    let mut s = Scene::new();
    s.append(&f, Some(Affine::scale(2.0)));
    let (plan, _) = prepare_scene(&s, 64, 64);
    let geom = &plan.gradients[0].geom;
    // t = 0.5 at twice the local midpoint (device x = 10).
    let t = f64::from(geom.m0[0]) * 10.0 + f64::from(geom.m0[1]) * 5.0 + f64::from(geom.m0[2]);
    assert!((t - 0.5).abs() < 1e-6, "{t}");
}

#[test]
fn fragment_unbalanced_push_is_closed_at_fragment_end() {
    let mut f = Scene::new();
    {
        let mut p = Painter::new(&mut f);
        p.push_layer(
            Fill::NonZero,
            peniko::Mix::Normal,
            0.5,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        );
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0));
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(4.0, 4.0, 12.0, 12.0));
    }
    let mut s = Scene::new();
    s.append(&f, None);
    // After the fragment the layer is closed: this fill is in the root, unclipped.
    Painter::new(&mut s).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &kurbo::Rect::new(40.0, 40.0, 60.0, 60.0),
    );
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.stats.autoclosed, 1);
    let kinds = test_util::cmd_kinds(&plan);
    assert_eq!(kinds, ["push_layer", "shapes", "pop_layer", "composite", "shapes"]);
}

#[test]
fn fragment_cannot_pop_outer_layer() {
    let mut f = Scene::new();
    Painter::new(&mut f).pop_layer();
    let mut s = Scene::new();
    {
        let mut p = Painter::new(&mut s);
        p.push_layer(
            Fill::NonZero,
            peniko::Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        );
    }
    s.append(&f, None);
    // Still inside the outer clip: this fill is scissored to it.
    Painter::new(&mut s).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.stats.unbalanced_pops, 1);
    let DrawCmd::Shapes { clip, .. } = &plan.cmds[0] else { panic!("{:?}", plan.cmds) };
    assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 32, y1: 32 });
}

/// L52 AC1: one scene with one of each clip and layer kind → each counter is 1.
#[test]
fn stats_count_each_clip_and_layer_kind() {
    use pebbles_render::paint::display_list::PathHint;
    let star = |r: f64| {
        let mut b = kurbo::BezPath::new();
        for i in 0..5 {
            let a = -std::f64::consts::FRAC_PI_2 + f64::from(i) * 4.0 * std::f64::consts::PI / 5.0;
            let pt = (32.0 + r * a.cos(), 32.0 + r * a.sin());
            if i == 0 { b.move_to(pt) } else { b.line_to(pt) }
        }
        b.close_path();
        b
    };
    let fill = |p: &mut Painter<'_>| {
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0));
    };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let n = peniko::BlendMode::default();
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0)); // noop
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Rect::new(1.0, 1.0, 63.0, 63.0)); // scissor
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Circle::new((32.0, 32.0), 30.0)); // sdf
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &star(30.0)); // mask
    p.set_path_hint(PathHint::Volatile);
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &star(28.0)); // gpu mask
    p.set_path_hint(PathHint::Auto);
    fill(&mut p);
    for _ in 0..5 {
        p.pop_layer();
    }
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Rect::new(100.0, 100.0, 120.0, 120.0)); // culled
    fill(&mut p);
    p.pop_layer();
    p.push_layer(Fill::NonZero, n, 0.0, Affine::IDENTITY, &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0)); // skipped
    fill(&mut p);
    p.pop_layer();
    p.push_layer(Fill::NonZero, n, 0.5, Affine::IDENTITY, &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0)); // folded
    fill(&mut p);
    p.pop_layer();
    // promoted (Over) parent with a Multiply (backdrop) child; a DestIn (compose) layer.
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Rect::new(0.0, 0.0, 16.0, 16.0));
    fill(&mut p);
    p.push_layer(
        Fill::NonZero,
        peniko::Mix::Multiply,
        1.0,
        Affine::IDENTITY,
        &kurbo::Rect::new(0.0, 0.0, 16.0, 16.0),
    );
    fill(&mut p);
    p.pop_layer();
    p.pop_layer();
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::new(peniko::Mix::Normal, peniko::Compose::DestIn),
        1.0,
        Affine::IDENTITY,
        &kurbo::Rect::new(0.0, 0.0, 16.0, 16.0),
    );
    fill(&mut p);
    p.pop_layer();
    p.pop_layer(); // unbalanced
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Rect::new(f64::NAN, 0.0, 8.0, 8.0)); // nonfinite
    p.pop_layer();
    p.push_layer(Fill::NonZero, n, 1.0, Affine::IDENTITY, &kurbo::Rect::new(2.0, 2.0, 60.0, 60.0)); // autoclosed
    let g = peniko::Gradient::new_linear((0.0, 0.0), (8.0, 0.0))
        .with_stops([Color::BLACK, Color::WHITE].as_slice());
    p.fill(Fill::NonZero, Affine::IDENTITY, &g, None, &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0));
    let img = crate::test_support::rgba_image(2, 2, [255; 4]);
    p.fill(Fill::NonZero, Affine::IDENTITY, &img, None, &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &img,
        Some(Affine::scale(0.0)),
        &kurbo::Rect::new(0.0, 0.0, 8.0, 8.0),
    );
    let (plan, _) = prepare_scene(&s, 64, 64);
    let st = &plan.stats;
    // Every layer needs a clip: the layer-kind layers add scissor / no-op clips of their own.
    assert!(
        st.clips_noop >= 1 && st.clips_scissor >= 1,
        "noop {} scissor {}",
        st.clips_noop,
        st.clips_scissor
    );
    let counters = [
        ("clips_sdf", st.clips_sdf),
        ("clips_mask", st.clips_mask),
        ("clips_gpu_mask", st.clips_gpu_mask),
        ("clips_culled", st.clips_culled),
        ("layers_skipped", st.layers_skipped),
        ("layers_folded", st.layers_folded),
        ("layers_promoted", st.layers_promoted),
        ("composites_backdrop", st.composites_backdrop),
        ("composites_compose", st.composites_compose),
        ("unbalanced_pops", st.unbalanced_pops),
        ("nonfinite", st.nonfinite),
        ("autoclosed", st.autoclosed),
        ("gradients", st.gradients),
        ("image_brushes", st.image_brushes),
        ("image_brush_skipped", st.image_brush_skipped),
    ];
    let wrong: Vec<_> = counters.iter().filter(|(_, v)| *v != 1).collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}
