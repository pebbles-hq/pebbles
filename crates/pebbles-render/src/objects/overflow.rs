//! [`RenderLimitedBox`] / [`RenderOverflowBox`] — the two constraint-tweak boxes.
//! `LimitedBox` caps its child only when the incoming constraint is unbounded
//! (Flutter: lets an unconstrained parent give a child a sane maximum).
//! `OverflowBox` lets its child size itself naturally and overflows the box
//! (paint is NOT clipped), positioned by an alignment.

use pebbles_foundation::{Alignment, Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Limits its child to `max_width`/`max_height`, but only on axes where the
/// incoming constraint is unbounded (a bounded axis passes through untouched).
pub struct RenderLimitedBox {
    pub max_width: f64,
    pub max_height: f64,
}

impl RenderLimitedBox {
    pub fn new(max_width: f64, max_height: f64) -> Self {
        RenderLimitedBox { max_width, max_height }
    }
}

impl RenderObject for RenderLimitedBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let child_constraints = BoxConstraints {
            max_width: if constraints.max_width.is_infinite() {
                constraints.max_width.min(self.max_width)
            } else {
                constraints.max_width
            },
            max_height: if constraints.max_height.is_infinite() {
                constraints.max_height.min(self.max_height)
            } else {
                constraints.max_height
            },
            ..constraints
        };
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, child_constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(constraints.biggest()),
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderLimitedBox"
    }
}

/// Lays its child out with no constraints (its natural size), positions it inside
/// itself per an [`Alignment`], and does not clip — so the child may paint (but
/// not be hit-tested) outside the box's own bounds.
pub struct RenderOverflowBox {
    pub alignment: Alignment,
    /// Child position computed by the last layout pass (consumed by the paint
    /// transform, which only sees this box's own size).
    position: Offset,
}

impl RenderOverflowBox {
    pub fn new(alignment: Alignment) -> Self {
        RenderOverflowBox { alignment, position: Offset::ZERO }
    }
}

impl RenderObject for RenderOverflowBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(constraints.biggest());
        };
        // The child sizes itself naturally (Flutter hands it unbounded
        // constraints); the box shrinks to the child clamped into the incoming
        // constraints.
        let child_size = cx.layout_child(child, BoxConstraints::UNBOUNDED);
        let size = constraints.constrain(child_size);
        let dw = size.width - child_size.width;
        let dh = size.height - child_size.height;
        self.position = Offset::new(dw * (self.alignment.x + 1.0) / 2.0, dh * (self.alignment.y + 1.0) / 2.0);
        cx.set_child_offset(child, Offset::ZERO);
        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset);
        }
    }

    fn transform(&self, _size: Size) -> Option<kurbo::Affine> {
        let pos = self.position;
        if pos == Offset::ZERO { None } else { Some(kurbo::Affine::translate((pos.x, pos.y))) }
    }

    fn debug_name(&self) -> &'static str {
        "RenderOverflowBox"
    }
}
