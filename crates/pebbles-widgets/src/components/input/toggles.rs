//! Selection controls: [`Checkbox`], [`Switch`], [`Radio`] and [`Toggle`].
//!
//! These are **controlled** (value in, `on_changed` out) and **animated**: each is a
//! function component that tweens its visual state whenever the value flips — the
//! switch thumb slides, the track/box colors cross-fade, and the check/dot fade in.
//!
//! Every control shares the button-grade capability set: three [`ToggleSize`]s, a
//! custom accent [`color`](Checkbox::color), a [`disabled`](Checkbox::disabled)
//! state (dimmed + not-allowed cursor, no callbacks), an optional in-line
//! [`label`](Checkbox::label) / [`description`](Checkbox::description) that becomes
//! part of the tap target, hover feedback, an animated focus ring, and keyboard
//! activation (Tab to focus, Space/Enter to toggle).

use pebbles_core::IntoCallback;
use std::rc::Rc;

use pebbles_foundation::{Alignment, Axis, Color, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, IconKind};

use crate::components::icon;
use crate::theme::{mix, theme};
use crate::widgets::{
    ClipRRect, Container, GestureDetector, Opacity, Positioned, center, column, gap_h, gap_w, row, stack,
    text,
};
use pebbles_core::context::Callback;
use pebbles_core::focus::{FocusNode, create_focus};
use pebbles_core::reactive::{Signal, create_signal};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, children, component_props};

/// Default transition time for the controls.
const DUR: f64 = 0.16;

/// The size of a selection control — scales the control, its label font and the gap
/// between them, mirroring [`ButtonSize`](super::ButtonSize).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToggleSize {
    Sm,
    #[default]
    Md,
    Lg,
}

