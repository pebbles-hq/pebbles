//! Prepare: walk the shared display list into an ordered [`FramePlan`] on the CPU (no device).
#![cfg_attr(not(test), warn(clippy::indexing_slicing))]

use kurbo::{Affine, Shape};

use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

use path_strategy::Tier;
use peniko::Fill;

use crate::limits::RendererLimits;
use crate::stats::{FrameStats, Unsupported};

mod admit;
mod clip;
pub(crate) mod color;
pub(crate) mod coverage;
mod cull;
mod features;
mod glyphs;
mod hygiene;
mod images;
mod layer;
pub(crate) mod num;
pub(crate) mod path_masks;
pub(crate) mod path_strategy;
mod plan;
#[cfg(test)]
mod property_tests;
mod shadows;
mod shapes;
#[cfg(test)]
mod snapshot_tests;
mod stroke;
pub(crate) mod tessellate;
#[cfg(test)]
mod test_util;
mod tier_a;

#[cfg(test)]
pub(crate) use plan::ClipState;
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
    pub tess: tessellate::PathTessellator,
    /// Debug switch (R10): draw Tier A shapes through tessellation instead (tests, render-check).
    pub sdf_disabled: bool,
}

/// Walk `scene` in painter's order into `scratch.plan`, recording fallbacks into `stats`.
pub(crate) fn prepare<'s>(
    scene: &Scene,
    input: &PrepareInput,
    limits: &RendererLimits,
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
        tess: &mut scratch.tess,
        stats,
        limits,
        visit: 0,
        vertex_limit: input.vertex_limit,
        out_of_vertices: false,
        sdf: !scratch.sdf_disabled,
    };
    walk.emit(scene.ops(), input.root_transform);
    &scratch.plan
}

/// The state of one prepare walk.
struct Walk<'a> {
    plan: &'a mut FramePlan,
    clips: &'a mut clip::ClipStack,
    layers: &'a mut layer::LayerStack,
    tess: &'a mut tessellate::PathTessellator,
    stats: &'a mut FrameStats,
    limits: &'a RendererLimits,
    visit: u32,
    vertex_limit: u32,
    out_of_vertices: bool,
    /// Tier A analytic shapes enabled (R10).
    sdf: bool,
}

