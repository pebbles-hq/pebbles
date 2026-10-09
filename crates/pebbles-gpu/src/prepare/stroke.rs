//! Stroke tessellation: expand the outline with kurbo, then fill it into the solid arena.

use std::ops::Range;

use kurbo::{Affine, BezPath, Stroke};
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers};
use peniko::Color;

use super::color::premul;
use super::plan::FramePlan;
use super::tessellate::{SolidVertexCtor, append, to_lyon_path, transformed};

/// Stroke-outline expansion tolerance (physical px).
const STROKE_TOLERANCE_PX: f64 = 0.1;

/// Expand `path`'s stroke under `transform` and append to the solid arenas, returning the range.
pub(crate) fn stroke(
    plan: &mut FramePlan,
    path: &BezPath,
    transform: Affine,
    style: &Stroke,
    color: Color,
) -> Range<u32> {
    let screen = transformed(path, transform);
    let outline = kurbo::stroke(
        screen.elements().iter().copied(),
        style,
        &kurbo::StrokeOpts::default(),
        STROKE_TOLERANCE_PX,
    );
    let lyon = to_lyon_path(&outline);
    let mut geometry = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    let options = FillOptions::default()
        .with_tolerance(super::tessellate::FILL_TOLERANCE_PX)
        .with_fill_rule(FillRule::NonZero);
    let _ = tess.tessellate_path(
        &lyon,
        &options,
        &mut BuffersBuilder::new(&mut geometry, SolidVertexCtor(premul(color))),
    );
    append(plan, geometry)
}
