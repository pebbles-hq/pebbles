//! Navigation & chrome components: [`Breadcrumb`], [`Toolbar`], [`StatusBar`] and
//! [`Pagination`].

use std::rc::Rc;

use pebbles_core::IntoCallback;
use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize};
use pebbles_render::{Border, BoxDecoration, IconData, IconKind, lucide};

use crate::style::{Style, styled};
use crate::theme::theme;
use crate::widgets::{Container, Padding, gap_w, row, text};
use pebbles_core::children;
use pebbles_core::context::Callback;
use pebbles_core::widget::{AnyWidget, IntoWidget};

use crate::components::icon;
use crate::components::{ButtonSize, ButtonVariant, button, dropdown_menu, icon_button, menu_item};

/// A breadcrumb trail of path segments. When there are more than
/// [`max_visible`](Breadcrumb::max_visible) segments, the middle ones collapse
/// into a "…" dropdown (shadcn's ellipsis breadcrumb).
#[derive(Clone)]
pub struct Breadcrumb {
    segments: Vec<String>,
    max_visible: usize,
    separator: pebbles_render::IconData,
    style: Option<Style>,
}

/// Create a [`Breadcrumb`] from path segments.
pub fn breadcrumb(segments: Vec<String>) -> Breadcrumb {
    Breadcrumb { segments, max_visible: usize::MAX, separator: IconKind::ChevronRight.into(), style: None }
}

impl Breadcrumb {
    /// Collapse middle segments into a "…" dropdown when the trail is longer
    /// than `n` (minimum 3 — first + "…" + `n - 2` trailing segments).
    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }
    /// The glyph between segments (default `ChevronRight`).
    pub fn separator(mut self, glyph: impl Into<pebbles_render::IconData>) -> Self {
        self.separator = glyph.into();
        self
    }
    /// Merge a [`Style`](crate::Style) over the trail: box props wrap the row;
    /// text props (color, size) style the segment labels (the active segment
    /// takes the color, the rest stay muted).
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

impl IntoWidget for Breadcrumb {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let segments = std::mem::take(&mut self.segments);
        let n = self.max_visible.max(3);
        let len = segments.len();
        let merged = crate::style::style().merge(self.style.clone().unwrap_or_default());
        let seg_color = merged.color.unwrap_or(th.colors.foreground);
        let seg_size = merged.font_size.unwrap_or(13.0);

        // Split into [first] + [hidden middle] + [trailing n-2] when overflowing.
        let hidden: Vec<String> = if len <= n { Vec::new() } else { segments[1..len - (n - 2)].to_vec() };
        let trailing: Vec<String> =
            if len <= n { segments[1..].to_vec() } else { segments[len - (n - 2)..].to_vec() };

        let mut items: Vec<AnyWidget> = Vec::new();
        let mut slots: Vec<String> = Vec::new();
        slots.push(segments[0].clone());
        if !hidden.is_empty() {
            slots.push(String::new()); // sentinel — rendered as the "…" dropdown
        }
        slots.extend(trailing);
        let last = slots.len().saturating_sub(1);
        for (i, seg) in slots.into_iter().enumerate() {
            if seg.is_empty() {
                let menu = dropdown_menu("…").trigger(text("…").size(13.0).color(th.colors.muted_foreground));
                let mut menu = menu;
                for h in &hidden {
                    menu = menu.item(menu_item(h.clone()).disabled(true));
                }
                items.push(menu.into_widget());
            } else {
                let color = if i == last { seg_color } else { th.colors.muted_foreground };
                items.push(text(seg).size(seg_size).color(color).into_widget());
            }
            if i != last {
                items.push(gap_w(6.0).into_widget());
                items.push(icon(self.separator).size(14.0).color(th.colors.muted_foreground).into_widget());
                items.push(gap_w(6.0).into_widget());
            }
        }
        styled(row(items).main_axis_size(MainAxisSize::Min), merged).into_widget()
    }
}

/// A horizontal chrome bar (top toolbar) with a bottom border.
#[derive(Clone)]
pub struct Toolbar {
    children: Vec<AnyWidget>,
}

/// Create a [`Toolbar`] row.
pub fn toolbar(children: Vec<AnyWidget>) -> Toolbar {
    Toolbar { children }
}

