//! Motion & transitions — Flutter's animation widget family, built on Pebbles'
//! animation hooks.
//!
//! Two flavors, matching Flutter:
//! * **Implicit** (`AnimatedOpacity`, `AnimatedScale`, `AnimatedRotation`,
//!   `AnimatedSlide`, `AnimatedAlign`, `AnimatedPadding`) — change a value and it
//!   tweens from the old one, no controller. Siblings of [`AnimatedContainer`](super::AnimatedContainer).
//! * **Explicit** (`FadeTransition`, `ScaleTransition`, `RotationTransition`,
//!   `SlideTransition`) — driven by a `Signal<f64>` you animate yourself (the
//!   Pebbles analog of Flutter's `Animation<double>`/`AnimationController`).
//!
//! Plus [`AnimatedSwitcher`] (cross-fade when the child changes) and
//! [`AnimatedCrossFade`] (cross-fade between two fixed children).

use std::f64::consts::TAU;

use pebbles_foundation::{Alignment, Axis, EdgeInsets, Offset, Rect};
use pebbles_render::{Border, BorderRadius, BorderSide, BoxDecoration, BoxShadow};

use std::rc::Rc;

use crate::theme::mix;
use crate::widgets::{
    Align, Container, DecoratedBox, GestureDetector, Opacity, Transform, clip_rrect, padding, positioned,
    stack,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    Curve, Element, Signal, action_event, animate_to, animated_with, component_props, create_signal,
    create_timeout,
};

/// Default implicit-animation duration (seconds), matching `AnimatedContainer`.
const DEFAULT_SECS: f64 = 0.25;

// ===========================================================================
// Implicit one-property animations (siblings of AnimatedContainer)
// ===========================================================================

