//! [`RenderWrap`] — a horizontal flow layout that wraps children onto new runs
//! (lines) when they exceed the available width. Backs `Wrap` (tag lists, chips,
//! responsive toolbars).

use pebbles_foundation::{Offset, Size, WrapAlignment};
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
    /// How children are distributed within a run (the main axis).
    pub alignment: WrapAlignment,
    /// How runs are distributed along the cross axis.
    pub run_alignment: WrapAlignment,
}

impl RenderWrap {
    pub fn new(
        spacing: f64,
        run_spacing: f64,
        alignment: WrapAlignment,
        run_alignment: WrapAlignment,
    ) -> Self {
        RenderWrap { spacing, run_spacing, alignment, run_alignment }
    }
}

/// Distribute `free` leftover space across `n` items per an alignment: returns
/// `(leading, between)` — the offset of the first item and the extra gap between
/// adjacent items (added on top of the fixed `spacing`).
fn distribute(free: f64, n: usize, alignment: WrapAlignment) -> (f64, f64) {
    match alignment {
        WrapAlignment::Start => (0.0, 0.0),
        WrapAlignment::End => (free, 0.0),
        WrapAlignment::Center => (free / 2.0, 0.0),
        WrapAlignment::SpaceBetween => {
            if n > 1 {
                (0.0, free / (n - 1) as f64)
            } else {
                (0.0, 0.0)
            }
        }
        WrapAlignment::SpaceAround => {
            let b = if n > 0 { free / n as f64 } else { 0.0 };
            (b / 2.0, b)
        }
        WrapAlignment::SpaceEvenly => {
            let b = free / (n as f64 + 1.0);
            (b, b)
        }
    }
}

impl RenderObject for RenderWrap {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
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

        let mut total_w = 0.0_f64;
        let mut content_h = 0.0_f64;
        for (i, (_, run_w, run_h)) in runs.iter().enumerate() {
            total_w = total_w.max(*run_w);
            content_h += run_h;
            if i + 1 < runs.len() {
                content_h += self.run_spacing;
            }
        }

        let size = constraints.constrain(Size::new(total_w, content_h));
        let final_w = size.width;
        let final_h = size.height;

        // Distribute runs along the cross axis (extra space only when the wrap is
        // given more cross extent than its content).
        let cross_extra = (final_h - content_h).max(0.0);
        let (run_lead, run_between) = distribute(cross_extra, runs.len(), self.run_alignment);

        let mut y = run_lead;
        for (i, (items, run_w, run_h)) in runs.iter().enumerate() {
            let leftover = (final_w - run_w).max(0.0);
            let (lead, between) = distribute(leftover, items.len(), self.alignment);
            let mut x = lead;
            for &child in items {
                cx.set_child_offset(child, Offset::new(x, y));
                x += cx.child_size(child).width + self.spacing + between;
            }
            y += run_h;
            if i + 1 < runs.len() {
                y += self.run_spacing + run_between;
            }
        }

        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderWrap"
    }
}
