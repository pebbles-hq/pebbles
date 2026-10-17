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

pub(crate) use plan::{DrawCmd, FramePlan, ScissorRect};

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
                // `glyph_transform` (synthetic oblique) is drawn upright until the outline route (R20).
                Op::Glyphs { font, font_size, normalized_coords, brush, transform, glyphs, .. } => {
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
                    let range =
                        glyphs::glyph_run(self.plan, font, *font_size, normalized_coords, color, xf, glyphs);
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
                    let range = images::image(self.plan, image, xf, k);
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
mod fallback_tests;
#[cfg(test)]
mod walk_tests;