macro_rules! implicit_builder {
    ($(#[$m:meta])* $Struct:ident, $ctor:ident, $field:ident : $ty:ty = $default:expr, $render:ident) => {
        $(#[$m])*
        #[derive(Clone)]
        pub struct $Struct {
            $field: $ty,
            duration: f64,
            curve: Curve,
            child: Option<AnyWidget>,
        }

        impl $Struct {
            /// Transition duration in seconds.
            pub fn duration(mut self, secs: f64) -> Self {
                self.duration = secs.max(0.0);
                self
            }
            /// Easing curve.
            pub fn curve(mut self, curve: Curve) -> Self {
                self.curve = curve;
                self
            }
        }

        impl IntoWidget for $Struct {
            fn into_widget(self) -> AnyWidget {
                component_props($render, self).into_widget()
            }
        }
    };
}

implicit_builder! {
    /// A child whose opacity animates implicitly on change (Flutter `AnimatedOpacity`).
    AnimatedOpacity, animated_opacity, opacity: f64 = 1.0, render_animated_opacity
}
/// Animate `child`'s opacity to `opacity` (`0..=1`) whenever it changes.
pub fn animated_opacity(opacity: f64, child: impl IntoWidget) -> AnimatedOpacity {
    AnimatedOpacity {
        opacity: opacity.clamp(0.0, 1.0),
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
    }
}
fn render_animated_opacity(b: &AnimatedOpacity) -> Element {
    let o = animated_with(b.opacity, b.duration, b.curve) as f32;
    with_child(b.child.clone(), move |c| Opacity::new(o, c).into_widget())
}

implicit_builder! {
    /// A child whose scale animates implicitly on change (Flutter `AnimatedScale`).
    AnimatedScale, animated_scale, scale: f64 = 1.0, render_animated_scale
}
/// Animate `child`'s scale to `scale` whenever it changes.
pub fn animated_scale(scale: f64, child: impl IntoWidget) -> AnimatedScale {
    AnimatedScale {
        scale,
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
    }
}
fn render_animated_scale(b: &AnimatedScale) -> Element {
    let s = animated_with(b.scale, b.duration, b.curve);
    with_child(b.child.clone(), move |c| Transform::scale(s, c).into_widget())
}

implicit_builder! {
    /// A child whose rotation (in **turns**, 1 = 360°) animates on change (Flutter `AnimatedRotation`).
    AnimatedRotation, animated_rotation, turns: f64 = 0.0, render_animated_rotation
}
/// Animate `child`'s rotation to `turns` (1.0 = a full turn) whenever it changes.
pub fn animated_rotation(turns: f64, child: impl IntoWidget) -> AnimatedRotation {
    AnimatedRotation {
        turns,
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
    }
}
fn render_animated_rotation(b: &AnimatedRotation) -> Element {
    let turns = animated_with(b.turns, b.duration, b.curve);
    with_child(b.child.clone(), move |c| Transform::rotate(turns * TAU, c).into_widget())
}

implicit_builder! {
    /// A child whose translation (logical px) animates on change (Flutter `AnimatedSlide`;
    /// Pebbles uses pixels, not child-size fractions).
    AnimatedSlide, animated_slide, offset: Offset = Offset::ZERO, render_animated_slide
}
/// Animate `child`'s translation to `(dx, dy)` logical px whenever it changes.
pub fn animated_slide(dx: f64, dy: f64, child: impl IntoWidget) -> AnimatedSlide {
    AnimatedSlide {
        offset: Offset::new(dx, dy),
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
    }
}
fn render_animated_slide(b: &AnimatedSlide) -> Element {
    let dx = animated_with(b.offset.x, b.duration, b.curve);
    let dy = animated_with(b.offset.y, b.duration, b.curve);
    with_child(b.child.clone(), move |c| Transform::translate(dx, dy, c).into_widget())
}

implicit_builder! {
    /// A child whose alignment animates implicitly on change (Flutter `AnimatedAlign`).
    AnimatedAlign, animated_align, alignment: Alignment = Alignment::CENTER, render_animated_align
}
/// Animate `child`'s alignment to `alignment` whenever it changes.
pub fn animated_align(alignment: Alignment, child: impl IntoWidget) -> AnimatedAlign {
    AnimatedAlign {
        alignment,
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
    }
}
fn render_animated_align(b: &AnimatedAlign) -> Element {
    let x = animated_with(b.alignment.x, b.duration, b.curve);
    let y = animated_with(b.alignment.y, b.duration, b.curve);
    with_child(b.child.clone(), move |c| Align::new(Alignment { x, y }, c).into_widget())
}

implicit_builder! {
    /// A child whose padding animates implicitly on change (Flutter `AnimatedPadding`).
    AnimatedPadding, animated_padding, insets: EdgeInsets = EdgeInsets::ZERO, render_animated_padding
}
/// Animate `child`'s padding to `insets` whenever it changes.
pub fn animated_padding(insets: EdgeInsets, child: impl IntoWidget) -> AnimatedPadding {
    AnimatedPadding {
        insets,
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
    }
}
fn render_animated_padding(b: &AnimatedPadding) -> Element {
    let (d, c) = (b.duration, b.curve);
    let insets = EdgeInsets {
        top: animated_with(b.insets.top, d, c),
        right: animated_with(b.insets.right, d, c),
        bottom: animated_with(b.insets.bottom, d, c),
        left: animated_with(b.insets.left, d, c),
    };
    with_child(b.child.clone(), move |ch| padding(insets, ch).into_widget())
}

// ===========================================================================
// Explicit transitions (driven by a Signal<f64> you animate yourself)
// ===========================================================================

/// Fade a child by an externally-driven `opacity` signal (Flutter `FadeTransition`).
#[derive(Clone)]
pub struct FadeTransition {
    value: Signal<f64>,
    child: Option<AnyWidget>,
}
/// Fade `child` by `opacity` (a `Signal<f64>` you animate, `0..=1`).
pub fn fade_transition(opacity: Signal<f64>, child: impl IntoWidget) -> FadeTransition {
    FadeTransition { value: opacity, child: Some(child.into_widget()) }
}
impl IntoWidget for FadeTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(
            |b: &FadeTransition| {
                let o = b.value.get().clamp(0.0, 1.0) as f32;
                with_child(b.child.clone(), move |c| Opacity::new(o, c).into_widget())
            },
            self,
        )
        .into_widget()
    }
}

