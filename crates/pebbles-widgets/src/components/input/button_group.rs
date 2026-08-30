//! [`ButtonGroup`] — shadcn's **button group**: a set of [`Button`]s joined into a
//! single segmented control. Adjacent buttons share a hairline divider and the
//! group's outer corners are rounded while the inner ones stay square (the group
//! clips them). Horizontal or vertical; pass a `spacing` to detach them into a
//! plain toolbar instead.

use pebbles_foundation::Axis;
use pebbles_render::{Border, BorderRadius, BoxDecoration};

use super::button::Button;
use crate::theme::theme;
use crate::widgets::{Container, column, row};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A joined group of buttons. Build with [`button_group`].
pub struct ButtonGroup {
    buttons: Vec<Button>,
    orientation: Axis,
    spacing: f64,
}

/// Create a [`ButtonGroup`] from a list of [`Button`]s.
pub fn button_group(buttons: Vec<Button>) -> ButtonGroup {
    ButtonGroup { buttons, orientation: Axis::Horizontal, spacing: 0.0 }
}

impl ButtonGroup {
    /// Stack the buttons vertically instead of horizontally.
    pub fn orientation(mut self, axis: Axis) -> Self {
        self.orientation = axis;
        self
    }
    /// Detach the buttons into a spaced toolbar (default `0` = joined/segmented).
    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }
}

impl IntoWidget for ButtonGroup {
    fn into_widget(self) -> AnyWidget {
        let th = theme();
        let horiz = self.orientation == Axis::Horizontal;

        // Detached toolbar: keep each button's own shape, just space them out.
        if self.spacing > 0.0 {
            let kids: Vec<AnyWidget> = self.buttons.into_iter().map(IntoWidget::into_widget).collect();
            let line: AnyWidget = if horiz {
                row(kids).main_axis_min().spacing(self.spacing).into_widget()
            } else {
                column(kids).main_axis_min().spacing(self.spacing).into_widget()
            };
            return line;
        }

        // Joined segmented control: flatten each button's radius, divide with a
        // hairline, and clip the whole strip to a rounded frame.
        let n = self.buttons.len();
        let mut kids: Vec<AnyWidget> = Vec::with_capacity(n * 2);
        for (i, b) in self.buttons.into_iter().enumerate() {
            kids.push(b.radius(0.0).into_widget());
            if i + 1 < n {
                let mut div = Container::new().color(th.colors.border);
                div = if horiz { div.width(1.0) } else { div.height(1.0) };
                kids.push(div.into_widget());
            }
        }
        let line: AnyWidget = if horiz {
            row(kids)
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Stretch)
                .main_axis_min()
                .into_widget()
        } else {
            column(kids)
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Stretch)
                .main_axis_min()
                .into_widget()
        };
        Container::new()
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(th.colors.border, 1.0))
                    .radius(BorderRadius::all(th.radius)),
            )
            .clip()
            .child(line)
            .into_widget()
    }
}
