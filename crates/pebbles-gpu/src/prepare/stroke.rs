//! Stroke tessellation: expand the outline with kurbo, then fill it.

use kurbo::{Affine, BezPath, Stroke};
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers};
use peniko::Color;

use super::Tess;
use super::color::premul;
use super::tessellate::{SolidVertexCtor, to_lyon_path, transformed};

/// Stroke-outline expansion tolerance (physical px).
const STROKE_TOLERANCE_PX: f64 = 0.1;

impl Tess {
    pub(crate) fn stroke(&mut self, path: &BezPath, transform: Affine, style: &Stroke, color: Color) {
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
        self.append(geometry);
    }
}
