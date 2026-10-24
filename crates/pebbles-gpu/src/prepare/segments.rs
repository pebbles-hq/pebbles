//! Frame segmentation (10 §2.5, M07): a frame whose arrays exceed the arena caps (or the
//! portable 2²⁴−1 vertex rule) is cut into segments of whole draw commands, uploaded and
//! submitted one after another. Normal frames are one segment and this pass costs one size
//! check. Merging never crosses a cap (see `FramePlan::push_draw`), so a segment boundary
//! never splits an op; a single command alone over a cap is dropped and counted.

use std::ops::Range;

use super::plan::{BrushCover, DrawCmd, FramePlan};

/// Vertices per segment on the portable profile (`FULL_DRAW_INDEX_UINT32` may be absent).
pub(crate) const MAX_PORTABLE_VERTICES_PER_SEGMENT: u32 = (1 << 24) - 1;

/// Per-array byte caps (each arena's `max_capacity`) and the vertex-count rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ArenaCaps {
    pub bytes: u64,
    pub max_vertices: u32,
}

impl Default for ArenaCaps {
    fn default() -> Self {
        Self { bytes: u64::MAX, max_vertices: MAX_PORTABLE_VERTICES_PER_SEGMENT }
    }
}

/// Element sizes of the arrays (bytes), in `Arrays` order.
const ELEM: [u64; 8] = [
    size_of::<crate::gpu_types::SolidVertex>() as u64,
    4,
    size_of::<crate::gpu_types::ShapeInstance>() as u64,
    size_of::<crate::gpu_types::ShadowInstance>() as u64,
    size_of::<crate::gpu_types::QuadInstance>() as u64,
    size_of::<crate::gpu_types::QuadInstance>() as u64,
    size_of::<crate::gpu_types::QuadInstance>() as u64,
    size_of::<crate::gpu_types::MeshVertex>() as u64,
];

/// Ranges of every array a segment uses, in this order: solid vertices, solid indices, shapes,
/// shadows, glyphs, images, GPU images, mesh vertices.
pub(crate) type Arrays = [Range<u32>; 8];

pub(crate) const VERTICES: usize = 0;
pub(crate) const INDICES: usize = 1;
pub(crate) const SHAPES: usize = 2;
pub(crate) const SHADOWS: usize = 3;
pub(crate) const GLYPHS: usize = 4;
pub(crate) const IMAGES: usize = 5;
pub(crate) const GPU_IMAGES: usize = 6;
pub(crate) const MESHES: usize = 7;

/// A contiguous range of `FramePlan::cmds` whose array data fits every cap (10 §2.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlanSegment {
    pub cmds: Range<usize>,
    /// The array ranges (into the FramePlan arrays; indices hold absolute vertex numbers, the
    /// upload rebases them by `arrays[VERTICES].start`).
    pub arrays: Arrays,
    /// `plan.gpu_mask_jobs` rendered with this segment's data.
    pub jobs: Range<usize>,
}

/// The array bases of the segment being drawn: the arenas hold only its slices, so every
/// range a command carries is rebased by them (indices also by the vertex base, at upload).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SegBase {
    pub start: [u32; 8],
    /// Images in this segment's image slice (its GPU-image instances follow them).
    pub images_len: u32,
}

impl SegBase {
    pub(crate) fn of(s: &PlanSegment) -> Self {
        Self {
            start: s.arrays.clone().map(|r| r.start),
            images_len: s.arrays[IMAGES].end - s.arrays[IMAGES].start,
        }
    }

    /// `r` in the segment's arena of array `a`.
    pub(crate) fn rebase(&self, a: usize, r: &Range<u32>) -> Range<u32> {
        let b = self.start.get(a).copied().unwrap_or(0);
        r.start.saturating_sub(b)..r.end.saturating_sub(b)
    }

    /// The arena instance of GPU image `i` (after the segment's images).
    pub(crate) fn gpu_instance(&self, i: u32) -> u32 {
        self.images_len + i.saturating_sub(self.start[GPU_IMAGES])
    }
}

fn empty() -> Arrays {
    std::array::from_fn(|_| 0..0)
}

/// The vertex span `min..max + 1` referenced by `indices` (absolute).
fn vertex_span(plan: &FramePlan, indices: &Range<u32>) -> Option<Range<u32>> {
    let ix = plan.solid_indices.get(indices.start as usize..indices.end as usize)?;
    let (lo, hi) = ix.iter().fold((u32::MAX, 0), |(lo, hi), &i| (lo.min(i), hi.max(i)));
    (lo <= hi).then(|| lo..hi + 1)
}

