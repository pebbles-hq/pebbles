//! [`RenderView`] — the root render object. It fills the window with an optional
//! background color and forces its single child to the window size.

use pebbles_foundation::{Color, Offset, Rect, Size};
use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// The root of the render tree. Always sized to the window; paints a background
/// then its (single, optional) child laid out under tight window constraints.
pub struct RenderView {
    pub background: Color,
}

impl RenderView {
    pub fn new(background: Color) -> Self {
        RenderView { background }
    }
}

impl RenderObject for RenderView {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        // The window hands us tight constraints; adopt that size and pass it down.
        let size = constraints.biggest();
        let child_constraints = BoxConstraints::tight(size);
        for child in cx.children() {
            cx.layout_child(child, child_constraints);
            cx.set_child_offset(child, Offset::ZERO);
        }
        size
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let rect = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(self.background), None, &rect);
        for child in cx.children() {
            let child_offset = offset + cx.child_offset(child);
            cx.paint_child(child, child_offset);
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderView"
    }
}
