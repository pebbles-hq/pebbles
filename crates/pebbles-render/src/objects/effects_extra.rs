//! More compositing effects: [`RenderClipOval`], [`RenderClipPath`],
//! [`RenderColorFilter`], and [`RenderShaderMask`]. Each wraps a single child and
//! pushes a vello layer around (or over) it — the same shape as [`RenderClipRRect`],
//! with a different clip shape or blend.

use std::rc::Rc;

use pebbles_foundation::{Color, Offset, Rect, Size};
use vello::kurbo::{Affine, BezPath, Ellipse};
use vello::peniko::{Fill, Mix};

use super::decorated::gradient_brush;
use crate::constraints::BoxConstraints;
use crate::decoration::Gradient;
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
// RenderClipOval — clip the child to the ellipse inscribed in its bounds
// ---------------------------------------------------------------------------

/// Clips its child to the ellipse (a circle for a square box) inscribed in its bounds.
pub struct RenderClipOval;

impl RenderObject for RenderClipOval {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        let ellipse = Ellipse::new(bounds.center(), (bounds.width() / 2.0, bounds.height() / 2.0), 0.0);
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &ellipse);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        cx.scene.pop_layer();
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "RenderClipOval"
    }
}

// ---------------------------------------------------------------------------
// RenderClipPath — clip the child to a caller-supplied path
// ---------------------------------------------------------------------------

/// Clips its child to a path built by a delegate from the box size (Flutter's
/// `ClipPath` + `CustomClipper<Path>`).
pub struct RenderClipPath {
    pub path_fn: Rc<dyn Fn(Size) -> BezPath>,
}

impl RenderObject for RenderClipPath {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let size = cx.size();
        let bounds = Rect::from_origin_size(offset.to_point(), size);
        // The delegate builds the path in local space; offset it to the box origin.
        let path = (self.path_fn)(size);
        let placed = Affine::translate((offset.x, offset.y)) * &path;
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &placed);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        cx.scene.pop_layer();
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "RenderClipPath"
    }
}

// ---------------------------------------------------------------------------
// RenderColorFilter — blend a color over the child
// ---------------------------------------------------------------------------

/// Paints the child, then blends `color` over it with `blend` (Flutter's
/// `ColorFiltered` with a `ColorFilter.mode`).
pub struct RenderColorFilter {
    pub color: Color,
    pub blend: Mix,
}

impl RenderObject for RenderColorFilter {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.paint_child(child, offset + cx.child_offset(child));
        // A layer whose content (the color) blends with the child beneath it.
        cx.scene.push_layer(Fill::NonZero, self.blend, 1.0, Affine::IDENTITY, &bounds);
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.color, None, &bounds);
        cx.scene.pop_layer();
    }

    fn debug_name(&self) -> &'static str {
        "RenderColorFilter"
    }
}

// ---------------------------------------------------------------------------
// RenderShaderMask — mask the child by a gradient's luminance
// ---------------------------------------------------------------------------

/// Paints the child, then masks it by a gradient's luminance — bright gradient areas
/// keep the child, dark areas hide it (Flutter's `ShaderMask`, the common fade/vignette).
pub struct RenderShaderMask {
    pub gradient: Gradient,
}

impl RenderObject for RenderShaderMask {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.paint_child(child, offset + cx.child_offset(child));
        // Content drawn in this layer (the gradient) is a luminance mask for the child.
        cx.scene.push_luminance_mask_layer(Fill::NonZero, 1.0, Affine::IDENTITY, &bounds);
        let brush = gradient_brush(&self.gradient, bounds);
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &bounds);
        cx.scene.pop_layer();
    }

    fn debug_name(&self) -> &'static str {
        "RenderShaderMask"
    }
}
