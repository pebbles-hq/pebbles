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
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
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

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderMeasureProbe"
    }
}

/// Lays its child out unchanged and reports its own **window-space rect** (paint
/// offset + laid-out size) after each paint. A node's global offset is only known
/// at paint time (the parent positions it after its own layout), so this is where
/// the rect becomes available. The basis for opening a menu/popover anchored to a
/// trigger via the **keyboard**, where there is no pointer event to derive the
/// trigger rect from.
pub struct RenderPositionProbe {
    pub report: Option<Rc<dyn Fn(Offset, Size)>>,
    size: Size,
}

impl RenderPositionProbe {
    pub fn new(report: Option<Rc<dyn Fn(Offset, Size)>>) -> Self {
        RenderPositionProbe { report, size: Size::new(0.0, 0.0) }
    }
}

impl RenderObject for RenderPositionProbe {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(constraints.biggest());
        };
        let size = cx.layout_child(child, constraints);
        cx.set_child_offset(child, Offset::ZERO);
        self.size = size;
        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        // `offset` plus the recording origin is this node's window-space top-left (inside
        // a retained layer, paint offsets are layer-local), so with the laid-out size it
        // is the trigger's window rect.
        if let Some(report) = &self.report {
            report(cx.window_offset(offset), self.size);
        }
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    /// The report must run every frame (an ancestor may have moved), so no enclosing
    /// layer may be replayed while one is attached.
    fn paint_is_volatile(&self) -> bool {
        self.report.is_some()
    }

    fn debug_name(&self) -> &'static str {
        "RenderPositionProbe"
    }
}
