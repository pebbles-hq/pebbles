//! [`SplitView`] — two resizable panes separated by a divider. The split `ratio`
//! is controlled (0.0..=1.0). Live drag-to-resize needs pointer-position callbacks
//! (a planned enhancement); for now set the ratio from your own state.

use pebbles_foundation::{Axis, CrossAxisAlignment};

use pebbles_core::children;
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{Container, Expanded, column, row};

/// A two-pane split.
#[derive(Clone)]
pub struct SplitView {
    axis: Axis,
    ratio: f64,
    first: Option<AnyWidget>,
    second: Option<AnyWidget>,
}

/// A horizontal split (side-by-side).
pub fn split_view(first: impl IntoWidget, second: impl IntoWidget) -> SplitView {
    SplitView {
        axis: Axis::Horizontal,
        ratio: 0.5,
        first: Some(first.into_widget()),
        second: Some(second.into_widget()),
    }
}

impl SplitView {
    /// A vertical split (stacked).
    pub fn vertical(first: impl IntoWidget, second: impl IntoWidget) -> Self {
        SplitView {
            axis: Axis::Vertical,
            ratio: 0.5,
            first: Some(first.into_widget()),
            second: Some(second.into_widget()),
        }
    }
    /// Set the first pane's fraction of the space (0.0..=1.0).
    pub fn ratio(mut self, ratio: f64) -> Self {
        self.ratio = ratio.clamp(0.05, 0.95);
        self
    }
}


impl IntoWidget for SplitView {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        // Proportional split via integer flex factors.
        let a = ((self.ratio * 1000.0) as u32).max(1);
        let b = (1000 - a as i64).max(1) as u32;
        let first = self.first.take().unwrap();
        let second = self.second.take().unwrap();

        match self.axis {
            Axis::Horizontal => {
                let divider = Container::new().color(c.border).width(1.0);
                row(children![
                    Expanded::new(first).flex(a),
                    divider,
                    Expanded::new(second).flex(b),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .into_widget()
            }
            Axis::Vertical => {
                let divider = Container::new().color(c.border).height(1.0);
                column(children![
                    Expanded::new(first).flex(a),
                    divider,
                    Expanded::new(second).flex(b),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .into_widget()
            }
        }
    }
}
