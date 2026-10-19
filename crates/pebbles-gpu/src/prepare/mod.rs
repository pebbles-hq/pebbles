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
pub(crate) mod glyph_outline;
pub(crate) mod glyphs;
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
pub(crate) mod test_util;
mod tier_a;

#[cfg(test)]
pub(crate) use plan::ClipState;
pub(crate) use plan::{DrawCmd, FramePlan, GlyphRequest, ScissorRect};

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
    /// Device-pixel ratio of the target (02 §7.8; `FrameParams::scale_factor`).
    pub scale_factor: f64,
    /// Hinting override (`FrameParams::glyph_hint`); `None` = `hint_policy`.
    pub glyph_hint: Option<bool>,
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
    /// swash outlines for the glyph outline route (R20).
    pub outlines: glyph_outline::OutlineSource,
    /// COLR glyph fragments (R29).
    pub colr: crate::raster::color_glyph::ColorGlyphCache,
    /// Debug switch (R39 tests): draw atlas-route COLR glyphs as vector fragments too.
    pub colr_vector_only: bool,
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
        outlines: &mut scratch.outlines,
        colr: &mut scratch.colr,
        colr_vector_only: scratch.colr_vector_only,
        stats,
        limits,
        visit: 0,
        vertex_limit: input.vertex_limit,
        scale_factor: input.scale_factor,
        glyph_hint: input.glyph_hint,
        out_of_vertices: false,
        sdf: !scratch.sdf_disabled,
    };
    walk.emit(scene.ops(), input.root_transform);
    scratch.outlines.end_frame();
    scratch.colr.end_frame();
    scratch.colr.end_frame();
    &scratch.plan
}

/// The state of one prepare walk.
struct Walk<'a> {
    plan: &'a mut FramePlan,
    clips: &'a mut clip::ClipStack,
    layers: &'a mut layer::LayerStack,
    tess: &'a mut tessellate::PathTessellator,
    outlines: &'a mut glyph_outline::OutlineSource,
    colr: &'a mut crate::raster::color_glyph::ColorGlyphCache,
    colr_vector_only: bool,
    stats: &'a mut FrameStats,
    limits: &'a RendererLimits,
    visit: u32,
    vertex_limit: u32,
    scale_factor: f64,
    glyph_hint: Option<bool>,
    out_of_vertices: bool,
    /// Tier A analytic shapes enabled (R10).
    sdf: bool,
}

/// One COLR glyph the renderer must render into the color atlas before the frame (R39).
pub(crate) struct ColrAtlasJob {
    pub glyph: glyphs::ColrAtlasGlyph,
    pub fragment: crate::raster::color_glyph::ColorGlyphFragment,
}

/// R39 pre-pass: every atlas-route COLR glyph of `ops` (under `outer`), deduplicated by key.
/// Mirrors the Glyphs arm's route decision and key computation exactly.
pub(crate) fn scan_colr_atlas(
    ops: &[Op],
    outer: Affine,
    colr: &mut crate::raster::color_glyph::ColorGlyphCache,
    out: &mut Vec<ColrAtlasJob>,
) {
    for op in ops {
        match op {
            Op::Fragment { transform, ops, .. } => {
                scan_colr_atlas(ops, outer * transform.unwrap_or(Affine::IDENTITY), colr, out);
            }
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
                let peniko::Brush::Solid(base) = brush else { continue };
                if !xf.is_finite() || !colr.font_has_colr(font) {
                    continue;
                }
                if !matches!(
                    glyph_outline::classify_run(xf, *glyph_transform, *font_size),
                    glyph_outline::GlyphRoute::Atlas { .. }
                ) {
                    continue;
                }
                let Ok(face) = skrifa::FontRef::from_index(font.data.data(), font.index) else { continue };
                let upem = skrifa::raw::TableProvider::head(&face)
                    .map_or(1000.0, |h| f64::from(h.units_per_em().max(1)));
                let fg = base.with_alpha(1.0);
                for g in glyphs {
                    let Ok(id) = u16::try_from(g.id) else { continue };
                    let Some(fragment) = colr.fragment(font, id, normalized_coords, fg) else { continue };
                    let Some(glyph) =
                        glyphs::colr_atlas_glyph(font, id, normalized_coords, *font_size, upem, xf, g, fg)
                    else {
                        continue;
                    };
                    if !out.iter().any(|j| j.glyph.key == glyph.key) {
                        out.push(ColrAtlasJob { glyph, fragment });
                    }
                }
            }
            _ => {}
        }
    }
}