/// What one command uses, per array.
fn usage(plan: &FramePlan, cmd: &DrawCmd) -> Arrays {
    let mut u = empty();
    let mesh = |u: &mut Arrays, indices: &Range<u32>| {
        u[INDICES] = indices.clone();
        if let Some(v) = vertex_span(plan, indices) {
            u[VERTICES] = v;
        }
    };
    match cmd {
        DrawCmd::Solid { indices, .. } | DrawCmd::Brush { cover: BrushCover::Mesh(indices), .. } => {
            mesh(&mut u, indices)
        }
        DrawCmd::Shapes { instances, .. } | DrawCmd::Brush { cover: BrushCover::Shape(instances), .. } => {
            u[SHAPES] = instances.clone();
        }
        DrawCmd::Shadows { instances, .. } => u[SHADOWS] = instances.clone(),
        DrawCmd::Glyphs { glyphs, .. } | DrawCmd::Brush { cover: BrushCover::Mask(glyphs), .. } => {
            u[GLYPHS] = glyphs.clone();
        }
        DrawCmd::Images { images, .. } => u[IMAGES] = images.clone(),
        DrawCmd::GpuImage { image, .. } => u[GPU_IMAGES] = *image..image + 1,
        DrawCmd::Vertices { vertices, .. } => u[MESHES] = vertices.clone(),
        DrawCmd::PushLayer { .. }
        | DrawCmd::PopLayer { .. }
        | DrawCmd::Composite { .. }
        | DrawCmd::Custom { .. } => {}
    }
    u
}

fn union(a: &mut Arrays, b: &Arrays) {
    for (x, y) in a.iter_mut().zip(b) {
        if y.start >= y.end {
            continue;
        }
        *x = if x.start >= x.end { y.clone() } else { x.start.min(y.start)..x.end.max(y.end) };
    }
}

fn fits(a: &Arrays, caps: ArenaCaps) -> bool {
    a.iter().zip(ELEM).all(|(r, e)| u64::from(r.end - r.start) * e <= caps.bytes)
        && a[VERTICES].len() as u64 <= u64::from(caps.max_vertices)
}

/// Whole-frame totals fit: one segment, no per-command scan.
fn frame_fits(plan: &FramePlan, caps: ArenaCaps) -> bool {
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    let all: Arrays = [
        0..n(plan.solid_vertices.len()),
        0..n(plan.solid_indices.len()),
        0..n(plan.shapes.len()),
        0..n(plan.shadows.len()),
        0..n(plan.glyphs.len()),
        0..n(plan.images.len()),
        0..n(plan.gpu_images.len()),
        0..n(plan.mesh_vertices.len()),
    ];
    fits(&all, caps)
}

