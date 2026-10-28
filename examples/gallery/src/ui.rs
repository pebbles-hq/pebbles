//! Page scaffolding shared by every gallery page: a scrollable page with a heading, labelled
//! sections ([`doc`]) and labelled samples ([`sample`] / [`samples`]).

use pebbles::prelude::*;

pub use pebbles::prelude::{gap_h, gap_w};

/// A scrollable, padded page with a heading.
#[derive(Clone, Default)]
pub struct Screen {
    title: String,
}

/// Create a [`Screen`] with a heading.
pub fn screen(title: &str) -> Screen {
    Screen { title: title.to_string() }
}

impl Screen {
    /// The page body. Builds and returns the `Element`.
    pub fn body(self, body: impl IntoChildren) -> Element {
        let mut items: Vec<AnyWidget> =
            vec![text(self.title.clone()).size(24.0).bold().into_widget(), gap_h(20.0).into_widget()];
        items.extend(body.into_children());
        scroll_view(container().padding(EdgeInsets::all(24.0)).child(
            column(items).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
        ))
        .into_widget()
    }
}

/// A labelled sub-section within a page (older pages' form of [`doc`]).
pub fn section(title: &str, body: impl IntoWidget) -> Element {
    doc(title).body(body)
}

/// A labelled section: a short title, then the samples.
#[derive(Clone, Default)]
pub struct Doc {
    title: String,
}

/// Create a [`Doc`] with a section title.
pub fn doc(title: &str) -> Doc {
    Doc { title: title.to_string() }
}

impl Doc {
    /// The samples. Builds and returns the `Element`.
    pub fn body(self, body: impl IntoWidget) -> Element {
        let c = theme().colors;
        column(children![
            text(self.title).size(15.0).semibold().color(c.foreground),
            gap_h(10.0),
            body,
            gap_h(26.0),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
    }
}

/// One sample with a short state label above it ("disabled", "error", "RTL", …).
pub fn sample(label: &str, child: impl IntoWidget) -> Element {
    column(children![
        text(label.to_string()).size(11.0).color(theme().colors.muted_foreground),
        gap_h(6.0),
        child,
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
    .into_widget()
}

/// Labelled samples laid out in a wrapping row.
pub fn samples(items: Vec<(&str, AnyWidget)>) -> Element {
    wrap(items.into_iter().map(|(l, w)| sample(l, w).into_widget()).collect::<Vec<_>>())
        .spacing(20.0)
        .run_spacing(16.0)
        .into_widget()
}

thread_local! {
    static OFFLINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Headless renders run offline: samples that would fetch from the network show their
/// fallback instead, so a render never depends on network timing.
pub fn set_offline(on: bool) {
    OFFLINE.with(|c| c.set(on));
}

/// A network image URL for a sample, or a never-resolving one while [`set_offline`] is on.
pub fn net_url(url: &str) -> String {
    if OFFLINE.with(|c| c.get()) { String::new() } else { url.to_string() }
}
