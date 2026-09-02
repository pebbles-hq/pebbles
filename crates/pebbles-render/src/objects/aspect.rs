//! [`RenderAspectRatio`] — forces its child to a fixed width:height ratio, as large
//! as the constraints allow.

use pebbles_foundation::{Axis, Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// Sizes its child to `ratio` = width / height.
pub struct RenderAspectRatio {
    pub ratio: f64,
}

impl RenderAspectRatio {
    pub fn new(ratio: f64) -> Self {
        RenderAspectRatio { ratio: ratio.max(0.0001) }
    }

    fn target(&self, constraints: BoxConstraints) -> Size {
        // Prefer filling the width, then clamp by height.
        let mut w = if constraints.has_bounded_width() {
            constraints.max_width
        } else if constraints.has_bounded_height() {
            constraints.max_height * self.ratio
        } else {
            0.0
        };
        let mut h = w / self.ratio;
        if constraints.has_bounded_height() && h > constraints.max_height {
            h = constraints.max_height;
            w = h * self.ratio;
        }
        constraints.constrain(Size::new(w, h))
    }
}

impl RenderObject for RenderAspectRatio {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let size = self.target(constraints);
        if let Some(child) = cx.children().first().copied() {
            cx.layout_child(child, BoxConstraints::tight(size));
            cx.set_child_offset(child, Offset::ZERO);
        }
        size
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx, axis: Axis, cross_extent: f64) -> Option<f64> {
        // From the child if it reports one, else from the aspect ratio itself.
        let from_child = cx
            .children()
            .first()
            .copied()
            .and_then(|child| cx.child_intrinsic(child, axis, cross_extent));
        match from_child {
            Some(v) => Some(v),
            None => match axis {
                Axis::Horizontal => {
                    if cross_extent.is_finite() {
                        Some(cross_extent * self.ratio)
                    } else {
                        None
                    }
                }
                Axis::Vertical => {
                    if cross_extent.is_finite() {
                        Some(cross_extent / self.ratio)
                    } else {
                        None
                    }
                }
            },
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderAspectRatio"
    }
}
