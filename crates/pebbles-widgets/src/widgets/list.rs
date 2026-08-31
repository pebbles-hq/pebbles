//! [`ListView`] — a **virtualized**, build-on-demand list (Flutter's
//! `ListView.builder` with a fixed item extent). Only the items intersecting the
//! viewport (plus a small overscan) are built each frame, so a list of a million
//! rows costs the same as a screenful.
//!
//! It is a *controlled* scroll view: the component owns the offset as a signal, so
//! a wheel/scrollbar scroll re-renders it and the visible window rebuilds. Wheel +
//! scrollbar drag are routed to the signal via `pebbles_core::scroll`, and the
//! viewport extent is read back from `pebbles_render::scroll_metrics`.

use std::rc::Rc;

use pebbles_foundation::Axis;
use pebbles_render::{RenderList, RenderObject, ScrollbarStyle, scroll_metrics};

use crate::widgets::{Positioned, stack};
use pebbles_core::scroll::{self, ScrollTo};
use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};
use pebbles_core::{Signal, animate_to, component_props, create_cleanup, create_signal, owner_id};

/// How many extra items to build above/below the viewport (smooths fast flings).
const OVERSCAN: isize = 3;

// ---------------------------------------------------------------------------
// ScrollController — programmatic control of a list's offset.
// ---------------------------------------------------------------------------

/// A handle to a scrollable list's position. Create one with
/// [`use_scroll_controller`], pass it to [`ListView::controller`], and call
/// [`jump_to`](ScrollController::jump_to) / [`animate_to`](ScrollController::animate_to)
/// / [`scroll_to_index`](ScrollController::scroll_to_index) to drive it.
#[derive(Clone, Copy)]
pub struct ScrollController {
    id: u64,
    offset: Signal<f64>,
}

/// Create a [`ScrollController`] (call at the top level of a component, like a
/// signal — it persists across renders).
pub fn use_scroll_controller() -> ScrollController {
    let offset = create_signal(0.0_f64);
    ScrollController { id: offset.raw_id(), offset }
}

impl ScrollController {
    fn max(&self) -> f64 {
        scroll_metrics::get(self.id).map(|m| (m.content - m.viewport).max(0.0)).unwrap_or(0.0)
    }
    /// The current pixel offset.
    pub fn offset(&self) -> f64 {
        self.offset.peek()
    }
    /// Jump instantly to a pixel offset.
    pub fn jump_to(&self, px: f64) {
        self.offset.set(px.clamp(0.0, self.max()));
    }
    /// Smoothly animate to a pixel offset.
    pub fn animate_to(&self, px: f64) {
        animate_to(self.offset, px.clamp(0.0, self.max()), 0.35);
    }
    /// Animate so item `index` (of `item_extent` each) reaches the top.
    pub fn scroll_to_index(&self, index: usize, item_extent: f64) {
        self.animate_to(index as f64 * item_extent);
    }
}

// ---------------------------------------------------------------------------
// Low-level render widget backing RenderList.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ListViewport {
    axis: Axis,
    offset: f64,
    content_extent: f64,
    id: u64,
    scrollbar: ScrollbarStyle,
    child: Option<AnyWidget>,
}

impl ListViewport {
    fn make(&self) -> RenderList {
        let mut r = RenderList::new(self.axis);
        r.offset = self.offset;
        r.content_extent = self.content_extent;
        r.id = self.id;
        r.scrollbar = self.scrollbar;
        r
    }
}

pebbles_core::render_widget!(ListViewport);

