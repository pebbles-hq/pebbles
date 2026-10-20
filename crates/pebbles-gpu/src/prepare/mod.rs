//! Prepare: walk the shared display list into an ordered [`FramePlan`] on the CPU (no device).
#![cfg_attr(not(test), warn(clippy::indexing_slicing))]

use kurbo::{Affine, Shape};

use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

use path_strategy::Tier;
use pebbles_render::paint::display_list::BlurStyle;
use peniko::Fill;

use crate::limits::RendererLimits;
use crate::stats::{FrameStats, Unsupported};

mod admit;
mod blurred;
mod clip;
pub(crate) mod color;
pub(crate) mod coverage;
mod cull;
pub(crate) mod effects;
mod features;
pub(crate) mod glyph_outline;
pub(crate) mod glyphs;
pub(crate) mod gpu_masks;
pub(crate) mod gradient;
mod hygiene;
pub(crate) mod image_brush;
mod image_nine;
mod images;
mod layer;
pub(crate) mod num;
pub(crate) mod passes;
pub(crate) mod path_masks;
pub(crate) mod path_strategy;
pub(crate) mod plan;
#[cfg(test)]
mod property_tests;
pub(crate) mod retained;
pub(crate) mod segments;
mod shader_brush;
mod shadows;
mod shapes;
#[cfg(test)]
mod snapshot_tests;
mod stroke;
pub(crate) mod tessellate;
#[cfg(test)]
pub(crate) mod test_util;
mod tier_a;
pub(crate) mod vertices;

pub(crate) use plan::ClipState;
pub(crate) use plan::{DrawCmd, FramePlan, GlyphRequest, ScissorRect, SdfClipId};

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
    /// Reused by the per-layer subtree scan (M18).
    pub layer_scan: layer::ScanScratch,
    pub tess: tessellate::PathTessellator,
    /// swash outlines for the glyph outline route (R20).
    pub outlines: glyph_outline::OutlineSource,
    /// COLR glyph fragments (R29).
    pub colr: crate::raster::color_glyph::ColorGlyphCache,
    /// Debug switch (R39 tests): draw atlas-route COLR glyphs as vector fragments too.
    pub colr_vector_only: bool,
    /// Debug switch (R10): draw Tier A shapes through tessellation instead (tests, render-check).
    pub sdf_disabled: bool,
    /// `GpuBudgets::gpu_mask_bytes`; `None` = the native default (tests).
    pub gpu_mask_budget: Option<u64>,
    /// Retained fragments (M29) and their byte budget (0 disables, `PEBBLES_GPU_RETAINED=0`).
    pub retained: retained::RetainedCache,
    pub retained_budget: u64,
    /// Arena caps for merging and segmentation (M07); unlimited by default (tests).
    pub arena_caps: segments::ArenaCaps,
    /// `GpuBudgets::path_mask_bytes`; `None` = the native default (tests, M28).
    pub path_mask_budget: Option<u64>,
    /// Path / clip masks admitted this frame (M28; cleared, capacity kept).
    pub mask_keys: std::collections::HashSet<u64>,
    /// The path-mask budget fallback was logged (once per renderer).
    pub path_budget_warned: bool,
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
    scratch.plan.arena_caps = scratch.arena_caps;
    scratch.clips.reset(input.target_size);
    scratch.layers.reset();
    scratch.mask_keys.clear();
    scratch.retained.begin_frame();
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
        clip_hint: pebbles_render::paint::display_list::PathHint::Auto,
        gpu_mask_budget: scratch.gpu_mask_budget.unwrap_or(gpu_masks::DEFAULT_BUDGET_BYTES),
        paint: None,
        pending_shader: None,
        layer_scan: &mut scratch.layer_scan,
        retained: &mut scratch.retained,
        retained_on: scratch.retained_budget > 0,
        rec_bounds: None,
        rec_admits: None,
        path_mask_bytes: 0,
        path_mask_budget: scratch.path_mask_budget.unwrap_or(8 << 20),
        mask_keys: &mut scratch.mask_keys,
        path_budget_warned: &mut scratch.path_budget_warned,
        layer_floor: 0,
        outer_override: None,
        outer_stack: Vec::new(),
    };
    walk.emit(scene.ops(), input.root_transform);
    // 09 §2.7: pushes still open at the end of the frame are auto-closed (emitting the pops
    // and composites of open offscreen layers).
    walk.close_open_layers();
    scratch.retained.end_frame(scratch.retained_budget);
    // 10 §2.5 (M07): cut the frame into segments under the arena caps.
    let dropped = segments::build(&mut scratch.plan, scratch.arena_caps);
    if dropped > 0 {
        stats.oversize_ops_dropped += dropped;
        for _ in 0..dropped {
            stats.record(Unsupported::GeometryTooLarge);
        }
    }
    scratch.outlines.end_frame();
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
    /// `PathHint` of the `PushLayer` being applied (row 9's `Volatile`).
    clip_hint: pebbles_render::paint::display_list::PathHint,
    /// `GpuBudgets::gpu_mask_bytes` (rows 9–10).
    gpu_mask_budget: u64,
    /// The brush painting the current op (09 §5, §6), if any.
    paint: Option<plan::BrushPaint>,
    /// Path-mask bytes admitted this frame and the budget (M28).
    path_mask_bytes: u64,
    path_mask_budget: u64,
    mask_keys: &'a mut std::collections::HashSet<u64>,
    path_budget_warned: &'a mut bool,
    /// The retained cache (M29), whether it is on, and the device bounds the fragment being
    /// recorded has touched (`culled()` boxes).
    retained: &'a mut retained::RetainedCache,
    retained_on: bool,
    rec_bounds: Option<kurbo::Rect>,
    /// Path-budget admissions of the fragment being recorded (M29 × M28).
    rec_admits: Option<Vec<(u64, u64)>>,
    /// `scan_one` scratch (M18).
    layer_scan: &'a mut layer::ScanScratch,
    /// A shader brush (L43) waiting for its coverage op's white solid brush.
    pending_shader: Option<u32>,
    /// Layer-stack depth at the current fragment's entry: pops below it are ignored (09 §7.2).
    layer_floor: usize,
    /// The device transform the ops of the current projected layer use (09 §10.7).
    outer_override: Option<Affine>,
    /// (layer depth after the push, the override to restore at its pop).
    outer_stack: Vec<(usize, Option<Affine>)>,
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