impl IntoWidget for Toolbar {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        Container::new()
            .decoration(
                BoxDecoration::new().color(th.colors.background).border(Border::new(th.colors.border, 1.0)),
            )
            .padding(EdgeInsets::symmetric(12.0, 8.0))
            .child(row(std::mem::take(&mut self.children)).main_axis_size(MainAxisSize::Min))
            .into_widget()
    }
}

/// A bottom status bar.
#[derive(Clone)]
pub struct StatusBar {
    text: String,
}

/// Create a [`StatusBar`] with the given text.
pub fn status_bar(text: impl Into<String>) -> StatusBar {
    StatusBar { text: text.into() }
}

impl IntoWidget for StatusBar {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        Container::new()
            .decoration(BoxDecoration::new().color(th.colors.muted))
            .padding(EdgeInsets::symmetric(12.0, 5.0))
            .child(
                row(children![
                    text(std::mem::take(&mut self.text)).size(12.0).color(th.colors.muted_foreground)
                ])
                .main_axis_alignment(MainAxisAlignment::Start),
            )
            .into_widget()
    }
}

/// Prev/Next pagination with a page indicator.
/// The visual design of a [`Pagination`] control.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PaginationVariant {
    /// Numbered page pills with ellipses and chevron arrows — shadcn's classic.
    #[default]
    Numbers,
    /// Chevron arrows around a "Page X of Y" label.
    Simple,
    /// Chevron arrows around a compact "X / Y" label.
    Arrows,
}

/// Prev/next pagination with numbered pages. Build with [`pagination`].
#[derive(Clone, Default)]
pub struct Pagination {
    page: usize,
    total: usize,
    variant: PaginationVariant,
    max_buttons: usize,
    edges: bool,
    on_page: Option<Rc<dyn Fn(usize)>>,
    on_prev: Option<Callback>,
    on_next: Option<Callback>,
    style: Option<Style>,
}

/// Create a [`Pagination`] control (1-based `page`). Shows first/last jump buttons
/// (double chevrons) by default — turn them off with [`Pagination::edges`]`(false)`.
pub fn pagination(page: usize, total: usize) -> Pagination {
    Pagination { page, total, max_buttons: 7, edges: true, ..Default::default() }
}

impl Pagination {
    /// The design (default [`PaginationVariant::Numbers`]).
    pub fn variant(mut self, variant: PaginationVariant) -> Self {
        self.variant = variant;
        self
    }
    /// The `Numbers` design: how many pills before collapsing to ellipses
    /// (default 7; minimum 5).
    pub fn max_buttons(mut self, n: usize) -> Self {
        self.max_buttons = n.max(5);
        self
    }
    /// Show the first/last jump buttons — the double-chevron controls that go straight
    /// to page 1 or the last page (default `true`).
    pub fn edges(mut self, on: bool) -> Self {
        self.edges = on;
        self
    }
    /// Reports EVERY page change (number pills, prev and next) as the target
    /// 1-based page — the unified callback. When set, `on_prev`/`on_next` are
    /// not used.
    pub fn on_page(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_page = Some(Rc::new(f));
        self
    }
    /// Legacy per-button callbacks (used only when [`on_page`](Pagination::on_page)
    /// is unset). Kept for compatibility with the previous API.
    pub fn on_prev(mut self, cb: impl IntoCallback) -> Self {
        self.on_prev = Some(cb.into_callback());
        self
    }
    /// Legacy per-button callbacks (used only when [`on_page`](Pagination::on_page)
    /// is unset).
    pub fn on_next(mut self, cb: impl IntoCallback) -> Self {
        self.on_next = Some(cb.into_callback());
        self
    }
    /// Merge a [`Style`](crate::Style) over the control's surface (background,
    /// border, radius, padding, …).
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// One item in the `Numbers` window.
enum PageItem {
    Page(usize),
    Ellipsis,
}

/// The numbered window: `1 … p-1 p p+1 … total`, collapsed to ellipses.
fn page_window(page: usize, total: usize, max: usize) -> Vec<PageItem> {
    if total <= max {
        return (1..=total).map(PageItem::Page).collect();
    }
    let mut items = vec![PageItem::Page(1)];
    if page > 3 {
        items.push(PageItem::Ellipsis);
    }
    for p in (page.saturating_sub(1)).max(2)..=(page + 1).min(total - 1) {
        items.push(PageItem::Page(p));
    }
    if page < total.saturating_sub(2) {
        items.push(PageItem::Ellipsis);
    }
    items.push(PageItem::Page(total));
    items
}

/// Fire a legacy plain callback, if it is one.
fn invoke(cb: &Callback) {
    if let Callback::Plain(f) = cb {
        f();
    }
}

impl IntoWidget for Pagination {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let total = self.total.max(1);
        let page = self.page.clamp(1, total);
        let edges = self.edges;
        let on_page = self.on_page.take();
        let on_prev = self.on_prev.take();
        let on_next = self.on_next.take();

