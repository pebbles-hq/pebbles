//! Sticky & collapsing headers (A3): [`StickyList`] pins the active section's
//! header to the viewport top (with a push-off as the next header arrives), and
//! [`CollapsingHeader`] shrinks a pinned hero from its expanded height to a
//! compact bar as the content scrolls. Full Flutter slivers stay OUT (p2 §J) —
//! both compose from scroll offset + Stack.

use std::rc::Rc;

use pebbles_foundation::{CrossAxisAlignment, MainAxisSize};

use crate::components::heading;
use crate::theme::theme;
use crate::widgets::{
    ListView, Padding, Positioned, ScrollController, SizedBox, stack, transform, use_scroll_controller,
};
use pebbles_core::children;
use pebbles_core::component_props;
use pebbles_core::widget::{AnyWidget, IntoWidget};

// ---------------------------------------------------------------------------
// StickyList
// ---------------------------------------------------------------------------

/// A section of a [`StickyList`]: a header row plus its content rows.
#[derive(Clone)]
pub struct StickySection {
    header: AnyWidget,
    rows: Vec<AnyWidget>,
}

/// Declare one section of a [`StickyList`]: `header` stays pinned while its rows
/// are on screen.
pub fn sticky_section(header: impl IntoWidget, rows: impl pebbles_core::IntoChildren) -> StickySection {
    StickySection { header: header.into_widget(), rows: rows.into_children() }
}

/// A plain section-header label (the standard [`StickySection`] header).
pub fn section_header(label: impl Into<String>) -> impl IntoWidget {
    let th = theme();
    Padding::new(
        pebbles_foundation::EdgeInsets::symmetric(12.0, 0.0),
        heading(label).size(13.0).color(th.colors.muted_foreground),
    )
}

/// A scrollable list whose section headers pin to the top of the viewport while
/// their section is visible (the classic grouped-list behavior). v1: headers
/// share one fixed extent and rows share one fixed extent.
#[derive(Clone)]
pub struct StickyList {
    sections: Vec<StickySection>,
    header_extent: f64,
    row_extent: f64,
    controller: Option<ScrollController>,
}

/// Create an empty [`StickyList`] and add sections with [`section`](StickyList::section).
pub fn sticky_list() -> StickyList {
    StickyList { sections: Vec::new(), header_extent: 40.0, row_extent: 48.0, controller: None }
}

impl StickyList {
    /// Add a section: its header pins while its rows are visible.
    pub fn section(mut self, header: impl IntoWidget, rows: impl pebbles_core::IntoChildren) -> Self {
        self.sections.push(StickySection { header: header.into_widget(), rows: rows.into_children() });
        self
    }
    /// The fixed height of every section header (v1: one shared extent).
    pub fn header_extent(mut self, extent: f64) -> Self {
        self.header_extent = extent.max(1.0);
        self
    }
    /// The fixed height of every row (v1: one shared extent).
    pub fn row_extent(mut self, extent: f64) -> Self {
        self.row_extent = extent.max(1.0);
        self
    }
    /// Drive the list programmatically.
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }
}

#[derive(Clone)]
struct StickyProps {
    sections: Vec<StickySection>,
    header_extent: f64,
    row_extent: f64,
    controller: Option<ScrollController>,
}

impl IntoWidget for StickyList {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_sticky,
            StickyProps {
                sections: self.sections,
                header_extent: self.header_extent,
                row_extent: self.row_extent,
                controller: self.controller,
            },
        )
        .into_widget()
    }
}

