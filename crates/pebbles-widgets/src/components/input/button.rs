//! [`Button`] and [`IconButton`] — function components with signal-based hover/press
//! state, the full Flutter event set, an arbitrary child, and per-instance style
//! overrides. The fluent builder *is* the component's props; `into_widget` wraps it
//! with [`component_props`](pebbles_core::component::component_props).

use pebbles_core::IntoCallback;
use std::rc::Rc;

use pebbles_foundation::{Color, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconData};

use pebbles_core::component::{Element, component_props};
use pebbles_core::context::{Callback, action};
use pebbles_core::focus::create_focus;
use pebbles_core::animated;
use pebbles_core::reactive::create_signal;
use crate::theme::{mix, theme};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{Container, GestureDetector, Opacity, SizedBox, center, row, spinner, text};

use crate::components::icon;

/// The visual style of a [`Button`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Outline,
    Ghost,
    Destructive,
    Link,
}

/// The size of a [`Button`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Sm,
    #[default]
    Md,
    Lg,
}

/// A clickable action button.
#[derive(Default)]
pub struct Button {
    label: String,
    leading: Option<IconData>,
    trailing: Option<IconData>,
    content: Option<AnyWidget>,
    variant: ButtonVariant,
    size: ButtonSize,
    color: Option<Color>,
    text_color: Option<Color>,
    radius: Option<f64>,
    padding: Option<EdgeInsets>,
    shadow: Option<BoxShadow>,
    full_width: bool,
    disabled: bool,
    loading: bool,
    autofocus: bool,
    on_pressed: Option<Callback>,
    on_long_press: Option<Callback>,
    on_double_tap: Option<Callback>,
    on_secondary_tap: Option<Callback>,
    on_tap_down: Option<Callback>,
    on_tap_up: Option<Callback>,
    on_tap_cancel: Option<Callback>,
    on_secondary_tap_down: Option<Callback>,
    on_secondary_tap_up: Option<Callback>,
    on_secondary_tap_cancel: Option<Callback>,
    on_hover_enter: Option<Callback>,
    on_hover_exit: Option<Callback>,
    on_focus_change: Option<Rc<dyn Fn(bool)>>,
    on_highlight_changed: Option<Rc<dyn Fn(bool)>>,
    on_long_press_down: Option<Callback>,
    on_long_press_start: Option<Callback>,
    on_long_press_move: Option<Callback>,
    on_long_press_up: Option<Callback>,
    on_long_press_end: Option<Callback>,
    on_long_press_cancel: Option<Callback>,
    on_tertiary_tap_down: Option<Callback>,
    on_tertiary_tap_up: Option<Callback>,
    on_tertiary_tap_cancel: Option<Callback>,
}

/// Create a [`Button`] with the given text label.
pub fn button(label: impl Into<String>) -> Button {
    Button { label: label.into(), ..Default::default() }
}

