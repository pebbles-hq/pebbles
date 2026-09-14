//! [`AnimatedContainer`] — a [`Container`] whose width, height, color, corner
//! radius, padding, margin and opacity **animate implicitly** whenever their
//! values change: rebuild the widget with a new value and it tweens from the old
//! one instead of jumping. Flutter's `AnimatedContainer`.
//!
//! Built on the animation hooks: each animatable property is its own
//! [`animated_with`] signal pair (scalars) or start→target tween (colors,
//! insets, radii), so re-rendering during a transition never restarts it.

use pebbles_foundation::{Color, EdgeInsets};
use pebbles_render::BorderRadius;

use crate::theme::mix;
use crate::widgets::{Container, Opacity};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Curve, animate_to_with, animated_with, component_props, create_signal};

/// A container whose box properties animate implicitly on change.
#[derive(Clone, Default)]
pub struct AnimatedContainer {
    width: Option<f64>,
    height: Option<f64>,
    color: Option<Color>,
    radius: BorderRadius,
    padding: EdgeInsets,
    margin: EdgeInsets,
    opacity: f64,
    /// Transition duration in seconds (default `0.25`).
    duration: f64,
    /// Easing curve (default [`Curve::EaseOutCubic`]).
    curve: Curve,
    child: Option<AnyWidget>,
}

/// Create an empty [`AnimatedContainer`] and compose it with the builder methods.
pub fn animated_container() -> AnimatedContainer {
    AnimatedContainer { duration: 0.25, curve: Curve::EaseOutCubic, opacity: 1.0, ..Default::default() }
}

impl AnimatedContainer {
    /// Animated target width.
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    /// Animated target height.
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
    /// Animated background color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Animated corner radius.
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }
    /// Animated inner padding.
    pub fn padding(mut self, insets: EdgeInsets) -> Self {
        self.padding = insets;
        self
    }
    /// Animated outer margin.
    pub fn margin(mut self, insets: EdgeInsets) -> Self {
        self.margin = insets;
        self
    }
    /// Animated opacity (`0..=1`).
    pub fn opacity(mut self, opacity: f64) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }
    /// Transition duration in seconds.
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
    /// Easing curve for every animating property.
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }
    /// The content inside the container.
    pub fn child(mut self, child: impl IntoWidget) -> Self {
        self.child = Some(child.into_widget());
        self
    }
}

fn colors_eq(a: Color, b: Color) -> bool {
    a.components == b.components
}

/// Animate `current` (an optional color) toward its latest value: restarts a
/// 0→1 progress tween each time the value changes and lerps from the previous
/// target. All hooks are created unconditionally so positions stay stable.
fn anim_color(current: Option<Color>, duration: f64, curve: Curve) -> Option<Color> {
    let start = create_signal(current);
    let target = create_signal(current);
    let progress = create_signal(1.0_f64);
    match (start.peek(), target.peek(), current) {
        (_, Some(old), Some(new)) if !colors_eq(old, new) => {
            start.set(Some(old));
            target.set(Some(new));
            progress.set(0.0);
            animate_to_with(progress, 1.0, duration, curve);
        }
        (_, None, Some(new)) => {
            start.set(None);
            target.set(Some(new));
            progress.set(0.0);
            animate_to_with(progress, 1.0, duration, curve);
        }
        _ => {}
    }
    let (s, t) = (start.get(), target.get());
    match (s, t) {
        (Some(a), Some(b)) => Some(mix(a, b, progress.get() as f32)),
        (None, Some(b)) => Some(b),
        _ => None,
    }
}

/// Animate an [`EdgeInsets`] toward its latest value (each edge tweens).
fn anim_insets(current: EdgeInsets, duration: f64, curve: Curve) -> EdgeInsets {
    let top = animated_with(current.top, duration, curve);
    let right = animated_with(current.right, duration, curve);
    let bottom = animated_with(current.bottom, duration, curve);
    let left = animated_with(current.left, duration, curve);
    EdgeInsets { top, right, bottom, left }
}

/// Animate a [`BorderRadius`] toward its latest value (each corner tweens).
fn anim_radius(current: BorderRadius, duration: f64, curve: Curve) -> BorderRadius {
    let tl = animated_with(current.top_left, duration, curve);
    let tr = animated_with(current.top_right, duration, curve);
    let br = animated_with(current.bottom_right, duration, curve);
    let bl = animated_with(current.bottom_left, duration, curve);
    BorderRadius { top_left: tl, top_right: tr, bottom_right: br, bottom_left: bl }
}

impl IntoWidget for AnimatedContainer {
    fn into_widget(self) -> AnyWidget {
        component_props(render_animated_container, self).into_widget()
    }
}

fn render_animated_container(b: &AnimatedContainer) -> pebbles_core::Element {
    let duration = b.duration;
    let curve = b.curve;
    let width = b.width.map(|w| animated_with(w, duration, curve));
    let height = b.height.map(|h| animated_with(h, duration, curve));
    let color = anim_color(b.color, duration, curve);
    let radius = anim_radius(b.radius, duration, curve);
    let padding = anim_insets(b.padding, duration, curve);
    let margin = anim_insets(b.margin, duration, curve);
    let opacity = animated_with(b.opacity, duration, curve);

    let mut container = Container::new();
    if let Some(w) = width {
        container = container.width(w);
    }
    if let Some(h) = height {
        container = container.height(h);
    }
    if let Some(c) = color {
        container = container.color(c);
    }
    if radius != BorderRadius::ZERO {
        container = container.radius(radius);
    }
    if padding != EdgeInsets::ZERO {
        container = container.padding(padding);
    }
    if margin != EdgeInsets::ZERO {
        container = container.margin(margin);
    }
    if let Some(child) = &b.child {
        container = container.child(child.clone());
    }
    let boxed: AnyWidget = container.into_widget();
    if opacity < 1.0 - 1e-9 { Opacity::new(opacity as f32, boxed).into_widget() } else { boxed }
}