impl ToggleSize {
    /// Edge length of the checkbox box / radio ring.
    fn box_dim(self) -> f64 {
        match self {
            Self::Sm => 16.0,
            Self::Md => 18.0,
            Self::Lg => 22.0,
        }
    }
    /// Label font size.
    fn font(self) -> f32 {
        match self {
            Self::Sm => 13.0,
            Self::Md => 14.0,
            Self::Lg => 15.0,
        }
    }
    /// Gap between the control and its label.
    fn gap(self) -> f64 {
        match self {
            Self::Sm => 8.0,
            _ => 10.0,
        }
    }
    /// Switch geometry: `(track_w, track_h, thumb, inset)`.
    fn switch(self) -> (f64, f64, f64, f64) {
        match self {
            Self::Sm => (36.0, 20.0, 16.0, 2.0),
            Self::Md => (44.0, 24.0, 18.0, 3.0),
            Self::Lg => (52.0, 28.0, 22.0, 3.0),
        }
    }
    /// Padding for a [`Toggle`] button.
    fn toggle_pad(self) -> EdgeInsets {
        match self {
            Self::Sm => EdgeInsets::symmetric(8.0, 5.0),
            Self::Md => EdgeInsets::symmetric(10.0, 7.0),
            Self::Lg => EdgeInsets::symmetric(13.0, 9.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared composition helpers
// ---------------------------------------------------------------------------

/// Lay an optional label / description beside a control. The whole row becomes the
/// tap target (wired by [`wire`]), so clicking the text toggles too. A single-line
/// label centers against the control; a description top-aligns them.
fn labeled(
    control: AnyWidget,
    size: ToggleSize,
    label: Option<String>,
    desc: Option<String>,
    disabled: bool,
) -> AnyWidget {
    if label.is_none() && desc.is_none() {
        return control;
    }
    let c = theme().colors;
    let has_desc = desc.is_some();
    let fg = if disabled { c.muted_foreground } else { c.foreground };
    let mut lines: Vec<AnyWidget> = Vec::new();
    if let Some(l) = label {
        lines.push(text(l).size(size.font()).weight(500.0).color(fg).into_widget());
    }
    if let Some(d) = desc {
        if !lines.is_empty() {
            lines.push(gap_h(2.0).into_widget());
        }
        lines.push(text(d).size(size.font() - 1.0).line_height(1.35).color(c.muted_foreground).into_widget());
    }
    let block =
        column(lines).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min);
    let cross = if has_desc { CrossAxisAlignment::Start } else { CrossAxisAlignment::Center };
    row(children![control, gap_w(size.gap()), block])
        .cross_axis_alignment(cross)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

/// Wrap the control body in its gesture + focus behavior. When `disabled`, the body
/// is dimmed and shows the not-allowed cursor with no handlers. Otherwise it takes a
/// pointer cursor, tracks hover, focuses on press, registers Space/Enter activation,
/// and fires `on` on tap.
fn wire(
    body: AnyWidget,
    disabled: bool,
    on: &Option<Callback>,
    hovered: Signal<bool>,
    node: FocusNode,
    autofocus: bool,
) -> AnyWidget {
    if disabled {
        return GestureDetector::new(Opacity::new(0.55, body)).cursor(Cursor::NotAllowed).into_widget();
    }
    // Keyboard activation reuses the tap callback (Space/Enter while focused).
    let activation: Rc<dyn Fn()> = match on {
        Some(Callback::Plain(f)) => f.clone(),
        _ => Rc::new(|| {}),
    };
    node.register(activation, None, autofocus);
    let mut g = GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_pointer_down(move || node.request_focus());
    if let Some(cb) = on.clone() {
        g = g.on_tap(cb);
    }
    // Selection controls consume right-clicks by default.
    g = g.on_secondary_tap(|| {});
    g.into_widget()
}

// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

/// A checkbox. `value` is the current state; `on_changed` fires on tap.
#[derive(Clone, Default)]
pub struct Checkbox {
    value: bool,
    indeterminate: bool,
    size: ToggleSize,
    color: Option<Color>,
    disabled: bool,
    autofocus: bool,
    label: Option<String>,
    description: Option<String>,
    on_changed: Option<Callback>,
}

/// Create a [`Checkbox`].
pub fn checkbox(value: bool) -> Checkbox {
    Checkbox { value, ..Default::default() }
}

impl Checkbox {
    pub fn size(mut self, size: ToggleSize) -> Self {
        self.size = size;
        self
    }
    /// Show the indeterminate ("mixed") state — a filled box with a dash instead of a
    /// check. Tapping it fires `on_changed` (the caller typically resolves to checked).
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }
    /// Custom accent color for the checked fill (defaults to the theme primary).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// An in-line label beside the box; clicking it toggles the box.
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    /// A muted secondary line under the label.
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    pub fn on_changed(mut self, cb: impl IntoCallback) -> Self {
        self.on_changed = Some(cb.into_callback());
        self
    }
}

impl IntoWidget for Checkbox {
    fn into_widget(self) -> AnyWidget {
        component_props(render_checkbox, self).into_widget()
    }
}

fn render_checkbox(p: &Checkbox) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let node = create_focus();
    let accent = p.color.unwrap_or(c.primary);
    // Both checked and indeterminate read as a filled box.
    let filled = p.value || p.indeterminate;
    let t = animated(if filled { 1.0 } else { 0.0 }, DUR);
    let hv = if p.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let focused = !p.disabled && node.is_focused();
    let dim = p.size.box_dim();
    let glyph = if p.indeterminate { IconKind::Minus } else { IconKind::Check };

    let bg = mix(c.background, accent, t as f32);
    // Fill/border track the accent as it checks; hover darkens the border only while
    // unchecked (checked already reads as filled).
    let mut border = mix(c.border, accent, t as f32);
    border = mix(border, c.foreground, 0.22 * hv as f32 * (1.0 - t as f32));

    let mut deco = BoxDecoration::new().color(bg).radius(BorderRadius::all(4.0));
    deco =
        if focused { deco.border(Border::new(c.ring, 2.0)) } else { deco.border(Border::new(border, 1.5)) };
    let box_ = Container::new()
        .decoration(deco)
        .width(dim)
        .height(dim)
        .alignment(Alignment::CENTER)
        .child(center(Opacity::new(t as f32, icon(glyph).size(dim * 0.72).color(c.primary_foreground))));

    let body = labeled(box_.into_widget(), p.size, p.label.clone(), p.description.clone(), p.disabled);
    let out = wire(body, p.disabled, &p.on_changed, hovered, node, p.autofocus);
    crate::widgets::semantics(
        crate::widgets::SemanticsRole::Checkbox,
        p.label.clone().unwrap_or_default(),
        out,
    )
    .checked(p.value)
    .disabled(p.disabled)
    .into_widget()
}

// ---------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------

/// A toggle switch. The thumb slides and the track fades between states.
#[derive(Clone, Default)]
pub struct Switch {
    value: bool,
    size: ToggleSize,
    color: Option<Color>,
    disabled: bool,
    autofocus: bool,
    label: Option<String>,
    description: Option<String>,
    on_changed: Option<Callback>,
}

/// Create a [`Switch`].
pub fn switch(value: bool) -> Switch {
    Switch { value, ..Default::default() }
}

impl Switch {
    pub fn size(mut self, size: ToggleSize) -> Self {
        self.size = size;
        self
    }
    /// Custom accent color for the "on" track (defaults to the theme primary).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    pub fn on_changed(mut self, cb: impl IntoCallback) -> Self {
        self.on_changed = Some(cb.into_callback());
        self
    }
}

impl IntoWidget for Switch {
    fn into_widget(self) -> AnyWidget {
        component_props(render_switch, self).into_widget()
    }
}

fn render_switch(p: &Switch) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let node = create_focus();
    let accent = p.color.unwrap_or(c.primary);
    let t = animated(if p.value { 1.0 } else { 0.0 }, 0.18);
    let focused = !p.disabled && node.is_focused();
    let (tw, th, thumb, inset) = p.size.switch();

