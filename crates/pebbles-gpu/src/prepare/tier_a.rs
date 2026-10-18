//! Tier A draws in the walk (R10, R11): one analytic quad per eligible primitive, snapped to
//! the pixel grid per the op's `PixelSnap` (02 §5.3, R18).

use kurbo::Affine;
use pebbles_render::paint::display_list::{Geometry, Op, PixelSnap};

use super::plan::DrawCmd;
use super::{Walk, cull, hygiene, num, shapes};

/// Whether `op` asks for pixel snapping (`PixelSnap::Auto`).
pub(crate) fn snaps(op: &Op) -> bool {
    match op {
        Op::Fill { snap, .. } | Op::Stroke { snap, .. } | Op::Image { snap, .. } => *snap == PixelSnap::Auto,
        _ => false,
    }
}

impl Walk<'_> {
    fn push_shape(&mut self, inst: crate::gpu_types::ShapeInstance, idx: u32) {
        let start = num::index_u32(self.plan.shapes.len());
        self.plan.shapes.push(inst);
        self.stats.tier_a_draws += 1;
        self.push(DrawCmd::Shapes { instances: start..start + 1, clip: self.clips.current() }, idx);
    }

    /// Draw a Tier A fill; `false` if the shape is not eligible.
    pub(super) fn draw_tier_a_fill(
        &mut self,
        op: &Op,
        shape: &Geometry,
        xf: Affine,
        premul: [f32; 4],
        idx: u32,
    ) -> bool {
        let Some(mut inst) = shapes::fill_instance(shape, xf, premul, snaps(op)) else { return false };
        // R04 for Tier A: a sharp-cornered quad is clamped to the scissor (+2 px, so its AA
        // edge is unchanged) so huge rects keep f32 precision in the shader.
        if inst.outer_radii == [0.0; 4] {
            shapes::clamp_outer(&mut inst, cull::scissor_rect(self.clips.current().scissor));
        }
        self.push_shape(inst, idx);
        true
    }

    /// Draw a Tier A stroke; `false` if not eligible or past `max_coord_px` (S04).
    pub(super) fn draw_tier_a_stroke(
        &mut self,
        op: &Op,
        shape: &Geometry,
        style: &kurbo::Stroke,
        xf: Affine,
        premul: [f32; 4],
        idx: u32,
    ) -> bool {
        let Some(inst) = shapes::stroke_instance(shape, xf, style, premul, snaps(op)) else { return false };
        if !hygiene::coords_in_range(shapes::outer_rect(&inst), self.limits) {
            return false;
        }
        self.push_shape(inst, idx);
        true
    }
}
