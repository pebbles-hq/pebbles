//! Shared CPU helpers for prepare tests (02 R01 step 6): run prepare on a scene and inspect
//! the plan. The RNG stays in `crate::test_support`.

use kurbo::{Affine, Point, Rect};
use pebbles_render::paint::Scene;

use super::{DrawCmd, FramePlan, PrepareInput, PrepareScratch, prepare};
use crate::stats::FrameStats;

/// Prepare `scene` into a `w`×`h` target with an identity root transform and fresh stats.
pub(crate) fn prepare_scene(scene: &Scene, w: u32, h: u32) -> (FramePlan, FrameStats) {
    let mut scratch = PrepareScratch::default();
    let mut stats = FrameStats::default();
    let input =
        PrepareInput { target_size: (w, h), root_transform: Affine::IDENTITY, vertex_limit: u32::MAX };
    prepare(scene, &input, &crate::test_support::webgl2_limits(), &mut scratch, &mut stats);
    (scratch.plan, stats)
}

/// One short name per `DrawCmd`, in order.
pub(crate) fn cmd_kinds(plan: &FramePlan) -> Vec<&'static str> {
    plan.cmds
        .iter()
        .map(|cmd| match cmd {
            DrawCmd::Solid { .. } => "triangles",
            DrawCmd::Shadows { .. } => "shadows",
            DrawCmd::Glyphs { .. } => "glyphs",
            DrawCmd::Images { .. } => "images",
        })
        .collect()
}

/// Bounding box of the solid vertex arena (`Rect::ZERO` when empty).
#[allow(dead_code)] // first caller: R04 (`huge_rect_is_clamped`)
pub(crate) fn solid_bbox(plan: &FramePlan) -> Rect {
    let mut it = plan.solid_vertices.iter().map(|v| Point::new(f64::from(v.pos[0]), f64::from(v.pos[1])));
    let Some(first) = it.next() else { return Rect::ZERO };
    it.fold(Rect::from_points(first, first), |r, p| r.union_pt(p))
}

/// Whether `p` lies inside any solid triangle (barycentric sign test, edges inclusive).
#[allow(dead_code)] // first caller: R03 (`even_odd_donut_has_a_hole`)
pub(crate) fn covers(plan: &FramePlan, p: Point) -> bool {
    let v = |i: u32| {
        let pos = plan.solid_vertices[i as usize].pos;
        Point::new(f64::from(pos[0]), f64::from(pos[1]))
    };
    plan.solid_indices.as_chunks::<3>().0.iter().any(|t| {
        let (a, b, c) = (v(t[0]), v(t[1]), v(t[2]));
        if (b - a).cross(c - a) == 0.0 {
            return false;
        }
        let d1 = (p - b).cross(a - b);
        let d2 = (p - c).cross(b - c);
        let d3 = (p - a).cross(c - a);
        let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
        let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
        !(neg && pos)
    })
}
