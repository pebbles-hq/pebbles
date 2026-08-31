//! [`Tabs`] — a tab bar plus the selected tab's content. Controlled: `selected`
//! is a prop and each tab carries an `on_select` [`Callback`].

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisSize, palette};
use pebbles_render::BoxDecoration;

use pebbles_core::children;
use pebbles_core::IntoCallback;
use pebbles_core::context::Callback;
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{Container, GestureDetector, Padding, column, gap_h, row, text};

#[derive(Clone)]
struct TabDef {
    label: String,
    content: AnyWidget,
    on_select: Option<Callback>,
}

/// A tabbed panel.
#[derive(Clone)]
pub struct Tabs {
    selected: usize,
    tabs: Vec<TabDef>,
}

/// Create a [`Tabs`] with the given selected index.
pub fn tabs(selected: usize) -> Tabs {
    Tabs { selected, tabs: Vec::new() }
}

impl Tabs {
    /// Add a tab with a label, content and a selection callback.
    pub fn tab(mut self, label: impl Into<String>, content: impl IntoWidget, on_select: impl IntoCallback) -> Self {
        self.tabs.push(TabDef {
            label: label.into(),
            content: content.into_widget(),
            on_select: Some(on_select.into_callback()),
        });
        self
    }
}


impl IntoWidget for Tabs {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let selected = self.selected;
        let mut bar = Vec::new();
        let mut selected_content: Option<AnyWidget> = None;

        for (i, tab) in std::mem::take(&mut self.tabs).into_iter().enumerate() {
            let is_sel = i == selected;
            let label_color =
                if is_sel { th.colors.foreground } else { th.colors.muted_foreground };
            let underline_color = if is_sel { th.colors.primary } else { palette::TRANSPARENT };

            let button = GestureDetector::new(
                column(children![
                    Padding::new(
                        EdgeInsets::symmetric(14.0, 8.0),
                        text(tab.label).size(14.0).weight(500.0).color(label_color),
                    ),
                    Container::new().color(underline_color).height(2.0),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            );
            let button = match tab.on_select {
                Some(cb) => button.on_tap(cb),
                None => button,
            };
            bar.push(button.into_widget());

            if is_sel {
                selected_content = Some(tab.content);
            }
        }

        let content = selected_content.unwrap_or_else(|| gap_h(0.0).into_widget());

        column(children![
            Container::new()
                .decoration(BoxDecoration::new().color(th.colors.background))
                .child(row(bar).main_axis_size(MainAxisSize::Min)),
            Container::new().color(th.colors.border).height(1.0),
            Padding::new(EdgeInsets::symmetric(0.0, 16.0), content),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
    }
}
