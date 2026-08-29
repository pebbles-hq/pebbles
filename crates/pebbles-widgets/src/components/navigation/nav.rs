//! Navigation & chrome components: [`Breadcrumb`], [`Toolbar`], [`StatusBar`] and
//! [`Pagination`].

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment};
use pebbles_render::{Border, BoxDecoration, IconKind};

use pebbles_core::children;
use pebbles_core::context::{BuildContext, Callback};
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{Container, SizedBox, row, text};

use crate::components::icon;
use crate::components::{ButtonSize, ButtonVariant, button};

/// A breadcrumb trail of path segments.
#[derive(Clone)]
pub struct Breadcrumb {
    segments: Vec<String>,
}

/// Create a [`Breadcrumb`] from path segments.
pub fn breadcrumb(segments: Vec<String>) -> Breadcrumb {
    Breadcrumb { segments }
}

pebbles_core::stateless_widget!(Breadcrumb);

impl StatelessWidget for Breadcrumb {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();
        let mut items: Vec<AnyWidget> = Vec::new();
        let last = self.segments.len().saturating_sub(1);
        for (i, seg) in std::mem::take(&mut self.segments).into_iter().enumerate() {
            let color =
                if i == last { th.colors.foreground } else { th.colors.muted_foreground };
            items.push(text(seg).size(13.0).color(color).into_widget());
            if i != last {
                items.push(SizedBox::spacer(6.0, 0.0).into_widget());
                items.push(
                    icon(IconKind::ChevronRight).size(14.0).color(th.colors.muted_foreground).into_widget(),
                );
                items.push(SizedBox::spacer(6.0, 0.0).into_widget());
            }
        }
        row(items).main_axis_min().into_widget()
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

pebbles_core::stateless_widget!(Toolbar);

impl StatelessWidget for Toolbar {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();
        Container::new()
            .decoration(
                BoxDecoration::new().color(th.colors.background).border(Border::new(th.colors.border, 1.0)),
            )
            .padding(EdgeInsets::symmetric(12.0, 8.0))
            .child(row(std::mem::take(&mut self.children)).main_axis_min())
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

pebbles_core::stateless_widget!(StatusBar);

impl StatelessWidget for StatusBar {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
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
    pub fn on_prev(mut self, cb: Callback) -> Self {
        self.on_prev = Some(cb);
        self
    }
    pub fn on_next(mut self, cb: Callback) -> Self {
        self.on_next = Some(cb);
        self
    }
}

pebbles_core::stateless_widget!(Pagination);

impl StatelessWidget for Pagination {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
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
            SizedBox::spacer(12.0, 0.0),
            text(format!("Page {} of {}", self.page, self.total)).size(13.0).color(th.colors.muted_foreground),
            SizedBox::spacer(12.0, 0.0),
            next,
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_min()
        .into_widget()
    }
}