impl Button {
    /// Replace the label with an arbitrary child (icon+text, a `Row`, a `Column`, …).
    pub fn child(mut self, child: impl IntoWidget) -> Self {
        self.content = Some(child.into_widget());
        self
    }
    /// An icon before the label.
    pub fn leading(mut self, icon: impl Into<IconData>) -> Self {
        self.leading = Some(icon.into());
        self
    }
    /// An icon after the label.
    pub fn trailing(mut self, icon: impl Into<IconData>) -> Self {
        self.trailing = Some(icon.into());
        self
    }
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = Some(radius);
        self
    }
    pub fn padding(mut self, padding: EdgeInsets) -> Self {
        self.padding = Some(padding);
        self
    }
    /// Cast a drop shadow under the button (elevation).
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
    pub fn full_width(mut self) -> Self {
        self.full_width = true;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Show a spinner and disable interaction while an async action runs.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }
    /// Grab keyboard focus on mount.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// Called with `true`/`false` when the button gains/loses focus.
    pub fn on_focus_change(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus_change = Some(Rc::new(f));
        self
    }
    /// Primary tap (Flutter's `onPressed`).
    pub fn on_pressed(mut self, cb: impl IntoCallback) -> Self {
        self.on_pressed = Some(cb.into_callback());
        self
    }
    /// Alias for [`Button::on_pressed`].
    pub fn on_click(self, cb: impl IntoCallback) -> Self {
        self.on_pressed(cb)
    }
    pub fn on_long_press(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press = Some(cb.into_callback());
        self
    }
    pub fn on_double_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_double_tap = Some(cb.into_callback());
        self
    }
    pub fn on_secondary_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap = Some(cb.into_callback());
        self
    }
    pub fn on_tap_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_tap_down = Some(cb.into_callback());
        self
    }
    pub fn on_tap_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_tap_up = Some(cb.into_callback());
        self
    }
    /// Press began but ended without a tap (released off / dragged away).
    pub fn on_tap_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_tap_cancel = Some(cb.into_callback());
        self
    }
    pub fn on_secondary_tap_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap_down = Some(cb.into_callback());
        self
    }
    pub fn on_secondary_tap_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap_up = Some(cb.into_callback());
        self
    }
    pub fn on_secondary_tap_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap_cancel = Some(cb.into_callback());
        self
    }
    /// Called with the pressed (highlight) state as it changes.
    pub fn on_highlight_changed(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_highlight_changed = Some(Rc::new(f));
        self
    }
    // ----- long-press lifecycle -----
    pub fn on_long_press_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_down = Some(cb.into_callback());
        self
    }
    pub fn on_long_press_start(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_start = Some(cb.into_callback());
        self
    }
    pub fn on_long_press_move(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_move = Some(cb.into_callback());
        self
    }
    pub fn on_long_press_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_up = Some(cb.into_callback());
        self
    }
    pub fn on_long_press_end(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_end = Some(cb.into_callback());
        self
    }
    pub fn on_long_press_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_cancel = Some(cb.into_callback());
        self
    }
    // ----- tertiary (middle) button -----
    pub fn on_tertiary_tap_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_tertiary_tap_down = Some(cb.into_callback());
        self
    }
    pub fn on_tertiary_tap_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_tertiary_tap_up = Some(cb.into_callback());
        self
    }
    pub fn on_tertiary_tap_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_tertiary_tap_cancel = Some(cb.into_callback());
        self
    }
    pub fn on_hover_enter(mut self, cb: impl IntoCallback) -> Self {
        self.on_hover_enter = Some(cb.into_callback());
        self
    }
    pub fn on_hover_exit(mut self, cb: impl IntoCallback) -> Self {
        self.on_hover_exit = Some(cb.into_callback());
        self
    }

    fn resolved_colors(&self) -> (Option<Color>, Color, bool) {
        let c = theme().colors;
        let (bg, fg, border) = match self.variant {
            ButtonVariant::Primary => (Some(c.primary), c.primary_foreground, false),
            ButtonVariant::Secondary => (Some(c.secondary), c.secondary_foreground, false),
            ButtonVariant::Outline => (Some(c.background), c.foreground, true),
            ButtonVariant::Ghost => (None, c.foreground, false),
            ButtonVariant::Destructive => (Some(c.destructive), c.destructive_foreground, false),
            ButtonVariant::Link => (None, c.primary, false),
        };
        (self.color.or(bg), self.text_color.unwrap_or(fg), border)
    }

    fn resolved_padding(&self) -> EdgeInsets {
        self.padding.unwrap_or(match self.size {
            ButtonSize::Sm => EdgeInsets::symmetric(12.0, 6.0),
            ButtonSize::Md => EdgeInsets::symmetric(16.0, 9.0),
            ButtonSize::Lg => EdgeInsets::symmetric(22.0, 12.0),
        })
    }

    fn font_size(&self) -> f32 {
        match self.size {
            ButtonSize::Sm => 13.0,
            ButtonSize::Md => 14.0,
            ButtonSize::Lg => 15.0,
        }
    }

}

