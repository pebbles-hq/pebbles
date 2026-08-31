//! Navigation & chrome components: [`Breadcrumb`], [`Toolbar`], [`StatusBar`] and
//! [`Pagination`].

use pebbles_core::IntoCallback;
use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize};
use pebbles_render::{Border, BoxDecoration, IconKind};

use pebbles_core::children;
use pebbles_core::context::Callback;
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{Container, gap_w, row, text};

use crate::components::icon;
use crate::components::{ButtonSize, ButtonVariant, button, dropdown_menu, menu_item};

/// A breadcrumb trail of path segments. When there are more than
/// [`max_visible`](Breadcrumb::max_visible) segments, the middle ones collapse
/// into a "…" dropdown (shadcn's ellipsis breadcrumb).
#[derive(Clone)]
pub struct Breadcrumb {
    segments: Vec<String>,
    max_visible: usize,
}

/// Create a [`Breadcrumb`] from path segments.
pub fn breadcrumb(segments: Vec<String>) -> Breadcrumb {
    Breadcrumb { segments, max_visible: usize::MAX }
}

impl Breadcrumb {
    /// Collapse middle segments into a "…" dropdown when the trail is longer
    /// than `n` (minimum 3 — first + "…" + `n - 2` trailing segments).
    pub fn max_visible(mut self, n: usize) -> Self {
        self.max_visible = n;
        self
    }
}

impl IntoWidget for Breadcrumb {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let segments = std::mem::take(&mut self.segments);
        let n = self.max_visible.max(3);
        let len = segments.len();

        // Split into [first] + [hidden middle] + [trailing n-2] when overflowing.
        let hidden: Vec<String> = if len <= n {
            Vec::new()
        } else {
            segments[1..len - (n - 2)].to_vec()
        };
        let trailing: Vec<String> = if len <= n {
            segments[1..].to_vec()
        } else {
            segments[len - (n - 2)..].to_vec()
        };

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
                let menu = dropdown_menu("…")
                    .trigger(text("…").size(13.0).color(th.colors.muted_foreground));
                let mut menu = menu;
                for h in &hidden {
                    menu = menu.item(menu_item(h.clone()).disabled(true));
                }
                items.push(menu.into_widget());
            } else {
                let color = if i == last { th.colors.foreground } else { th.colors.muted_foreground };
                items.push(text(seg).size(13.0).color(color).into_widget());
            }
            if i != last {
                items.push(gap_w(6.0).into_widget());
                items.push(
                    icon(IconKind::ChevronRight).size(14.0).color(th.colors.muted_foreground).into_widget(),
                );
                items.push(gap_w(6.0).into_widget());
            }
        }
        row(items).main_axis_size(MainAxisSize::Min).into_widget()
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
                row(children![text(std::mem::take(&mut self.text)).size(12.0).color(th.colors.muted_foreground)])
                    .main_axis_alignment(MainAxisAlignment::Start),
            )
            .into_widget()
    }
}

/// Prev/Next pagination with a page indicator.
#[derive(Clone)]
pub struct Pagination {
    page: usize,
    total: usize,
    on_prev: Option<Callback>,
    on_next: Option<Callback>,
}

/// Create a [`Pagination`] control (1-based `page`).
pub fn pagination(page: usize, total: usize) -> Pagination {
    Pagination { page, total, on_prev: None, on_next: None }
}

impl Pagination {
    pub fn on_prev(mut self, cb: impl IntoCallback) -> Self {
        self.on_prev = Some(cb.into_callback());
        self
    }
    pub fn on_next(mut self, cb: impl IntoCallback) -> Self {
        self.on_next = Some(cb.into_callback());
        self
    }
}


impl IntoWidget for Pagination {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let mut prev = button("Previous").variant(ButtonVariant::Outline).size(ButtonSize::Sm);
        if let Some(cb) = self.on_prev.take() {
            prev = prev.on_click(cb);
        }
        let mut next = button("Next").variant(ButtonVariant::Outline).size(ButtonSize::Sm);
        if let Some(cb) = self.on_next.take() {
            next = next.on_click(cb);
        }
        row(children![
            prev,
            gap_w(12.0),
            text(format!("Page {} of {}", self.page, self.total)).size(13.0).color(th.colors.muted_foreground),
            gap_w(12.0),
            next,
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
    }
}
