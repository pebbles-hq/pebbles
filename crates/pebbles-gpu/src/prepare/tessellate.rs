//! Fill tessellation: lyon fill tessellation of device-space outlines.

use kurbo::{Affine, BezPath};
use lyon_tessellation::math::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};
use peniko::Color;

use super::Tess;
use super::color::premul;
use crate::gpu_types::SolidVertex;

/// Tessellation tolerance for screen-space geometry (physical px).
pub(crate) const FILL_TOLERANCE_PX: f32 = 0.1;

impl Tess {
    pub(crate) fn fill(&mut self, path: &BezPath, transform: Affine, style: peniko::Fill, color: Color) {
        let screen = transformed(path, transform);
        let lyon = to_lyon_path(&screen);
        let mut geometry = VertexBuffers::new();
        let mut tess = FillTessellator::new();
        let options = FillOptions::default().with_tolerance(FILL_TOLERANCE_PX).with_fill_rule(match style {
            peniko::Fill::EvenOdd => FillRule::EvenOdd,
            peniko::Fill::NonZero => FillRule::NonZero,
        });
        let _ = tess.tessellate_path(
            &lyon,
            &options,
            &mut BuffersBuilder::new(&mut geometry, SolidVertexCtor(premul(color))),
        );
        self.append(geometry);
    }
}

pub(crate) fn transformed(path: &BezPath, transform: Affine) -> BezPath {
    let mut screen = path.clone();
    screen.apply_affine(transform);
    screen
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
                builder.begin(point(to.x as f32, to.y as f32));
                open = true;
            }
            kurbo::PathEl::LineTo(to) if open => {
                builder.line_to(point(to.x as f32, to.y as f32));
            }
            kurbo::PathEl::QuadTo(ctrl, to) if open => {
                builder.quadratic_bezier_to(
                    point(ctrl.x as f32, ctrl.y as f32),
                    point(to.x as f32, to.y as f32),
                );
            }
            kurbo::PathEl::CurveTo(a, b, to) if open => {
                builder.cubic_bezier_to(
                    point(a.x as f32, a.y as f32),
                    point(b.x as f32, b.y as f32),
                    point(to.x as f32, to.y as f32),
                );
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