impl IntoWidget for Button {
    fn into_widget(self) -> AnyWidget {
        Box::new(component_props(render_button, self))
    }
}

/// Resolve the background for the current interaction state, blended continuously
/// by the animated hover (`hv`) and press (`pr`) factors (each `0.0..=1.0`).
fn interactive_bg(
    variant: ButtonVariant,
    base_bg: Option<Color>,
    override_color: bool,
    hv: f64,
    pr: f64,
) -> Option<Color> {
    let c = theme().colors;
    let filled = override_color
        || matches!(variant, ButtonVariant::Primary | ButtonVariant::Secondary | ButtonVariant::Destructive);
    if filled {
        let base = base_bg.unwrap_or(c.primary);
        let hover_c = mix(base, c.background, 0.14);
        let press_c = mix(base, c.foreground, 0.16);
        let cur = mix(base, hover_c, hv as f32);
        Some(mix(cur, press_c, pr as f32))
    } else if matches!(variant, ButtonVariant::Outline) {
        let base = base_bg.unwrap_or(c.background);
        let acc = mix(c.accent, c.foreground, 0.10 * pr as f32);
        Some(mix(base, acc, hv.max(pr) as f32))
    } else {
        // Ghost / Link: fade the accent's opacity in from fully transparent.
        let level = hv.max(pr);
        if level < 0.004 {
            None
        } else {
            let acc = mix(c.accent, c.foreground, 0.10 * pr as f32);
            let [r, g, b, _] = acc.components;
            Some(Color::new([r, g, b, level as f32]))
        }
    }
}

