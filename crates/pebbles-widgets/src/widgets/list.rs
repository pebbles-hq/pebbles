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
    let content_extent = p.count as f64 * p.item_extent;

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
    let first = (((o / ext).floor() as isize) - OVERSCAN).max(0) as usize;
    let last = ((((o + viewport) / ext).ceil() as isize) + OVERSCAN).max(0) as usize;
    let last = last.min(p.count);

    let mut items: Vec<AnyWidget> = Vec::new();
    for i in first..last {
        let item = (p.builder)(i);
        let placed = match p.axis {
            Axis::Vertical => Positioned::new(item)
                .top(i as f64 * ext)
                .left(0.0)
                .right(0.0)
                .height(ext),
            Axis::Horizontal => Positioned::new(item)
                .left(i as f64 * ext)
                .top(0.0)
                .bottom(0.0)
                .width(ext),
        };
        items.push(placed.into_widget());
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
/// = viewport width / columns. Only the visible rows are built.
pub struct GridView {
    count: usize,
    columns: usize,
    row_extent: f64,
    scrollbar: ScrollbarStyle,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
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
        }
    }
    /// Customize the scrollbar.
    pub fn scrollbar(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar = style;
        self
    }
}

struct GridProps {
    count: usize,
    columns: usize,
    row_extent: f64,
    scrollbar: ScrollbarStyle,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
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
            },
        )
        .into_widget()
    }
}

fn render_grid(p: &GridProps) -> ListViewport {
    let id = owner_id().unwrap_or(0);
    let offset = create_signal(0.0_f64);
    let cols = p.columns.max(1);
    let rows = p.count.div_ceil(cols);
    let row_h = p.row_extent.max(1.0);
    let content_extent = rows as f64 * row_h;

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
    let last_row = last_row.min(rows);

    let mut items: Vec<AnyWidget> = Vec::new();
    for r in first_row..last_row {
        for c in 0..cols {
            let idx = r * cols + c;
            if idx >= p.count {
                break;
            }
            let cell = (p.builder)(idx);
            items.push(
                Positioned::new(cell)
                    .left(c as f64 * cell_w)
                    .top(r as f64 * row_h)
                    .width(cell_w)
                    .height(row_h)
                    .into_widget(),
            );
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