impl RenderWidget for ListViewport {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(self.make())
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderList>() {
            *r = self.make();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// ListView — the virtualized public widget.
// ---------------------------------------------------------------------------

/// A virtualized, fixed-extent list.
pub struct ListView {
    count: usize,
    item_extent: f64,
    axis: Axis,
    scrollbar: ScrollbarStyle,
    controller: Option<ScrollController>,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    /// `(separator extent, builder)` when built with [`separated`](ListView::separated).
    separator: Option<(f64, Rc<dyn Fn(usize) -> AnyWidget>)>,
}

impl ListView {
    /// Build a vertical list of `count` rows, each `item_extent` tall; `builder`
    /// is called only for the visible rows.
    pub fn builder<W: IntoWidget>(
        count: usize,
        item_extent: f64,
        builder: impl Fn(usize) -> W + 'static,
    ) -> Self {
        ListView {
            count,
            item_extent,
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            controller: None,
            builder: Rc::new(move |i| builder(i).into_widget()),
            separator: None,
        }
    }

    /// A list with separators between the items (Flutter's `ListView.separated`):
    /// each item is `item_extent` tall, each separator `separator_extent` tall —
    /// both virtualized, no separator after the last item.
    pub fn separated<W: IntoWidget, S: IntoWidget>(
        count: usize,
        item_extent: f64,
        separator_extent: f64,
        item: impl Fn(usize) -> W + 'static,
        separator: impl Fn(usize) -> S + 'static,
    ) -> Self {
        ListView {
            count,
            item_extent,
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            controller: None,
            builder: Rc::new(move |i| item(i).into_widget()),
            separator: Some((separator_extent, Rc::new(move |i| separator(i).into_widget()))),
        }
    }

    /// Lay the list out horizontally instead (each item `item_extent` wide).
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
    /// Customize the scrollbar.
    pub fn scrollbar(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar = style;
        self
    }
    /// Drive this list programmatically with a [`ScrollController`].
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }
}

struct Props {
    count: usize,
    item_extent: f64,
    axis: Axis,
    scrollbar: ScrollbarStyle,
    controller: Option<ScrollController>,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    separator: Option<(f64, Rc<dyn Fn(usize) -> AnyWidget>)>,
}

impl IntoWidget for ListView {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_list,
            Props {
                count: self.count,
                item_extent: self.item_extent,
                axis: self.axis,
                scrollbar: self.scrollbar,
                controller: self.controller,
                builder: self.builder,
                separator: self.separator,
            },
        )
        .into_widget()
    }
}

fn render_list(p: &Props) -> ListViewport {
    // Use the controller's offset signal if one was supplied, else an internal one.
    let (id, offset) = match &p.controller {
        Some(c) => (c.id, c.offset),
        None => {
            let offset = create_signal(0.0_f64);
            (owner_id().unwrap_or(0), offset)
        }
    };
    let sep_ext = p.separator.as_ref().map(|(e, _)| *e).unwrap_or(0.0);
    let stride = p.item_extent + sep_ext;
    let content_extent = if sep_ext > 0.0 {
        p.count as f64 * p.item_extent + (p.count.saturating_sub(1)) as f64 * sep_ext
    } else {
        p.count as f64 * p.item_extent
    };

    // Drop this list's registry entries when it unmounts.
    create_cleanup(move || {
        scroll::clear(id);
        scroll_metrics::clear(id);
    });

    // Route wheel + scrollbar drag into the offset signal (clamped to the live
    // viewport). Re-installed each render (idempotent) so `content_extent` stays
    // current if `count` changes.
    {
        let ce = content_extent;
        scroll::install(
            id,
            Rc::new(move |to| {
                let vp = scroll_metrics::get(id).map(|m| m.viewport).unwrap_or(0.0);
                let max = (ce - vp).max(0.0);
                let next = match to {
                    ScrollTo::By(d) => offset.peek() + d,
                    ScrollTo::ToFraction(f) => f * max,
                };
                offset.set(next.clamp(0.0, max));
            }),
        );
    }

    // Visible window from the current offset + last-known viewport extent.
    let o = offset.get();
    let viewport = scroll_metrics::get(id).map(|m| m.viewport).unwrap_or(800.0);
    let ext = p.item_extent.max(1.0);
    let unit = if sep_ext > 0.0 { stride } else { ext };
    let first = (((o / unit).floor() as isize) - OVERSCAN).max(0) as usize;
    let last = ((((o + viewport) / unit).ceil() as isize) + OVERSCAN).max(0) as usize;
    let last = last.min(p.count);

    let mut items: Vec<AnyWidget> = Vec::new();
    for i in first..last {
        let item = (p.builder)(i);
        let at = if sep_ext > 0.0 { i as f64 * stride } else { i as f64 * ext };
        let placed = match p.axis {
            Axis::Vertical => Positioned::new(item)
                .top(at)
                .left(0.0)
                .right(0.0)
                .height(ext),
            Axis::Horizontal => Positioned::new(item)
                .left(at)
                .top(0.0)
                .bottom(0.0)
                .width(ext),
        };
        items.push(placed.into_widget());
        if let Some((se, sep_builder)) = &p.separator {
            if i + 1 < p.count {
                let sep_widget = (sep_builder)(i);
                let placed_sep = match p.axis {
                    Axis::Vertical => Positioned::new(sep_widget)
                        .top(at + ext)
                        .left(0.0)
                        .right(0.0)
                        .height(*se),
                    Axis::Horizontal => Positioned::new(sep_widget)
                        .left(at + ext)
                        .top(0.0)
                        .bottom(0.0)
                        .width(*se),
                };
                items.push(placed_sep.into_widget());
            }
        }
    }

    let max = (content_extent - viewport).max(0.0);
    ListViewport {
        axis: p.axis,
        offset: o.clamp(0.0, max),
        content_extent,
        id,
        scrollbar: p.scrollbar,
        child: Some(stack(items).into_widget()),
    }
}