impl Walk<'_> {
    /// `Op::Fragment` (09 §7): replayed from the retained cache when unchanged (M29), else
    /// walked — and recorded when cacheable.
    #[inline(never)]
    fn emit_fragment(&mut self, ops: &std::sync::Arc<Vec<Op>>, xf: Affine) {
        let key = self.retained_key(ops, xf);
        let origin = (xf.translation().x.floor(), xf.translation().y.floor());
        #[allow(clippy::cast_possible_truncation)] // finite and checked against the target below
        let origin = (origin.0 as i32, origin.1 as i32);
        if let Some(key) = key {
            let scissor_px = self.clips.current().scissor;
            if let Some((b, mode)) = self.retained.hit_bounds(&key, ops, origin)
                && match mode {
                    retained::Mode::Inside => cull::scissor_rect(scissor_px).contains_rect(b),
                    retained::Mode::Outside => cull::is_outside(b, scissor_px),
                }
                && self.admit_all(&key)
            {
                let clip = self.clips.current();
                #[cfg(test)]
                let visit = self.visit;
                if let Some((fd, pd, features, visits, b)) =
                    self.retained.replay(&key, ops, origin, clip, self.plan, |plan, c| {
                        #[cfg(test)]
                        {
                            if plan.push_draw(c) {
                                plan.cmd_ops.push(visit + 1);
                            }
                        }
                        #[cfg(not(test))]
                        plan.push_draw(c);
                    })
                {
                    self.stats.add_prepare_delta(&fd);
                    self.plan.stats.add(&pd);
                    self.plan.fallback_features.union(features);
                    self.visit += visits;
                    if let Some(r) = &mut self.rec_bounds {
                        *r = retained::join(*r, b);
                    }
                    return;
                }
            }
        }
        let Some(key) = key else {
            // Not cacheable here: walk it with no recording bookkeeping (fragments are hot —
            // glyph runs splice COLR fragments per glyph).
            self.walk_fragment_ops(ops, xf);
            return;
        };
        let key = Some(key);
        let m0 = retained::Marks::of(self.plan);
        let last_before = self.plan.cmds.last().cloned();
        // The fragment's own fallback rows are collected from empty, then OR-ed back in.
        let features0 = std::mem::take(&mut self.plan.fallback_features);
        let (stats0, plan0, visit0) = (*self.stats, self.plan.stats.clone(), self.visit);
        let outer_rec = self.rec_bounds.replace(kurbo::Rect::ZERO);
        let outer_admits = self.rec_admits.replace(Vec::new());
        self.walk_fragment_ops(ops, xf);
        let mut bounds = self.rec_bounds.take().unwrap_or(kurbo::Rect::ZERO);
        let admits = std::mem::replace(&mut self.rec_admits, outer_admits).unwrap_or_default();
        if let Some(outer) = &mut self.rec_admits {
            outer.extend_from_slice(&admits);
        }
        let features = self.plan.fallback_features;
        self.plan.fallback_features.union(features0);
        if let Some(key) = key {
            let pd = self.plan.stats.delta(&plan0);
            let m1 = retained::Marks::of(self.plan);
            let clean = m1.other == m0.other
                && pd.culled_glyphs == 0
                && pd.nonfinite == 0
                && pd.autoclosed == 0
                && pd.unbalanced_pops == 0
                && pd.path_mask_budget_fallbacks == 0
                && !self.out_of_vertices;
            if clean {
                bounds = retained::join(bounds, retained_data_bounds(self.plan, &m0));
                let scissor_px = self.clips.current().scissor;
                let produced = m1.cmds != m0.cmds
                    || m1.vertices != m0.vertices
                    || m1.shapes != m0.shapes
                    || m1.shadows != m0.shadows
                    || m1.glyphs != m0.glyphs
                    || m1.images != m0.images
                    || m1.masks != m0.masks
                    || last_before.as_ref() != self.plan.cmds.last();
                let mode = if pd.culled == 0 && cull::scissor_rect(scissor_px).contains_rect(bounds) {
                    Some(retained::Mode::Inside)
                } else if !produced && bounds != kurbo::Rect::ZERO && cull::is_outside(bounds, scissor_px) {
                    Some(retained::Mode::Outside)
                } else {
                    None
                };
                if let Some(mode) = mode {
                    let fd = self.stats.prepare_delta(&stats0);
                    let deltas = (fd, pd, features, self.visit - visit0);
                    self.retained.record(
                        key,
                        ops,
                        origin,
                        &m0,
                        last_before.as_ref(),
                        self.plan,
                        deltas,
                        bounds,
                        mode,
                        admits,
                    );
                }
            }
        }
        if let Some(r) = outer_rec {
            self.rec_bounds = Some(retained::join(r, bounds));
        }
    }

    /// M29 × M28: admit every mask a replay of `key` brings, exactly as its walk would; false
    /// (nothing admitted) when one would not fit — the fragment is walked instead.
    fn admit_all(&mut self, key: &retained::RetainedKey) -> bool {
        let admits = self.retained.admits(key);
        let mut need = 0u64;
        let mut seen = 0usize;
        for (i, &(k, b)) in admits.iter().enumerate() {
            let repeat = admits.get(..i).is_some_and(|p| p.iter().any(|(x, _)| *x == k));
            if !self.mask_keys.contains(&k) && !repeat {
                need = need.saturating_add(b);
                seen += 1;
            }
        }
        if seen > 0 && self.path_mask_bytes.saturating_add(need) > self.path_mask_budget {
            return false;
        }
        for &(k, b) in admits {
            if self.mask_keys.insert(k) {
                self.path_mask_bytes += b;
            }
            if let Some(a) = &mut self.rec_admits {
                a.push((k, b));
            }
        }
        true
    }

    /// Walk a fragment's ops (09 §7.2: a fragment balances its own pushes; it can never pop
    /// outer ones).
    fn walk_fragment_ops(&mut self, ops: &[Op], xf: Affine) {
        let floor = std::mem::replace(&mut self.layer_floor, self.layers.open());
        let saved = self.outer_override.take();
        self.emit(ops, xf);
        while self.layers.open() > self.layer_floor {
            self.pop_layer(self.visit);
            self.plan.stats.autoclosed += 1;
        }
        self.outer_override = saved;
        self.layer_floor = floor;
    }

    /// The M29 key of a fragment placed at `xf`, or `None` when caching cannot apply here.
    fn retained_key(&self, ops: &std::sync::Arc<Vec<Op>>, xf: Affine) -> Option<retained::RetainedKey> {
        use std::hash::{Hash, Hasher};
        if !self.retained_on
            || ops.len() < retained::MIN_OPS
            || self.outer_override.is_some()
            || self.layers.is_skipped()
            || self.clips.is_culled()
            || self.out_of_vertices
        {
            return None;
        }
        let clip = self.clips.current();
        let mask = self.plan.mask_clips.get(clip.mask.0 as usize)?;
        if mask.gpu.is_some() {
            return None;
        }
        let [a, b, c, d, e, f] = xf.as_coeffs();
        let t = |v: f64| v.is_finite() && v.abs() < 1.0e7;
        if !(t(e) && t(f)) {
            return None;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let s = clip.scissor;
        (s.x0, s.y0, s.x1, s.y1, mask.key).hash(&mut h);
        if let Some(sdf) = self.plan.sdf_clips.get(clip.sdf.0 as usize) {
            sdf.rect.iter().chain(&sdf.radii).for_each(|v| v.to_bits().hash(&mut h));
        }
        self.layers.fold().to_bits().hash(&mut h);
        (self.scale_factor.to_bits(), self.glyph_hint, self.sdf, self.vertex_limit, self.plan.target_size)
            .hash(&mut h);
        Some(retained::RetainedKey {
            ops_ptr: std::sync::Arc::as_ptr(ops) as usize,
            linear: [a, b, c, d].map(f64::to_bits),
            frac: ((e - e.floor()).to_bits(), (f - f.floor()).to_bits()),
            context: h.finish(),
        })
    }

    /// M28: admit a path or clip mask of `bytes` (keyed by `key`) against this frame's path
    /// budget, in display-list order; a key already admitted this frame is free. False = over
    /// budget (logged once per renderer): the caller falls back (Tier C / a GPU mask).
    fn admit_path_mask(&mut self, key: u64, bytes: u64) -> bool {
        if let Some(a) = &mut self.rec_admits {
            a.push((key, bytes));
        }
        if self.mask_keys.contains(&key) {
            return true;
        }
        if self.path_mask_bytes.saturating_add(bytes) > self.path_mask_budget {
            self.plan.stats.path_mask_budget_fallbacks += 1;
            if !std::mem::replace(self.path_budget_warned, true) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "this frame's path and clip masks exceed the {} B coverage budget; the rest render as meshes / GPU masks (10 §5)",
                        self.path_mask_budget
                    ),
                );
            }
            return false;
        }
        self.path_mask_bytes += bytes;
        self.mask_keys.insert(key);
        true
    }

    /// A skipped (or hidden) layer: a culled clip frame until the matching pop.
    fn push_skip_layer(&mut self) {
        let scissor = self.clips.current().scissor;
        self.clips.push(clip::ClipDecision { kind: clip::ClipKind::Cull, scissor }, &mut self.plan.sdf_clips);
        self.layers.push(layer::Frame::Skip);
    }

    /// Apply a layer's clip to the current target (09 §2.2), counting the decision.
    fn push_layer_clip(
        &mut self,
        clip: &pebbles_render::paint::display_list::Geometry,
        xf: Affine,
        fill: Fill,
    ) {
        let cur = self.clips.current();
        let volatile = self.clip_hint == pebbles_render::paint::display_list::PathHint::Volatile;
        let mut decision = clip::classify_clip_hinted(clip, xf, &cur, volatile);
        // M28: a row-8 CPU mask over this frame's coverage budget becomes a GPU mask (row 9).
        if decision.kind == clip::ClipKind::Mask {
            use std::hash::{Hash, Hasher};
            let s = decision.scissor;
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (s.x0, s.y0, s.x1, s.y1, fill == Fill::EvenOdd).hash(&mut h);
            xf.as_coeffs().iter().for_each(|v| v.to_bits().hash(&mut h));
            let b = clip.bounding_box();
            [b.x0, b.x1, b.y0, b.y1].iter().for_each(|v| v.to_bits().hash(&mut h));
            let pad = 2 + 2 * u64::from(crate::execute::resources::atlas_alloc::GLYPH_GUTTER_PX);
            let area = (u64::from(s.width()) + pad) * (u64::from(s.height()) + pad);
            if !self.admit_path_mask(h.finish(), area) {
                decision.kind = clip::ClipKind::GpuMask;
            }
        }
        // Row 8 under an active GPU mask takes row 9 with that mask as its parent (§2.2).
        let parent_gpu = self.plan.mask_clips.get(cur.mask.0 as usize).is_some_and(|r| r.gpu.is_some());
        if decision.kind == clip::ClipKind::Mask && parent_gpu {
            decision.kind = clip::ClipKind::GpuMask;
        }
        if decision.kind == clip::ClipKind::GpuMask && !self.push_gpu_mask(clip, xf, fill, decision.scissor) {
            decision.kind = clip::ClipKind::Overflow;
        }
        self.note_clip(clip, xf, fill, &decision.kind);
        match decision.kind {
            clip::ClipKind::Noop => self.plan.stats.clips_noop += 1,
            clip::ClipKind::Cull => self.plan.stats.clips_culled += 1,
            clip::ClipKind::Scissor => self.plan.stats.clips_scissor += 1,
            clip::ClipKind::Sdf(_) => self.plan.stats.clips_sdf += 1,
            clip::ClipKind::Mask => self.plan.stats.clips_mask += 1,
            clip::ClipKind::GpuMask => self.plan.stats.clips_gpu_mask += 1,
            clip::ClipKind::Overflow => {}
        }
        match decision.kind {
            clip::ClipKind::Mask => self.push_mask_clip(clip, xf, fill, decision.scissor),
            // `push_gpu_mask` pushed the clip state already.
            clip::ClipKind::GpuMask => {}
            _ => self.clips.push(decision, &mut self.plan.sdf_clips),
        }
    }

    /// 09 §2.2 rows 9–10: queue a GPU coverage mask job (§2.9) and push its clip state; false
    /// (row 10, nothing pushed) when `MAX_GPU_MASKS` are active, the mask pool budget is spent,
    /// or the clip path fails to tessellate (R03 records that; no `ClipOverflow`).
    fn push_gpu_mask(
        &mut self,
        geom: &pebbles_render::paint::display_list::Geometry,
        xf: Affine,
        rule: Fill,
        rect: ScissorRect,
    ) -> bool {
        let parent = self.clips.current().mask;
        let depth = self.plan.mask_clips.get(parent.0 as usize).map_or(0, |r| r.gpu_depth);
        let bytes = gpu_masks::job_bytes(rect);
        if depth >= clip::MAX_GPU_MASKS || self.plan.gpu_mask_bytes + bytes > self.gpu_mask_budget {
            self.stats.record(Unsupported::ClipOverflow);
            self.plan.stats.gpu_mask_overflow += 1;
            return false;
        }
        let path = geom.to_path(0.1 / xf.spectral_norm().max(1e-9));
        let els = path.elements().iter().copied();
        let Some(indices) = self.tess.fill(self.plan, self.stats, els, xf, rule, [1.0; 4]) else {
            return false;
        };
        self.plan.gpu_mask_bytes += bytes;
        let job = num::index_u32(self.plan.gpu_mask_jobs.len());
        let cmd_at = num::index_u32(self.plan.cmds.len());
        self.plan.gpu_mask_jobs.push(plan::GpuMaskJob { rect, indices, parent, cmd_at });
        self.plan.mask_clips.push(plan::MaskClipRecord {
            key: 0,
            sources: 0..0,
            rect,
            gpu: Some(job),
            gpu_depth: depth + 1,
        });
        let id = plan::MaskClipId(num::index_u32(self.plan.mask_clips.len() - 1));
        self.clips.push_mask(rect, id);
        true
    }

    /// 09 §3.5 push: the own clip on the parent (the composite's clip), the layer desc, the
    /// `PushLayer` cmd and a fresh clip state for the layer target. The composite path follows
    /// §4.2: `Over` for (Normal, SrcOver); fixed-function `Compose` for a Normal mix under a
    /// scissor-only clip; else `Backdrop` (W3C blend math in the shader, coverage-weighted).
    #[allow(clippy::too_many_arguments)]
    fn push_offscreen(
        &mut self,
        clip: &pebbles_render::paint::display_list::Geometry,
        xf: Affine,
        fill: Fill,
        blend: peniko::BlendMode,
        alpha: f32,
        promoted: bool,
        idx: u32,
    ) {
        let prev_fold = self.layers.fold();
        self.push_layer_clip(clip, xf, fill);
        let comp = self.clips.current();
        if self.clips.is_culled() {
            self.layers.push(layer::Frame::Clip { prev_fold });
            return;
        }
        let parent = match self.layers.current_target() {
            Some(l) => plan::TargetRef::Layer(l),
            None => plan::TargetRef::Root,
        };
        let path = if blend == peniko::BlendMode::default() {
            plan::CompositePath::Over
        } else if blend.mix == peniko::Mix::Normal
            && comp.sdf == SdfClipId::NONE
            && comp.mask == plan::MaskClipId::NONE
        {
            plan::CompositePath::Compose
        } else {
            plan::CompositePath::Backdrop
        };
        match path {
            plan::CompositePath::Over => self.plan.stats.composites_over += 1,
            plan::CompositePath::Compose => self.plan.stats.composites_compose += 1,
            plan::CompositePath::Backdrop => self.plan.stats.composites_backdrop += 1,
            plan::CompositePath::Projected => {}
        }
        if path == plan::CompositePath::Backdrop && parent == plan::TargetRef::Root {
            self.plan.root_needs_backdrop = true;
        }
        let id = num::index_u32(self.plan.layers.len());
        self.plan.layers.push(plan::LayerDesc {
            bounds: comp.scissor,
            alpha,
            mix: blend.mix,
            compose: blend.compose,
            path,
            parent,
            effect: None,
        });
        self.push(DrawCmd::PushLayer { layer: id }, idx);
        self.clips.push_fresh(comp.scissor);
        self.layers.push(layer::Frame::Offscreen { layer: id, comp, prev_fold });
        self.layers.set_fold(1.0);
        self.plan.stats.layers_offscreen += 1;
        self.plan.stats.layers_promoted += u32::from(promoted);
    }

    /// 09 §10.3 push of an effect layer: the optional own clip on the parent (the composite's
    /// clip), a layer target covering that clip grown by the effect's reach (∩ the parent
    /// scissor), the device-space steps, and a fresh clip state for the children.
    pub(super) fn push_effect_layer(
        &mut self,
        effect: &pebbles_render::paint::display_list::LayerEffect,
        clip: Option<&pebbles_render::paint::display_list::Geometry>,
        xf: Affine,
        idx: u32,
    ) {
        let prev_fold = self.layers.fold();
        let parent_scissor = self.clips.current().scissor;
        match clip {
            Some(c) => self.push_layer_clip(c, xf, Fill::NonZero),
            None => self.clips.push(
                clip::ClipDecision { kind: clip::ClipKind::Noop, scissor: parent_scissor },
                &mut self.plan.sdf_clips,
            ),
        }
        let comp = self.clips.current();
        if self.clips.is_culled() || self.layers.offscreen_open() >= self.limits.max_layer_targets as usize {
            self.layers.push(layer::Frame::Clip { prev_fold });
            return;
        }
        // Device parameters = local × s, the larger singular value of the linear part (09 §10.1).
        let s = xf.spectral_norm();
        let (rx, ry) = effect.reach();
        let grow = |v: u32, d: f64, up: bool| {
            let d = u32::try_from(num::to_u64_sat((d * s).ceil())).unwrap_or(u32::MAX);
            if up { v.saturating_add(d) } else { v.saturating_sub(d) }
        };
        let g = comp.scissor;
        let p = parent_scissor;
        let bounds = ScissorRect {
            x0: grow(g.x0, rx, false).max(p.x0),
            y0: grow(g.y0, ry, false).max(p.y0),
            x1: grow(g.x1, rx, true).min(p.x1),
            y1: grow(g.y1, ry, true).min(p.y1),
        };
        let mut steps = Vec::new();
        effects::device_steps(effect, s, &mut steps);
        let parent = match self.layers.current_target() {
            Some(l) => plan::TargetRef::Layer(l),
            None => plan::TargetRef::Root,
        };
        let id = num::index_u32(self.plan.layers.len());
        self.plan.effects.push(steps);
        self.plan.layers.push(plan::LayerDesc {
            bounds,
            alpha: 1.0,
            mix: peniko::Mix::Normal,
            compose: peniko::Compose::SrcOver,
            path: plan::CompositePath::Over,
            parent,
            effect: Some(num::index_u32(self.plan.effects.len() - 1)),
        });
        self.push(DrawCmd::PushLayer { layer: id }, idx);
        self.clips.push_fresh(bounds);
        self.layers.push(layer::Frame::Offscreen { layer: id, comp, prev_fold });
        self.layers.set_fold(1.0);
        self.plan.stats.layers_offscreen += 1;
        self.plan.stats.composites_over += 1;
    }

    /// 09 §10.4 push of a backdrop filter: a layer whose content is the parent's pixels under
    /// the clip (grown by the effect's reach), filtered with Clamp edges, composited back with
    /// `blend`/`alpha` through the backdrop path; then the children draw inline under the clip.
    #[allow(clippy::too_many_arguments)]
    fn push_backdrop_filter(
        &mut self,
        effect: &pebbles_render::paint::display_list::LayerEffect,
        clip: &pebbles_render::paint::display_list::Geometry,
        xf: Affine,
        blend: peniko::BlendMode,
        alpha: f32,
        idx: u32,
    ) {
        let prev_fold = self.layers.fold();
        let parent_scissor = self.clips.current().scissor;
        self.push_layer_clip(clip, xf, Fill::NonZero);
        self.layers.push(layer::Frame::Clip { prev_fold });
        let comp = self.clips.current();
        let (a, _) = hygiene::sanitize_layer_alpha(alpha);
        if self.clips.is_culled()
            || a <= 0.0
            || self.layers.offscreen_open() >= self.limits.max_layer_targets as usize
        {
            return;
        }
        let s = xf.spectral_norm();
        let (rx, ry) = effect.reach();
        let grow = |v: u32, d: f64, up: bool| {
            let d = u32::try_from(num::to_u64_sat((d * s).ceil())).unwrap_or(u32::MAX);
            if up { v.saturating_add(d) } else { v.saturating_sub(d) }
        };
        let (g, p) = (comp.scissor, parent_scissor);
        let bounds = ScissorRect {
            x0: grow(g.x0, rx, false).max(p.x0),
            y0: grow(g.y0, ry, false).max(p.y0),
            x1: grow(g.x1, rx, true).min(p.x1),
            y1: grow(g.y1, ry, true).min(p.y1),
        };
        let mut steps = Vec::new();
        effects::device_steps(effect, s, &mut steps);
        for st in &mut steps {
            if let plan::EffectStep::Blur { clamp, .. } = st {
                *clamp = true;
            }
        }
        let parent = match self.layers.current_target() {
            Some(l) => plan::TargetRef::Layer(l),
            None => plan::TargetRef::Root,
        };
        if parent == plan::TargetRef::Root {
            self.plan.root_needs_backdrop = true;
        }
        let id = num::index_u32(self.plan.layers.len());
        self.plan.effects.push(steps);
        self.plan.layers.push(plan::LayerDesc {
            bounds,
            alpha: a,
            mix: blend.mix,
            compose: blend.compose,
            path: plan::CompositePath::Backdrop,
            parent,
            effect: Some(num::index_u32(self.plan.effects.len() - 1)),
        });
        self.push(DrawCmd::PushLayer { layer: id }, idx);
        self.push(DrawCmd::PopLayer { layer: id }, idx);
        self.push(DrawCmd::Composite { layer: id, clip: comp }, idx);
        self.plan.stats.layers_offscreen += 1;
        self.plan.stats.composites_backdrop += 1;
    }

    /// 09 §10.7: an affine projection draws its children inline under `outer × A` (a pure clip
    /// layer of `bounds`); a perspective one renders them offscreen in texture px at scale ρ and
    /// composites the texture through `H_dev = outer · H` (`CompositePath::Projected`).
    fn push_projected_layer(
        &mut self,
        projection: &pebbles_render::paint::Projective,
        bounds: kurbo::Rect,
        clip: Option<&pebbles_render::paint::display_list::Geometry>,
        outer: Affine,
        idx: u32,
    ) {
        use pebbles_render::paint::Projective;
        let prev_fold = self.layers.fold();
        let prev_override = self.outer_override;
        if projection.is_affine() {
            let a = outer * projection.to_affine();
            let shape = clip.cloned().unwrap_or(pebbles_render::paint::display_list::Geometry::Rect(bounds));
            self.push_layer_clip(&shape, a, Fill::NonZero);
            self.layers.push(layer::Frame::Clip { prev_fold });
            self.outer_stack.push((self.layers.open(), prev_override));
            self.outer_override = Some(a);
            return;
        }
        let h_dev = projection.then(&Projective::from_affine(outer));
        let corners =
            [(bounds.x0, bounds.y0), (bounds.x1, bounds.y0), (bounds.x0, bounds.y1), (bounds.x1, bounds.y1)];
        let rho = corners
            .iter()
            .filter_map(|&(x, y)| h_dev.local_scale(kurbo::Point::new(x, y)))
            .fold(0.0f64, f64::max)
            .clamp(0.25, 2.0);
        let max_side = f64::from(self.limits.max_texture_dim);
        let rho = rho.min(max_side / bounds.width().max(1.0)).min(max_side / bounds.height().max(1.0));
        let to_u32 = |v: f64| u32::try_from(num::to_u64_sat(v.ceil())).unwrap_or(u32::MAX).max(1);
        let tex =
            ScissorRect { x0: 0, y0: 0, x1: to_u32(bounds.width() * rho), y1: to_u32(bounds.height() * rho) };
        // The parent's own clip state is the composite clip (no own clip on the parent: the
        // optional `clip` is local and applies inside the layer, exactly).
        let comp = self.clips.current();
        self.clips.push(
            clip::ClipDecision { kind: clip::ClipKind::Noop, scissor: comp.scissor },
            &mut self.plan.sdf_clips,
        );
        if self.clips.is_culled() || self.layers.offscreen_open() >= self.limits.max_layer_targets as usize {
            self.layers.push(layer::Frame::Skip);
            return;
        }
        let local_to_tex = Affine::scale(rho) * Affine::translate((-bounds.x0, -bounds.y0));
        let tex_to_device = Projective::from_affine(local_to_tex.inverse()).then(&h_dev);
        let parent = match self.layers.current_target() {
            Some(l) => plan::TargetRef::Layer(l),
            None => plan::TargetRef::Root,
        };
        let id = num::index_u32(self.plan.layers.len());
        self.plan.layers.push(plan::LayerDesc {
            bounds: tex,
            alpha: 1.0,
            mix: peniko::Mix::Normal,
            compose: peniko::Compose::SrcOver,
            path: plan::CompositePath::Projected,
            parent,
            effect: None,
        });
        self.plan.projections.push((id, tex_to_device.0.map(num::narrow_f32)));
        self.push(DrawCmd::PushLayer { layer: id }, idx);
        self.clips.push_fresh(tex);
        self.layers.push(layer::Frame::Offscreen { layer: id, comp, prev_fold });
        self.layers.set_fold(1.0);
        self.outer_stack.push((self.layers.open(), prev_override));
        self.outer_override = Some(local_to_tex);
        if let Some(c) = clip {
            // The local clip applies exactly in texture space, folded into the fresh state (it
            // ends with the layer).
            self.push_layer_clip(c, local_to_tex, Fill::NonZero);
            self.clips.squash();
        }
        self.plan.stats.layers_offscreen += 1;
        self.plan.stats.composites_over += 1;
    }

    /// The matching pop of any layer frame; false for an unbalanced pop.
    pub(super) fn pop_layer(&mut self, idx: u32) -> bool {
        let depth = self.layers.open();
        if self.outer_stack.last().is_some_and(|(d, _)| *d == depth) {
            self.outer_override = self.outer_stack.pop().and_then(|(_, prev)| prev);
        }
        let Some(frame) = self.layers.pop() else { return false };
        match frame {
            layer::Frame::Skip => {
                self.clips.pop();
            }
            layer::Frame::Clip { prev_fold } => {
                self.clips.pop();
                self.layers.set_fold(prev_fold);
            }
            layer::Frame::Offscreen { layer, comp, prev_fold } => {
                self.clips.pop(); // the layer target's fresh state
                self.push(DrawCmd::PopLayer { layer }, idx);
                self.push(DrawCmd::Composite { layer, clip: comp }, idx);
                self.clips.pop(); // the own clip on the parent
                self.layers.set_fold(prev_fold);
            }
        }
        true
    }

    /// 09 §2.7: auto-close every layer still open at the end of the frame.
    fn close_open_layers(&mut self) {
        while self.layers.open() > 0 {
            let idx = self.visit;
            self.pop_layer(idx);
            self.plan.stats.autoclosed += 1;
        }
    }

    /// 09 L08: append a mask record = the active chain + this clip, over `scissor`.
    fn push_mask_clip(
        &mut self,
        geom: &pebbles_render::paint::display_list::Geometry,
        xf: Affine,
        rule: Fill,
        scissor: ScissorRect,
    ) {
        use std::hash::{Hash, Hasher};
        let parent = self.clips.current().mask;
        let parent_rec = self.plan.mask_clips.get(parent.0 as usize).cloned();
        let path = geom.to_path(0.01 / xf.spectral_norm().max(1e-9)).into_owned();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        parent_rec.as_ref().map_or(0, |r| r.key).hash(&mut h);
        for el in path.elements() {
            let pts: &[kurbo::Point] = match el {
                kurbo::PathEl::MoveTo(a) | kurbo::PathEl::LineTo(a) => std::slice::from_ref(a),
                kurbo::PathEl::QuadTo(a, b) => &[*a, *b],
                kurbo::PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
                kurbo::PathEl::ClosePath => &[],
            };
            std::mem::discriminant(el).hash(&mut h);
            pts.iter().for_each(|p| [p.x, p.y].iter().for_each(|v| v.to_bits().hash(&mut h)));
        }
        xf.as_coeffs().iter().for_each(|v| v.to_bits().hash(&mut h));
        (rule == Fill::EvenOdd).hash(&mut h);
        (scissor.x0, scissor.y0, scissor.x1, scissor.y1).hash(&mut h);
        let start = num::index_u32(self.plan.clip_sources.len());
        if let Some(r) = &parent_rec {
            for i in r.sources.clone() {
                if let Some(s) = self.plan.clip_sources.get(i as usize).cloned() {
                    self.plan.clip_sources.push(s);
                }
            }
        }
        self.plan.clip_sources.push(plan::MaskClipSource { path, xf, rule });
        let end = num::index_u32(self.plan.clip_sources.len());
        self.plan.mask_clips.push(plan::MaskClipRecord {
            key: h.finish().max(1),
            sources: start..end,
            rect: scissor,
            gpu: None,
            gpu_depth: 0,
        });
        let id = plan::MaskClipId(num::index_u32(self.plan.mask_clips.len() - 1));
        self.clips.push_mask(scissor, id);
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

    fn emit(&mut self, ops: &[Op], outer0: Affine) {
        for (i, op) in ops.iter().enumerate() {
            // A projected layer (09 §10.7) re-bases the ops between its push and pop.
            let outer = self.outer_override.unwrap_or(outer0);
            self.visit += 1;
            let idx = self.visit;
            self.plan.stats.ops += 1;
            // M29: ops actually walked (replayed fragments are not counted).
            self.stats.ops_prepared += 1;
            self.paint = None;
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
                    let paint_xf = xf * brush_transform.unwrap_or(Affine::IDENTITY);
                    let Some(color) = self.brush_paint(brush, paint_xf) else { continue };
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
                    let paint_xf = xf * brush_transform.unwrap_or(Affine::IDENTITY);
                    let Some(color) = self.brush_paint(brush, paint_xf) else { continue };
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
                Op::PushLayer { transform, clip, alpha, blend, clip_fill, hint, .. } => {
                    let xf = outer * *transform;
                    self.clip_hint = *hint;
                    if !xf.is_finite() || !hygiene::geometry_is_finite(clip) {
                        // Keep the layer (stack balance) with an empty clip: its content is culled.
                        self.skip_non_finite();
                        self.plan.stats.nonfinite += 1;
                        self.push_skip_layer();
                        continue;
                    }
                    if self.layers.is_skipped() || self.clips.is_culled() {
                        self.push_skip_layer();
                        continue;
                    }
                    let (a, non_finite) = hygiene::sanitize_layer_alpha(*alpha);
                    if non_finite {
                        self.stats.record(Unsupported::NonFinite);
                    }
                    let info = layer::scan_one(ops, i, self.layer_scan);
                    let mut plan_kind = layer::classify_layer(info, *blend, a);
                    // S12 (11 §2.5): past the nesting or live-target caps a layer flattens to a
                    // pure clip (its content still draws; opacity and blend are ignored).
                    if !matches!(plan_kind, layer::LayerPlan::Skip | layer::LayerPlan::PureClip) {
                        if self.layers.open() >= self.limits.max_layer_depth as usize {
                            self.stats.record(Unsupported::LayerDepthExceeded);
                            plan_kind = layer::LayerPlan::PureClip;
                        } else if matches!(plan_kind, layer::LayerPlan::Offscreen { .. })
                            && self.layers.offscreen_open() >= self.limits.max_layer_targets as usize
                        {
                            self.stats.record(Unsupported::LayerPoolExhausted);
                            plan_kind = layer::LayerPlan::PureClip;
                        }
                    }
                    match plan_kind {
                        layer::LayerPlan::Skip => {
                            self.plan.stats.layers_skipped += 1;
                            self.push_skip_layer();
                        }
                        layer::LayerPlan::PureClip => {
                            self.plan.stats.layers_pure_clip += 1;
                            self.push_layer_clip(clip, xf, *clip_fill);
                            let prev_fold = self.layers.fold();
                            self.layers.push(layer::Frame::Clip { prev_fold });
                        }
                        layer::LayerPlan::Fold { alpha } => {
                            self.plan.stats.layers_folded += 1;
                            self.push_layer_clip(clip, xf, *clip_fill);
                            let prev_fold = self.layers.fold();
                            self.layers.push(layer::Frame::Clip { prev_fold });
                            self.layers.set_fold(prev_fold * alpha);
                        }
                        layer::LayerPlan::Offscreen { promoted } => {
                            self.push_offscreen(
                                clip,
                                xf,
                                *clip_fill,
                                *blend,
                                if promoted { 1.0 } else { a },
                                promoted,
                                idx,
                            );
                        }
                    }
                }
                Op::PushBackdropFilter { effect, clip, transform, blend, alpha, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !hygiene::geometry_is_finite(clip) {
                        self.skip_non_finite();
                        self.push_skip_layer();
                        continue;
                    }
                    if self.layers.is_skipped() || self.clips.is_culled() {
                        self.push_skip_layer();
                        continue;
                    }
                    self.push_backdrop_filter(effect, clip, xf, *blend, *alpha, idx);
                }
                Op::PushEffectLayer { effect, clip, transform, .. } => {
                    let xf = outer * *transform;
                    let finite = clip.as_ref().is_none_or(hygiene::geometry_is_finite);
                    if !xf.is_finite() || !finite || self.layers.is_skipped() || self.clips.is_culled() {
                        if !xf.is_finite() || !finite {
                            self.skip_non_finite();
                        }
                        self.push_skip_layer();
                        continue;
                    }
                    self.push_effect_layer(effect, clip.as_ref(), xf, idx);
                }
                Op::PopLayer => {
                    if self.layers.open() <= self.layer_floor || !self.pop_layer(idx) {
                        // 09 §2.7: an unbalanced pop is ignored and counted.
                        self.plan.stats.unbalanced_pops += 1;
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
                    let base = match brush {
                        peniko::Brush::Solid(c) => *c,
                        // 09 §5.6 (L36): the run's paint transform is `outer × op.transform`.
                        peniko::Brush::Gradient(g) => match self.brush_paint(brush, xf) {
                            // Color (COLR) fonts ignore the brush like any color glyph: the
                            // foreground index takes the first stop.
                            Some(_) if self.paint.is_some() && self.colr.font_has_colr(font) => {
                                self.paint = None;
                                g.stops
                                    .first()
                                    .map_or(peniko::Color::TRANSPARENT, |s| s.color.to_alpha_color())
                            }
                            Some(c) => c,
                            None => continue,
                        },
                        // 09 §6 (L41 step 3): image-brush runs draw through their outlines too.
                        peniko::Brush::Image(_) => match self.brush_paint(brush, xf) {
                            Some(_) if self.colr.font_has_colr(font) => {
                                self.paint = None;
                                continue;
                            }
                            Some(c) => c,
                            None => continue,
                        },
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
                    if self.paint.is_some() {
                        // Gradient text is drawn from outlines (§5.6), never atlas bitmaps.
                        let (g, c) = (run.glyph_transform, run.coords);
                        self.draw_glyph_outlines(
                            run.font,
                            run.font_size,
                            c,
                            run.color,
                            run.xf,
                            g,
                            glyphs,
                            idx,
                        );
                    } else if self.colr.font_has_colr(font) {
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
                    self.emit_fragment(ops, xf);
                }
                Op::PushProjectedLayer { projection, bounds, clip, .. } => {
                    let finite = projection.0.iter().all(|v| v.is_finite())
                        && bounds.is_finite()
                        && clip.as_ref().is_none_or(hygiene::geometry_is_finite);
                    if !finite {
                        self.skip_non_finite();
                        self.push_skip_layer();
                        continue;
                    }
                    if self.layers.is_skipped() || self.clips.is_culled() {
                        self.push_skip_layer();
                        continue;
                    }
                    self.push_projected_layer(projection, *bounds, clip.as_ref(), outer, idx);
                }
                Op::Vertices { mesh, brush, color_mode, transform, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite()
                        || mesh.positions.iter().any(|p| !p[0].is_finite() || !p[1].is_finite())
                    {
                        self.skip_non_finite();
                        continue;
                    }
                    if self.hidden() {
                        continue;
                    }
                    self.draw_mesh(mesh, brush, *color_mode, xf, idx);
                }
                Op::ShaderFill { transform, shader, shape, .. } => {
                    // 09 §6.1: the coverage is the same fill/stroke with a white brush, routed by
                    // tier like an image brush; its solid brush then becomes the shader paint.
                    let Some(coverage) = op.shader_coverage() else { continue };
                    let paint_xf = outer * *transform * shader.transform;
                    let Some(draw) = shader_brush::draw(shader, paint_xf, shape.bounding_box().size()) else {
                        self.skip_non_finite();
                        continue;
                    };
                    self.plan.shader_brushes.push(draw);
                    self.pending_shader = Some(num::index_u32(self.plan.shader_brushes.len() - 1));
                    // The coverage op stands in for this op (same visit index, one op).
                    self.visit -= 1;
                    self.plan.stats.ops -= 1;
                    self.emit(std::slice::from_ref(&coverage), outer0);
                    if self.pending_shader.take().is_some() {
                        // Culled, hidden or skipped: nothing references the draw.
                        self.plan.shader_brushes.pop();
                    }
                }
                Op::GpuImage { id, transform, rect, alpha, sampler, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !rect.is_finite() || !alpha.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    let k = self.alpha() * alpha.clamp(0.0, 1.0) * sampler.alpha.clamp(0.0, 1.0);
                    if k <= 0.0 || rect.is_zero_area() || self.hidden() {
                        continue;
                    }
                    let r = rect.abs();
                    let m = xf * Affine::translate(r.origin().to_vec2());
                    if !self.transform_is_sane(m) || self.culled(xf.transform_rect_bbox(r)) {
                        continue;
                    }
                    let image = images::gpu_image(self.plan, id.0, *sampler, m, (r.width(), r.height()), k);
                    self.push(DrawCmd::GpuImage { image, clip: self.clips.current() }, idx);
                }
                Op::Custom { bounds, transform, paint, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if self.hidden() {
                        continue;
                    }
                    let b = xf.transform_rect_bbox(*bounds);
                    if self.culled(b) {
                        continue;
                    }
                    let clip = self.clips.current();
                    let Some(scissor) =
                        ScissorRect::from_device_bounds(b.x0, b.y0, b.x1, b.y1, self.plan.target_size)
                            .and_then(|s| s.intersect(clip.scissor))
                    else {
                        self.plan.stats.culled += 1;
                        continue;
                    };
                    let op = num::index_u32(self.plan.customs.len());
                    self.plan.customs.push(plan::CustomItem {
                        paint: std::sync::Arc::clone(paint),
                        transform: xf,
                        scissor,
                    });
                    self.push(DrawCmd::Custom { op }, idx);
                }
                Op::Atlas { image, sprites, transform, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if self.hidden() {
                        continue;
                    }
                    self.draw_sprites(image, sprites, xf, idx);
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
                Op::BlurredFill { style, transform, color, shape, sigma, blur_style, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !sigma.is_finite() || !hygiene::color_is_finite(color) {
                        self.skip_non_finite();
                        continue;
                    }
                    let k = self.alpha();
                    if color.components[3] * k <= 0.0 || self.hidden() || !self.transform_is_sane(xf) {
                        continue;
                    }
                    let pad = 3.0 * f64::from(sigma.max(0.0));
                    if self
                        .culled(xf.transform_rect_bbox(shape.to_path(0.1).bounding_box().inflate(pad, pad)))
                    {
                        continue;
                    }
                    let premul = color::premul(color.multiply_alpha(k));
                    self.draw_blurred(shape, xf, *style, premul, *sigma, *blur_style, idx);
                }
                Op::BlurredGlyphs {
                    font,
                    font_size,
                    normalized_coords,
                    transform,
                    glyph_transform,
                    glyphs,
                    color,
                    sigma,
                    ..
                } => {
                    let xf = outer * *transform;
                    if !xf.is_finite()
                        || !sigma.is_finite()
                        || !font_size.is_finite()
                        || !hygiene::color_is_finite(color)
                    {
                        self.skip_non_finite();
                        continue;
                    }
                    let k = self.alpha();
                    if color.components[3] * k <= 0.0
                        || *font_size <= 0.0
                        || self.hidden()
                        || !self.transform_is_sane(xf)
                    {
                        continue;
                    }
                    // R42: blur the run's union, never glyph by glyph.
                    let union =
                        self.run_outline(font, *font_size, normalized_coords, *glyph_transform, glyphs);
                    if union.elements().is_empty() {
                        continue;
                    }
                    let shape = pebbles_render::paint::display_list::Geometry::Path(union);
                    let premul = color::premul(color.multiply_alpha(k));
                    self.draw_blurred(&shape, xf, Fill::NonZero, premul, *sigma, BlurStyle::Normal, idx);
                }
                Op::DynamicImage { frame, quality, alpha, transform, .. } => {
                    let xf = outer * *transform;
                    if !xf.is_finite() || !alpha.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if frame.width == 0 || frame.height == 0 || *alpha <= 0.0 || self.hidden() {
                        continue;
                    }
                    if !self.transform_is_sane(xf)
                        || self.culled(cull::image_bounds(frame.width, frame.height, xf))
                    {
                        continue;
                    }
                    let k = self.alpha() * alpha.clamp(0.0, 1.0);
                    let range = images::dynamic_image(self.plan, frame, *quality, xf, k, tier_a::snaps(op));
                    self.push(DrawCmd::Images { images: range, clip: self.clips.current() }, idx);
                }
                Op::ImageNine { image, center, dst, transform, .. } => {
                    // R36: validated like any image (R33), then split into ≤ 9 quads.
                    match images::validate_image(&image.image) {
                        Ok(()) => {}
                        Err(images::ImageProblem::ZeroSize) => continue,
                        Err(_) => {
                            self.fell_back(coverage::Feature::ImageMalformed);
                            self.stats.record(Unsupported::ImageMalformed);
                            continue;
                        }
                    }
                    let a = image.sampler.alpha;
                    let xf = outer * *transform;
                    if !xf.is_finite() || !a.is_finite() || !center.is_finite() || !dst.is_finite() {
                        self.skip_non_finite();
                        continue;
                    }
                    if a <= 0.0 || self.hidden() || !self.transform_is_sane(xf) {
                        continue;
                    }
                    if self.culled(xf.transform_rect_bbox(dst.abs())) {
                        continue;
                    }
                    let k = self.alpha() * a.clamp(0.0, 1.0);
                    let (range, clamped) = image_nine::image_nine(self.plan, image, *center, *dst, xf, k);
                    self.stats.nine_slice_clamped += u32::from(clamped);
                    if !range.is_empty() {
                        self.push(DrawCmd::Images { images: range, clip: self.clips.current() }, idx);
                    }
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
                    // R41: a transparent shadow draws nothing.
                    if brush.components[3] * k <= 0.0 {
                        continue;
                    }
                    // R40: the closed form in local space (any affine); an empty rect is skipped.
                    let max = self.limits.max_coord_px;
                    if let Some(range) =
                        shadows::shadow(self.plan, rect, xf, *radius, *std_dev, brush.multiply_alpha(k), max)
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
        // A brush-painted op (09 §5, §6): its tier's coverage cmd becomes a Brush cmd.
        let cmd = match (self.paint, cmd) {
            (Some(paint), DrawCmd::Shapes { instances, clip }) => {
                DrawCmd::Brush { cover: plan::BrushCover::Shape(instances), paint, clip }
            }
            (Some(paint), DrawCmd::Glyphs { glyphs, clip }) => {
                DrawCmd::Brush { cover: plan::BrushCover::Mask(glyphs), paint, clip }
            }
            (Some(paint), DrawCmd::Solid { indices, clip }) => {
                DrawCmd::Brush { cover: plan::BrushCover::Mesh(indices), paint, clip }
            }
            (_, cmd) => cmd,
        };
        push_cmd(self.plan, cmd, visit);
    }

    /// The color an op's geometry is emitted with: the solid color, or for a valid gradient
    /// white (coverage × alpha only) with `self.paint` set to its `GradientDraw`; an invalid
    /// gradient draws its §5.1 fallback solid. Image brushes record their fallback (L40).
    fn brush_paint(&mut self, brush: &peniko::Brush, paint_xf: Affine) -> Option<peniko::Color> {
        match brush {
            peniko::Brush::Gradient(g) => match gradient::validate_gradient(g, paint_xf) {
                Err(c) => {
                    self.plan.stats.gradient_fallbacks += 1;
                    Some(c)
                }
                Ok(()) => {
                    let first = g.stops.first().map_or([0.0; 4], |s| color::premul(s.color.to_alpha_color()));
                    self.plan.gradients.push(plan::GradientDraw {
                        geom: gradient::gradient_geom(g, paint_xf),
                        ramp: gradient::RampKey::of(g),
                        first,
                    });
                    self.plan.stats.gradients += 1;
                    self.paint =
                        Some(plan::BrushPaint::Gradient(num::index_u32(self.plan.gradients.len() - 1)));
                    Some(peniko::Color::WHITE)
                }
            },
            peniko::Brush::Image(b) => {
                // 09 §6: P not finite or singular, or an empty image → skip the op.
                let (w, h) = (b.image.width, b.image.height);
                if w == 0 || h == 0 || !paint_xf.is_finite() || paint_xf.determinant().abs() < 1e-12 {
                    self.plan.stats.image_brush_skipped += 1;
                    return None;
                }
                let params = image_brush::params(paint_xf, w, h);
                self.plan.image_brushes.push(plan::ImageBrushDraw {
                    image: b.image.clone(),
                    key: crate::execute::resources::image_cache::ImageKey::of(&b.image),
                    sampler: images::sampler_key(b.sampler),
                    params,
                });
                self.plan.stats.image_brushes += 1;
                self.paint = Some(plan::BrushPaint::Image(num::index_u32(self.plan.image_brushes.len() - 1)));
                // Coverage × alpha only: a = sampler.alpha × fold (× hairline, applied by the stroke).
                Some(peniko::Color::WHITE.with_alpha(b.sampler.alpha.clamp(0.0, 1.0)))
            }
            peniko::Brush::Solid(_) => {
                if let Some(i) = self.pending_shader.take() {
                    self.paint = Some(plan::BrushPaint::Shader(i));
                    return Some(peniko::Color::WHITE);
                }
                self.solid_color(brush)
            }
        }
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
mod alloc_gate_tests;
#[cfg(test)]
mod bench_tests;
#[cfg(test)]
mod fallback_tests;
#[cfg(test)]
mod fragment_tests;
#[cfg(test)]
pub(crate) mod mask_budget_tests;
#[cfg(test)]
mod retained_tests;
#[cfg(test)]
mod walk_tests;

/// The device bounds of the data a fragment appended since `m0` that `culled()` does not see:
/// atlas glyphs (their culling box, R25) and solid vertices (outline-route glyphs).
fn retained_data_bounds(plan: &FramePlan, m0: &retained::Marks) -> kurbo::Rect {
    let mut b: Option<kurbo::Rect> = None;
    let mut add = |r: kurbo::Rect| b = Some(b.map_or(r, |x| x.union(r)));
    for g in plan.glyphs.get(m0.glyphs..).unwrap_or_default() {
        // Tier B path masks reported their bounds through `culled()`.
        if !matches!(g.key, crate::execute::resources::atlas_alloc::CoverageKey::Glyph(_)) {
            continue;
        }
        let s = plan.glyph_runs.get(g.run as usize).map_or(64.0, |r| f64::from(r.size_px));
        let (x, y) = (f64::from(g.origin[0]), f64::from(g.origin[1]));
        add(kurbo::Rect::new(x - 2.0 * s - 1.0, y - 2.0 * s - 1.0, x + 3.0 * s + 1.0, y + 2.0 * s + 1.0));
    }
    let v = plan.solid_vertices.get(m0.vertices..).unwrap_or_default();
    if let Some(first) = v.first() {
        let p =
            |v: &crate::gpu_types::SolidVertex| kurbo::Point::new(f64::from(v.pos[0]), f64::from(v.pos[1]));
        let r = v.iter().fold(kurbo::Rect::from_points(p(first), p(first)), |r, v| r.union_pt(p(v)));
        add(r.inflate(1.0, 1.0));
    }
    b.unwrap_or(kurbo::Rect::ZERO)
}
