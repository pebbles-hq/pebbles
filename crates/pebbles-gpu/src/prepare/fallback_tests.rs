//! C17 fallback tests: layer alpha, blend layers, unsupported brushes, WarnOnce.

use super::*;
use pebbles_render::paint::{Glyph, Painter};
use peniko::{Brush, Color, Fill};

fn glyph() -> Glyph {
    Glyph { id: 36, x: 10.0, y: 10.0 }
}

fn plan_and_stats(scene: &Scene) -> (FramePlan, FrameStats) {
    // The tessellated tier (Tier A off): these tests read alpha from solid vertices.
    let mut scratch = PrepareScratch { sdf_disabled: true, ..PrepareScratch::default() };
    let mut stats = FrameStats::default();
    let input = PrepareInput {
        target_size: (100, 100),
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
    };
    prepare(scene, &input, &crate::test_support::webgl2_limits(), &mut scratch, &mut stats);
    (scratch.plan, stats)
}

fn layer(p: &mut Painter<'_>, blend: impl Into<peniko::BlendMode>, alpha: f32) {
    p.push_layer(Fill::NonZero, blend, alpha, Affine::IDENTITY, &kurbo::Rect::new(0.0, 0.0, 100.0, 100.0));
}

fn red_square(p: &mut Painter<'_>) {
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &kurbo::Rect::new(10.0, 10.0, 20.0, 20.0),
    );
}

fn gradient() -> peniko::Gradient {
    peniko::Gradient::new_linear((0.0, 0.0), (10.0, 0.0))
        .with_stops([Color::from_rgba8(0, 0, 255, 255), Color::from_rgba8(0, 255, 0, 255)].as_slice())
}

#[test]
fn opacity_zero_layer_draws_nothing() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    layer(&mut p, peniko::Mix::Normal, 0.0);
    red_square(&mut p);
    p.pop_layer();
    let (plan, stats) = plan_and_stats(&scene);
    assert!(plan.cmds.is_empty());
    assert_eq!(stats.fallbacks, 0, "alpha 0 is exact, never logged");
}

#[test]
fn half_alpha_layer_multiplies_draw_alpha() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    layer(&mut p, peniko::Mix::Normal, 0.5);
    red_square(&mut p);
    p.pop_layer();
    let (plan, stats) = plan_and_stats(&scene);
    assert_eq!(plan.solid_vertices[0].color, [0.5, 0.0, 0.0, 0.5]);
    // A single-draw layer folds exactly (09 §3.4, L12): nothing unsupported.
    assert_eq!(stats.unsupported[Unsupported::LayerAlpha as usize], 0);
}

#[test]
fn nested_alpha_layers_multiply() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    layer(&mut p, peniko::Mix::Normal, 0.5);
    layer(&mut p, peniko::Mix::Normal, 0.5);
    red_square(&mut p);
    p.pop_layer();
    red_square(&mut p);
    p.pop_layer();
    let (plan, _) = plan_and_stats(&scene);
    // The outer layer has two draws: it goes offscreen (alpha applied once at composite, L14);
    // the inner single-draw layer folds into its fill.
    let alphas: Vec<f32> = plan.solid_vertices.iter().map(|v| v.color[3]).collect();
    assert_eq!(alphas.first(), Some(&0.5));
    assert_eq!(alphas.last(), Some(&1.0));
    assert_eq!(plan.layers.len(), 1);
    assert_eq!(plan.layers[0].alpha, 0.5);
}

#[test]
fn blend_layer_goes_offscreen_through_the_backdrop() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    layer(&mut p, peniko::Mix::Multiply, 1.0);
    red_square(&mut p);
    p.pop_layer();
    red_square(&mut p);
    let (plan, stats) = plan_and_stats(&scene);
    // 09 §4.2: a non-Normal mix composites through the backdrop shader (L23/L24).
    assert_eq!(plan.layers.len(), 1);
    assert_eq!(plan.layers[0].path, plan::CompositePath::Backdrop);
    assert!(plan.root_needs_backdrop);
    assert_eq!(stats.unsupported[Unsupported::LayerBlend as usize], 0);
}

#[test]
fn gradient_fill_is_a_gradient_draw() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, &gradient(), None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
    let (plan, stats) = plan_and_stats(&scene);
    // 09 §5 (L35): the tessellated tier's mesh, painted by the gradient.
    assert!(matches!(
        plan.cmds[..],
        [DrawCmd::Gradient { cover: plan::GradientCover::Mesh(_), gradient: 0, .. }]
    ));
    assert_eq!(plan.gradients.len(), 1);
    assert_eq!(stats.unsupported[Unsupported::GradientBrush as usize], 0);
}

#[test]
fn gradient_text_uses_first_stop() {
    let font = crate::test_support::bundled_font();
    let mut scene = Scene::new();
    Painter::new(&mut scene).draw_glyphs(
        &font,
        16.0,
        &[],
        &Brush::Gradient(gradient()),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(glyph()),
    );
    let (plan, stats) = plan_and_stats(&scene);
    assert_eq!(plan.glyphs[0].color, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(stats.unsupported[Unsupported::GradientText as usize], 1);
}

#[test]
fn warn_once_logs_once_per_kind() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let image = crate::test_support::rgba_image(4, 4, [255; 4]);
    for _ in 0..100 {
        p.fill(Fill::NonZero, Affine::IDENTITY, &image, None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
    }
    let mut warn = crate::stats::WarnOnce::default();
    let mut lines = 0;
    for _frame in 0..3 {
        let (_, stats) = plan_and_stats(&scene);
        assert_eq!(stats.unsupported[Unsupported::ImageBrush as usize], 100);
        for kind in Unsupported::ALL {
            lines += u32::from(warn.report(kind, stats.unsupported[kind as usize]));
        }
    }
    assert_eq!(lines, 1);
}