impl Walk<'_> {
    fn emit(&mut self, ops: &[Op], outer: Affine) {
        for op in ops {
            self.visit += 1;
            let idx = self.visit;
            self.plan.stats.ops += 1;
            match op {
                Op::Fill { style, transform, brush, brush_transform, shape, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !hygiene::geometry_is_finite(shape) {
                        self.skip_non_finite();
                        continue;
                    }
                    if !self.paint_is_sane(brush, xf)
                        || hygiene::geometry_is_empty(shape)
                        || self.hidden()
                        || self.culled(cull::device_bounds(shape, xf, None))
                    {
                        continue;
                    }
                    self.note_paint(brush, *brush_transform, false);
                    let Some(color) = self.solid_color(brush) else { continue };
                    if self.vertex_budget_spent() {
                        continue;
                    }
                    let premul = color::premul_scaled(color, self.alpha());
                    let tier = self.tier(op, xf);
                    if tier == Tier::B && self.draw_mask(shape, xf, *style, None, premul, idx) {
                        continue;
                    }
                    // Tier A (R10): an eligible rect / rounded rect / circle is one analytic quad.
                    if tier == Tier::A && self.draw_tier_a_fill(op, shape, xf, premul, idx) {
                        continue;
                    }
                    let scissor = self.clips.current().scissor;
                    let fill = if let Some(r) = cull::clamped_rect(shape, xf, scissor) {
                        let els = r.path_elements(TOL);
                        self.tess.fill(self.plan, self.stats, els, Affine::IDENTITY, *style, premul)
                    } else {
                        let path = shape.to_path(TOL * self.tess.tol_scale);
                        let els = path.elements().iter().copied();
                        self.tess.fill(self.plan, self.stats, els, xf, *style, premul)
                    };
                    self.plan.stats.tess_calls += 1;
                    if let Some(indices) = fill {
                        self.stats.tier_c_draws += 1;
                        self.push(DrawCmd::Solid { indices, clip: self.clips.current() }, idx);
                    }
                }
                Op::Stroke { style, transform, brush, brush_transform, shape, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite()
                        || !hygiene::geometry_is_finite(shape)
                        || !style.is_finite()
                        || style.width < 0.0
                    {
                        self.skip_non_finite();
                        continue;
                    }
                    if !self.paint_is_sane(brush, xf)
                        || style.width == 0.0
                        || self.hidden()
                        || self.culled(cull::device_bounds(shape, xf, Some(style)))
                    {
                        continue;
                    }
                    self.note_paint(brush, *brush_transform, true);
                    self.note_stroke(style);
                    let Some(color) = self.solid_color(brush) else { continue };
                    if self.vertex_budget_spent() {
                        continue;
                    }
                    let tier = self.tier(op, xf);
                    let premul = color::premul_scaled(color, self.alpha());
                    if tier == Tier::B && self.draw_mask(shape, xf, Fill::NonZero, Some(style), premul, idx) {
                        continue;
                    }
                    // Tier A (R11): a uniform, undashed stroke of a rect / rrect / circle is one
                    // analytic quad. Bounds past max_coord_px stay on the tessellated path (S04).
                    if tier == Tier::A && self.draw_tier_a_stroke(op, shape, style, xf, premul, idx) {
                        continue;
                    }
                    // Flatten at the scale of the path itself: S04 clamps a pathological width to
                    // just cover the target, so the width must not coarsen the tolerance.
                    let path_bounds = cull::device_bounds(shape, xf, None);
                    let tol_scale = tessellate::tolerance_scale(path_bounds, self.limits.max_coord_px);
                    self.tess.tol_scale = tol_scale;
                    let path = shape.to_path(TOL * tol_scale);
                    let target = cull::scissor_rect(self.clips.current().scissor);
                    let outcome =
                        stroke::expand(&path, style, xf, target, tol_scale, &mut self.tess.stroke_ctx);
                    if outcome.width_clamped {
                        self.stats.record(Unsupported::StrokeWidthClamped);
                    }
                    if let Some(kind) = outcome.dash_fallback {
                        self.stats.record(kind);
                    }
                    if outcome.drew {
                        let k = self.alpha();
                        let premul = color::premul_scaled(color, outcome.alpha * k);
                        let fill = self.tess.fill_stroke_outline(self.plan, self.stats, xf, premul);
                        self.plan.stats.tess_calls += 1;
                        if let Some(indices) = fill {
                            self.stats.tier_c_draws += 1;
                            self.push(DrawCmd::Solid { indices, clip: self.clips.current() }, idx);
                        }
                    }
                }
                Op::PushLayer { transform, clip, alpha, blend, clip_fill, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !hygiene::geometry_is_finite(clip) {
                        // Keep the layer (stack balance) with an empty clip: its content is culled.
                        self.skip_non_finite();
                        let scissor = self.clips.current().scissor;
                        self.clips.push(clip::ClipDecision { kind: clip::ClipKind::Cull, scissor });
                        self.layers.push(1.0, false);
                        continue;
                    }
                    // Every layer's clip is pushed — including alpha < 1 or a non-default blend
                    // (defect D06's "layer clip lost").
                    let decision = clip::classify_clip(clip, xf, &self.clips.current());
                    if !self.layers.is_skipped() {
                        self.note_clip(clip, xf, *clip_fill);
                        self.note_layer(*alpha, *blend);
                    }
                    if decision.kind == clip::ClipKind::Bounds && !self.layers.is_skipped() {
                        self.stats.record(Unsupported::NonRectClip);
                    }
                    self.clips.push(decision);
                    let blended = *blend != peniko::BlendMode::default();
                    if blended && !self.layers.is_skipped() {
                        self.stats.record(Unsupported::LayerBlend);
                    }
                    // alpha == 0 is exact (nothing visible), so it is skipped without a log; a
                    // non-finite alpha becomes 0 and is recorded (S02). The layer is always kept.
                    let (alpha, non_finite) = hygiene::sanitize_layer_alpha(*alpha);
                    if non_finite {
                        self.stats.record(Unsupported::NonFinite);
                    }
                    self.layers.push(alpha, blended || alpha == 0.0);
                }
                Op::PopLayer => {
                    if self.layers.pop() {
                        self.clips.pop();
                    }
                }
                // `glyph_transform` (synthetic oblique) is drawn upright until the outline route (R20).
                Op::Glyphs {
                    font,
                    font_size,
                    normalized_coords,
                    brush,
                    transform,
                    glyph_transform,
                    glyphs,
                    ..
                } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !font_size.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if !self.paint_is_sane(brush, xf) || self.hidden() {
                        continue;
                    }
                    self.note_glyphs(brush, *glyph_transform, xf, *font_size);
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
                    let k = self.alpha();
                    let color = color::premul_scaled(base, k);
                    let range =
                        glyphs::glyph_run(self.plan, font, *font_size, normalized_coords, color, xf, glyphs);
                    self.push(DrawCmd::Glyphs { glyphs: range, clip: self.clips.current() }, idx);
                }
                Op::Fragment { transform, ops, .. } => {
                    let xf = outer * transform.unwrap_or(Affine::IDENTITY);
                    if !xf.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    self.emit(ops, xf);
                }
                Op::Image { image, transform, .. } => {
                    // Zero-size images are skipped silently.
                    if image.image.width == 0 || image.image.height == 0 || self.hidden() {
                        continue;
                    }
                    let xf = outer * *transform;
                    if !xf.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if !self.transform_is_sane(xf)
                        || self.culled(cull::image_bounds(image.image.width, image.image.height, xf))
                    {
                        continue;
                    }
                    self.note_image(image, xf);
                    let s = &image.sampler;
                    if s.x_extend != peniko::Extend::Pad || s.y_extend != peniko::Extend::Pad {
                        self.stats.record(Unsupported::ImageExtend);
                    }
                    let k = self.alpha() * s.alpha.clamp(0.0, 1.0);
                    let range = images::image(self.plan, image, xf, k, tier_a::snaps(op));
                    self.push(DrawCmd::Images { images: range, clip: self.clips.current() }, idx);
                }
                Op::BlurredRect { transform, rect, brush, radius, std_dev, .. } => {
                    if self.hidden() {
                        continue;
                    }
                    let xf = outer * *transform;
                    if !xf.is_finite() || !rect.is_finite() || !radius.is_finite() || !std_dev.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if !hygiene::color_is_finite(brush) {
                        self.skip_non_finite();
                        continue;
                    }
                    if !self.transform_is_sane(xf) || self.culled(cull::blurred_bounds(*rect, *std_dev, xf)) {
                        continue;
                    }
                    self.note_shadow(xf);
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
