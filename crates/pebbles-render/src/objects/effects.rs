//! Compositing effects: [`RenderOpacity`] and [`RenderClipRRect`]. Both wrap a
//! single child and push a vello layer around it.

use pebbles_foundation::{Offset, Rect, Size};
use vello::kurbo::Affine;
use vello::peniko::{Fill, Mix};

use crate::constraints::BoxConstraints;
use crate::decoration::BorderRadius;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

fn layout_single_child(cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
    match cx.children().first().copied() {
        Some(child) => {
            let size = cx.layout_child(child, constraints);
            cx.set_child_offset(child, Offset::ZERO);
            size
        }
        None => constraints.constrain(Size::ZERO),
    }
}

// ---------------------------------------------------------------------------
// RenderOpacity
// ---------------------------------------------------------------------------

/// Applies a uniform alpha to its child subtree via a compositing layer.
pub struct RenderOpacity {
    pub opacity: f32,
}

impl RenderOpacity {
    pub fn new(opacity: f32) -> Self {
        RenderOpacity { opacity }
    }
}

impl RenderObject for RenderOpacity {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, self.opacity, Affine::IDENTITY, &bounds);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        cx.scene.pop_layer();
    }

    fn clips_children(&self) -> bool {
        true // the opacity layer's clip shape bounds the subtree's ink
    }

    fn debug_name(&self) -> &'static str {
        "RenderOpacity"
    }
}

// ---------------------------------------------------------------------------
// RenderClipRRect
// ---------------------------------------------------------------------------

/// Clips its child to a rounded rectangle.
pub struct RenderClipRRect {
    pub radius: BorderRadius,
}

impl RenderClipRRect {
    pub fn new(radius: BorderRadius) -> Self {
        RenderClipRRect { radius }
    }
}

impl RenderObject for RenderClipRRect {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        let rounded = bounds.to_rounded_rect(self.radius.to_radii());
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &rounded);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        cx.scene.pop_layer();
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "RenderClipRRect"
    }
}
