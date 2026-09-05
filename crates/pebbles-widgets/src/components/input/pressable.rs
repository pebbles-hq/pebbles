//! Pebbles' answer to Flutter's **InkWell / InkResponse / Ink** — a tappable region
//! with shadcn-style feedback (hover tint, press tint, focus ring, pointer cursor,
//! keyboard activation), **minus the Material ripple** (decided-out, WIDGETS.md §10).
//! Interactivity is signalled by an animated background tint + focus ring.
//!
//! One engine ([`Pressable`]) backs four constructors:
//! - [`pressable`] — the idiomatic name (rectangular, bounded).
//! - [`ink_well`] — Flutter's `InkWell` (rectangular, bounded) — an alias for migrants.
//! - [`ink_response`] — Flutter's `InkResponse` (circular highlight by default).
//! - [`ink`] — Flutter's `Ink`: a decorated surface the tint draws *over* (see [`Ink`]).

use std::rc::Rc;

use pebbles_core::IntoCallback;
use pebbles_foundation::{Color, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShape, Cursor};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Opacity};
use pebbles_core::animated;
use pebbles_core::component::{Element, component_props};
use pebbles_core::context::Callback;
use pebbles_core::focus::create_focus;
use pebbles_core::reactive::create_signal;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// The highlight shape of a [`Pressable`] — Flutter's `highlightShape`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InkShape {
    /// A (rounded) rectangle bounded to the box — Flutter's `InkWell`.
    #[default]
    Rectangle,
    /// A circle centered on the box — Flutter's `InkResponse` default.
    Circle,
}

/// A tappable region wrapping `child`. Build with [`pressable`] / [`ink_well`] /
/// [`ink_response`].
#[derive(Clone)]
pub struct Pressable {
    child: AnyWidget,
    on_tap: Option<Callback>,
    on_long_press: Option<Callback>,
    on_double_tap: Option<Callback>,
    on_secondary_tap: Option<Callback>,
    on_hover: Option<Rc<dyn Fn(bool)>>,
    on_highlight_changed: Option<Rc<dyn Fn(bool)>>,
    on_focus_change: Option<Rc<dyn Fn(bool)>>,
    radius: Option<f64>,
    hover_tint: Option<Color>,
    shape: InkShape,
    disabled: bool,
    label: String,
    autofocus: bool,
}

fn make(child: AnyWidget, shape: InkShape) -> Pressable {
    Pressable {
        child,
        on_tap: None,
        on_long_press: None,
        on_double_tap: None,
        on_secondary_tap: None,
        on_hover: None,
        on_highlight_changed: None,
        on_focus_change: None,
        radius: None,
        hover_tint: None,
        shape,
        disabled: false,
        label: String::new(),
        autofocus: false,
    }
}

/// Wrap `child` in a [`Pressable`] — tappable, with hover/press feedback (rectangular).
pub fn pressable(child: impl IntoWidget) -> Pressable {
    make(child.into_widget(), InkShape::Rectangle)
}

/// Flutter's `InkWell`: a bounded, rectangular tappable region — an alias for
/// [`pressable`] so code migrating from Flutter reads naturally.
pub fn ink_well(child: impl IntoWidget) -> Pressable {
    make(child.into_widget(), InkShape::Rectangle)
}

/// Flutter's `InkResponse`: like [`ink_well`] but the highlight is a **circle** by
/// default (switch with [`Pressable::shape`]) — the icon-button / avatar tap idiom.
pub fn ink_response(child: impl IntoWidget) -> Pressable {
    make(child.into_widget(), InkShape::Circle)
}

impl Pressable {
    /// The tap handler (also the Space/Enter keyboard activation).
    pub fn on_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_tap = Some(cb.into_callback());
        self
    }
    /// A long-press handler (Flutter's `onLongPress`).
    pub fn on_long_press(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press = Some(cb.into_callback());
        self
    }
    /// A double-tap handler (Flutter's `onDoubleTap`).
    pub fn on_double_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_double_tap = Some(cb.into_callback());
        self
    }
    /// A secondary (right-click) tap handler (Flutter's `onSecondaryTap`).
    pub fn on_secondary_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap = Some(cb.into_callback());
        self
    }
    /// Called with the hover state as the pointer enters/leaves (Flutter's `onHover`).
    pub fn on_hover(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_hover = Some(Rc::new(f));
        self
    }
    /// Called with the highlight (pressed) state (Flutter's `onHighlightChanged`).
    pub fn on_highlight_changed(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_highlight_changed = Some(Rc::new(f));
        self
    }
    /// Called with the focus state when it changes (Flutter's `onFocusChange`).
    pub fn on_focus_change(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus_change = Some(Rc::new(f));
        self
    }
    /// The highlight shape (default [`InkShape::Rectangle`]; `ink_response` defaults to
    /// [`InkShape::Circle`]).
    pub fn shape(mut self, shape: InkShape) -> Self {
        self.shape = shape;
        self
    }
    /// Corner radius of the rectangular tint/focus-ring surface (default: the theme
    /// radius; ignored for a circle).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = Some(radius);
        self
    }
    /// The hover/press tint color (default: the theme accent), applied as a low-alpha
    /// overlay behind the child.
    pub fn hover_tint(mut self, color: Color) -> Self {
        self.hover_tint = Some(color);
        self
    }
    /// Dim and disable the region (no tap, no feedback, `NotAllowed` cursor).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// An accessibility label announced for the region (it takes the `Button` role).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }
    /// Focus this region on mount (keyboard-first flows).
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
}

