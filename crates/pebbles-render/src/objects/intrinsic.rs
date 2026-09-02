//! [`RenderIntrinsicWidth`] / [`RenderIntrinsicHeight`] — size a child to its
//! intrinsic extent on one axis instead of the box protocol. Flutter's
//! `IntrinsicWidth`/`IntrinsicHeight`: the parent asks "how big would you be if
//! nobody constrained you?" and tightens that axis to the answer. The basis for
//! shrink-wrap layouts (e.g. a column as wide as its widest child).

use pebbles_foundation::{Axis, Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Sizes its child's **width** to the child's intrinsic width (given the incoming
/// height), then lays it out with that width fixed.
pub struct RenderIntrinsicWidth;

impl RenderIntrinsicWidth {
    pub fn new() -> Self {
        RenderIntrinsicWidth
    }
}

/// Sizes its child's **height** to the child's intrinsic height (given the incoming
/// width), then lays it out with that height fixed.
pub struct RenderIntrinsicHeight;

impl RenderIntrinsicHeight {
    pub fn new() -> Self {
        RenderIntrinsicHeight
    }
}

/// The shared algorithm: query the child's intrinsic extent on `axis` (unless the
/// incoming constraints already fix it), tighten that axis, and lay the child out.
fn layout_intrinsic(cx: &mut LayoutCx, constraints: BoxConstraints, axis: Axis) -> Size {
    let Some(child) = cx.children().first().copied() else {
        return constraints.constrain(constraints.biggest());
    };
    let cross_extent = match axis {
        Axis::Horizontal => constraints.max_height,
        Axis::Vertical => constraints.max_width,
    };
    // Flutter: when the axis is already tight, the intrinsic query is skipped (the
    // child has no freedom there anyway).
    let tight = match axis {
        Axis::Horizontal => constraints.has_tight_width(),
        Axis::Vertical => constraints.has_tight_height(),
    };
    let extent = if tight {
        match axis {
            Axis::Horizontal => constraints.max_width,
            Axis::Vertical => constraints.max_height,
        }
    } else {
        cx.child_intrinsic(child, axis, cross_extent).unwrap_or(match axis {
            Axis::Horizontal => constraints.max_width,
            Axis::Vertical => constraints.max_height,
        })
    };
    let child_constraints = match axis {
        Axis::Horizontal => BoxConstraints {
            min_width: extent,
            max_width: extent,
            min_height: constraints.min_height,
            max_height: constraints.max_height,
        },
        Axis::Vertical => BoxConstraints {
            min_width: constraints.min_width,
            max_width: constraints.max_width,
            min_height: extent,
            max_height: extent,
        },
    };
    let size = cx.layout_child(child, child_constraints);
    cx.set_child_offset(child, Offset::ZERO);
    size
}

impl RenderObject for RenderIntrinsicWidth {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        layout_intrinsic(cx, constraints, Axis::Horizontal)
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderIntrinsicWidth"
    }
}

impl RenderObject for RenderIntrinsicHeight {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        layout_intrinsic(cx, constraints, Axis::Vertical)
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderIntrinsicHeight"
    }
}