// ---------------------------------------------------------------------------
// GridView — 2D virtualized grid (fixed columns × fixed row height).
// ---------------------------------------------------------------------------

/// A virtualized grid: `columns` cells per row, each `row_extent` tall, cell width
/// = viewport width / columns. Only the visible rows are built. Cells may SPAN
/// columns and rows ([`spans`](GridView::spans)) — the CSS-grid-style
/// `rowspan`/`colspan` (the layout packs around them).
pub struct GridView {
    count: usize,
    columns: usize,
    row_extent: f64,
    scrollbar: ScrollbarStyle,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    spans: Option<Rc<dyn Fn(usize) -> (u32, u32)>>,
}

impl GridView {
    /// Build a grid of `count` cells in `columns` columns, each `row_extent` tall.
    pub fn builder<W: IntoWidget>(
        count: usize,
        columns: usize,
        row_extent: f64,
        builder: impl Fn(usize) -> W + 'static,
    ) -> Self {
        GridView {
            count,
            columns: columns.max(1),
            row_extent,
            scrollbar: ScrollbarStyle::default(),
            builder: Rc::new(move |i| builder(i).into_widget()),
            spans: None,
        }
    }
    /// Customize the scrollbar.
    pub fn scrollbar(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar = style;
        self
    }
    /// Let each cell declare its (columns, rows) span — the CSS-grid
    /// `colspan`/`rowspan`. Spanning cells occupy that many grid cells; the
    /// packing wraps to the next row when a span doesn't fit. Defaults to
    /// (1, 1) per cell.
    pub fn spans(mut self, spans: impl Fn(usize) -> (u32, u32) + 'static) -> Self {
        self.spans = Some(Rc::new(spans));
        self
    }
}

struct GridProps {
    count: usize,
    columns: usize,
    row_extent: f64,
    scrollbar: ScrollbarStyle,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    spans: Option<Rc<dyn Fn(usize) -> (u32, u32)>>,
}

impl IntoWidget for GridView {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_grid,
            GridProps {
                count: self.count,
                columns: self.columns,
                row_extent: self.row_extent,
                scrollbar: self.scrollbar,
                builder: self.builder,
                spans: self.spans,
            },
        )
        .into_widget()
    }
}

/// One packed grid item: (row, col, col-span, row-span).
type Placement = (usize, usize, u32, u32);

/// Pack `count` items with their spans into a `cols`-column grid (the
/// CSS-grid packing: fill left-to-right, top-to-bottom, wrapping when a span
/// doesn't fit). Returns the placements and the number of rows used.
pub(crate) fn pack_grid(count: usize, cols: usize, spans: &dyn Fn(usize) -> (u32, u32)) -> (Vec<Placement>, usize) {
    let cols = cols.max(1);
    let mut occupied: Vec<u8> = Vec::new();
    let mut placements: Vec<Placement> = Vec::with_capacity(count);
    let mut rows_used = 0usize;
    for i in 0..count {
        let (raw_cs, raw_rs) = spans(i);
        let cs = raw_cs.clamp(1, cols as u32);
        let rs = raw_rs.max(1);
        // Find the first position (row-major) where the span fits.
        let mut placed = None;
        'scan: for row in 0usize.. {
            let needed = (row + rs as usize) * cols;
            if occupied.len() < needed {
                occupied.resize(needed, 0);
            }
            for c in 0..cols {
                if c as u32 + cs > cols as u32 {
                    break;
                }
                let mut fits = true;
                for rr in 0..rs as usize {
                    for cc in 0..cs as usize {
                        if occupied[(row + rr) * cols + c + cc] != 0 {
                            fits = false;
                            break;
                        }
                    }
                    if !fits {
                        break;
                    }
                }
                if fits {
                    for rr in 0..rs as usize {
                        for cc in 0..cs as usize {
                            occupied[(row + rr) * cols + c + cc] = 1;
                        }
                    }
                    placed = Some((row, c, cs, rs));
                    rows_used = rows_used.max(row + rs as usize);
                    break 'scan;
                }
            }
        }
        placements.push(placed.expect("a free cell always exists"));
    }
    (placements, rows_used)
}