    let track_color = mix(c.input, accent, t as f32);
    let left = inset + (tw - thumb - inset * 2.0) * t;
    let thumb_w = ClipRRect::new(
        BorderRadius::all(thumb / 2.0),
        Container::new().color(c.background).width(thumb).height(thumb),
    );
    let mut deco = BoxDecoration::new().color(track_color).radius(BorderRadius::all(999.0));
    if focused {
        deco = deco.border(Border::new(c.ring, 2.0));
    }
    let track = Container::new()
        .decoration(deco)
        .width(tw)
        .height(th)
        .child(stack(children![Positioned::new(thumb_w).left(left).top(inset)]));

    let body = labeled(track.into_widget(), p.size, p.label.clone(), p.description.clone(), p.disabled);
    let out = wire(body, p.disabled, &p.on_changed, hovered, node, p.autofocus);
    crate::widgets::semantics(crate::widgets::SemanticsRole::Switch, p.label.clone().unwrap_or_default(), out)
        .checked(p.value)
        .disabled(p.disabled)
        .into_widget()
}

// ---------------------------------------------------------------------------
// Radio
// ---------------------------------------------------------------------------

/// A radio button. `selected` is the current state; `on_selected` fires on tap.
#[derive(Clone, Default)]
pub struct Radio {
    selected: bool,
    size: ToggleSize,
    color: Option<Color>,
    disabled: bool,
    autofocus: bool,
    label: Option<String>,
    description: Option<String>,
    on_selected: Option<Callback>,
}

/// Create a [`Radio`].
pub fn radio(selected: bool) -> Radio {
    Radio { selected, ..Default::default() }
}

impl Radio {
    pub fn size(mut self, size: ToggleSize) -> Self {
        self.size = size;
        self
    }
    /// Custom accent color for the selected dot/ring (defaults to the theme primary).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    pub fn on_selected(mut self, cb: impl IntoCallback) -> Self {
        self.on_selected = Some(cb.into_callback());
        self
    }
}

impl IntoWidget for Radio {
    fn into_widget(self) -> AnyWidget {
        component_props(render_radio, self).into_widget()
    }
}