/// Scale a child by an externally-driven `scale` signal (Flutter `ScaleTransition`).
#[derive(Clone)]
pub struct ScaleTransition {
    value: Signal<f64>,
    child: Option<AnyWidget>,
}
/// Scale `child` by `scale` (a `Signal<f64>` you animate).
pub fn scale_transition(scale: Signal<f64>, child: impl IntoWidget) -> ScaleTransition {
    ScaleTransition { value: scale, child: Some(child.into_widget()) }
}
impl IntoWidget for ScaleTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(
            |b: &ScaleTransition| {
                let s = b.value.get();
                with_child(b.child.clone(), move |c| Transform::scale(s, c).into_widget())
            },
            self,
        )
        .into_widget()
    }
}

/// Rotate a child by an externally-driven `turns` signal (Flutter `RotationTransition`).
#[derive(Clone)]
pub struct RotationTransition {
    value: Signal<f64>,
    child: Option<AnyWidget>,
}
/// Rotate `child` by `turns` (a `Signal<f64>` you animate; 1.0 = 360°).
pub fn rotation_transition(turns: Signal<f64>, child: impl IntoWidget) -> RotationTransition {
    RotationTransition { value: turns, child: Some(child.into_widget()) }
}
impl IntoWidget for RotationTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(
            |b: &RotationTransition| {
                let r = b.value.get() * TAU;
                with_child(b.child.clone(), move |c| Transform::rotate(r, c).into_widget())
            },
            self,
        )
        .into_widget()
    }
}

/// Slide a child by an externally-driven `offset` signal (Flutter `SlideTransition`;
/// Pebbles uses logical px, not child-size fractions).
#[derive(Clone)]
pub struct SlideTransition {
    value: Signal<Offset>,
    child: Option<AnyWidget>,
}
/// Translate `child` by `offset` (a `Signal<Offset>` you animate, in logical px).
pub fn slide_transition(offset: Signal<Offset>, child: impl IntoWidget) -> SlideTransition {
    SlideTransition { value: offset, child: Some(child.into_widget()) }
}
impl IntoWidget for SlideTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(
            |b: &SlideTransition| {
                let o = b.value.get();
                with_child(b.child.clone(), move |c| Transform::translate(o.x, o.y, c).into_widget())
            },
            self,
        )
        .into_widget()
    }
}

/// Reveal a child along one axis by an externally-driven `factor` signal (Flutter
/// `SizeTransition`): clips + sizes the child to `factor × natural size`.
#[derive(Clone)]
pub struct SizeTransition {
    value: Signal<f64>,
    axis: Axis,
    child: Option<AnyWidget>,
}
/// Reveal `child` vertically by `factor` (a `Signal<f64>`, `0..=1`). Use
/// [`axis`](SizeTransition::axis) for a horizontal reveal.
pub fn size_transition(factor: Signal<f64>, child: impl IntoWidget) -> SizeTransition {
    SizeTransition { value: factor, axis: Axis::Vertical, child: Some(child.into_widget()) }
}
impl SizeTransition {
    /// Reveal along this axis (default [`Axis::Vertical`]).
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
}
impl IntoWidget for SizeTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(render_size_transition, self).into_widget()
    }
}
fn render_size_transition(b: &SizeTransition) -> Element {
    let f = b.value.get().max(0.0);
    let child = b.child.clone().unwrap_or_else(|| Container::new().into_widget());
    let aligned = match b.axis {
        Axis::Vertical => Align::new(Alignment::CENTER, child).height_factor(f),
        Axis::Horizontal => Align::new(Alignment::CENTER, child).width_factor(f),
    };
    clip_rrect(BorderRadius::ZERO, aligned).into_widget()
}

