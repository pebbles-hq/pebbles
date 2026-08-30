//! The [`RenderObject`] trait — the unit of layout and painting.
//!
//! A render object knows how to (a) size itself within [`BoxConstraints`] while
//! laying out and positioning its children, and (b) paint itself and its children
//! into a [`vello::Scene`]. Render objects live in the [`RenderTree`](crate::RenderTree)
//! arena and refer to their children by [`RenderId`](crate::RenderId); all tree
//! traversal goes through the [`LayoutCx`] / [`PaintCx`] passed to these methods.

use std::any::Any;

use pebbles_foundation::{Offset, Size};
use vello::kurbo::Affine;

use crate::constraints::BoxConstraints;
use crate::tree::{LayoutCx, PaintCx};

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
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size;

    /// Paint this object into the scene. `offset` is this object's absolute
    /// top-left position in the window's coordinate space. Children are painted
    /// with [`PaintCx::paint_child`].
    fn paint(&self, cx: &mut PaintCx, offset: Offset);

    /// An optional paint/hit-test transform applied to this object's whole subtree,
    /// expressed in the object's **local** space (already resolved around its
    /// transform origin). `None` (the default) means an ordinary translated box.
    /// The tree applies it during painting (as a transformed sub-scene) and inverts
    /// it during hit-testing, so pointer events still land on transformed widgets.
    fn transform(&self, _size: Size) -> Option<Affine> {
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