fn render_radio(p: &Radio) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let node = create_focus();
    let accent = p.color.unwrap_or(c.primary);
    let t = animated(if p.selected { 1.0 } else { 0.0 }, DUR);
    let hv = if p.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let focused = !p.disabled && node.is_focused();
    let dim = p.size.box_dim();

    let mut border = mix(c.border, accent, t as f32);
    border = mix(border, c.foreground, 0.22 * hv as f32 * (1.0 - t as f32));
    let dot = Opacity::new(
        t as f32,
        Container::new()
            .decoration(BoxDecoration::new().color(accent).radius(BorderRadius::all(999.0)))
            .width(dim * 0.5)
            .height(dim * 0.5),
    );
    let mut deco = BoxDecoration::new().color(c.background).radius(BorderRadius::all(999.0));
    deco =
        if focused { deco.border(Border::new(c.ring, 2.0)) } else { deco.border(Border::new(border, 1.5)) };
    let ring = Container::new()
        .decoration(deco)
        .width(dim)
        .height(dim)
        .alignment(Alignment::CENTER)
        .child(center(dot));

    let body = labeled(ring.into_widget(), p.size, p.label.clone(), p.description.clone(), p.disabled);
    let out = wire(body, p.disabled, &p.on_selected, hovered, node, p.autofocus);
    crate::widgets::semantics(
        crate::widgets::SemanticsRole::RadioButton,
        p.label.clone().unwrap_or_default(),
        out,
    )
    .checked(p.selected)
    .disabled(p.disabled)
    .into_widget()
}

// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

/// The visual style of a [`Toggle`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToggleVariant {
    /// Transparent until hovered/active (shadcn `default`).
    #[default]
    Default,
    /// Bordered even when inactive (shadcn `outline`).
    Outline,
}

/// A two-state toggle button wrapping any child.
#[derive(Clone, Default)]
pub struct Toggle {
    pressed: bool,
    child: Option<AnyWidget>,
    variant: ToggleVariant,
    size: ToggleSize,
    color: Option<Color>,
    radius: Option<f64>,
    disabled: bool,
    autofocus: bool,
    on_changed: Option<Callback>,
}

/// Create a [`Toggle`] around `child`.
pub fn toggle(pressed: bool, child: impl IntoWidget) -> Toggle {
    Toggle { pressed, child: Some(child.into_widget()), ..Default::default() }
}

impl Toggle {
    pub fn variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: ToggleSize) -> Self {
        self.size = size;
        self
    }
    /// Custom background for the active (pressed) state (defaults to the theme accent).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Corner radius override (defaults to the theme radius; a joined
    /// [`ToggleGroup`](super::ToggleGroup) flattens its cells with `0.0`).
    pub fn radius(mut self, radius: f64) -> Self {
        self.radius = Some(radius);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    pub fn on_changed(mut self, cb: impl IntoCallback) -> Self {
        self.on_changed = Some(cb.into_callback());
        self
    }
}

struct ToggleProps {
    pressed: bool,
    child: AnyWidget,
    variant: ToggleVariant,
    size: ToggleSize,
    color: Option<Color>,
    radius: Option<f64>,
    disabled: bool,
    autofocus: bool,
    on_changed: Option<Callback>,
}

impl IntoWidget for Toggle {
    fn into_widget(mut self) -> AnyWidget {
        let child = self.child.take().expect("toggle child");
        component_props(
            render_toggle,
            ToggleProps {
                pressed: self.pressed,
                child,
                variant: self.variant,
                size: self.size,
                color: self.color,
                radius: self.radius,
                disabled: self.disabled,
                autofocus: self.autofocus,
                on_changed: self.on_changed,
            },
        )
        .into_widget()
    }
}

