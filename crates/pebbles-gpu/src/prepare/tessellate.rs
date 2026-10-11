//! Fill tessellation: lyon fill tessellation of device-space outlines into the solid arena.

use std::ops::Range;

use kurbo::{Affine, BezPath};
use lyon_tessellation::math::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};
use peniko::Color;

use super::color::premul;
use super::num::{index_u32, to_f32};
use super::plan::FramePlan;
use crate::gpu_types::SolidVertex;

/// Tessellation tolerance for screen-space geometry (physical px).
pub(crate) const FILL_TOLERANCE_PX: f32 = 0.1;

/// Tessellate `path` under `transform` and append to the solid arenas, returning the index range.
pub(crate) fn fill(
    plan: &mut FramePlan,
    path: &BezPath,
    transform: Affine,
    style: peniko::Fill,
    color: Color,
) -> Range<u32> {
    let rule = match style {
        peniko::Fill::EvenOdd => FillRule::EvenOdd,
        peniko::Fill::NonZero => FillRule::NonZero,
    };
    fill_premul(plan, path, transform, rule, premul(color))
}

/// Tessellate `path` under `transform` with an already-premultiplied color, appending to the
/// solid arenas and returning the index range.
pub(crate) fn fill_premul(
    plan: &mut FramePlan,
    path: &BezPath,
    transform: Affine,
    rule: FillRule,
    color: [f32; 4],
) -> Range<u32> {
    let screen = transformed(path, transform);
    let lyon = to_lyon_path(&screen);
    let mut geometry = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    let options = FillOptions::default().with_tolerance(FILL_TOLERANCE_PX).with_fill_rule(rule);
    let _ = tess.tessellate_path(
        &lyon,
        &options,
        &mut BuffersBuilder::new(&mut geometry, SolidVertexCtor(color)),
    );
    append(plan, geometry)
}

/// Append tessellated geometry to the solid arenas, rebasing indices.
pub(crate) fn append(plan: &mut FramePlan, geometry: VertexBuffers<SolidVertex, u32>) -> Range<u32> {
    let start = index_u32(plan.solid_indices.len());
    let base = index_u32(plan.solid_vertices.len());
    plan.solid_vertices.extend(geometry.vertices);
    plan.solid_indices.extend(geometry.indices.into_iter().map(|index| base + index));
    start..index_u32(plan.solid_indices.len())
}

pub(crate) fn transformed(path: &BezPath, transform: Affine) -> BezPath {
    let mut screen = path.clone();
    screen.apply_affine(transform);
    screen
}

/// A kurbo point narrowed to lyon's f32 point (non-finite → 0, R02 handles degenerate geometry).
fn pt(p: kurbo::Point) -> lyon_tessellation::math::Point {
    point(to_f32(p.x).unwrap_or(0.0), to_f32(p.y).unwrap_or(0.0))
}

pub(crate) fn to_lyon_path(path: &BezPath) -> Path {
    let mut builder = Path::builder();
    let mut open = false;
    for element in path.elements() {
        match *element {
            kurbo::PathEl::MoveTo(to) => {
                if open {
                    builder.end(false);
                }
                builder.begin(pt(to));
                open = true;
            }
            kurbo::PathEl::LineTo(to) if open => {
                builder.line_to(pt(to));
            }
            kurbo::PathEl::QuadTo(ctrl, to) if open => {
                builder.quadratic_bezier_to(pt(ctrl), pt(to));
            }
            kurbo::PathEl::CurveTo(a, b, to) if open => {
                builder.cubic_bezier_to(pt(a), pt(b), pt(to));
            }
            kurbo::PathEl::ClosePath if open => {
                builder.end(true);
                open = false;
            }
            _ => {}
        }
    }
    if open {
        builder.end(false);
    }
    builder.build()
}

pub(crate) struct SolidVertexCtor(pub(crate) [f32; 4]);
impl FillVertexConstructor<SolidVertex> for SolidVertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> SolidVertex {
        let p = vertex.position();
        SolidVertex { pos: [p.x, p.y], color: self.0 }
    }
}
