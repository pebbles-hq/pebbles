//! Prepare: walk the shared display list into an ordered [`FramePlan`] on the CPU (no device).

use kurbo::Affine;

use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

use crate::stats::{FrameStats, Unsupported};

mod clip;
pub(crate) mod color;
mod glyphs;
mod images;
mod layer;
pub(crate) mod num;
mod plan;
#[cfg(test)]
mod property_tests;
mod shadows;
#[cfg(test)]
mod snapshot_tests;
mod stroke;
mod tessellate;

pub(crate) use images::{ImageUpload, image_uploads};
pub(crate) use plan::{DrawCmd, FramePlan, GlyphRun, ScissorRect};

/// Path-flattening tolerance for recorded shapes (logical px) — a local copy, because
/// `display_list::TOL` is `pub(crate)` to pebbles-render.
pub(crate) const TOL: f64 = 0.1;

/// Per-frame prepare inputs.
pub(crate) struct PrepareInput {
    pub target_size: (u32, u32),
    /// Logical → device transform for the whole scene (identity during the transition).
    pub root_transform: Affine,
    /// Most vertices one frame may hold: `u32::MAX` with `FULL_DRAW_INDEX_UINT32`, else 2^24 − 1.
    pub vertex_limit: u32,
}

/// Vertex limit on devices without `FULL_DRAW_INDEX_UINT32` (WebGL2).
pub(crate) const VERTEX_LIMIT_NO_FULL_INDEX: u32 = 16_777_215;

/// Reusable per-Renderer prepare state (zero steady-state allocation).
#[derive(Default)]
pub(crate) struct PrepareScratch {
    pub plan: FramePlan,
    pub clips: clip::ClipStack,
    pub layers: layer::LayerStack,
    pub stroke: stroke::StrokeScratch,
}

/// Walk `scene` in painter's order into `scratch.plan`, recording fallbacks into `stats`.
pub(crate) fn prepare<'s>(
    scene: &Scene,
    input: &PrepareInput,
    scratch: &'s mut PrepareScratch,
    stats: &mut FrameStats,
) -> &'s FramePlan {
    scratch.plan.clear();
    scratch.plan.target_size = input.target_size;
    scratch.clips.reset(input.target_size);
    scratch.layers.reset();
    let mut walk = Walk {
        plan: &mut scratch.plan,
        clips: &mut scratch.clips,
        layers: &mut scratch.layers,
        stroke: &mut scratch.stroke,
        stats,
        visit: 0,
        vertex_limit: input.vertex_limit,
        out_of_vertices: false,
    };
    walk.emit(scene.ops(), input.root_transform);
    &scratch.plan
}

/// The state of one prepare walk.
struct Walk<'a> {
    plan: &'a mut FramePlan,
    clips: &'a mut clip::ClipStack,
    layers: &'a mut layer::LayerStack,
    stroke: &'a mut stroke::StrokeScratch,
    stats: &'a mut FrameStats,
    visit: u32,
    vertex_limit: u32,
    out_of_vertices: bool,
}

fn finite_affine(xf: Affine) -> bool {
    xf.as_coeffs().iter().all(|c| c.is_finite())
}

fn finite_rect(b: kurbo::Rect) -> bool {
    [b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite())
}