fn render_grid(p: &GridProps) -> ListViewport {
    let id = owner_id().unwrap_or(0);
    let offset = create_signal(0.0_f64);
    let cols = p.columns.max(1);
    let row_h = p.row_extent.max(1.0);
    let (placements, rows_used) = match &p.spans {
        Some(spans) => pack_grid(p.count, cols, spans.as_ref()),
        None => {
            let rows = p.count.div_ceil(cols);
            let placements = (0..p.count)
                .map(|i| (i / cols, i % cols, 1u32, 1u32))
                .collect();
            (placements, rows)
        }
    };
    let content_extent = rows_used as f64 * row_h;

    create_cleanup(move || {
        scroll::clear(id);
        scroll_metrics::clear(id);
    });

    {
        let ce = content_extent;
        scroll::install(
            id,
            Rc::new(move |to| {
                let vp = scroll_metrics::get(id).map(|m| m.viewport).unwrap_or(0.0);
                let max = (ce - vp).max(0.0);
                let next = match to {
                    ScrollTo::By(d) => offset.peek() + d,
                    ScrollTo::ToFraction(f) => f * max,
                };
                offset.set(next.clamp(0.0, max));
            }),
        );
    }

    let o = offset.get();
    let m = scroll_metrics::get(id);
    let viewport = m.map(|x| x.viewport).unwrap_or(800.0);
    let cross = m.map(|x| x.cross).filter(|c| *c > 0.0).unwrap_or(800.0);
    let cell_w = cross / cols as f64;

    let first_row = (((o / row_h).floor() as isize) - OVERSCAN).max(0) as usize;
    let last_row = ((((o + viewport) / row_h).ceil() as isize) + OVERSCAN).max(0) as usize;
    let last_row = last_row.min(rows_used);

    let mut items: Vec<AnyWidget> = Vec::new();
    for (i, &(row, col, cs, rs)) in placements.iter().enumerate() {
        let row_end = row + rs as usize;
        if row_end <= first_row || row > last_row {
            continue;
        }
        {
            let item = (p.builder)(i);
            let placed = Positioned::new(item)
                .top(row as f64 * row_h)
                .left(col as f64 * cell_w)
                .width(cs as f64 * cell_w)
                .height(rs as f64 * row_h);
            items.push(placed.into_widget());
        }
    }
    let max = (content_extent - viewport).max(0.0);
    ListViewport {
        axis: Axis::Vertical,
        offset: o.clamp(0.0, max),
        content_extent,
        id,
        scrollbar: p.scrollbar,
        child: Some(stack(items).into_widget()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_span_packing_follows_css_grid_rules() {
        // 4 columns; item 0 spans 2×2, item 4 spans 2×1, the rest 1×1.
        let spans = |i: usize| match i {
            0 => (2, 2),
            4 => (2, 1),
            _ => (1, 1),
        };
        let (placements, rows) = pack_grid(5, 4, &spans);
        assert_eq!(rows, 3, "3 rows packed");
        assert_eq!(
            placements,
            vec![(0, 0, 2, 2), (0, 2, 1, 1), (0, 3, 1, 1), (1, 2, 1, 1), (2, 0, 2, 1)],
            "spans pack around each other, wrapping when they don't fit"
        );

        // A span wider than the grid clamps to the grid.
        let wide = |_: usize| (9, 3);
        let (placements, rows) = pack_grid(2, 4, &wide);
        assert_eq!(placements, vec![(0, 0, 4, 3), (3, 0, 4, 3)]);
        assert_eq!(rows, 6, "oversized spans clamp to the column count and stack");
    }
}
