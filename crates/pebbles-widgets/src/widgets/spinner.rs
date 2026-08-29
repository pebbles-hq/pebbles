//! [`Spinner`] — a circular indeterminate loading indicator. A function component
//! that spins a [`pebbles_render::RenderSpinner`] arc via a looping animation
//! (`create_loop`), so it runs on its own until unmounted. Used by loading buttons,
//! but generally useful anywhere.

use std::f64::consts::TAU;

use pebbles_foundation::Color;
use pebbles_render::{RenderObject, RenderSpinner};

use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};
use pebbles_core::{component_props, create_loop};

// ---------------------------------------------------------------------------
// Low-level render widget.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SpinnerView {
    angle: f64,
    color: Color,
    diameter: f64,
}

impl SpinnerView {
    fn make(&self) -> RenderSpinner {
        let mut r = RenderSpinner::new(self.diameter, self.color);
        r.angle = self.angle;
        r
    }
}

pebbles_core::render_widget!(SpinnerView);

impl RenderWidget for SpinnerView {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(self.make())
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderSpinner>() {
            *r = self.make();
        }
    }
}

// ---------------------------------------------------------------------------
// Public component.
// ---------------------------------------------------------------------------

/// A circular loading spinner.
pub struct Spinner {
    diameter: f64,
    color: Option<Color>,
    period: f64,
}

/// Create a [`Spinner`] of the given diameter.
pub fn spinner(diameter: f64) -> Spinner {
    Spinner { diameter, color: None, period: 0.9 }
}

impl Spinner {
    /// The arc color (defaults to the theme's primary).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Seconds per rotation (default `0.9`).
    pub fn period(mut self, period: f64) -> Self {
        self.period = period;
        self
    }
}

struct Props {
    diameter: f64,
    color: Option<Color>,
    period: f64,
}

impl IntoWidget for Spinner {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_spinner,
            Props { diameter: self.diameter, color: self.color, period: self.period },
        )
        .into_widget()
    }
}

fn render_spinner(p: &Props) -> SpinnerView {
    let phase = create_loop(p.period);
    let color = p.color.unwrap_or_else(|| theme().colors.primary);
    SpinnerView { angle: phase.get() * TAU, color, diameter: p.diameter }
}