/// Position + size a Stack child by an externally-driven `rect` signal, in window/
/// stack coordinates (Flutter `PositionedTransition`; `Rect` = left/top → right/bottom).
#[derive(Clone)]
pub struct PositionedTransition {
    value: Signal<Rect>,
    child: Option<AnyWidget>,
}
/// Drive a `Stack` child's rect from a `Signal<Rect>` you animate.
pub fn positioned_transition(rect: Signal<Rect>, child: impl IntoWidget) -> PositionedTransition {
    PositionedTransition { value: rect, child: Some(child.into_widget()) }
}
impl IntoWidget for PositionedTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(
            |b: &PositionedTransition| {
                let r = b.value.get();
                let child = b.child.clone().unwrap_or_else(|| Container::new().into_widget());
                positioned(child)
                    .left(r.x0)
                    .top(r.y0)
                    .width(r.width().max(0.0))
                    .height(r.height().max(0.0))
                    .into_widget()
            },
            self,
        )
        .into_widget()
    }
}

/// Cross-fade a child's `BoxDecoration` from `from` to `to` by an externally-driven
/// `t` signal (Flutter `DecoratedBoxTransition`). Color, radius, border and shadows
/// interpolate; gradient/image/shape/blend snap at the midpoint.
#[derive(Clone)]
pub struct DecoratedBoxTransition {
    from: BoxDecoration,
    to: BoxDecoration,
    value: Signal<f64>,
    child: Option<AnyWidget>,
}
/// Animate `child`'s decoration between `from` and `to` by `t` (a `Signal<f64>`, `0..=1`).
pub fn decorated_box_transition(
    from: BoxDecoration,
    to: BoxDecoration,
    t: Signal<f64>,
    child: impl IntoWidget,
) -> DecoratedBoxTransition {
    DecoratedBoxTransition { from, to, value: t, child: Some(child.into_widget()) }
}
impl IntoWidget for DecoratedBoxTransition {
    fn into_widget(self) -> AnyWidget {
        component_props(render_decorated_box_transition, self).into_widget()
    }
}
fn render_decorated_box_transition(b: &DecoratedBoxTransition) -> Element {
    let t = b.value.get().clamp(0.0, 1.0);
    let dec = lerp_decoration(&b.from, &b.to, t);
    let child = b.child.clone().unwrap_or_else(|| Container::new().into_widget());
    DecoratedBox::new(dec, child).into_widget()
}

fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
fn lerp_color(
    a: pebbles_foundation::Color,
    b: pebbles_foundation::Color,
    t: f64,
) -> pebbles_foundation::Color {
    mix(a, b, t as f32)
}
fn lerp_radius(a: BorderRadius, b: BorderRadius, t: f64) -> BorderRadius {
    BorderRadius {
        top_left: lerp_f64(a.top_left, b.top_left, t),
        top_right: lerp_f64(a.top_right, b.top_right, t),
        bottom_right: lerp_f64(a.bottom_right, b.bottom_right, t),
        bottom_left: lerp_f64(a.bottom_left, b.bottom_left, t),
    }
}
fn lerp_side(a: BorderSide, b: BorderSide, t: f64) -> BorderSide {
    BorderSide { color: lerp_color(a.color, b.color, t), width: lerp_f64(a.width, b.width, t) }
}
fn lerp_shadow(a: &BoxShadow, b: &BoxShadow, t: f64) -> BoxShadow {
    BoxShadow {
        color: lerp_color(a.color, b.color, t),
        offset: Offset::new(lerp_f64(a.offset.x, b.offset.x, t), lerp_f64(a.offset.y, b.offset.y, t)),
        blur: lerp_f64(a.blur, b.blur, t),
        spread: lerp_f64(a.spread, b.spread, t),
    }
}
/// Interpolate the animatable parts of a decoration; snap the rest at the midpoint.
fn lerp_decoration(a: &BoxDecoration, b: &BoxDecoration, t: f64) -> BoxDecoration {
    let snap = if t < 0.5 { a } else { b };
    BoxDecoration {
        color: match (a.color, b.color) {
            (Some(x), Some(y)) => Some(lerp_color(x, y, t)),
            (x, y) => y.or(x),
        },
        radius: lerp_radius(a.radius, b.radius, t),
        border: match (a.border, b.border) {
            (Some(x), Some(y)) => Some(Border {
                top: lerp_side(x.top, y.top, t),
                right: lerp_side(x.right, y.right, t),
                bottom: lerp_side(x.bottom, y.bottom, t),
                left: lerp_side(x.left, y.left, t),
            }),
            (x, y) => y.or(x),
        },
        shadows: if a.shadows.len() == b.shadows.len() {
            a.shadows.iter().zip(&b.shadows).map(|(x, y)| lerp_shadow(x, y, t)).collect()
        } else {
            snap.shadows.clone()
        },
        // Non-scalar / rare fields: snap at the midpoint.
        gradient: snap.gradient.clone(),
        shape: snap.shape,
        image: snap.image.clone(),
        image_fit: snap.image_fit,
        blend: snap.blend,
    }
}

// ===========================================================================
// AnimatedCrossFade — cross-fade between two fixed children
// ===========================================================================

/// Cross-fade between `first` and `second` based on a `bool` (Flutter `AnimatedCrossFade`).
#[derive(Clone)]
pub struct AnimatedCrossFade {
    first: Option<AnyWidget>,
    second: Option<AnyWidget>,
    show_second: bool,
    duration: f64,
    curve: Curve,
}
/// Cross-fade between two children; `show_second` picks which is fully visible.
pub fn animated_cross_fade(
    first: impl IntoWidget,
    second: impl IntoWidget,
    show_second: bool,
) -> AnimatedCrossFade {
    AnimatedCrossFade {
        first: Some(first.into_widget()),
        second: Some(second.into_widget()),
        show_second,
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
    }
}
impl AnimatedCrossFade {
    /// Transition duration in seconds.
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
    /// Easing curve.
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }
}
impl IntoWidget for AnimatedCrossFade {
    fn into_widget(self) -> AnyWidget {
        component_props(render_cross_fade, self).into_widget()
    }
}
fn render_cross_fade(b: &AnimatedCrossFade) -> Element {
    let t = animated_with(if b.show_second { 1.0 } else { 0.0 }, b.duration, b.curve);
    let first = b.first.clone().unwrap_or_else(|| Container::new().into_widget());
    let second = b.second.clone().unwrap_or_else(|| Container::new().into_widget());
    // Both stay laid out (Stack sizes to the larger); opacity cross-fades.
    stack(pebbles_core::children![Opacity::new((1.0 - t) as f32, first), Opacity::new(t as f32, second),])
        .into_widget()
}

// ===========================================================================
// AnimatedSwitcher — cross-fade whenever the child (keyed) changes
// ===========================================================================

