//! Shared UI helpers, plus a **props** demonstration. Everything returns
//! `impl IntoWidget`, so screens compose with no `.into_widget()`.
//!
//! ### On props in Rust
//! Props work (see [`stat_card`]), but the idiomatic split is: **local state** →
//! `create_signal` in the component; **shared/app state** → a global signal/`Store`
//! (no prop-drilling); **props** → only for reusable, parameterized widgets.

use pebbles::prelude::*;

pub fn gap_w(n: f64) -> impl IntoWidget {
    SizedBox::spacer(n, 0.0)
}
pub fn gap_h(n: f64) -> impl IntoWidget {
    SizedBox::spacer(0.0, n)
}

/// A scrollable, padded screen with a heading + subtitle.
pub fn screen<I, W>(title: &str, sub: &str, body: I) -> impl IntoWidget
where
    I: IntoIterator<Item = W>,
    W: IntoWidget,
{
    let mut items: Vec<AnyWidget> = vec![
        heading(title).into_widget(),
        gap_h(4.0).into_widget(),
        subtitle(sub).into_widget(),
        gap_h(24.0).into_widget(),
    ];
    items.extend(body.into_iter().map(IntoWidget::into_widget));
    SingleChildScrollView::vertical(
        Container::new()
            .padding(EdgeInsets::all(30.0))
            .child(column(items).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_min()),
    )
}

/// A labeled sub-section within a screen.
pub fn section(title: &str, body: impl IntoWidget) -> impl IntoWidget {
    column(children![
        text(title.to_string()).size(12.0).semibold().color(theme().colors.muted_foreground),
        gap_h(12.0),
        body,
        gap_h(28.0),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_min()
}

/// A documentation-style section: a title, a descriptive sentence, then examples.
/// The house style for the showcase screens.
pub fn doc(title: &str, desc: &str, body: impl IntoWidget) -> impl IntoWidget {
    let c = theme().colors;
    column(children![
        text(title.to_string()).size(16.0).semibold().color(c.foreground),
        gap_h(4.0),
        text(desc.to_string()).size(13.5).line_height(1.45).color(c.muted_foreground),
        gap_h(16.0),
        body,
        gap_h(34.0),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_min()
}

/// A horizontal group with even gaps.
pub fn hstack<I, W>(items: I, gap: f64) -> impl IntoWidget
where
    I: IntoIterator<Item = W>,
    W: IntoWidget,
{
    let items: Vec<AnyWidget> = items.into_iter().map(IntoWidget::into_widget).collect();
    let mut spaced: Vec<AnyWidget> = Vec::new();
    let last = items.len().saturating_sub(1);
    for (i, item) in items.into_iter().enumerate() {
        spaced.push(item);
        if i != last {
            spaced.push(gap_w(gap).into_widget());
        }
    }
    row(spaced).cross_axis_alignment(CrossAxisAlignment::Center).main_axis_min()
}

/// A vertical group with even gaps.
pub fn vstack<I, W>(items: I, gap: f64) -> impl IntoWidget
where
    I: IntoIterator<Item = W>,
    W: IntoWidget,
{
    let items: Vec<AnyWidget> = items.into_iter().map(IntoWidget::into_widget).collect();
    let mut spaced: Vec<AnyWidget> = Vec::new();
    let last = items.len().saturating_sub(1);
    for (i, item) in items.into_iter().enumerate() {
        spaced.push(item);
        if i != last {
            spaced.push(gap_h(gap).into_widget());
        }
    }
    column(spaced).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min()
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
        StatCardProps { title: title.into(), value: value.into(), icon, tint },
    )
}

fn render_stat_card(p: &StatCardProps) -> Card {
    let c = theme().colors;
    Card::new(
        column(children![
            row(children![
                Container::new()
                    .decoration(BoxDecoration::new().color(p.tint).radius(BorderRadius::all(8.0)))
                    .padding(EdgeInsets::all(8.0))
                    .child(icon(p.icon).size(18.0).color(palette::WHITE)),
                gap_w(10.0),
                text(p.title.clone()).size(13.0).color(c.muted_foreground),
            ])
            .main_axis_min(),
            gap_h(10.0),
            text(p.value.clone()).size(26.0).bold().color(c.foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min(),
    )
}
