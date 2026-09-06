//! [`RenderBoundary`] — a repaint boundary: the subtree encodes into a retained
//! scene fragment that is re-APPENDED (translated) each frame and re-ENCODED only
//! when something inside actually changed. Dirty marks travel up the tree and
//! stop at the nearest boundary (`RenderTree::mark_needs_*`), so a clean list
//! item costs one `Scene::append` per frame — no glyph or path re-encoding.
//!
//! Fragments are encoded at the LOCAL origin with an unbounded visible window
//! (viewport-INDEPENDENT by contract — that's what makes re-appending at any
//! scroll offset sound). Content much taller than the window bypasses the cache
//! and paints directly, keeping line-level culling effective for pathological
//! single blocks.

use std::cell::{Cell, RefCell};

use pebbles_foundation::{Offset, Size};
use vello::kurbo::Affine;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// A single-child repaint boundary with a retained scene fragment.
pub struct RenderBoundary {
    fragment: RefCell<crate::paint::Scene>,
    dirty: Cell<bool>,
}

impl Default for RenderBoundary {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBoundary {
    pub fn new() -> Self {
        RenderBoundary { fragment: RefCell::new(crate::paint::scene()), dirty: Cell::new(true) }
    }

    /// Invalidate the retained fragment (a dirty mark reached this boundary).
    pub(crate) fn mark_dirty(&self) {
        self.dirty.set(true);
    }
}

impl RenderObject for RenderBoundary {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(constraints.smallest()),
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let at = offset + cx.child_offset(child);
        // Bypass: content much taller than the window would defeat line culling
        // if frozen into a viewport-independent fragment — paint it directly.
        let vis = cx.visible();
        if cx.child_size(child).height > 3.0 * (vis.y1 - vis.y0).max(1.0) {
            cx.paint_child(child, at);
            return;
        }
        if self.dirty.get() {
            cx.encode_fragment(child, &mut self.fragment.borrow_mut());
            self.dirty.set(false);
            crate::stats::bump_fragment_encode();
        } else {
            crate::stats::bump_fragment_reuse();
        }
        cx.scene.append(&self.fragment.borrow(), Some(Affine::translate((at.x, at.y))));
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        cx.children().first().copied().and_then(|c| cx.child_baseline(c))
    }

    fn debug_name(&self) -> &'static str {
        "RenderBoundary"
    }
}