/// Cross-fade to a new child whenever `key` changes (Flutter `AnimatedSwitcher`).
/// The outgoing child fades out while the incoming fades in.
#[derive(Clone)]
pub struct AnimatedSwitcher {
    key: u64,
    child: Option<AnyWidget>,
    duration: f64,
    curve: Curve,
}
/// Cross-fade to `child` whenever `key` changes (give each distinct content a
/// distinct `key`).
pub fn animated_switcher(key: u64, child: impl IntoWidget) -> AnimatedSwitcher {
    AnimatedSwitcher {
        key,
        child: Some(child.into_widget()),
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
    }
}
impl AnimatedSwitcher {
    /// Transition duration in seconds.
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
    /// Easing curve.
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }
}
impl IntoWidget for AnimatedSwitcher {
    fn into_widget(self) -> AnyWidget {
        component_props(render_switcher, self).into_widget()
    }
}
fn render_switcher(b: &AnimatedSwitcher) -> Element {
    let new_child = b.child.clone().unwrap_or_else(|| Container::new().into_widget());
    // Position-stable per-instance state: current + outgoing child, and progress.
    let cur_key = create_signal(b.key);
    let cur = create_signal(new_child.clone());
    let prev = create_signal::<Option<AnyWidget>>(None);
    let progress = create_signal(1.0_f64);

    if cur_key.peek() != b.key {
        // Key changed → the current becomes outgoing, the new becomes current.
        prev.set(Some(cur.peek()));
        cur.set(new_child);
        cur_key.set(b.key);
        progress.set(0.0);
        animate_to(progress, 1.0, b.duration);
    }
    let t = progress.get();
    // Drop the outgoing child once it has faded out (one extra frame; harmless).
    if t >= 0.999 && prev.peek().is_some() {
        prev.set(None);
    }

    let mut layers: Vec<AnyWidget> = Vec::new();
    if let Some(old) = prev.get() {
        layers.push(Opacity::new((1.0 - t) as f32, old).into_widget());
    }
    layers.push(Opacity::new(t as f32, cur.get()).into_widget());
    stack(layers).into_widget()
}

// ===========================================================================
// AnimatedPositioned — a Stack child whose position/size animate on change
// ===========================================================================

/// A [`Stack`](crate::widgets::Stack) child whose `left`/`top`/`right`/`bottom`/
/// `width`/`height` animate implicitly on change (Flutter `AnimatedPositioned`).
/// Use inside a `Stack`, like `positioned`.
#[derive(Clone, Default)]
pub struct AnimatedPositioned {
    left: Option<f64>,
    top: Option<f64>,
    right: Option<f64>,
    bottom: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    duration: f64,
    curve: Curve,
    child: Option<AnyWidget>,
}
/// A `Stack` child that animates to its target edges/size whenever they change.
pub fn animated_positioned(child: impl IntoWidget) -> AnimatedPositioned {
    AnimatedPositioned {
        duration: DEFAULT_SECS,
        curve: Curve::EaseOutCubic,
        child: Some(child.into_widget()),
        ..Default::default()
    }
}
impl AnimatedPositioned {
    /// Animated distance from the stack's left edge.
    pub fn left(mut self, v: f64) -> Self {
        self.left = Some(v);
        self
    }
    /// Animated distance from the stack's top edge.
    pub fn top(mut self, v: f64) -> Self {
        self.top = Some(v);
        self
    }
    /// Animated distance from the stack's right edge.
    pub fn right(mut self, v: f64) -> Self {
        self.right = Some(v);
        self
    }
    /// Animated distance from the stack's bottom edge.
    pub fn bottom(mut self, v: f64) -> Self {
        self.bottom = Some(v);
        self
    }
    /// Animated width.
    pub fn width(mut self, v: f64) -> Self {
        self.width = Some(v);
        self
    }
    /// Animated height.
    pub fn height(mut self, v: f64) -> Self {
        self.height = Some(v);
        self
    }
    /// Transition duration in seconds.
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
    /// Easing curve.
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }
}
impl IntoWidget for AnimatedPositioned {
    fn into_widget(self) -> AnyWidget {
        component_props(render_animated_positioned, self).into_widget()
    }
}
fn render_animated_positioned(b: &AnimatedPositioned) -> Element {
    let (d, c) = (b.duration, b.curve);
    let child = b.child.clone().unwrap_or_else(|| Container::new().into_widget());
    let mut p = positioned(child);
    if let Some(v) = b.left {
        p = p.left(animated_with(v, d, c));
    }
    if let Some(v) = b.top {
        p = p.top(animated_with(v, d, c));
    }
    if let Some(v) = b.right {
        p = p.right(animated_with(v, d, c));
    }
    if let Some(v) = b.bottom {
        p = p.bottom(animated_with(v, d, c));
    }
    if let Some(v) = b.width {
        p = p.width(animated_with(v, d, c));
    }
    if let Some(v) = b.height {
        p = p.height(animated_with(v, d, c));
    }
    p.into_widget()
}