fn render_sticky(p: &StickyProps) -> pebbles_core::Element {
    let controller = match &p.controller {
        Some(c) => *c,
        None => {
            let c = use_scroll_controller();
            c
        }
    };
    let he = p.header_extent;
    let re = p.row_extent;

    // Flatten the sections into slots: header + rows interleaved.
    let mut slots: Vec<AnyWidget> = Vec::new();
    let mut extents: Vec<f64> = Vec::new();
    let mut section_tops: Vec<f64> = Vec::new();
    let mut acc = 0.0_f64;
    for section in &p.sections {
        section_tops.push(acc);
        slots.push(section.header.clone());
        extents.push(he);
        acc += he;
        for row in &section.rows {
            slots.push(row.clone());
            extents.push(re);
            acc += re;
        }
    }
    let count = slots.len();
    let slots = Rc::new(slots);
    let extents = Rc::new(extents);
    let list =
        ListView::variable(count, move |i| extents[i], move |i| slots[i].clone()).controller(controller);

    // The active section: the last one whose top is at/above the viewport top.
    let offset = controller.offset_signal().get();
    let active = section_tops.partition_point(|&t| t <= offset + 1.0).saturating_sub(1);
    let active = active.min(p.sections.len().saturating_sub(1));
    let pinned_header = p.sections[active].header.clone();

    // Push-off: as the NEXT header approaches the top, the pinned one slides up
    // with it (translate = remaining gap − header extent).
    let next_top = section_tops.get(active + 1).copied();
    let push = next_top.map(|nt| (he - (nt - offset)).clamp(0.0, he)).unwrap_or(0.0);
    let pinned = transform(
        pebbles_render::Affine::translate((0.0, -push)),
        SizedBox::new(None, Some(he), Some(pinned_header)),
    );

    stack(children![list.into_widget(), Positioned::new(pinned).top(0.0).left(0.0).right(0.0).into_widget(),])
        .into_widget()
}

// ---------------------------------------------------------------------------
// CollapsingHeader
// ---------------------------------------------------------------------------

/// A pinned hero that collapses from `expanded` px to `collapsed` px as the
/// content scrolls; the builder receives the progress `t` (0 = collapsed, 1 =
/// expanded) so it can fade/scale its contents.
#[derive(Clone)]
pub struct CollapsingHeader {
    expanded: f64,
    collapsed: f64,
    builder: Rc<dyn Fn(f64) -> AnyWidget>,
    content: Option<AnyWidget>,
    controller: Option<ScrollController>,
}

/// Create a [`CollapsingHeader`]: `builder(t)` renders the pinned hero for the
/// collapse progress `t` in `0..=1`.
pub fn collapsing_header<W: IntoWidget>(
    expanded: f64,
    collapsed: f64,
    builder: impl Fn(f64) -> W + 'static,
) -> CollapsingHeader {
    CollapsingHeader {
        expanded: expanded.max(1.0),
        collapsed: collapsed.max(0.0),
        builder: Rc::new(move |t| builder(t).into_widget()),
        content: None,
        controller: None,
    }
}

impl CollapsingHeader {
    /// The scrollable content under the hero (scrolled with the header pinned
    /// on top).
    pub fn content(mut self, content: impl pebbles_core::IntoChildren) -> Self {
        self.content = Some(column_content(content.into_children()));
        self
    }
    /// Drive the scroll programmatically.
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }
}

fn column_content(items: Vec<AnyWidget>) -> AnyWidget {
    crate::widgets::column(items)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

#[derive(Clone)]
struct CollapseProps {
    expanded: f64,
    collapsed: f64,
    builder: Rc<dyn Fn(f64) -> AnyWidget>,
    content: AnyWidget,
    controller: Option<ScrollController>,
}

impl IntoWidget for CollapsingHeader {
    fn into_widget(self) -> AnyWidget {
        let content = self.content.unwrap_or_else(|| column_content(Vec::new()));
        component_props(
            render_collapse,
            CollapseProps {
                expanded: self.expanded,
                collapsed: self.collapsed,
                builder: self.builder,
                content,
                controller: self.controller,
            },
        )
        .into_widget()
    }
}

fn render_collapse(p: &CollapseProps) -> pebbles_core::Element {
    let controller = match &p.controller {
        Some(c) => *c,
        None => use_scroll_controller(),
    };
    let offset = controller.offset_signal();
    let range = (p.expanded - p.collapsed).max(1.0);
    let t = ((p.expanded - offset.get()) / range).clamp(0.0, 1.0);
    let hero = (p.builder)(t);
    let hero_height = p.collapsed + (p.expanded - p.collapsed) * t;

    // The content: top padding = the expanded height, so the hero covers the
    // first `expanded` px and the content scrolls under it. Auto-measured (A1)
    // so the content extent needs no manual computation.
    let content = p.content.clone();
    let expanded = p.expanded;
    let scrollable = ListView::builder_auto(1, move |_| {
        Padding::new(pebbles_foundation::EdgeInsets::only(0.0, expanded, 0.0, 0.0), content.clone())
    })
    .estimated_extent(600.0)
    .controller(controller);

    stack(children![
        scrollable.into_widget(),
        Positioned::new(SizedBox::new(None, Some(hero_height), Some(hero)))
            .top(0.0)
            .left(0.0)
            .right(0.0)
            .into_widget(),
    ])
    .into_widget()
}
