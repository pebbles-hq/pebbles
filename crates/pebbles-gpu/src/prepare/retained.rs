//! Retained prepare (DR-25, M29): a fragment (`Op::Fragment`, shared `Arc<Vec<Op>>`) that is
//! unchanged since the last frame replays its prepared draws instead of re-walking its ops.
//!
//! What may be cached is deliberately narrow: fragments whose walk produced only solid
//! meshes, analytic shapes, shadows, atlas glyphs and images — no layers, clips, paints, masks,
//! meshes, GPU images or custom ops — under the same clip content, fold alpha and transform
//! (linear part and sub-pixel translation exactly equal), with every op inside the scissor (so
//! culling decides nothing). A hit copies the saved data, adding the integer device move to
//! every position field, rebases every range, re-issues the atlas/image requests (they hit)
//! and re-applies the saved stats, so the plan equals a from-scratch prepare. A replay at the
//! same position is byte-identical; a moved one can differ in the last f32 bit of a position
//! that crosses a power of two (sub-1/1000 px; the integer translation of Flutter's raster cache).

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, Weak};

use pebbles_render::paint::display_list::Op;

use super::coverage::FeatureSet;
use super::plan::{DrawCmd, GlyphRequest, GlyphRunInfo, ImageRequest, PrepareStats};
use crate::execute::resources::atlas_alloc::CoverageKey;
use crate::gpu_types::{ShadowInstance, ShapeInstance, SolidVertex};
use crate::stats::PrepareDelta;

/// The union of two bounds where `Rect::ZERO` means "nothing yet".
pub(crate) fn join(a: kurbo::Rect, b: kurbo::Rect) -> kurbo::Rect {
    if a == kurbo::Rect::ZERO {
        b
    } else if b == kurbo::Rect::ZERO {
        a
    } else {
        a.union(b)
    }
}

/// Fragments with fewer ops are re-prepared (cheaper than a lookup + copy).
pub(crate) const MIN_OPS: usize = 8;
/// Entries unused this many frames are dropped.
pub(crate) const IDLE_FRAMES: u64 = 120;

/// Identity of a prepared fragment: the shared op list (verified by `Weak`), the exact linear
/// part and sub-pixel translation of its placement, and the content of everything that
/// shaped its draws (incoming clip, fold alpha, glyph inputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RetainedKey {
    pub ops_ptr: usize,
    pub linear: [u64; 4],
    pub frac: (u64, u64),
    pub context: u64,
}

/// Everything one fragment's walk produced, relative to its recorded array starts and
/// integer device origin.
pub(crate) struct RetainedEntry {
    weak: Weak<Vec<Op>>,
    origin: (i32, i32),
    cmds: Vec<DrawCmd>,
    vertices: Vec<SolidVertex>,
    indices: Vec<u32>,
    shapes: Vec<ShapeInstance>,
    shadows: Vec<ShadowInstance>,
    runs: Vec<GlyphRunInfo>,
    coords: Vec<i16>,
    glyphs: Vec<GlyphRequest>,
    images: Vec<ImageRequest>,
    /// Tier B mask sources and the path-budget admissions (key hash, bytes) they needed (M28).
    masks: Vec<super::path_masks::MaskSource>,
    admits: Vec<(u64, u64)>,
    features: FeatureSet,
    frame_delta: PrepareDelta,
    plan_delta: PrepareStats,
    /// Ops visited (visit-index advance).
    visits: u32,
    /// Union of the ops' device bounds at record time.
    bounds: kurbo::Rect,
    /// Replay needs the moved bounds inside (`Inside`) or outside (`Outside`) the scissor again.
    mode: Mode,
    bytes: u64,
    last_used: u64,
}

/// How a recorded fragment related to the scissor (culling must decide the same on replay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Every op inside the scissor.
    Inside,
    /// Every op culled and nothing produced.
    Outside,
}

/// Array starts of a plan at one point of the walk.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Marks {
    pub cmds: usize,
    pub vertices: usize,
    pub indices: usize,
    pub shapes: usize,
    pub shadows: usize,
    pub runs: usize,
    pub coords: usize,
    pub glyphs: usize,
    pub images: usize,
    pub masks: usize,
    /// Arrays a cacheable fragment must not touch, summed.
    pub other: usize,
}