impl IntoWidget for Pressable {
    fn into_widget(self) -> AnyWidget {
        component_props(render_pressable, self).into_widget()
    }
}

/// The same color at a new alpha (for the translucent tint overlay).
fn with_alpha(c: Color, alpha: f32) -> Color {
    let [r, g, b, _] = c.components;
    Color::new([r, g, b, alpha])
}

fn render_pressable(p: &Pressable) -> Element {
    let hovered = create_signal(false);
    let pressed = create_signal(false);
    let node = create_focus();
    let c = theme().colors;
    let radius = p.radius.unwrap_or(theme().radius);

    let hv = if p.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let pr = if p.disabled { 0.0 } else { animated(if pressed.get() { 1.0 } else { 0.0 }, 0.07) };
    let focused = !p.disabled && node.is_focused();

    // The tint: an accent-colored overlay whose alpha rises on hover/press.
    let tint = p.hover_tint.unwrap_or(c.accent);
    let alpha = (0.10 * hv + 0.06 * pr).min(0.20) as f32;

    let mut decoration = BoxDecoration::new().color(with_alpha(tint, alpha));
    decoration = match p.shape {
        InkShape::Rectangle => decoration.radius(BorderRadius::all(radius)),
        InkShape::Circle => decoration.shape(BoxShape::Circle),
    };
    if focused {
        decoration = decoration.border(Border::new(c.ring, 2.0));
    }
    let container = Container::new().decoration(decoration).child(p.child.clone());

    let a11y = |w: AnyWidget, disabled: bool| {
        crate::widgets::semantics(crate::widgets::SemanticsRole::Button, p.label.clone(), w)
            .disabled(disabled)
            .into_widget()
    };

    if p.disabled {
        return a11y(
            GestureDetector::new(Opacity::new(0.55, container)).cursor(Cursor::NotAllowed).into_widget(),
            true,
        );
    }

    // Space/Enter activation reuses the tap handler; focus-change is announced.
    let activation: Rc<dyn Fn()> = match &p.on_tap {
        Some(Callback::Plain(f)) => f.clone(),
        _ => Rc::new(|| {}),
    };
    node.register(activation, p.on_focus_change.clone(), p.autofocus);

    let on_hover = p.on_hover.clone();
    let hi_down = p.on_highlight_changed.clone();
    let hi_exit = p.on_highlight_changed.clone();
    let (hover_in, hover_out) = (on_hover.clone(), on_hover);

    let mut gesture = GestureDetector::new(container)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || {
            hovered.set(true);
            if let Some(h) = &hover_in {
                h(true);
            }
        })
        .on_hover_exit(move || {
            if pressed.peek()
                && let Some(h) = &hi_exit
            {
                h(false);
            }
            hovered.set(false);
            pressed.set(false);
            if let Some(h) = &hover_out {
                h(false);
            }
        })
        .on_pointer_down(move || {
            pressed.set(true);
            node.request_focus(); // clicking focuses, like Flutter
            if let Some(h) = &hi_down {
                h(true);
            }
        })
        .on_pointer_up(move || pressed.set(false));

    if let Some(cb) = p.on_tap.clone() {
        gesture = gesture.on_tap(cb);
    }
    if let Some(cb) = p.on_long_press.clone() {
        gesture = gesture.on_long_press(cb);
    }
    if let Some(cb) = p.on_double_tap.clone() {
        gesture = gesture.on_double_tap(cb);
    }
    if let Some(cb) = p.on_secondary_tap.clone() {
        gesture = gesture.on_secondary_tap(cb);
    }
    a11y(gesture.into_widget(), false)
}

// ===========================================================================
// Ink — a decorated surface the tap tint draws OVER (Flutter's Ink)
// ===========================================================================

/// Flutter's `Ink`: a decorated surface meant as the child of a [`pressable`] /
/// [`ink_well`], so the tap tint draws **over** its background instead of being
/// hidden behind a plain container's own color.
///
/// In Pebbles this is a themed [`Container`]: `pressable` already paints its tint
/// above the child, so `ink` just gives that surface a background/decoration with
/// the familiar name — `pressable(ink(content).color(c.card).radius(8.0)).on_tap(..)`.
#[derive(Clone)]
pub struct Ink {
    child: AnyWidget,
    color: Option<Color>,
    decoration: Option<BoxDecoration>,
    radius: Option<f64>,
    padding: Option<EdgeInsets>,
}

/// Wrap `child` in an [`Ink`] surface.
pub fn ink(child: impl IntoWidget) -> Ink {
    Ink { child: child.into_widget(), color: None, decoration: None, radius: None, padding: None }
}

impl Ink {
    /// A solid background color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// A full [`BoxDecoration`] (color / border / gradient / image / shadow) — wins
    /// over [`color`](Ink::color).
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = Some(decoration);
        self
    }
    /// Corner radius (applied to the color background, or the decoration if none set it).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = Some(radius);
        self
    }
    /// Inner padding around the child.
    pub fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = Some(padding);
        self
    }
}

impl IntoWidget for Ink {
    fn into_widget(self) -> AnyWidget {
        let mut deco = self.decoration.unwrap_or_else(|| {
            let mut d = BoxDecoration::new();
            if let Some(color) = self.color {
                d = d.color(color);
            }
            d
        });
        if let Some(r) = self.radius {
            deco = deco.radius(BorderRadius::all(r));
        }
        let mut container = Container::new().decoration(deco);
        if let Some(pad) = self.padding {
            container = container.padding(pad);
        }
        container.child(self.child).into_widget()
    }
}