/// Cut `plan.cmds` into `plan.segments` under `caps`. Returns how many commands were dropped
/// for exceeding a cap alone.
pub(crate) fn build(plan: &mut FramePlan, caps: ArenaCaps) -> u32 {
    plan.segments.clear();
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    if frame_fits(plan, caps) {
        let arrays = [
            0..n(plan.solid_vertices.len()),
            0..n(plan.solid_indices.len()),
            0..n(plan.shapes.len()),
            0..n(plan.shadows.len()),
            0..n(plan.glyphs.len()),
            0..n(plan.images.len()),
            0..n(plan.gpu_images.len()),
            0..n(plan.mesh_vertices.len()),
        ];
        plan.segments.push(PlanSegment {
            cmds: 0..plan.cmds.len(),
            arrays,
            jobs: 0..plan.gpu_mask_jobs.len(),
        });
        return 0;
    }
    // Drop commands that exceed a cap alone (an op over the cap: 10 §2.5).
    let mut dropped = 0;
    let mut i = 0;
    while i < plan.cmds.len() {
        if plan.cmds.get(i).is_some_and(|c| fits(&usage(plan, c), caps)) {
            i += 1;
            continue;
        }
        plan.cmds.remove(i);
        for j in &mut plan.gpu_mask_jobs {
            if j.cmd_at as usize > i {
                j.cmd_at -= 1;
            }
        }
        dropped += 1;
    }
    // M33: a dropped fill no longer covers its shadows' holes.
    if dropped > 0 {
        for s in &mut plan.shadows {
            s.params1 = [0.0; 4];
        }
    }
    let mut seg_start = 0;
    let mut cur = empty();
    let mut job = 0;
    let job_usage = |plan: &FramePlan, j: usize| {
        let mut u = empty();
        if let Some(job) = plan.gpu_mask_jobs.get(j) {
            u[INDICES] = job.indices.clone();
            if let Some(v) = vertex_span(plan, &job.indices) {
                u[VERTICES] = v;
            }
        }
        u
    };
    let mut seg_job_start = 0;
    for c in 0..=plan.cmds.len() {
        // The GPU mask jobs recorded before cmd `c` belong with it.
        let mut add = empty();
        let job_end =
            plan.gpu_mask_jobs.iter().skip(job).take_while(|j| (j.cmd_at as usize) <= c).count() + job;
        for j in job..job_end {
            union(&mut add, &job_usage(plan, j));
        }
        if let Some(cmd) = plan.cmds.get(c) {
            union(&mut add, &usage(plan, cmd));
        }
        let mut next = cur.clone();
        union(&mut next, &add);
        if c < plan.cmds.len() && !fits(&next, caps) && c > seg_start {
            plan.segments.push(PlanSegment {
                cmds: seg_start..c,
                arrays: cur.clone(),
                jobs: seg_job_start..job,
            });
            seg_start = c;
            seg_job_start = job;
            cur = add;
        } else {
            cur = next;
        }
        job = job_end;
    }
    plan.segments.push(PlanSegment {
        cmds: seg_start..plan.cmds.len(),
        arrays: cur,
        jobs: seg_job_start..plan.gpu_mask_jobs.len(),
    });
    dropped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::test_util::{prepare_scene, prepare_scene_tessellated};
    use kurbo::{Affine, Rect};
    use pebbles_render::paint::{Painter, Scene};
    use peniko::{Color, Fill};

    fn rects(n: u32) -> Scene {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for i in 0..n {
            let (x, y) = (f64::from(i % 25) * 10.0, f64::from(i / 25) * 10.0);
            let c = if i % 2 == 0 { Color::BLACK } else { Color::from_rgb8(200, 0, 0) };
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(x, y, x + 8.0, y + 8.0));
        }
        s
    }

    fn capped(scene: &Scene, sdf: bool, caps: ArenaCaps) -> FramePlan {
        let mut scratch =
            crate::prepare::PrepareScratch { arena_caps: caps, sdf_disabled: !sdf, ..Default::default() };
        let mut stats = crate::stats::FrameStats::default();
        let input = crate::prepare::PrepareInput {
            target_size: (256, 256),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
            scale_factor: 1.0,
            glyph_hint: None,
        };
        crate::prepare::prepare(
            scene,
            &input,
            &crate::test_support::webgl2_limits(),
            &mut scratch,
            &mut stats,
        );
        scratch.plan
    }

    #[test]
    fn single_segment_for_normal_scene() {
        let (mut plan, _) = prepare_scene(&rects(500), 256, 256);
        assert_eq!(build(&mut plan, ArenaCaps::default()), 0);
        assert_eq!(plan.segments.len(), 1);
        assert_eq!(plan.segments[0].cmds, 0..plan.cmds.len());
    }

    #[test]
    fn tiny_cap_splits_at_op_boundaries() {
        let caps = ArenaCaps { bytes: 4096, max_vertices: MAX_PORTABLE_VERTICES_PER_SEGMENT };
        for (name, plan) in
            [("tier a", capped(&rects(500), true, caps)), ("tessellated", capped(&rects(500), false, caps))]
        {
            assert!(plan.segments.len() > 1, "{name}: {} segments", plan.segments.len());
            let mut next = 0;
            for s in &plan.segments {
                assert_eq!(s.cmds.start, next, "{name}: contiguous");
                next = s.cmds.end;
                assert!(fits(&s.arrays, caps), "{name}: {:?}", s.arrays);
                let v = &s.arrays[VERTICES];
                for &ix in
                    &plan.solid_indices[s.arrays[INDICES].start as usize..s.arrays[INDICES].end as usize]
                {
                    assert!(v.contains(&ix), "{name}: every index lies in its segment's vertices");
                    assert!(ix - v.start < v.end - v.start);
                }
            }
            assert_eq!(next, plan.cmds.len());
        }
    }

    #[test]
    fn merge_never_crosses_segment() {
        let caps = ArenaCaps { bytes: 4096, max_vertices: MAX_PORTABLE_VERTICES_PER_SEGMENT };
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        // One color: without the cap every rect merges into one command.
        for i in 0..500u32 {
            let (x, y) = (f64::from(i % 25) * 10.0, f64::from(i / 25) * 10.0);
            p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(x, y, x + 8.0, y + 8.0));
        }
        let (uncapped, _) = prepare_scene_tessellated(&s, 256, 256);
        assert_eq!(uncapped.cmds.len(), 1, "uncapped: one merged command");
        let plan = capped(&s, false, caps);
        for c in &plan.cmds {
            assert!(fits(&usage(&plan, c), caps), "a merged command stays under the cap: {c:?}");
        }
        assert!(plan.segments.len() > 1);
        for w in plan.segments.windows(2) {
            assert_eq!(w[0].cmds.end, w[1].cmds.start, "boundaries fall between commands");
        }
    }

    #[test]
    fn oversize_op_dropped_and_counted() {
        let caps = ArenaCaps { bytes: 64, max_vertices: MAX_PORTABLE_VERTICES_PER_SEGMENT };
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        // A circle tessellates to far more than 64 bytes of vertices.
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Circle::new((50.0, 50.0), 40.0));
        let (mut plan, _) = prepare_scene_tessellated(&s, 128, 128);
        assert_eq!(build(&mut plan, caps), 1);
        assert!(plan.cmds.is_empty());
    }
}
