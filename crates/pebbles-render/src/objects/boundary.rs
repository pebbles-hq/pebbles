//! [`RenderBoundary`] — an explicit repaint boundary (Flutter `RepaintBoundary`): a
//! single-child pass-through whose node is a repaint boundary
//! ([`RenderObject::is_repaint_boundary`]), so its subtree is recorded into a retained
//! layer that the tree re-APPENDS (translated) each frame and re-RECORDS only when
//! something inside it was invalidated (see `PaintCx::paint_child`). A clean list item
//! costs one `Scene::append` per frame — no glyph or path re-encoding — and reaches the
//! renderer as the SAME fragment, so damage tracking skips it.

use pebbles_foundation::{Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// A single-child repaint boundary.
#[derive(Default)]
pub struct RenderBoundary;

impl RenderBoundary {
    pub fn new() -> Self {
        RenderBoundary
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
        if let Some(child) = cx.children().first().copied() {
            let at = offset + cx.child_offset(child);
            cx.paint_child(child, at);
        }
    }

    fn is_repaint_boundary(&self) -> bool {
        true
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        cx.children().first().copied().and_then(|c| cx.child_baseline(c))
    }

    fn debug_name(&self) -> &'static str {
        "RenderBoundary"
    }
}
