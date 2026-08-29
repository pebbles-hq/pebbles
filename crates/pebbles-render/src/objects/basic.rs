//! Basic single-child render objects: colored box, padding, align, constrained box.
//! Each takes at most one child (the first entry in its child list).

use pebbles_foundation::{Alignment, Color, EdgeInsets, Offset, Rect, Size};
use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Convenience: the single (first) child of the object being laid out/painted.
fn only_child_layout(cx: &LayoutCx) -> Option<crate::RenderId> {
    cx.children().first().copied()
}
fn only_child_paint(cx: &PaintCx) -> Option<crate::RenderId> {
    cx.children().first().copied()
}

// ---------------------------------------------------------------------------
// RenderColoredBox
// ---------------------------------------------------------------------------

/// Paints a solid color behind its child, sizing itself to the child (or filling
/// the available space when childless).
pub struct RenderColoredBox {
    pub color: Color,
}

impl RenderColoredBox {
    pub fn new(color: Color) -> Self {
        RenderColoredBox { color }
    }
}

impl RenderObject for RenderColoredBox {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        match only_child_layout(cx) {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(constraints.biggest()),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let rect = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(self.color), None, &rect);
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderColoredBox"
    }
}

// ---------------------------------------------------------------------------
// RenderPadding
// ---------------------------------------------------------------------------

/// Insets its child by [`EdgeInsets`] and grows by the same amount.
pub struct RenderPadding {
    pub insets: EdgeInsets,
}

impl RenderPadding {
    pub fn new(insets: EdgeInsets) -> Self {
        RenderPadding { insets }
    }
}

impl RenderObject for RenderPadding {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let insets = self.insets;
        match only_child_layout(cx) {
            Some(child) => {
                let inner = constraints.deflate(insets);
                let child_size = cx.layout_child(child, inner);
                cx.set_child_offset(child, insets.top_left());
                constraints.constrain(Size::new(
                    child_size.width + insets.horizontal(),
                    child_size.height + insets.vertical(),
                ))
            }
            None => constraints.constrain(insets.collapsed_size()),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderPadding"
    }
}

// ---------------------------------------------------------------------------
// RenderAlign
// ---------------------------------------------------------------------------

/// Positions its child within itself per an [`Alignment`]. Expands to the biggest
/// allowed size on each bounded axis and shrink-wraps the child on unbounded axes.
pub struct RenderAlign {
    pub alignment: Alignment,
}

impl RenderAlign {
    pub fn new(alignment: Alignment) -> Self {
        RenderAlign { alignment }
    }
}

impl RenderObject for RenderAlign {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let alignment = self.alignment;
        match only_child_layout(cx) {
            Some(child) => {
                let child_size = cx.layout_child(child, constraints.loosen());
                let width =
                    if constraints.has_bounded_width() { constraints.max_width } else { child_size.width };
                let height = if constraints.has_bounded_height() {
                    constraints.max_height
                } else {
                    child_size.height
                };
                let size = constraints.constrain(Size::new(width, height));
                cx.set_child_offset(child, alignment.inscribe(child_size, size));
                size
            }
            None => constraints.constrain(constraints.biggest()),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderAlign"
    }
}

// ---------------------------------------------------------------------------
// RenderConstrainedBox
// ---------------------------------------------------------------------------

/// Imposes additional [`BoxConstraints`] on its child. Backs both `SizedBox`
/// (tight additional constraints) and `ConstrainedBox`.
pub struct RenderConstrainedBox {
    pub additional: BoxConstraints,
}

impl RenderConstrainedBox {
    pub fn new(additional: BoxConstraints) -> Self {
        RenderConstrainedBox { additional }
    }
}

impl RenderObject for RenderConstrainedBox {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let effective = self.additional.enforce(constraints);
        match only_child_layout(cx) {
            Some(child) => {
                let size = cx.layout_child(child, effective);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => effective.constrain(Size::ZERO),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderConstrainedBox"
    }
}
