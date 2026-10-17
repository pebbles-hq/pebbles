//! Per-op admission checks of the prepare walk: input hygiene (02 §6.1, 11 §2.3), culling,
//! fallbacks that skip an op, group alpha and the vertex budget.

use kurbo::Affine;

use super::{Walk, cull, hygiene};
use crate::stats::Unsupported;

impl Walk<'_> {
    /// Skip an op with non-finite input (02 §6.1): counted and recorded, never repaired.
    pub(super) fn skip_non_finite(&mut self) {
        self.plan.fallback_features.insert(super::coverage::Feature::NonFinite);
        self.stats.non_finite_skipped += 1;
        self.stats.record(Unsupported::NonFinite);
    }

    /// S02 paint hygiene, after R02's checks: a non-finite solid or gradient-stop colour skips
    /// the op (counted, recorded); a singular transform skips it silently (`degenerate_skipped`).
    pub(super) fn paint_is_sane(&mut self, brush: &peniko::Brush, xf: Affine) -> bool {
        let finite = match brush {
            peniko::Brush::Solid(c) => hygiene::color_is_finite(c),
            peniko::Brush::Gradient(g) => hygiene::stop_colors_are_finite(g),
            peniko::Brush::Image(_) => true,
        };
        if !finite {
            self.skip_non_finite();
            return false;
        }
        self.transform_is_sane(xf)
    }

    /// A singular op transform skips the op silently (11 §2.3).
    pub(super) fn transform_is_sane(&mut self, xf: Affine) -> bool {
        if hygiene::transform_is_singular(&xf) {
            self.stats.degenerate_skipped += 1;
            return false;
        }
        true
    }

    /// The solid colour of a fill/stroke brush; other brushes record their fallback.
    pub(super) fn solid_color(&mut self, brush: &peniko::Brush) -> Option<peniko::Color> {
        match brush {
            peniko::Brush::Solid(c) => Some(*c),
            peniko::Brush::Gradient(_) => {
                self.stats.record(Unsupported::GradientBrush);
                None
            }
            peniko::Brush::Image(_) => {
                self.stats.record(Unsupported::ImageBrush);
                None
            }
        }
    }

    /// Whether draws at the current position are hidden (culled clip or skipped layer).
    /// Counts culled ops.
    pub(super) fn hidden(&mut self) -> bool {
        if self.layers.is_skipped() {
            return true;
        }
        if self.clips.is_culled() {
            self.plan.stats.culled += 1;
            return true;
        }
        false
    }

    /// Whether device bounds `b` miss the active scissor; counts the culled op (ORD-7).
    /// Bounds beyond `max_coord_px` first record `coordinate-out-of-range` (still drawn, S02).
    /// Also sets the op's flattening-tolerance scale (`tessellate::tolerance_scale`).
    pub(super) fn culled(&mut self, b: kurbo::Rect) -> bool {
        self.tess.tol_scale = 1.0;
        // Device bounds past the f32 range (or non-finite) cannot become vertices: every point
        // would narrow to ±inf. Skip as non-finite before any path is built (02 §6.1).
        if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite() && v.abs() <= f64::from(f32::MAX)) {
            self.skip_non_finite();
            return true;
        }
        if !hygiene::coords_in_range(b, self.limits) {
            self.stats.record(Unsupported::CoordOutOfRange);
            self.tess.tol_scale = super::tessellate::tolerance_scale(b, self.limits.max_coord_px);
        }
        if !cull::is_outside(b, self.clips.current().scissor) {
            return false;
        }
        self.plan.stats.culled += 1;
        #[cfg(debug_assertions)]
        {
            self.plan.stats.culled_ops += 1;
        }
        true
    }

    /// Group alpha for the current draw; records `LayerAlpha` when it folds a fraction.
    pub(super) fn alpha(&mut self) -> f32 {
        let a = self.layers.group_alpha();
        if a < 1.0 {
            self.stats.record(Unsupported::LayerAlpha);
        }
        a
    }

    /// Whether the frame's vertex budget is spent (records `VertexLimit` once per frame).
    pub(super) fn vertex_budget_spent(&mut self) -> bool {
        if !self.out_of_vertices && self.plan.solid_vertices.len() as u64 >= u64::from(self.vertex_limit) {
            self.out_of_vertices = true;
            self.stats.record(Unsupported::VertexLimit);
        }
        self.out_of_vertices
    }
}
