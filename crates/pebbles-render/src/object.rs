//! The [`RenderObject`] trait — the unit of layout and painting.
//!
//! A render object knows how to (a) size itself within [`BoxConstraints`] while
//! laying out and positioning its children, and (b) paint itself and its children
//! into a [`vello::Scene`]. Render objects live in the [`RenderTree`](crate::RenderTree)
//! arena and refer to their children by [`RenderId`](crate::RenderId); all tree
//! traversal goes through the [`LayoutCx`] / [`PaintCx`] passed to these methods.

use std::any::Any;

use pebbles_foundation::{Axis, Offset, Rect, Size};
use vello::kurbo::Affine;

use crate::constraints::BoxConstraints;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// A node in the render tree: it computes its own [`Size`] under a set of
/// [`BoxConstraints`] and paints itself into a scene.
///
/// Implementors are stored type-erased as `Box<dyn RenderObject>`. The `Any`
/// supertrait enables safe downcasting (e.g. to read a pointer-listener's
/// callbacks, or to mutate a property in place without rebuilding the node).
pub trait RenderObject: Any {
    /// Compute this object's size for the given constraints, laying out and
    /// positioning children via `cx`. Must return a [`Size`] that satisfies
    /// `constraints`. This is the "sizes come up" half of the protocol.
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size;

    /// Paint this object into the scene. `offset` is this object's absolute
    /// top-left position in the window's coordinate space. Children are painted
    /// with [`PaintCx::paint_child`].
    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset);

    /// The rect this object (and its own drawing — not its children) may paint
    /// into, in its **local** space. The paint-time viewport culling judges a
    /// subtree by this rect at the subtree root, and each child again by its own
    /// rect — so children may overflow their parent freely. The default is the
    /// layout rect; objects that draw OUTSIDE it (drop shadows, glows) must
    /// override, or their out-of-rect ink pops when the layout rect scrolls out.
    fn paint_bounds(&self, size: Size) -> Rect {
        Rect::from_origin_size((0.0, 0.0), size)
    }

    /// Whether this object CLIPS its children's painting to its own bounds (scroll
    /// viewports, clip-rrects, layer effects). Clipping objects cap their subtree
    /// paint rect at their own bounds, so a scrolled-out card containing a huge
    /// inner scroll view still culls as one small rect.
    fn clips_children(&self) -> bool {
        false
    }

    /// An optional paint/hit-test transform applied to this object's whole subtree,
    /// expressed in the object's **local** space (already resolved around its
    /// transform origin). `None` (the default) means an ordinary translated box.
    /// The tree applies it during painting (as a transformed sub-scene) and inverts
    /// it during hit-testing, so pointer events still land on transformed widgets.
    fn transform(&self, _size: Size) -> Option<Affine> {
        None
    }

    /// This node's **intrinsic extent** on `axis` — the size it would choose with
    /// no external constraints on that axis, given that the perpendicular axis is
    /// fixed at `cross_extent` (infinite when unconstrained). `None` means the
    /// object has no intrinsic notion (plain boxes, flex containers of children
    /// that don't report one) and must be sized by the box protocol alone.
    /// [`RenderIntrinsicWidth`](crate::RenderIntrinsicWidth) / `RenderIntrinsicHeight`
    /// drive layout from this instead of the ordinary constraint pass.
    fn intrinsic(&self, _cx: &mut IntrinsicCx<'_>, _axis: Axis, _cross_extent: f64) -> Option<f64> {
        None
    }

    /// The distance from this object's top edge to its first text baseline, in its
    /// own coordinate space — the input to `CrossAxisAlignment::Baseline`. Text
    /// reports its first line's baseline; single-child wrappers (padding, boxes,
    /// decoration) pass it through with their child's top inset added. `None` for
    /// objects with no baseline notion.
    fn baseline(&self, _cx: &mut LayoutCx<'_>) -> Option<f64> {
        None
    }

    /// A human-readable name for diagnostics and tree dumps.
    fn debug_name(&self) -> &'static str {
        "RenderObject"
    }
}

/// Blanket helpers for downcasting a `dyn RenderObject`. Uses trait upcasting to
/// `dyn Any` (stable since Rust 1.86), so implementors need no boilerplate.
impl dyn RenderObject {
    pub fn downcast_ref<T: RenderObject>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref::<T>()
    }

    pub fn downcast_mut<T: RenderObject>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut::<T>()
    }

    pub fn is<T: RenderObject>(&self) -> bool {
        (self as &dyn Any).is::<T>()
    }
}