        // The unified navigation: on_page wins; legacy callbacks are the fallback.
        let go: Rc<dyn Fn(usize)> = Rc::new(move |p| {
            if let Some(f) = &on_page {
                f(p);
            } else if p < page
                && let Some(cb) = &on_prev
            {
                invoke(cb);
            } else if p > page
                && let Some(cb) = &on_next
            {
                invoke(cb);
            }
        });

        // A bordered (Outline) arrow control — clearly a button, disabled at the bounds.
        let arrow = |ic: IconData, enabled: bool, target: usize, go: &Rc<dyn Fn(usize)>| -> AnyWidget {
            let mut b = icon_button(ic).variant(ButtonVariant::Outline).size(15.0);
            if enabled {
                let go = go.clone();
                b = b.on_pressed(move || go(target));
            } else {
                b = b.disabled(true);
            }
            b.into_widget()
        };
        let at_start = page <= 1;
        let at_end = page >= total;
        let first = arrow(lucide::CHEVRONS_LEFT, !at_start, 1, &go);
        let prev = arrow(IconKind::ChevronLeft.into(), !at_start, page.saturating_sub(1).max(1), &go);
        let next = arrow(IconKind::ChevronRight.into(), !at_end, (page + 1).min(total), &go);
        let last = arrow(lucide::CHEVRONS_RIGHT, !at_end, total, &go);

        let line: AnyWidget = match self.variant {
            PaginationVariant::Numbers => {
                let mut kids: Vec<AnyWidget> = Vec::new();
                if edges {
                    kids.push(first);
                    kids.push(gap_w(4.0).into_widget());
                }
                kids.push(prev);
                for item in page_window(page, total, self.max_buttons) {
                    kids.push(gap_w(4.0).into_widget());
                    match item {
                        PageItem::Page(p) => {
                            let active = p == page;
                            let mut b = button(format!("{p}")).size(ButtonSize::Sm).variant(if active {
                                ButtonVariant::Primary
                            } else {
                                ButtonVariant::Outline
                            });
                            let go = go.clone();
                            b = b.on_pressed(move || go(p));
                            kids.push(b.into_widget());
                        }
                        PageItem::Ellipsis => kids.push(
                            Padding::new(
                                EdgeInsets::symmetric(4.0, 2.0),
                                text("…").size(13.0).color(th.colors.muted_foreground),
                            )
                            .into_widget(),
                        ),
                    }
                }
                kids.push(gap_w(4.0).into_widget());
                kids.push(next);
                if edges {
                    kids.push(gap_w(4.0).into_widget());
                    kids.push(last);
                }
                row(kids)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min)
                    .into_widget()
            }
            PaginationVariant::Simple | PaginationVariant::Arrows => {
                let label = if self.variant == PaginationVariant::Simple {
                    format!("Page {page} of {total}")
                } else {
                    format!("{page} / {total}")
                };
                let mut kids: Vec<AnyWidget> = Vec::new();
                if edges {
                    kids.push(first);
                    kids.push(gap_w(6.0).into_widget());
                }
                kids.push(prev);
                kids.push(gap_w(10.0).into_widget());
                kids.push(text(label).size(13.0).color(th.colors.muted_foreground).into_widget());
                kids.push(gap_w(10.0).into_widget());
                kids.push(next);
                if edges {
                    kids.push(gap_w(6.0).into_widget());
                    kids.push(last);
                }
                row(kids)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min)
                    .into_widget()
            }
        };
        styled(line, self.style.unwrap_or_default()).into_widget()
    }
}