/// The [`Button`] component.
fn render_button(b: &Button) -> Element {
    let hovered = create_signal(false);
    let pressed = create_signal(false);
    let node = create_focus();
    let th = theme();
    let (base_bg, fg, border) = b.resolved_colors();
    let inert = b.disabled || b.loading;
    let hv = if inert { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let pr = if inert { 0.0 } else { animated(if pressed.get() { 1.0 } else { 0.0 }, 0.07) };
    let focused = !inert && node.is_focused();
    let bg = interactive_bg(b.variant, base_bg, b.color.is_some(), hv, pr);

    let content: AnyWidget = match b.content.clone() {
        Some(child) => child,
        None => {
            let label = text(b.label.clone()).size(b.font_size()).weight(500.0).color(fg);
            if b.leading.is_none() && b.trailing.is_none() {
                label.into_widget()
            } else {
                let isz = b.font_size() as f64 + 2.0;
                let mut kids: Vec<AnyWidget> = Vec::new();
                if let Some(kind) = b.leading {
                    kids.push(icon(kind).size(isz).color(fg).into_widget());
                    if !b.label.is_empty() {
                        kids.push(SizedBox::spacer(8.0, 0.0).into_widget());
                    }
                }
                kids.push(label.into_widget());
                if let Some(kind) = b.trailing {
                    if !b.label.is_empty() {
                        kids.push(SizedBox::spacer(8.0, 0.0).into_widget());
                    }
                    kids.push(icon(kind).size(isz).color(fg).into_widget());
                }
                row(kids).main_axis_min().into_widget()
            }
        }
    };
    // While loading, prepend a spinner (keeps the label so the button doesn't jump).
    let content: AnyWidget = if b.loading {
        let sp = spinner(b.font_size() as f64 + 3.0).color(fg).into_widget();
        row([sp, SizedBox::spacer(8.0, 0.0).into_widget(), content]).main_axis_min().into_widget()
    } else {
        content
    };
    let inner: AnyWidget = if b.full_width { center(content).into_widget() } else { content };

    let mut decoration =
        BoxDecoration::new().radius(BorderRadius::all(b.radius.unwrap_or(th.radius)));
    if let Some(shadow) = b.shadow {
        decoration = decoration.shadow(shadow);
    }
    if let Some(bg) = bg {
        decoration = decoration.color(bg);
    }
    if border {
        decoration = decoration.border(Border::new(th.colors.border, 1.0));
    }
    // Focus ring.
    if focused {
        decoration = decoration.border(Border::new(th.colors.ring, 2.0));
    }
    // The button shrink-wraps its content (+ padding) — shadcn-style. `full_width`
    // opts into filling the parent (handled above via `center`).
    let container =
        Container::new().decoration(decoration).padding(b.resolved_padding()).child(inner);

    // Accessibility wrapper — applied at every exit so disabled/loading buttons are
    // announced too (with their disabled state).
    let a11y = |w: AnyWidget, disabled: bool| {
        crate::widgets::semantics(crate::widgets::SemanticsRole::Button, b.label.clone(), w)
            .disabled(disabled)
            .into_widget()
    };
    if b.disabled {
        return a11y(
            GestureDetector::new(Opacity::new(0.55, container))
                .cursor(Cursor::NotAllowed)
                .into_widget(),
            true,
        );
    }
    if b.loading {
        return a11y(GestureDetector::new(container).cursor(Cursor::Default).into_widget(), true);
    }

    // Register keyboard activation (Enter/Space), focus-change, and autofocus.
    let activation: Rc<dyn Fn()> = match &b.on_pressed {
        Some(Callback::Plain(f)) => f.clone(),
        _ => Rc::new(|| {}),
    };
    node.register(activation, b.on_focus_change.clone(), b.autofocus);

    // onHighlightChanged: fire on pressed transitions.
    let hi_down = b.on_highlight_changed.clone();
    let hi_up = b.on_highlight_changed.clone();
    let hi_exit = b.on_highlight_changed.clone();

    let mut gesture = GestureDetector::new(container)
        .cursor(Cursor::Pointer)
        .on_hover_enter(action(move || hovered.set(true)))
        .on_hover_exit(action(move || {
            if pressed.peek() {
                if let Some(h) = &hi_exit {
                    h(false);
                }
            }
            hovered.set(false);
            pressed.set(false);
        }))
        .on_pointer_down(action(move || {
            pressed.set(true);
            node.request_focus(); // clicking focuses, like Flutter
            if let Some(h) = &hi_down {
                h(true);
            }
        }))
        .on_pointer_up(action(move || {
            pressed.set(false);
            if let Some(h) = &hi_up {
                h(false);
            }
        }));

    for (slot, cb) in [
        (0, &b.on_pressed),
        (1, &b.on_long_press),
        (2, &b.on_double_tap),
        (3, &b.on_secondary_tap),
        (4, &b.on_tap_down),
        (5, &b.on_tap_up),
        (6, &b.on_hover_enter),
        (7, &b.on_hover_exit),
    ] {
        if let Some(cb) = cb.clone() {
            gesture = match slot {
                0 => gesture.on_tap(cb),
                1 => gesture.on_long_press(cb),
                2 => gesture.on_double_tap(cb),
                3 => gesture.on_secondary_tap(cb),
                4 => gesture.on_pointer_down(cb),
                5 => gesture.on_pointer_up(cb),
                6 => gesture.on_hover_enter(cb),
                _ => gesture.on_hover_exit(cb),
            };
        }
    }
    if let Some(cb) = b.on_tap_cancel.clone() {
        gesture = gesture.on_tap_cancel(cb);
    }
    if let Some(cb) = b.on_secondary_tap_down.clone() {
        gesture = gesture.on_secondary_tap_down(cb);
    }
    if let Some(cb) = b.on_secondary_tap_up.clone() {
        gesture = gesture.on_secondary_tap_up(cb);
    }
    if let Some(cb) = b.on_secondary_tap_cancel.clone() {
        gesture = gesture.on_secondary_tap_cancel(cb);
    }
    if let Some(cb) = b.on_long_press_down.clone() {
        gesture = gesture.on_long_press_down(cb);
    }
    if let Some(cb) = b.on_long_press_start.clone() {
        gesture = gesture.on_long_press_start(cb);
    }
    if let Some(cb) = b.on_long_press_move.clone() {
        gesture = gesture.on_long_press_move(cb);
    }
    if let Some(cb) = b.on_long_press_up.clone() {
        gesture = gesture.on_long_press_up(cb);
    }
    if let Some(cb) = b.on_long_press_end.clone() {
        gesture = gesture.on_long_press_end(cb);
    }
    if let Some(cb) = b.on_long_press_cancel.clone() {
        gesture = gesture.on_long_press_cancel(cb);
    }
    if let Some(cb) = b.on_tertiary_tap_down.clone() {
        gesture = gesture.on_tertiary_tap_down(cb);
    }
    if let Some(cb) = b.on_tertiary_tap_up.clone() {
        gesture = gesture.on_tertiary_tap_up(cb);
    }
    if let Some(cb) = b.on_tertiary_tap_cancel.clone() {
        gesture = gesture.on_tertiary_tap_cancel(cb);
    }
    // Accessibility: expose the button (its label + disabled state) to screen readers.
    a11y(gesture.into_widget(), inert)
}

/// A square icon-only button with hover + pressed feedback.
pub struct IconButton {
    kind: IconData,
    icon_size: f64,
    variant: ButtonVariant,
    on_pressed: Option<Callback>,
    disabled: bool,
}

/// Create an [`IconButton`] for the given icon.
pub fn icon_button(kind: impl Into<IconData>) -> IconButton {
    IconButton {
        kind: kind.into(),
        icon_size: 18.0,
        variant: ButtonVariant::Ghost,
        on_pressed: None,
        disabled: false,
    }
}

impl IconButton {
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: f64) -> Self {
        self.icon_size = size;
        self
    }
    pub fn on_pressed(mut self, cb: impl IntoCallback) -> Self {
        self.on_pressed = Some(cb.into_callback());
        self
    }
    pub fn on_click(self, cb: impl IntoCallback) -> Self {
        self.on_pressed(cb)
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl IntoWidget for IconButton {
    fn into_widget(self) -> AnyWidget {
        Box::new(component_props(render_icon_button, self))
    }
}

fn render_icon_button(b: &IconButton) -> Element {
    let hovered = create_signal(false);
    let pressed = create_signal(false);
    let th = theme();
    let (base_bg, border) = match b.variant {
        ButtonVariant::Primary => (Some(th.colors.primary), false),
        ButtonVariant::Secondary => (Some(th.colors.secondary), false),
        ButtonVariant::Outline => (Some(th.colors.background), true),
        ButtonVariant::Destructive => (Some(th.colors.destructive), false),
        _ => (None, false),
    };
    let hv = if b.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let pr = if b.disabled { 0.0 } else { animated(if pressed.get() { 1.0 } else { 0.0 }, 0.07) };
    let bg = interactive_bg(b.variant, base_bg, false, hv, pr);
    let fg = th.colors.foreground;

    let mut decoration = BoxDecoration::new().radius(BorderRadius::all(th.radius));
    if let Some(bg) = bg {
        decoration = decoration.color(bg);
    }
    if border {
        decoration = decoration.border(Border::new(th.colors.border, 1.0));
    }
    let container = Container::new()
        .decoration(decoration)
        .padding(EdgeInsets::all(8.0))
        .child(icon(b.kind).size(b.icon_size).color(fg));

    if b.disabled {
        return GestureDetector::new(Opacity::new(0.55, container))
            .cursor(Cursor::NotAllowed)
            .into_widget();
    }

    let mut gesture = GestureDetector::new(container)
        .cursor(Cursor::Pointer)
        .on_hover_enter(action(move || hovered.set(true)))
        .on_hover_exit(action(move || {
            hovered.set(false);
            pressed.set(false);
        }))
        .on_pointer_down(action(move || pressed.set(true)))
        .on_pointer_up(action(move || pressed.set(false)));
    if let Some(cb) = b.on_pressed.clone() {
        gesture = gesture.on_tap(cb);
    }
    gesture.into_widget()
}