impl Walk<'_> {
    /// Whether draws at the current position are hidden (culled clip or skipped layer).
    /// Counts culled ops.
    fn hidden(&mut self) -> bool {
        if self.layers.is_skipped() {
            return true;
        }
        if self.clips.is_culled() {
            self.plan.stats.culled += 1;
            return true;
        }
        false
    }

    /// Group alpha for the current draw; records `LayerAlpha` when it folds a fraction.
    fn alpha(&mut self) -> f32 {
        let a = self.layers.group_alpha();
        if a < 1.0 {
            self.stats.record(Unsupported::LayerAlpha);
        }
        a
    }

    /// Whether the frame's vertex budget is spent (records `VertexLimit` once per frame).
    fn vertex_budget_spent(&mut self) -> bool {
        if !self.out_of_vertices && self.plan.solid_vertices.len() as u64 >= u64::from(self.vertex_limit) {
            self.out_of_vertices = true;
            self.stats.record(Unsupported::VertexLimit);
        }
        self.out_of_vertices
    }

    fn emit(&mut self, ops: &[Op], outer: Affine) {
        for op in ops {
            self.visit += 1;
            let idx = self.visit;
            self.plan.stats.ops += 1;
            match op {
                Op::Fill { style, transform, brush, shape, .. } => {
                    if self.hidden() {
                        continue;
                    }
                    let xf = outer * *transform;
                    let color = match brush {
                        peniko::Brush::Solid(c) => *c,
                        peniko::Brush::Gradient(_) => {
                            self.stats.record(Unsupported::GradientBrush);
                            continue;
                        }
                        peniko::Brush::Image(_) => {
                            self.stats.record(Unsupported::ImageBrush);
                            continue;
                        }
                    };
                    if !finite_affine(xf) || !finite_rect(shape.bounding_box()) {
                        self.stats.record(Unsupported::NonFinite);
                        continue;
                    }
                    if self.vertex_budget_spent() {
                        continue;
                    }
                    let path = shape.to_path(TOL);
                    let rule = match style {
                        peniko::Fill::EvenOdd => lyon_tessellation::FillRule::EvenOdd,
                        peniko::Fill::NonZero => lyon_tessellation::FillRule::NonZero,
                    };
                    let k = self.alpha();
                    let fill =
                        tessellate::fill_premul(self.plan, &path, xf, rule, color::premul_scaled(color, k));
                    self.plan.stats.tess_calls += 1;
                    match fill {
                        Some(indices) => {
                            self.push(DrawCmd::Solid { indices, clip: self.clips.current() }, idx)
                        }
                        None => self.stats.record(Unsupported::TessError),
                    }
                }
                Op::Stroke { style, transform, brush, shape, .. } => {
                    if self.hidden() {
                        continue;
                    }
                    let xf = outer * *transform;
                    let color = match brush {
                        peniko::Brush::Solid(c) => *c,
                        peniko::Brush::Gradient(_) => {
                            self.stats.record(Unsupported::GradientBrush);
                            continue;
                        }
                        peniko::Brush::Image(_) => {
                            self.stats.record(Unsupported::ImageBrush);
                            continue;
                        }
                    };
                    if !finite_affine(xf) || !finite_rect(shape.bounding_box()) || !style.width.is_finite() {
                        self.stats.record(Unsupported::NonFinite);
                        continue;
                    }
                    if self.vertex_budget_spent() {
                        continue;
                    }
                    let path = shape.to_path(TOL);
                    let outcome = stroke::expand(&path, style, xf, self.stroke);
                    if outcome.drew {
                        let k = self.alpha();
                        let premul = color::premul_scaled(color, outcome.alpha * k);
                        let fill = stroke::fill_outline(self.plan, self.stroke, premul);
                        self.plan.stats.tess_calls += 1;
                        match fill {
                            Some(indices) => {
                                self.push(DrawCmd::Solid { indices, clip: self.clips.current() }, idx)
                            }
                            None => self.stats.record(Unsupported::TessError),
                        }
                    }
                }
                Op::PushLayer { transform, clip, alpha, blend, .. } => {
                    // Every layer's clip is pushed — including alpha < 1 or a non-default blend
                    // (defect D06's "layer clip lost").
                    let decision = clip::classify_clip(clip, outer * *transform, &self.clips.current());
                    if decision.kind == clip::ClipKind::Bounds && !self.layers.is_skipped() {
                        self.stats.record(Unsupported::NonRectClip);
                    }
                    self.clips.push(decision);
                    let blended = *blend != peniko::BlendMode::default();
                    if blended && !self.layers.is_skipped() {
                        self.stats.record(Unsupported::LayerBlend);
                    }
                    // alpha == 0 is exact (nothing visible), so it is skipped without a log.
                    let alpha = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 1.0 };
                    self.layers.push(alpha, blended || alpha == 0.0);
                }
                Op::PopLayer => {
                    if self.layers.pop() {
                        self.clips.pop();
                    }
                }
                Op::Glyphs {
                    font,
                    font_size,
                    font_weight,
                    brush,
                    transform,
                    glyph_transform,
                    glyphs,
                    ..
                } => {
                    if self.hidden() {
                        continue;
                    }
                    let base = match brush {
                        peniko::Brush::Solid(c) => *c,
                        peniko::Brush::Gradient(g) => {
                            self.stats.record(Unsupported::GradientText);
                            g.stops.first().map_or(peniko::Color::TRANSPARENT, |s| s.color.to_alpha_color())
                        }
                        peniko::Brush::Image(_) => {
                            self.stats.record(Unsupported::ImageBrush);
                            continue;
                        }
                    };
                    let xf = outer * *transform;
                    if !finite_affine(xf) || !font_size.is_finite() {
                        self.stats.record(Unsupported::NonFinite);
                        continue;
                    }
                    let k = self.alpha();
                    let color = color::premul_scaled(base, k);
                    let range = glyphs::glyph_run(
                        self.plan,
                        font,
                        *font_size,
                        *font_weight,
                        color,
                        xf,
                        *glyph_transform,
                        glyphs,
                    );
                    self.push(DrawCmd::Glyphs { glyphs: range, clip: self.clips.current() }, idx);
                }
                Op::Fragment { transform, ops, .. } => {
                    self.emit(ops, outer * transform.unwrap_or(Affine::IDENTITY));
                }
                Op::Image { image, transform, .. } => {
                    // Zero-size images are skipped silently in both walks (image_uploads aligns on this).
                    if image.image.width == 0 || image.image.height == 0 || self.hidden() {
                        continue;
                    }
                    let xf = outer * *transform;
                    if !finite_affine(xf) {
                        self.stats.record(Unsupported::NonFinite);
                        continue;
                    }
                    let s = &image.sampler;
                    if s.x_extend != peniko::Extend::Pad || s.y_extend != peniko::Extend::Pad {
                        self.stats.record(Unsupported::ImageExtend);
                    }
                    let k = self.alpha() * s.alpha.clamp(0.0, 1.0);
                    let index = num::index_u32(self.plan.images.len());
                    let range = images::image(self.plan, image, xf, index, k);
                    self.push(DrawCmd::Images { images: range, clip: self.clips.current() }, idx);
                }
                Op::BlurredRect { transform, rect, brush, radius, std_dev, .. } => {
                    if self.hidden() {
                        continue;
                    }
                    let xf = outer * *transform;
                    if !finite_affine(xf)
                        || !finite_rect(*rect)
                        || !radius.is_finite()
                        || !std_dev.is_finite()
                    {
                        self.stats.record(Unsupported::NonFinite);
                        continue;
                    }
                    let k = self.alpha();
                    let range =
                        shadows::shadow(self.plan, rect, xf, *radius, *std_dev, brush.multiply_alpha(k));
                    self.push(DrawCmd::Shadows { instances: range, clip: self.clips.current() }, idx);
                }
                // `Op` is `#[non_exhaustive]`: a variant added upstream is skipped and counted.
                #[allow(unreachable_patterns)]
                _ => self.stats.record(Unsupported::UnknownOp),
            }
        }
    }

    fn push(&mut self, cmd: DrawCmd, visit: u32) {
        push_cmd(self.plan, cmd, visit);
    }
}

