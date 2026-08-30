//! [`RenderTransform`] — applies an affine transform (rotate/scale/translate/skew)
//! to its child's painting and hit-testing, around a configurable origin. Layout is
//! a pass-through: the child keeps its size and the box occupies its untransformed
//! bounds for layout; only paint + hit-testing are transformed (Flutter's
//! `Transform` / `Container.transform`).

use pebbles_foundation::{Alignment, Offset, Size};
use vello::kurbo::Affine;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Transforms its single child.
pub struct RenderTransform {
    /// The raw transform (e.g. `Affine::rotate` / `Affine::scale`).
    pub matrix: Affine,
    /// Where the transform's origin sits within the box (`-1..1` per axis).
    pub alignment: Alignment,
}

impl RenderTransform {
    pub fn new(matrix: Affine, alignment: Alignment) -> Self {
        RenderTransform { matrix, alignment }
    }
}

impl RenderObject for RenderTransform {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(Size::ZERO),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        // The transform itself is applied by the parent's `paint_child` (which reads
        // `transform()`); here we just paint the child normally into that space.
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn transform(&self, size: Size) -> Option<Affine> {
        // Resolve the alignment to a pixel origin, then transform around it.
        let ox = (self.alignment.x + 1.0) / 2.0 * size.width;
        let oy = (self.alignment.y + 1.0) / 2.0 * size.height;
        Some(Affine::translate((ox, oy)) * self.matrix * Affine::translate((-ox, -oy)))
    }

    fn debug_name(&self) -> &'static str {
        "RenderTransform"
    }
}