impl Marks {
    pub(crate) fn of(p: &super::plan::FramePlan) -> Self {
        Self {
            cmds: p.cmds.len(),
            vertices: p.solid_vertices.len(),
            indices: p.solid_indices.len(),
            shapes: p.shapes.len(),
            shadows: p.shadows.len(),
            runs: p.glyph_runs.len(),
            coords: p.coords.len(),
            glyphs: p.glyphs.len(),
            images: p.images.len(),
            masks: p.mask_sources.len(),
            other: p.layers.len()
                + p.effects.len()
                + p.projections.len()
                + p.sdf_clips.len()
                + p.mask_clips.len()
                + p.clip_sources.len()
                + p.gpu_mask_jobs.len()
                + p.gradients.len()
                + p.image_brushes.len()
                + p.shader_brushes.len()
                + p.customs.len()
                + p.gpu_images.len()
                + p.mesh_vertices.len()
                + p.mesh_images.len(),
        }
    }
}

/// The renderer's retained fragments (census `retained_entries`, `retained_bytes`).
#[derive(Default)]
pub(crate) struct RetainedCache {
    entries: HashMap<RetainedKey, RetainedEntry>,
    frame: u64,
    bytes: u64,
}

fn u32_of(v: usize) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn shift_range(r: &Range<u32>, by: i64) -> Range<u32> {
    let f = |v: u32| u32::try_from(i64::from(v) + by).unwrap_or(0);
    f(r.start)..f(r.end)
}

fn offset(v: f32, d: i32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // |d| ≤ target size (≪ 2^24)
    let d = d as f32;
    v + d
}

/// `cmd`'s ranges shifted by the per-array deltas (`[indices, shapes, shadows, glyphs, images]`).
fn rebase_cmd(cmd: &DrawCmd, d: [i64; 5], clip: super::plan::ClipState) -> Option<DrawCmd> {
    Some(match cmd {
        DrawCmd::Solid { indices, .. } => DrawCmd::Solid { indices: shift_range(indices, d[0]), clip },
        DrawCmd::Shapes { instances, .. } => {
            DrawCmd::Shapes { instances: shift_range(instances, d[1]), clip }
        }
        DrawCmd::Shadows { instances, .. } => {
            DrawCmd::Shadows { instances: shift_range(instances, d[2]), clip }
        }
        DrawCmd::Glyphs { glyphs, .. } => DrawCmd::Glyphs { glyphs: shift_range(glyphs, d[3]), clip },
        DrawCmd::Images { images, .. } => DrawCmd::Images { images: shift_range(images, d[4]), clip },
        _ => return None,
    })
}

