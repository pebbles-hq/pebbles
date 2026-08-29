//! [`RenderStack`] — overlays children on top of one another. Non-positioned
//! children are aligned within the stack; [`StackParentData`]-positioned children
//! are placed by their edge insets. Backs `Stack`/`Positioned`.

use pebbles_foundation::{Alignment, Offset, Size};

use crate::RenderId;
use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// How a stack sizes its non-positioned children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackFit {
    /// Children are loosely constrained (size to their content).
    Loose,
    /// Children are forced to the stack's size.
    Expand,
}

/// Placement data for a `Positioned` child. Any `Some` field makes the child
/// "positioned" (removed from the alignment flow).
#[derive(Clone, Copy, Debug, Default)]
pub struct StackParentData {
    pub left: Option<f64>,
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

impl StackParentData {
    fn is_positioned(&self) -> bool {
        self.left.is_some()
            || self.top.is_some()
            || self.right.is_some()
            || self.bottom.is_some()
            || self.width.is_some()
            || self.height.is_some()
    }
}

/// A box that overlays its children.
pub struct RenderStack {
    pub alignment: Alignment,
    pub fit: StackFit,
}

impl RenderStack {
    pub fn new(alignment: Alignment, fit: StackFit) -> Self {
        RenderStack { alignment, fit }
    }
}

impl RenderObject for RenderStack {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let children: Vec<(RenderId, StackParentData)> = cx
            .children()
            .into_iter()
            .map(|c| (c, cx.child_parent_data::<StackParentData>(c).copied().unwrap_or_default()))
            .collect();

        // Pass 1: lay out non-positioned children to discover the stack size.
        let non_positioned_constraints = match self.fit {
            StackFit::Loose => constraints.loosen(),
            StackFit::Expand => BoxConstraints::tight(constraints.biggest()),
        };
        let mut width = 0.0_f64;
        let mut height = 0.0_f64;
        let mut has_non_positioned = false;
        for &(child, data) in &children {
            if data.is_positioned() {
                continue;
            }
            has_non_positioned = true;
            let size = cx.layout_child(child, non_positioned_constraints);
            width = width.max(size.width);
            height = height.max(size.height);
        }

        let size = if has_non_positioned {
            constraints.constrain(Size::new(width, height))
        } else if constraints.has_bounded_width() && constraints.has_bounded_height() {
            constraints.biggest()
        } else {
            constraints.smallest()
        };

        // Position non-positioned children by alignment.
        for &(child, data) in &children {
            if data.is_positioned() {
                continue;
            }
            let child_size = cx.child_size(child);
            cx.set_child_offset(child, self.alignment.inscribe(child_size, size));
        }

        // Pass 2: lay out and place positioned children by their edges.
        for &(child, data) in &children {
            if !data.is_positioned() {
                continue;
            }
            let (min_w, max_w) = axis_constraint(data.width, data.left, data.right, size.width);
            let (min_h, max_h) = axis_constraint(data.height, data.top, data.bottom, size.height);
            let child_size = cx.layout_child(
                child,
                BoxConstraints { min_width: min_w, max_width: max_w, min_height: min_h, max_height: max_h },
            );

            let x = edge_position(data.left, data.right, child_size.width, size.width, self.alignment.x);
            let y = edge_position(data.top, data.bottom, child_size.height, size.height, self.alignment.y);
            cx.set_child_offset(child, Offset::new(x, y));
        }

        size
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderStack"
    }
}

/// Derive a child's min/max extent on one axis from an explicit size and/or a
/// pair of edge insets.
fn axis_constraint(size: Option<f64>, start: Option<f64>, end: Option<f64>, extent: f64) -> (f64, f64) {
    if let Some(s) = size {
        (s, s)
    } else if let (Some(a), Some(b)) = (start, end) {
        let s = (extent - a - b).max(0.0);
        (s, s)
    } else {
        (0.0, extent)
    }
}

/// Resolve a child's position on one axis from its edge insets, falling back to
/// alignment when unconstrained.
fn edge_position(start: Option<f64>, end: Option<f64>, child: f64, extent: f64, align: f64) -> f64 {
    match (start, end) {
        (Some(a), _) => a,
        (None, Some(b)) => extent - b - child,
        (None, None) => (extent - child).max(0.0) * (align + 1.0) / 2.0,
    }
}
