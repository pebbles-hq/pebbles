//! Shared UI helpers, plus a **props** demonstration. Everything returns
//! `impl IntoWidget`, so screens compose with no `.into_widget()`.
//!
//! ### On props in Rust
//! Props work (see [`stat_card`]), but the idiomatic split is: **local state** →
//! `create_signal` in the component; **shared/app state** → a global signal/`Store`
//! (no prop-drilling); **props** → only for reusable, parameterized widgets.

use pebbles::prelude::*;

pub use pebbles::prelude::{gap_h, gap_w};

/// A scrollable, padded screen with a heading + subtitle.
#[derive(Clone, Default)]
pub struct Screen {
    title: String,
    description: Option<String>,
}

/// Create a [`Screen`] with a heading.
pub fn screen(title: &str) -> Screen {
    Screen {
        title: title.to_string(),
        ..Default::default()
    }
}

impl Screen {
    /// The muted one-liner under the heading.
    pub fn description(mut self, sub: &str) -> Self {
        self.description = Some(sub.to_string());
        self
    }
    /// The screen body (sections / docs). Builds and returns the `Element`.
    pub fn body(self, body: impl IntoChildren) -> Element {
        let mut items: Vec<AnyWidget> = vec![
            heading(self.title.clone()).into_widget(),
            gap_h(4.0).into_widget(),
        ];
        if let Some(sub) = &self.description {
            items.push(subtitle(sub.clone()).into_widget());
        }
        items.push(gap_h(24.0).into_widget());
        items.extend(body.into_children());
        SingleChildScrollView::vertical(
            Container::new().padding(EdgeInsets::all(30.0)).child(
                column(items)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
            ),
        )
        .into_widget()
    }
}

/// A labeled sub-section within a screen.
pub fn section(title: &str, body: impl IntoWidget) -> Element {
    column(children![
        text(title.to_string())
            .size(12.0)
            .semibold()
            .color(theme().colors.muted_foreground),
        gap_h(12.0),
        body,
        gap_h(28.0),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min)
    .into_widget()
}

/// A documentation-style section: a title, a descriptive sentence, then examples.
/// The house style for the showcase screens.
#[derive(Clone, Default)]
pub struct Doc {
    title: String,
    description: Option<String>,
}

/// Create a [`Doc`] with a section title.
pub fn doc(title: &str) -> Doc {
    Doc {
        title: title.to_string(),
        ..Default::default()
    }
}

impl Doc {
    /// The one-sentence explanation under the title.
    pub fn description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }
    /// The example body. Builds and returns the `Element`.
    pub fn body(self, body: impl IntoWidget) -> Element {
        let c = theme().colors;
        let mut items: Vec<AnyWidget> = vec![
            text(self.title)
                .size(16.0)
                .semibold()
                .color(c.foreground)
                .into_widget(),
            gap_h(4.0).into_widget(),
        ];
        if let Some(desc) = &self.description {
            items.push(
                text(desc.clone())
                    .size(13.5)
                    .line_height(1.45)
                    .color(c.muted_foreground)
                    .into_widget(),
            );
        }
        items.push(gap_h(16.0).into_widget());
        items.push(body.into_widget());
        items.push(gap_h(34.0).into_widget());
        column(items)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .into_widget()
    }
}

// ---------------------------------------------------------------------------
// Props demo: a reusable stat card via `component_props`.
// ---------------------------------------------------------------------------

pub struct StatCardProps {
    pub title: String,
    pub value: String,
    pub icon: IconKind,
    pub tint: Color,
}

/// A reusable stat tile — the canonical props case: a parameterized widget reused
/// with different inputs.
pub fn stat_card(title: &str, value: &str, icon: IconKind, tint: Color) -> impl IntoWidget {
    component_props(
        render_stat_card,
        StatCardProps {
            title: title.into(),
            value: value.into(),
            icon,
            tint,
        },
    )
}

fn render_stat_card(p: &StatCardProps) -> Card {
    let c = theme().colors;
    card().child(
        column(children![
            row(children![
                Container::new()
                    .decoration(
                        BoxDecoration::new()
                            .color(p.tint)
                            .radius(BorderRadius::all(8.0))
                    )
                    .padding(EdgeInsets::all(8.0))
                    .child(icon(p.icon).size(18.0).color(palette::WHITE)),
                gap_w(10.0),
                text(p.title.clone()).size(13.0).color(c.muted_foreground),
            ])
            .main_axis_size(MainAxisSize::Min),
            gap_h(10.0),
            text(p.value.clone()).size(26.0).bold().color(c.foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}
