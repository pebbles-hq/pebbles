//! [`Panel`] — a titled, bordered surface for docking-style desktop layouts (a
//! side panel, an inspector, a tool window).

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize};
use pebbles_render::{Border, BorderRadius, BoxDecoration};

use crate::theme::theme;
use crate::widgets::{Container, Expanded, column, row, text};
use pebbles_core::children;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A titled panel: a header bar over a bordered content area.
#[derive(Clone)]
pub struct Panel {
    title: String,
    child: Option<AnyWidget>,
    actions: Option<AnyWidget>,
}

/// Create a [`Panel`] with a title and body.
pub fn panel(title: impl Into<String>, child: impl IntoWidget) -> Panel {
    Panel { title: title.into(), child: Some(child.into_widget()), actions: None }
}

impl Panel {
    /// A trailing widget in the header (e.g. buttons).
    pub fn actions(mut self, actions: impl IntoWidget) -> Self {
        self.actions = Some(actions.into_widget());
        self
    }
}

impl IntoWidget for Panel {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;

        let mut header_row: Vec<AnyWidget> = vec![
            text(std::mem::take(&mut self.title))
                .size(12.0)
                .semibold()
                .color(c.muted_foreground)
                .into_widget(),
        ];
        if let Some(actions) = self.actions.take() {
            header_row.push(Expanded::new(crate::widgets::gap_h(0.0)).into_widget());
            header_row.push(actions);
        }
        let header = Container::new()
            .color(c.muted)
            .padding(EdgeInsets::symmetric(12.0, 8.0))
            .child(row(header_row).main_axis_alignment(MainAxisAlignment::SpaceBetween));

        let body =
            Container::new().color(c.card).padding(EdgeInsets::all(12.0)).child(self.child.take().unwrap());

        Container::new()
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .child(
                column(children![header, Container::new().color(c.border).height(1.0), body,])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
            )
            .into_widget()
    }
}
