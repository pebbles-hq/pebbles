//! [`RenderMeasureProbe`] — a layout pass-through that reports its child's
//! main-axis extent after laying it out. The basis for auto-measured virtualized
//! lists (A1): each visible item is probed; the extent cache updates as real
//! measurements replace estimates.

use pebbles_foundation::{Axis, Offset, Size};
use std::rc::Rc;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Lays out its child with the incoming constraints and reports the resulting
/// extent along `axis` to `report` (called after every layout — the receiver
/// decides whether anything changed).
pub struct RenderMeasureProbe {
    pub axis: Axis,
    pub report: Option<Rc<dyn Fn(f64)>>,
    /// When true, the child is laid out with the measured axis UNBOUNDED (its
    /// true natural extent) — the auto-measured list mode. When false, the child
    /// gets the incoming constraints as-is and the probe reports the extent it
    /// actually took (the carousel page-width mode).
    pub unbound: bool,
}

impl RenderMeasureProbe {
    pub fn new(axis: Axis, report: Option<Rc<dyn Fn(f64)>>) -> Self {
        RenderMeasureProbe { axis, report, unbound: false }
    }
}

impl RenderObject for RenderMeasureProbe {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(constraints.biggest());
        };
        // With `unbound`, the child sizes itself NATURALLY along the measured
        // axis (the position wrapper may hand it a capped estimate — measuring
        // the true extent is the whole point). The probe returns that true size
        // even if it exceeds the incoming constraints; the scroll clip layer
        // handles the overpaint.
        let child_constraints = if self.unbound {
            match self.axis {
                Axis::Vertical => BoxConstraints {
                    min_width: constraints.min_width,
                    max_width: constraints.max_width,
                    min_height: 0.0,
                    max_height: f64::INFINITY,
                },
                Axis::Horizontal => BoxConstraints {
                    min_width: 0.0,
                    max_width: f64::INFINITY,
                    min_height: constraints.min_height,
                    max_height: constraints.max_height,
                },
            }
        } else {
            constraints
        };
        let size = cx.layout_child(child, child_constraints);
        cx.set_child_offset(child, Offset::ZERO);
        let extent = match self.axis {
            Axis::Vertical => size.height,
            Axis::Horizontal => size.width,
        };
        if let Some(report) = &self.report {
            report(extent);
        }
        size
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderMeasureProbe"
    }
}
