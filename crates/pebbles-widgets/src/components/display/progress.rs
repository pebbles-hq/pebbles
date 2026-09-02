//! [`Progress`] — a linear progress bar in the shadcn style: a rounded `muted`
//! track with a `primary` indicator. Determinate by default (a `value` over a
//! `max`), or [`indeterminate`](Progress::indeterminate) for an animated
//! unknown-duration bar. (The draggable value slider lives in the `input` group as
//! [`Slider`](crate::components::Slider).)

use pebbles_foundation::{Alignment, Color};
use pebbles_render::{BorderRadius, BoxDecoration};

use crate::theme::theme;
use crate::widgets::{ClipRRect, Container, Positioned, stack};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{children, component_props, create_loop};

/// A linear progress bar.
pub struct Progress {
    value: f64,
    max: f64,
    width: f64,
    thickness: f64,
    color: Option<Color>,
    indeterminate: bool,
}

/// Create a [`Progress`] bar of the given width. `value` is a fraction (`0.0..=1.0`)
/// by default; call [`max`](Progress::max) to use another domain (e.g. `0..=100`).
pub fn progress(value: f64, width: f64) -> Progress {
    Progress { value, max: 1.0, width, thickness: 8.0, color: None, indeterminate: false }
}

impl Progress {
    /// Upper bound of the value domain (default `1.0`).
    pub fn max(mut self, max: f64) -> Self {
        self.max = max.max(1e-9);
        self
    }
    /// Bar thickness (default `8`).
    pub fn thickness(mut self, t: f64) -> Self {
        self.thickness = t;
        self
    }
    /// Custom indicator color (defaults to the theme primary).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Animate an unknown-duration bar (a segment sweeps across the track).
    pub fn indeterminate(mut self) -> Self {
        self.indeterminate = true;
        self
    }
}

impl IntoWidget for Progress {
    fn into_widget(self) -> AnyWidget {
        component_props(render_progress, self).into_widget()
    }
}

fn render_progress(p: &Progress) -> AnyWidget {
    let c = theme().colors;
    let color = p.color.unwrap_or(c.primary);
    let radius = BorderRadius::all(999.0);
    let track = || {
        Container::new()
            .decoration(BoxDecoration::new().color(c.muted).radius(radius))
            .width(p.width)
            .height(p.thickness)
    };

    if p.indeterminate {
        // A ~40%-wide segment sweeps left→right, clipped to the track.
        let phase = create_loop(1.4).get();
        let seg = p.width * 0.4;
        let x = -seg + (p.width + seg) * phase;
        let bar = Container::new()
            .decoration(BoxDecoration::new().color(color).radius(radius))
            .width(seg)
            .height(p.thickness);
        let bar = track().child(ClipRRect::new(
            radius,
            stack(children![Positioned::new(bar).left(x).top(0.0)]),
        ));
        // C7: an indeterminate ProgressBar (no value).
        return crate::widgets::semantics(pebbles_render::SemanticsRole::ProgressBar, "", bar)
            .into_widget();
    }

    let frac = (p.value / p.max).clamp(0.0, 1.0);
    let pct = (frac * 100.0).round() as i64;
    let bar = track().alignment(Alignment::CENTER_LEFT).child(
        Container::new()
            .decoration(BoxDecoration::new().color(color).radius(radius))
            .width(p.width * frac)
            .height(p.thickness),
    );
    // C7: a determinate ProgressBar announced with its percentage.
    crate::widgets::semantics(pebbles_render::SemanticsRole::ProgressBar, "", bar)
        .value(format!("{pct}%"))
        .into_widget()
}
