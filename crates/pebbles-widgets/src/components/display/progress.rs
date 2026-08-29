//! [`Progress`] — a linear determinate progress bar. (The draggable value slider
//! lives in the `input` group as [`Slider`](crate::components::Slider).)

use pebbles_foundation::Alignment;
use pebbles_render::{BorderRadius, BoxDecoration};

use crate::theme::theme;
use crate::widgets::Container;
use pebbles_core::context::BuildContext;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};

/// A determinate linear progress bar. `value` is clamped to `0.0..=1.0`.
#[derive(Clone)]
pub struct Progress {
    value: f64,
    width: f64,
}

/// Create a [`Progress`] bar of the given width.
pub fn progress(value: f64, width: f64) -> Progress {
    Progress { value: value.clamp(0.0, 1.0), width }
}

pebbles_core::stateless_widget!(Progress);

impl StatelessWidget for Progress {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        let track = Container::new()
            .decoration(BoxDecoration::new().color(c.muted).radius(BorderRadius::all(999.0)))
            .width(self.width)
            .height(8.0)
            .alignment(Alignment::CENTER_LEFT)
            .child(
                Container::new()
                    .decoration(BoxDecoration::new().color(c.primary).radius(BorderRadius::all(999.0)))
                    .width(self.width * self.value)
                    .height(8.0),
            );
        track.into_widget()
    }
}
