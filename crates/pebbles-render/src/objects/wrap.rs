//! [`RenderWrap`] — a horizontal flow layout that wraps children onto new runs
//! (lines) when they exceed the available width. Backs `Wrap` (tag lists, chips,
//! responsive toolbars).

use pebbles_foundation::{Offset, Size};
use smallvec::SmallVec;

use crate::RenderId;
use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// A flow container: children fill a run left-to-right, then wrap down.
pub struct RenderWrap {
    /// Gap between children within a run.
    pub spacing: f64,
    /// Gap between runs.
    pub run_spacing: f64,
}

impl RenderWrap {
    pub fn new(spacing: f64, run_spacing: f64) -> Self {
        RenderWrap { spacing, run_spacing }
    }
}

impl RenderObject for RenderWrap {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let max_w = constraints.max_width;
        let child_constraints =
            BoxConstraints { min_width: 0.0, max_width: max_w, min_height: 0.0, max_height: f64::INFINITY };

        let children: SmallVec<[RenderId; 8]> = cx.children().into_iter().collect();

        // (items, run_width, run_height)
        let mut runs: Vec<(SmallVec<[RenderId; 8]>, f64, f64)> = Vec::new();
        let mut cur: SmallVec<[RenderId; 8]> = SmallVec::new();
        let mut cur_w = 0.0_f64;
        let mut cur_h = 0.0_f64;

        for child in children {
            let sz = cx.layout_child(child, child_constraints);
            let prospective = if cur.is_empty() { sz.width } else { cur_w + self.spacing + sz.width };
            if !cur.is_empty() && prospective > max_w {
                runs.push((std::mem::take(&mut cur), cur_w, cur_h));
                cur_w = 0.0;
                cur_h = 0.0;
            }
            cur_w = if cur.is_empty() { sz.width } else { cur_w + self.spacing + sz.width };
            cur_h = cur_h.max(sz.height);
            cur.push(child);
        }
        if !cur.is_empty() {
            runs.push((cur, cur_w, cur_h));
        }

        // Position runs top-to-bottom, children left-to-right.
        let mut total_w = 0.0_f64;
        let mut y = 0.0_f64;
        for (i, (items, run_w, run_h)) in runs.iter().enumerate() {
            let mut x = 0.0_f64;
            for &child in items {
                cx.set_child_offset(child, Offset::new(x, y));
                x += cx.child_size(child).width + self.spacing;
            }
            total_w = total_w.max(*run_w);
            y += run_h;
            if i + 1 < runs.len() {
                y += self.run_spacing;
            }
        }

        constraints.constrain(Size::new(total_w, y))
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderWrap"
    }
}