/// One glyph run's drawing inputs (R29 splits a run around its COLR glyphs).
struct GlyphRunArgs<'r> {
    font: &'r peniko::FontData,
    font_size: f32,
    coords: &'r [i16],
    /// The run color (straight); COLR palette index `0xFFFF` uses it (02 §7.7).
    base: peniko::Color,
    /// Premultiplied tint × fold alpha.
    color: [f32; 4],
    xf: Affine,
    glyph_transform: Option<Affine>,
}

impl Walk<'_> {
    /// Glyphs with no COLR representation: the atlas or outline route (§7.5).
    fn draw_glyph_segment(
        &mut self,
        run: &GlyphRunArgs<'_>,
        glyphs: &[pebbles_render::paint::Glyph],
        idx: u32,
    ) {
        if glyphs.is_empty() {
            return;
        }
        match glyph_outline::classify_run(run.xf, run.glyph_transform, run.font_size) {
            glyph_outline::GlyphRoute::Atlas { .. } => {
                let scissor = self.clips.current().scissor;
                let range = glyphs::glyph_run(
                    self.plan,
                    self.stats,
                    scissor,
                    run.font,
                    run.font_size,
                    run.coords,
                    run.color,
                    run.xf,
                    glyphs,
                    self.glyph_hint,
                    self.scale_factor,
                );
                self.push(DrawCmd::Glyphs { glyphs: range, clip: self.clips.current() }, idx);
            }
            glyph_outline::GlyphRoute::Outline => {
                let (g, c) = (run.glyph_transform, run.coords);
                self.draw_glyph_outlines(run.font, run.font_size, c, run.color, run.xf, g, glyphs, idx);
            }
            glyph_outline::GlyphRoute::Skip => {}
        }
    }

    /// R29: a run whose font has COLR — each COLR glyph splices its fragment under
    /// `M · translate(pen) · G · scale(k, −k)` (`k = font_size / upem`; any transform), the
    /// other glyphs keep their route, in glyph order (§7.7).
    fn draw_colr_run(&mut self, run: &GlyphRunArgs<'_>, glyphs: &[pebbles_render::paint::Glyph], idx: u32) {
        let upem = skrifa::raw::TableProvider::head(&match skrifa::FontRef::from_index(
            run.font.data.data(),
            run.font.index,
        ) {
            Ok(f) => f,
            Err(_) => return self.draw_glyph_segment(run, glyphs, idx),
        })
        .map_or(1000.0, |h| f64::from(h.units_per_em().max(1)));
        let k = f64::from(run.font_size) / upem;
        // Tint (§7.7): color glyphs use only the run alpha (× fold alpha); index 0xFFFF the
        // run color at full alpha (the alpha applies once, through the layer).
        let alpha = run.color[3];
        let foreground = run.base.with_alpha(1.0);
        // R39: on the atlas route a COLR glyph is a color-atlas quad (rendered once by the
        // renderer's pre-pass); the outline route keeps the vector fragment.
        let atlas = !self.colr_vector_only
            && matches!(
                glyph_outline::classify_run(run.xf, run.glyph_transform, run.font_size),
                glyph_outline::GlyphRoute::Atlas { .. }
            );
        let mut atlas_run = None;
        let mut start = 0;
        for (i, g) in glyphs.iter().enumerate() {
            let Ok(id) = u16::try_from(g.id) else { continue };
            let Some(frag) = self.colr.fragment(run.font, id, run.coords, foreground) else { continue };
            self.draw_glyph_segment(run, glyphs.get(start..i).unwrap_or_default(), idx);
            start = i + 1;
            if atlas {
                let placed = glyphs::colr_atlas_glyph(
                    run.font,
                    id,
                    run.coords,
                    run.font_size,
                    upem,
                    run.xf,
                    g,
                    foreground,
                );
                if let Some(c) = placed {
                    let r = *atlas_run.get_or_insert_with(|| {
                        let c0 = num::index_u32(self.plan.coords.len());
                        self.plan.coords.extend_from_slice(run.coords);
                        self.plan.glyph_runs.push(plan::GlyphRunInfo {
                            font: run.font.clone(),
                            coords: c0..num::index_u32(self.plan.coords.len()),
                            size_px: run.font_size,
                        });
                        num::index_u32(self.plan.glyph_runs.len() - 1)
                    });
                    let s = num::index_u32(self.plan.glyphs.len());
                    let req = GlyphRequest {
                        key: c.key.into(),
                        run: r,
                        origin: c.origin,
                        color: run.color,
                        linear: None,
                    };
                    self.plan.glyphs.push(req);
                    self.push(DrawCmd::Glyphs { glyphs: s..s + 1, clip: self.clips.current() }, idx);
                }
                continue;
            }
            let m = run.xf
                * Affine::translate((f64::from(g.x), f64::from(g.y)))
                * run.glyph_transform.unwrap_or(Affine::IDENTITY)
                * Affine::scale_non_uniform(k, -k);
            if alpha >= 1.0 {
                self.emit(frag.scene.ops(), m);
            } else {
                // A group-opacity layer around the fragment (R29 step 5).
                let mut wrapped = Scene::new();
                let mut p = pebbles_render::paint::Painter::new(&mut wrapped);
                p.push_layer(
                    Fill::NonZero,
                    peniko::BlendMode::default(),
                    alpha,
                    Affine::IDENTITY,
                    &frag.bounds,
                );
                p.append(&frag.scene, None);
                p.pop_layer();
                self.emit(wrapped.ops(), m);
            }
        }
        self.draw_glyph_segment(run, glyphs.get(start..).unwrap_or_default(), idx);
    }

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
                    // R22: non-finite size/transform or out-of-range variation coords skip the run.
                    let coords_ok = normalized_coords.iter().all(|c| (-16384..=16384).contains(c));
                    if !xf.is_finite() || !font_size.is_finite() || !coords_ok {
                        self.skip_non_finite();
                        continue;
                    }
                    if !self.paint_is_sane(brush, xf) || self.hidden() {
                        continue;
                    }
                    self.note_glyphs(brush);
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
                    let run = GlyphRunArgs {
                        font,
                        font_size: *font_size,
                        coords: normalized_coords,
                        base,
                        color,
                        xf,
                        glyph_transform: *glyph_transform,
                    };
                    if self.colr.font_has_colr(font) {
                        self.draw_colr_run(&run, glyphs, idx);
                    } else {
                        self.draw_glyph_segment(&run, glyphs, idx);
                    }
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
                    // R33: validate before any key. Zero-size images are skipped silently.
                    match images::validate_image(&image.image) {
                        Ok(()) => {}
                        Err(images::ImageProblem::ZeroSize) => continue,
                        Err(_) => {
                            self.fell_back(coverage::Feature::ImageMalformed);
                            self.stats.record(Unsupported::ImageMalformed);
                            continue;
                        }
                    }
                    // R32: a non-finite alpha skips the op; alpha 0 draws nothing.
                    let a = image.sampler.alpha;
                    if !a.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if a <= 0.0 || self.hidden() {
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
                    let s = &image.sampler;
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
                    let k = self.alpha();
                    // R40: the closed form in local space (any affine). Degenerate → R41.
                    if let Some(range) =
                        shadows::shadow(self.plan, rect, xf, *radius, *std_dev, brush.multiply_alpha(k))
                    {
                        self.push(DrawCmd::Shadows { instances: range, clip: self.clips.current() }, idx);
                    }
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
