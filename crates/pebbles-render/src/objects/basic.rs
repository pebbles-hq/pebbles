//! Basic single-child render objects: colored box, padding, align, constrained box.
//! Each takes at most one child (the first entry in its child list).

use pebbles_foundation::{Alignment, Axis, Color, EdgeInsets, Offset, Rect, Size};
use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::{HitBehavior, RenderObject};
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// Convenience: the single (first) child of the object being laid out/painted.
fn only_child_layout(cx: &LayoutCx<'_>) -> Option<crate::RenderId> {
    cx.children().first().copied()
}
fn only_child_paint(cx: &PaintCx<'_>) -> Option<crate::RenderId> {
    cx.children().first().copied()
}
fn only_child_intrinsic(cx: &IntrinsicCx<'_>) -> Option<crate::RenderId> {
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
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        match only_child_layout(cx) {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(constraints.biggest()),
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let rect = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(self.color), None, &rect);
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        only_child_layout(cx).and_then(|child| cx.child_baseline(child))
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
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
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

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        // The child's intrinsic extent plus the insets; the cross extent the child
        // is asked about shrinks by the insets on that axis.
        let insets = self.insets;
        let (add, deflate_cross) = match axis {
            Axis::Horizontal => (insets.horizontal(), insets.horizontal()),
            Axis::Vertical => (insets.vertical(), insets.vertical()),
        };
        let cross =
            if cross_extent.is_finite() { (cross_extent - deflate_cross).max(0.0) } else { cross_extent };
        only_child_intrinsic(cx).and_then(|child| cx.child_intrinsic(child, axis, cross)).map(|v| v + add)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        only_child_layout(cx).and_then(|child| cx.child_baseline(child))
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
    /// When set, size to `factor × child` on that axis instead of filling the
    /// constraints (Flutter's `Align.widthFactor`/`heightFactor`). A `height_factor`
    /// tween + a clip is exactly how `SizeTransition` reveals its child.
    pub width_factor: Option<f64>,
    pub height_factor: Option<f64>,
}

impl RenderAlign {
    pub fn new(alignment: Alignment) -> Self {
        RenderAlign { alignment, width_factor: None, height_factor: None }
    }
}

impl RenderObject for RenderAlign {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let alignment = self.alignment;
        match only_child_layout(cx) {
            Some(child) => {
                let child_size = cx.layout_child(child, constraints.loosen());
                let width = if let Some(f) = self.width_factor {
                    child_size.width * f.max(0.0)
                } else if constraints.has_bounded_width() {
                    constraints.max_width
                } else {
                    child_size.width
                };
                let height = if let Some(f) = self.height_factor {
                    child_size.height * f.max(0.0)
                } else if constraints.has_bounded_height() {
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

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        // Alignment is placement, not size — pass the child's intrinsic through.
        only_child_intrinsic(cx).and_then(|child| cx.child_intrinsic(child, axis, cross_extent))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        only_child_layout(cx).and_then(|child| cx.child_baseline(child))
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

/// A transparent single-child box that only changes how its subtree is hit-tested
/// — the render backing for `IgnorePointer` / `AbsorbPointer`. Layout and paint
/// pass straight through to the child.
pub struct RenderPointerBarrier {
    pub behavior: HitBehavior,
}

impl RenderPointerBarrier {
    pub fn new(behavior: HitBehavior) -> Self {
        RenderPointerBarrier { behavior }
    }
}

impl RenderObject for RenderPointerBarrier {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        match only_child_layout(cx) {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(Size::ZERO),
        }
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        only_child_intrinsic(cx).and_then(|child| cx.child_intrinsic(child, axis, cross_extent))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        only_child_layout(cx).and_then(|child| cx.child_baseline(child))
    }

    fn hit_behavior(&self) -> HitBehavior {
        self.behavior
    }

    fn debug_name(&self) -> &'static str {
        "RenderPointerBarrier"
    }
}

impl RenderObject for RenderConstrainedBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
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

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        // A constrained box clamps the child's intrinsic extent to the additional
        // constraints (a tight SizedBox's intrinsic is exactly its size).
        let from_child =
            only_child_intrinsic(cx).and_then(|child| cx.child_intrinsic(child, axis, cross_extent));
        match from_child {
            Some(v) => Some(match axis {
                Axis::Horizontal => self.additional.constrain_width(v),
                Axis::Vertical => self.additional.constrain_height(v),
            }),
            // Tight additional constraints on this axis are an intrinsic even when
            // the child has none of its own.
            None => match axis {
                Axis::Horizontal if self.additional.has_tight_width() => Some(self.additional.min_width),
                Axis::Vertical if self.additional.has_tight_height() => Some(self.additional.min_height),
                _ => None,
            },
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = only_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        only_child_layout(cx).and_then(|child| cx.child_baseline(child))
    }

    fn debug_name(&self) -> &'static str {
        "RenderConstrainedBox"
    }
}