#[cfg(test)]
fn push_cmd(plan: &mut FramePlan, cmd: DrawCmd, visit: u32) {
    if plan.push_draw(cmd) {
        plan.cmd_ops.push(visit);
    }
}

#[cfg(not(test))]
fn push_cmd(plan: &mut FramePlan, cmd: DrawCmd, _visit: u32) {
    plan.push_draw(cmd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pebbles_render::paint::{Glyph, Painter, Stroke};
    use peniko::{Brush, Color, Fill};

    fn plan_of(scene: &Scene, size: (u32, u32)) -> FramePlan {
        let mut scratch = PrepareScratch::default();
        let input =
            PrepareInput { target_size: size, root_transform: Affine::IDENTITY, vertex_limit: u32::MAX };
        prepare(scene, &input, &mut scratch, &mut FrameStats::default());
        scratch.plan
    }

    fn full(size: (u32, u32)) -> ScissorRect {
        ScissorRect { x0: 0, y0: 0, x1: size.0, y1: size.1 }
    }

    fn glyph() -> Glyph {
        Glyph { id: 36, x: 10.0, y: 10.0 }
    }

    #[test]
    fn shadow_draws_before_card_fill() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.draw_blurred_rounded_rect(
            Affine::IDENTITY,
            kurbo::Rect::new(20.0, 20.0, 180.0, 120.0),
            Color::BLACK,
            12.0,
            8.0,
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(20.0, 20.0, 180.0, 120.0),
        );

        let plan = plan_of(&scene, (200, 160));
        assert_eq!(plan.cmds.len(), 2);
        assert!(matches!(plan.cmds[0], DrawCmd::Shadows { .. }));
        assert!(matches!(plan.cmds[1], DrawCmd::Solid { .. }));
    }

    #[test]
    fn text_after_scrim_draws_after_it() {
        let font = crate::test_support::bundled_font();
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.draw_glyphs(
            &font,
            32.0,
            400,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            std::iter::once(glyph()),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 128),
            None,
            &kurbo::Rect::new(0.0, 0.0, 200.0, 160.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(40.0, 40.0, 160.0, 120.0),
        );
        p.draw_glyphs(
            &font,
            32.0,
            400,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            std::iter::once(Glyph { id: 36, x: 50.0, y: 50.0 }),
        );

        let plan = plan_of(&scene, (200, 160));
        assert_eq!(plan.cmds.len(), 3);
        assert!(matches!(plan.cmds[0], DrawCmd::Glyphs { .. }));
        assert!(matches!(plan.cmds[1], DrawCmd::Solid { .. }));
        assert!(matches!(plan.cmds[2], DrawCmd::Glyphs { .. }));
    }

    #[test]
    fn image_between_fills_keeps_its_slot() {
        let image = peniko::ImageData {
            data: peniko::Blob::from(vec![255u8; 100]),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
            width: 5,
            height: 5,
        };
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
        p.draw_image(&peniko::ImageBrush::new(image), Affine::IDENTITY);
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(20.0, 20.0, 30.0, 30.0),
        );

        let plan = plan_of(&scene, (100, 100));
        assert_eq!(plan.cmds.len(), 3);
        assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
        assert!(matches!(plan.cmds[1], DrawCmd::Images { .. }));
        assert!(matches!(plan.cmds[2], DrawCmd::Solid { .. }));
    }

    #[test]
    fn merge_only_adjacent_same_kind() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(20.0, 20.0, 30.0, 30.0),
        );
        let font = crate::test_support::bundled_font();
        p.draw_glyphs(
            &font,
            32.0,
            400,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            std::iter::once(glyph()),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(40.0, 40.0, 50.0, 50.0),
        );

        let plan = plan_of(&scene, (100, 100));
        assert_eq!(plan.cmds.len(), 3);
        assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
        assert!(matches!(plan.cmds[1], DrawCmd::Glyphs { .. }));
        assert!(matches!(plan.cmds[2], DrawCmd::Solid { .. }));
    }

    #[test]
    fn unbalanced_pop_is_ignored_and_counted() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.pop_layer();
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let plan = plan_of(&scene, (100, 100));
        assert_eq!(plan.cmds.len(), 1);
        assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
        assert_eq!(plan.stats.ops, 2); // the unmatched pop + the fill
    }

    #[test]
    fn fragment_transform_composes() {
        let mut inner = Scene::new();
        Painter::new(&mut inner).fill(
            Fill::NonZero,
            Affine::translate((10.0, 20.0)),
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let mut scene = Scene::new();
        Painter::new(&mut scene).append(&inner, Some(Affine::translate((5.0, 7.0))));

        let plan = plan_of(&scene, (100, 100));
        assert_eq!(plan.cmds.len(), 1);
        assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
        let min_x = plan.solid_vertices.iter().map(|v| v.pos[0]).fold(f32::MAX, f32::min);
        let min_y = plan.solid_vertices.iter().map(|v| v.pos[1]).fold(f32::MAX, f32::min);
        assert!((min_x - 15.0).abs() < 0.5, "composed x should be ~15, got {min_x}");
        assert!((min_y - 27.0).abs() < 0.5, "composed y should be ~27, got {min_y}");
    }

    // — C17 fallback tests —

    fn plan_and_stats(scene: &Scene) -> (FramePlan, FrameStats) {
        let mut scratch = PrepareScratch::default();
        let mut stats = FrameStats::default();
        let input = PrepareInput {
            target_size: (100, 100),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
        };
        prepare(scene, &input, &mut scratch, &mut stats);
        (scratch.plan, stats)
    }

    fn layer(p: &mut Painter<'_>, blend: impl Into<peniko::BlendMode>, alpha: f32) {
        p.push_layer(
            Fill::NonZero,
            blend,
            alpha,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
        );
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
        assert_eq!(stats.unsupported[Unsupported::LayerAlpha as usize], 1);
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
        let alphas: Vec<f32> = plan.solid_vertices.iter().map(|v| v.color[3]).collect();
        assert_eq!(alphas.first(), Some(&0.25));
        assert_eq!(alphas.last(), Some(&0.5));
    }

    #[test]
    fn blend_layer_content_is_skipped_and_counted() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        layer(&mut p, peniko::Mix::Multiply, 1.0);
        red_square(&mut p);
        p.pop_layer();
        red_square(&mut p);
        let (plan, stats) = plan_and_stats(&scene);
        assert_eq!(plan.cmds.len(), 1, "only the square after the blend layer draws");
        assert_eq!(stats.unsupported[Unsupported::LayerBlend as usize], 1);
    }

    #[test]
    fn gradient_fill_is_skipped_and_counted() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.fill(Fill::NonZero, Affine::IDENTITY, &gradient(), None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
        let (plan, stats) = plan_and_stats(&scene);
        assert!(plan.cmds.is_empty());
        assert_eq!(stats.unsupported[Unsupported::GradientBrush as usize], 1);
    }

    #[test]
    fn gradient_text_uses_first_stop() {
        let font = crate::test_support::bundled_font();
        let mut scene = Scene::new();
        Painter::new(&mut scene).draw_glyphs(
            &font,
            16.0,
            400,
            &[],
            &Brush::Gradient(gradient()),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            std::iter::once(glyph()),
        );
        let (plan, stats) = plan_and_stats(&scene);
        assert_eq!(plan.glyph_runs[0].color, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(stats.unsupported[Unsupported::GradientText as usize], 1);
    }

    #[test]
    fn warn_once_logs_once_per_kind() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        for _ in 0..100 {
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &gradient(),
                None,
                &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
            );
        }
        let mut warn = crate::stats::WarnOnce::default();
        let mut lines = 0;
        for _frame in 0..3 {
            let (_, stats) = plan_and_stats(&scene);
            assert_eq!(stats.unsupported[Unsupported::GradientBrush as usize], 100);
            for kind in Unsupported::ALL {
                lines += u32::from(warn.report(kind, stats.unsupported[kind as usize]));
            }
        }
        assert_eq!(lines, 1);
    }

    // — rewritten C05 tests (06 §12.2 row B45) —

    #[test]
    fn solid_fill_tessellates_to_a_single_unclipped_batch() {
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let plan = plan_of(&scene, (100, 100));
        assert_eq!(plan.cmds.len(), 1);
        match &plan.cmds[0] {
            DrawCmd::Solid { clip, .. } => assert_eq!(clip.scissor, full((100, 100))),
            _ => panic!("expected a Solid command"),
        }
    }

    #[test]
    fn rect_clip_partitions_geometry_into_scissored_batches() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.push_layer(
            Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 50.0, 25.0),
        );
        p.pop_layer();
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 255, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let plan = plan_of(&scene, (200, 200));
        assert_eq!(plan.cmds.len(), 2);
        match &plan.cmds[0] {
            DrawCmd::Solid { clip, .. } => {
                assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 100, y1: 50 })
            }
            _ => panic!("expected a clipped Solid"),
        }
        match &plan.cmds[1] {
            DrawCmd::Solid { clip, .. } => assert_eq!(clip.scissor, full((200, 200))),
            _ => panic!("expected an unclipped Solid"),
        }
    }

    #[test]
    fn rotated_clip_is_bounds_clipped() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.push_layer(
            Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::rotate(std::f64::consts::FRAC_PI_4),
            &kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 50.0, 25.0),
        );
        p.pop_layer();

        let plan = plan_of(&scene, (200, 200));
        assert_eq!(plan.cmds.len(), 1);
        match &plan.cmds[0] {
            // A rotated rect is not a pure scissor: it clips to its device bounding box.
            DrawCmd::Solid { clip, .. } => {
                assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 71, y1: 107 })
            }
            _ => panic!("expected a Solid"),
        }
    }

    #[test]
    fn alpha_layer_keeps_its_clip() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.push_layer(
            Fill::NonZero,
            peniko::BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 50.0, 50.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        p.pop_layer();

        let plan = plan_of(&scene, (100, 100));
        assert_eq!(plan.cmds.len(), 1);
        match &plan.cmds[0] {
            DrawCmd::Solid { clip, .. } => {
                assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 50, y1: 50 })
            }
            _ => panic!("expected a Solid"),
        }
    }

    #[test]
    fn dashed_stroke_tessellates_geometry() {
        let mut scene = Scene::new();
        let style = Stroke::new(2.0).with_dashes(0.0, [6.0, 3.0]);
        Painter::new(&mut scene).stroke(
            &style,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 100.0, 40.0),
        );

        let plan = plan_of(&scene, (200, 200));
        assert!(plan.solid_vertices.len() > 4);
    }
}
