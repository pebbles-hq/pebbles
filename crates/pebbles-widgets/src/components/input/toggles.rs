//! Selection controls: [`Checkbox`], [`Switch`], [`Radio`] and [`Toggle`].
//!
//! These are **controlled** (value in, `on_changed` out) and **animated**: each is a
//! function component that tweens its visual state whenever the value flips — the
//! switch thumb slides, the track/box colors cross-fade, and the check/dot fade in.

use pebbles_foundation::{Alignment, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, IconKind};

use crate::components::icon;
use crate::theme::{mix, theme};
use crate::widgets::{ClipRRect, Container, GestureDetector, Opacity, Positioned, center, stack};
use pebbles_core::context::Callback;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, children, component_props};

/// Default transition time for the controls.
const DUR: f64 = 0.16;

/// Attach the optional tap callback + a pointer cursor.
fn interactive(child: impl IntoWidget, on: &Option<Callback>) -> GestureDetector {
    let g = GestureDetector::new(child).cursor(Cursor::Pointer);
    match on {
        Some(cb) => g.on_tap(cb.clone()),
        None => g,
    }
}

// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

/// A checkbox. `value` is the current state; `on_changed` fires on tap.
pub struct Checkbox {
    value: bool,
    on_changed: Option<Callback>,
}

/// Create a [`Checkbox`].
pub fn checkbox(value: bool) -> Checkbox {
    Checkbox { value, on_changed: None }
}

impl Checkbox {
    pub fn on_changed(mut self, cb: Callback) -> Self {
        self.on_changed = Some(cb);
        self
    }
}

struct CheckboxProps {
    value: bool,
    on_changed: Option<Callback>,
}

impl IntoWidget for Checkbox {
    fn into_widget(self) -> AnyWidget {
        component_props(render_checkbox, CheckboxProps { value: self.value, on_changed: self.on_changed })
            .into_widget()
    }
}

fn render_checkbox(p: &CheckboxProps) -> GestureDetector {
    let c = theme().colors;
    let t = animated(if p.value { 1.0 } else { 0.0 }, DUR);
    let bg = mix(c.background, c.primary, t as f32);
    let border = mix(c.border, c.primary, t as f32);
    let box_ = Container::new()
        .decoration(
            BoxDecoration::new()
                .color(bg)
                .border(Border::new(border, 1.5))
                .radius(BorderRadius::all(4.0)),
        )
        .width(18.0)
        .height(18.0)
        .alignment(Alignment::CENTER)
        .child(center(Opacity::new(
            t as f32,
            icon(IconKind::Check).size(13.0).color(c.primary_foreground),
        )));
    interactive(box_, &p.on_changed)
}

// ---------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------

/// A toggle switch. The thumb slides and the track fades between states.
pub struct Switch {
    value: bool,
    on_changed: Option<Callback>,
}

/// Create a [`Switch`].
pub fn switch(value: bool) -> Switch {
    Switch { value, on_changed: None }
}

impl Switch {
    pub fn on_changed(mut self, cb: Callback) -> Self {
        self.on_changed = Some(cb);
        self
    }
}

struct SwitchProps {
    value: bool,
    on_changed: Option<Callback>,
}

impl IntoWidget for Switch {
    fn into_widget(self) -> AnyWidget {
        component_props(render_switch, SwitchProps { value: self.value, on_changed: self.on_changed })
            .into_widget()
    }
}

fn render_switch(p: &SwitchProps) -> GestureDetector {
    let c = theme().colors;
    let t = animated(if p.value { 1.0 } else { 0.0 }, 0.18);
    let track_color = mix(c.input, c.primary, t as f32);
    let thumb = ClipRRect::new(
        BorderRadius::all(9.0),
        Container::new().color(c.background).width(18.0).height(18.0),
    );
    // Track 44 wide, 3px inset each side, 18px thumb → 20px of travel.
    let left = 3.0 + (44.0 - 18.0 - 6.0) * t;
    let track = Container::new()
        .decoration(BoxDecoration::new().color(track_color).radius(BorderRadius::all(999.0)))
        .width(44.0)
        .height(24.0)
        .child(stack(children![Positioned::new(thumb).left(left).top(3.0)]));
    interactive(track, &p.on_changed)
}

// ---------------------------------------------------------------------------
// Radio
// ---------------------------------------------------------------------------

/// A radio button. `selected` is the current state; `on_selected` fires on tap.
pub struct Radio {
    selected: bool,
    on_selected: Option<Callback>,
}

/// Create a [`Radio`].
pub fn radio(selected: bool) -> Radio {
    Radio { selected, on_selected: None }
}

impl Radio {
    pub fn on_selected(mut self, cb: Callback) -> Self {
        self.on_selected = Some(cb);
        self
    }
}

struct RadioProps {
    selected: bool,
    on_selected: Option<Callback>,
}

impl IntoWidget for Radio {
    fn into_widget(self) -> AnyWidget {
        component_props(render_radio, RadioProps { selected: self.selected, on_selected: self.on_selected })
            .into_widget()
    }
}

fn render_radio(p: &RadioProps) -> GestureDetector {
    let c = theme().colors;
    let t = animated(if p.selected { 1.0 } else { 0.0 }, DUR);
    let border = mix(c.border, c.primary, t as f32);
    let dot = Opacity::new(
        t as f32,
        Container::new()
            .decoration(BoxDecoration::new().color(c.primary).radius(BorderRadius::all(999.0)))
            .width(9.0)
            .height(9.0),
    );
    let ring = Container::new()
        .decoration(
            BoxDecoration::new()
                .color(c.background)
                .border(Border::new(border, 1.5))
                .radius(BorderRadius::all(999.0)),
        )
        .width(18.0)
        .height(18.0)
        .alignment(Alignment::CENTER)
        .child(center(dot));
    interactive(ring, &p.on_selected)
}

// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

/// A two-state toggle button wrapping any child.
pub struct Toggle {
    pressed: bool,
    child: Option<AnyWidget>,
    on_changed: Option<Callback>,
}

/// Create a [`Toggle`] around `child`.
pub fn toggle(pressed: bool, child: impl IntoWidget) -> Toggle {
    Toggle { pressed, child: Some(child.into_widget()), on_changed: None }
}

impl Toggle {
    pub fn on_changed(mut self, cb: Callback) -> Self {
        self.on_changed = Some(cb);
        self
    }
}

struct ToggleProps {
    pressed: bool,
    child: AnyWidget,
    on_changed: Option<Callback>,
}

impl IntoWidget for Toggle {
    fn into_widget(mut self) -> AnyWidget {
        let child = self.child.take().expect("toggle child");
        component_props(
            render_toggle,
            ToggleProps { pressed: self.pressed, child, on_changed: self.on_changed },
        )
        .into_widget()
    }
}

fn render_toggle(p: &ToggleProps) -> GestureDetector {
    let c = theme().colors;
    let t = animated(if p.pressed { 1.0 } else { 0.0 }, DUR);
    let bg = mix(c.background, c.accent, t as f32);
    let container = Container::new()
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(theme().radius)))
        .padding(EdgeInsets::all(8.0))
        .alignment(Alignment::CENTER)
        .child(center(p.child.clone()));
    interactive(container, &p.on_changed)
}