impl RetainedCache {
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Drop entries idle for `IDLE_FRAMES`, then the least recently used over `budget`.
    pub(crate) fn end_frame(&mut self, budget: u64) {
        let frame = self.frame;
        let mut freed = 0;
        self.entries.retain(|_, e| {
            let keep = frame.saturating_sub(e.last_used) < IDLE_FRAMES && e.weak.strong_count() > 0;
            if !keep {
                freed += e.bytes;
            }
            keep
        });
        self.bytes -= freed;
        while self.bytes > budget {
            let Some(k) = self.entries.iter().min_by_key(|(_, e)| e.last_used).map(|(k, _)| *k) else {
                break;
            };
            if let Some(e) = self.entries.remove(&k) {
                self.bytes -= e.bytes;
            }
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn bytes(&self) -> u64 {
        self.bytes
    }

    /// A live entry for `key` whose op list is still `ops` (never a reused address, ABA).
    fn live(&mut self, key: &RetainedKey, ops: &Arc<Vec<Op>>) -> Option<&mut RetainedEntry> {
        let e = self.entries.get_mut(key)?;
        (Weak::ptr_eq(&e.weak, &Arc::downgrade(ops)) && e.weak.strong_count() > 0).then_some(e)
    }

    /// The recorded device bounds of `key`'s entry moved to `origin` and its mode, if live.
    pub(crate) fn hit_bounds(
        &mut self,
        key: &RetainedKey,
        ops: &Arc<Vec<Op>>,
        origin: (i32, i32),
    ) -> Option<(kurbo::Rect, Mode)> {
        let e = self.live(key, ops)?;
        let (dx, dy) = (origin.0 - e.origin.0, origin.1 - e.origin.1);
        Some((e.bounds + kurbo::Vec2::new(f64::from(dx), f64::from(dy)), e.mode))
    }

    /// The path-budget admissions `key`'s entry needs (checked by the walk before a replay).
    pub(crate) fn admits(&self, key: &RetainedKey) -> &[(u64, u64)] {
        self.entries.get(key).map_or(&[], |e| &e.admits)
    }

    /// Replay `key`'s entry into `plan` at `origin` under `clip`; returns the saved deltas and
    /// visit count. The caller has checked `hit_bounds`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn replay(
        &mut self,
        key: &RetainedKey,
        ops: &Arc<Vec<Op>>,
        origin: (i32, i32),
        clip: super::plan::ClipState,
        plan: &mut super::plan::FramePlan,
        mut push: impl FnMut(&mut super::plan::FramePlan, DrawCmd),
    ) -> Option<(PrepareDelta, PrepareStats, FeatureSet, u32, kurbo::Rect)> {
        let frame = self.frame;
        let e = self.live(key, ops)?;
        e.last_used = frame;
        let (dx, dy) = (origin.0 - e.origin.0, origin.1 - e.origin.1);
        let m = Marks::of(plan);
        let vbase = u32_of(m.vertices);
        plan.solid_vertices.extend(
            e.vertices
                .iter()
                .map(|v| SolidVertex { pos: [offset(v.pos[0], dx), offset(v.pos[1], dy)], ..*v }),
        );
        plan.solid_indices.extend(e.indices.iter().map(|i| i + vbase));
        let rect = |r: [f32; 4]| [offset(r[0], dx), offset(r[1], dy), offset(r[2], dx), offset(r[3], dy)];
        plan.shapes.extend(e.shapes.iter().map(|s| ShapeInstance {
            outer: rect(s.outer),
            // A fill's inner rect is the `[0; 4]` sentinel, not a position.
            inner: if s.inner == [0.0; 4] { s.inner } else { rect(s.inner) },
            ..*s
        }));
        plan.shadows.extend(e.shadows.iter().map(|s| {
            let mut s = *s;
            s.origin_size[0] = offset(s.origin_size[0], dx);
            s.origin_size[1] = offset(s.origin_size[1], dy);
            s
        }));
        let coord_base = u32_of(m.coords);
        plan.coords.extend_from_slice(&e.coords);
        plan.glyph_runs.extend(
            e.runs
                .iter()
                .map(|r| GlyphRunInfo { coords: shift_range(&r.coords, i64::from(coord_base)), ..r.clone() }),
        );
        let run_base = u32_of(m.runs);
        let mask_base = u32_of(m.masks);
        for src in &e.masks {
            let slot = plan.mask_sources.next_slot();
            slot.path.extend(src.path.elements().iter().copied());
            (slot.device, slot.rule, slot.stroke, slot.blur) =
                (src.device, src.rule, src.stroke.clone(), src.blur);
            plan.mask_sources.commit();
        }
        plan.glyphs.extend(e.glyphs.iter().map(|g| GlyphRequest {
            run: g.run + if matches!(g.key, CoverageKey::Path(_)) { mask_base } else { run_base },
            origin: [offset(g.origin[0], dx), offset(g.origin[1], dy)],
            ..*g
        }));
        plan.images.extend(e.images.iter().map(|r| {
            let mut r = r.clone();
            r.quad.origin = [offset(r.quad.origin[0], dx), offset(r.quad.origin[1], dy)];
            r
        }));
        let d = [m.indices, m.shapes, m.shadows, m.glyphs, m.images].map(|v| i64::try_from(v).unwrap_or(0));
        for c in &e.cmds {
            if let Some(c) = rebase_cmd(c, d, clip) {
                push(plan, c);
            }
        }
        let bounds = e.bounds + kurbo::Vec2::new(f64::from(dx), f64::from(dy));
        Some((e.frame_delta, e.plan_delta.clone(), e.features, e.visits, bounds))
    }

    /// Record the walk's output between `m0` and now as `key`'s entry. `first_merged` is the
    /// pre-fragment last command if the fragment's first draw merged into it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &mut self,
        key: RetainedKey,
        ops: &Arc<Vec<Op>>,
        origin: (i32, i32),
        m0: &Marks,
        last_before: Option<&DrawCmd>,
        plan: &super::plan::FramePlan,
        deltas: (PrepareDelta, PrepareStats, FeatureSet, u32),
        bounds: kurbo::Rect,
        mode: Mode,
        admits: Vec<(u64, u64)>,
    ) {
        // A dynamic image can change behind an unchanged op: never cached.
        if plan.images.get(m0.images..).unwrap_or_default().iter().any(|r| r.dynamic.is_some()) {
            return;
        }
        let vbase = u32_of(m0.vertices);
        let rel_cmd = |c: &DrawCmd| -> Option<DrawCmd> {
            let clip = super::plan::ClipState::full((0, 0));
            let d = [m0.indices, m0.shapes, m0.shadows, m0.glyphs, m0.images]
                .map(|v| -i64::try_from(v).unwrap_or(0));
            rebase_cmd(c, d, clip)
        };
        let mut cmds = Vec::new();
        // The fragment's first draw merged into the previous command: keep its part only.
        if let (Some(before), Some(after)) = (last_before, plan.cmds.get(m0.cmds.wrapping_sub(1)))
            && before != after
        {
            let part = match (before, after) {
                (DrawCmd::Solid { indices: b, clip }, DrawCmd::Solid { indices: a, .. }) => {
                    DrawCmd::Solid { indices: b.end..a.end, clip: *clip }
                }
                (DrawCmd::Shapes { instances: b, clip }, DrawCmd::Shapes { instances: a, .. }) => {
                    DrawCmd::Shapes { instances: b.end..a.end, clip: *clip }
                }
                (DrawCmd::Shadows { instances: b, clip }, DrawCmd::Shadows { instances: a, .. }) => {
                    DrawCmd::Shadows { instances: b.end..a.end, clip: *clip }
                }
                (DrawCmd::Glyphs { glyphs: b, clip }, DrawCmd::Glyphs { glyphs: a, .. }) => {
                    DrawCmd::Glyphs { glyphs: b.end..a.end, clip: *clip }
                }
                (DrawCmd::Images { images: b, clip }, DrawCmd::Images { images: a, .. }) => {
                    DrawCmd::Images { images: b.end..a.end, clip: *clip }
                }
                _ => return,
            };
            let Some(c) = rel_cmd(&part) else { return };
            cmds.push(c);
        }
        for c in plan.cmds.get(m0.cmds..).unwrap_or_default() {
            let Some(c) = rel_cmd(c) else { return };
            cmds.push(c);
        }
        let e = RetainedEntry {
            weak: Arc::downgrade(ops),
            origin,
            cmds,
            vertices: plan.solid_vertices.get(m0.vertices..).unwrap_or_default().to_vec(),
            indices: plan
                .solid_indices
                .get(m0.indices..)
                .unwrap_or_default()
                .iter()
                .map(|i| i.wrapping_sub(vbase))
                .collect(),
            shapes: plan.shapes.get(m0.shapes..).unwrap_or_default().to_vec(),
            shadows: plan.shadows.get(m0.shadows..).unwrap_or_default().to_vec(),
            runs: plan
                .glyph_runs
                .get(m0.runs..)
                .unwrap_or_default()
                .iter()
                .map(|r| GlyphRunInfo {
                    coords: shift_range(&r.coords, -i64::try_from(m0.coords).unwrap_or(0)),
                    ..r.clone()
                })
                .collect(),
            coords: plan.coords.get(m0.coords..).unwrap_or_default().to_vec(),
            glyphs: plan
                .glyphs
                .get(m0.glyphs..)
                .unwrap_or_default()
                .iter()
                .map(|g| GlyphRequest {
                    run: g.run.wrapping_sub(u32_of(if matches!(g.key, CoverageKey::Path(_)) {
                        m0.masks
                    } else {
                        m0.runs
                    })),
                    ..*g
                })
                .collect(),
            images: plan.images.get(m0.images..).unwrap_or_default().to_vec(),
            masks: (m0.masks..plan.mask_sources.len())
                .filter_map(|i| plan.mask_sources.get(i).cloned())
                .collect(),
            admits,
            features: deltas.2,
            frame_delta: deltas.0,
            plan_delta: deltas.1,
            visits: deltas.3,
            bounds,
            mode,
            bytes: 0,
            last_used: self.frame,
        };
        let bytes = (size_of::<SolidVertex>() * e.vertices.len()
            + 4 * e.indices.len()
            + size_of::<ShapeInstance>() * e.shapes.len()
            + size_of::<ShadowInstance>() * e.shadows.len()
            + size_of::<GlyphRequest>() * e.glyphs.len()
            + size_of::<ImageRequest>() * e.images.len()
            + size_of::<DrawCmd>() * e.cmds.len()
            + 256) as u64;
        let e = RetainedEntry { bytes, ..e };
        if let Some(old) = self.entries.insert(key, e) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
    }
}