// ===========================================================================
// Dismissible — swipe horizontally to dismiss (Flutter `Dismissible`)
// ===========================================================================

/// Swipe a child horizontally past a threshold to dismiss it; it slides off and
/// fades, then fires `on_dismissed` (where you remove the item from your data).
/// Flutter's `Dismissible`.
#[derive(Clone)]
pub struct Dismissible {
    child: Option<AnyWidget>,
    on_dismissed: Option<Rc<dyn Fn()>>,
    threshold: f64,
    duration: f64,
}
/// Make `child` swipe-to-dismiss; `on_dismissed` runs after it slides away.
pub fn dismissible(child: impl IntoWidget, on_dismissed: impl Fn() + 'static) -> Dismissible {
    Dismissible {
        child: Some(child.into_widget()),
        on_dismissed: Some(Rc::new(on_dismissed)),
        threshold: 96.0,
        duration: 0.2,
    }
}
impl Dismissible {
    /// Horizontal distance (logical px) past which a release dismisses (default 96).
    pub fn threshold(mut self, px: f64) -> Self {
        self.threshold = px.max(1.0);
        self
    }
    /// Slide-off duration in seconds.
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
}
impl IntoWidget for Dismissible {
    fn into_widget(self) -> AnyWidget {
        component_props(render_dismissible, self).into_widget()
    }
}
fn render_dismissible(b: &Dismissible) -> Element {
    let dx = create_signal(0.0_f64); // live horizontal offset
    let start = create_signal(0.0_f64); // pointer x at pan-start
    let gone = create_signal(false); // dismissed → collapse away
    let threshold = b.threshold;
    let duration = b.duration;
    let on_dismissed = b.on_dismissed.clone();

    let offset = dx.get();
    // Fade proportionally to how far it's been dragged (fully gone by ~3× threshold).
    let opacity = (1.0 - offset.abs() / (threshold * 3.0)).clamp(0.0, 1.0) as f32;
    let child = b.child.clone().unwrap_or_else(|| Container::new().into_widget());
    let content = Transform::translate(offset, 0.0, Opacity::new(opacity, child)).into_widget();

    if gone.get() {
        // After dismissal, render nothing until the parent drops us.
        return Container::new().into_widget();
    }

    let s_start = start;
    let d_up = dx;
    let s_read = start;
    GestureDetector::new(content)
        .on_pan_start(action_event(move |e| s_start.set(e.position.x)))
        .on_pan_update(action_event(move |e| d_up.set(e.position.x - s_read.peek())))
        .on_pan_end({
            let on_dismissed = on_dismissed.clone();
            move || {
                let cur = dx.peek();
                if cur.abs() >= threshold {
                    // Fling off-screen in the swipe direction, then notify.
                    animate_to(dx, cur.signum() * 1200.0, duration);
                    let cb = on_dismissed.clone();
                    create_timeout(duration, move || {
                        gone.set(true);
                        if let Some(f) = &cb {
                            f();
                        }
                    });
                } else {
                    animate_to(dx, 0.0, 0.18); // snap back
                }
            }
        })
        .into_widget()
}

// ===========================================================================
// helpers
// ===========================================================================

/// Wrap an optional child (or an empty box) with `f`. Keeps the render fns terse.
fn with_child(child: Option<AnyWidget>, f: impl FnOnce(AnyWidget) -> AnyWidget) -> Element {
    let c = child.unwrap_or_else(|| Container::new().into_widget());
    f(c).into_widget()
}