fn render_toggle(p: &ToggleProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let node = create_focus();
    let t = animated(if p.pressed { 1.0 } else { 0.0 }, DUR);
    let hv = if p.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let focused = !p.disabled && node.is_focused();

    // Active = accent fill; inactive fades the accent in on hover.
    let active = p.color.unwrap_or(c.accent);
    let hover_bg = mix(c.background, c.accent, 0.6 * hv as f32);
    let bg = mix(hover_bg, active, t as f32);

    let mut deco =
        BoxDecoration::new().color(bg).radius(BorderRadius::all(p.radius.unwrap_or(theme().radius)));
    if focused {
        deco = deco.border(Border::new(c.ring, 2.0));
    } else if p.variant == ToggleVariant::Outline {
        deco = deco.border(Border::new(c.border, 1.0));
    }
    let container = Container::new()
        .decoration(deco)
        .padding(p.size.toggle_pad())
        .alignment(Alignment::CENTER)
        .child(center(p.child.clone()));

    wire(container.into_widget(), p.disabled, &p.on_changed, hovered, node, p.autofocus)
}

// ---------------------------------------------------------------------------
// RadioGroup
// ---------------------------------------------------------------------------

/// A set of mutually-exclusive [`Radio`]s — shadcn's `RadioGroup`. Self-managing
/// (seed the choice with [`value`](RadioGroup::value)); picking one selects it and
/// reports the chosen index through [`on_changed`](RadioGroup::on_changed).
pub struct RadioGroup {
    options: Vec<String>,
    descriptions: Vec<Option<String>>,
    value: usize,
    orientation: Axis,
    size: ToggleSize,
    color: Option<Color>,
    disabled: bool,
    on_changed: Option<Rc<dyn Fn(usize)>>,
}

/// Create a [`RadioGroup`] over `options`.
pub fn radio_group<I, S>(options: I) -> RadioGroup
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let options: Vec<String> = options.into_iter().map(Into::into).collect();
    let descriptions = vec![None; options.len()];
    RadioGroup {
        options,
        descriptions,
        value: 0,
        orientation: Axis::Vertical,
        size: ToggleSize::default(),
        color: None,
        disabled: false,
        on_changed: None,
    }
}

impl RadioGroup {
    /// The initially-selected index (default `0`).
    pub fn value(mut self, index: usize) -> Self {
        self.value = index;
        self
    }
    /// A muted description under option `index`.
    pub fn description(mut self, index: usize, s: impl Into<String>) -> Self {
        if index < self.descriptions.len() {
            self.descriptions[index] = Some(s.into());
        }
        self
    }
    /// Lay the options out horizontally instead of vertically.
    pub fn orientation(mut self, axis: Axis) -> Self {
        self.orientation = axis;
        self
    }
    pub fn size(mut self, size: ToggleSize) -> Self {
        self.size = size;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Called with the chosen index when the selection changes.
    pub fn on_changed(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

struct RadioGroupProps {
    options: Vec<String>,
    descriptions: Vec<Option<String>>,
    value: usize,
    orientation: Axis,
    size: ToggleSize,
    color: Option<Color>,
    disabled: bool,
    on_changed: Option<Rc<dyn Fn(usize)>>,
}

impl IntoWidget for RadioGroup {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_radio_group,
            RadioGroupProps {
                options: self.options,
                descriptions: self.descriptions,
                value: self.value,
                orientation: self.orientation,
                size: self.size,
                color: self.color,
                disabled: self.disabled,
                on_changed: self.on_changed,
            },
        )
        .into_widget()
    }
}

fn render_radio_group(p: &RadioGroupProps) -> AnyWidget {
    let selected = create_signal(p.value);
    let cur = selected.get();

    let items: Vec<AnyWidget> = p
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let mut r = radio(cur == i).label(opt.clone()).size(p.size).disabled(p.disabled);
            if let Some(c) = p.color {
                r = r.color(c);
            }
            if let Some(Some(d)) = p.descriptions.get(i) {
                r = r.description(d.clone());
            }
            let on = p.on_changed.clone();
            r.on_selected(move || {
                selected.set(i);
                if let Some(cb) = &on {
                    cb(i);
                }
            })
            .into_widget()
        })
        .collect();

    match p.orientation {
        Axis::Vertical => column(items)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(12.0)
            .into_widget(),
        Axis::Horizontal => row(items).main_axis_size(MainAxisSize::Min).spacing(20.0).into_widget(),
    }
}
