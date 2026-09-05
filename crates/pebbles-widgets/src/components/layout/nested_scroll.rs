//! [`nested_scroll_view`] — a header above a scrolling body (Flutter's
//! `NestedScrollView`, non-sliver case).
//!
//! Two modes:
//! - **scroll-away** (default): the header and body share **one** scroll position —
//!   the header scrolls off the top as you scroll down, then the body content
//!   continues. Built as `scroll_view(column([header, body]))`; the body is plain
//!   (non-viewport) content that shares the outer scroll.
//! - **pinned** (`.pinned(true)`): the header stays fixed at the top and the body
//!   scrolls independently beneath it.
//!
//! The coordinated *collapsing* header (a hero that shrinks as an independently-
//! scrolling body moves, à la `SliverAppBar`) is [`collapsing_header`](crate::collapsing_header) /
//! [`sticky_list`](crate::sticky_list) — those cover the sliver outcome without the
//! sliver machinery.

use pebbles_foundation::{CrossAxisAlignment, MainAxisSize};

use crate::widgets::{Expanded, column, scroll_view};
use pebbles_core::children;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A header over a scrolling body. Build with [`nested_scroll_view`].
#[derive(Clone)]
pub struct NestedScrollView {
    header: AnyWidget,
    body: AnyWidget,
    pinned: bool,
}

/// Create a [`NestedScrollView`] with a `header` above a scrolling `body`.
pub fn nested_scroll_view(header: impl IntoWidget, body: impl IntoWidget) -> NestedScrollView {
    NestedScrollView { header: header.into_widget(), body: body.into_widget(), pinned: false }
}

impl NestedScrollView {
    /// Pin the header at the top so the body scrolls beneath it (default `false`,
    /// where the header shares the scroll and moves off the top).
    pub fn pinned(mut self, pinned: bool) -> Self {
        self.pinned = pinned;
        self
    }
}

impl IntoWidget for NestedScrollView {
    fn into_widget(self) -> AnyWidget {
        if self.pinned {
            // Header fixed at the top; the body scrolls in the remaining space.
            column(children![self.header, Expanded::new(scroll_view(self.body))])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .into_widget()
        } else {
            // One shared scroll position: the header scrolls off, then the body.
            scroll_view(
                column(children![self.header, self.body])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
            )
            .into_widget()
        }
    }
}
