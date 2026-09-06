//! [`Slider`] — a draggable value track in the shadcn style. A rounded `secondary`
//! track, a `primary`-filled range, and circular thumbs (2px primary border on
//! `background`, soft shadow, hover + focus ring).
//!
//! Mirrors shadcn/Radix `Slider`: a real `min`/`max`/`step` value domain, **one or
//! more thumbs** (pass [`range`](Slider::range) for a two-thumb range selector),
//! horizontal or vertical [`orientation`](Slider::orientation), a
//! [`disabled`](Slider::disabled) state, and full keyboard control (arrows step the
//! focused thumb, Home/End jump to the ends). Click/drag the track to move the
//! nearest thumb; every change is reported through `on_changed` as the current
//! value list.

use std::rc::Rc;

use pebbles_foundation::{Alignment, Axis, Color, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Opacity, Positioned, stack};
use pebbles_core::focus::create_focus;
use pebbles_core::keyboard::{KeyInput, Motion};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action_event, component_props, create_signal};

const HIT: f64 = 20.0; // interaction thickness (cross-axis)
const TRACK: f64 = 6.0; // shadcn track thickness
const THUMB: f64 = 16.0; // thumb diameter

/// A draggable value slider over a `min..=max` domain with one or more thumbs.
pub struct Slider {
    length: f64,
    min: f64,
    max: f64,
    step: f64,
    values: Option<Vec<f64>>,
    orientation: Axis,
    disabled: bool,
    autofocus: bool,
    /// The accessible name (read out by the screen reader).
    label: Option<String>,
    on_changed: Option<Rc<dyn Fn(Vec<f64>)>>,
}

/// Create a [`Slider`] of the given length (width when horizontal, height when
/// vertical). Defaults to the shadcn domain: `0..=100`, `step` 1, a single thumb at
/// the midpoint.
pub fn slider(length: f64) -> Slider {
    Slider {
        length,
        min: 0.0,
        max: 100.0,
        step: 1.0,
        values: None,
        orientation: Axis::Horizontal,
        disabled: false,
        autofocus: false,
        label: None,
        on_changed: None,
    }
}

impl Slider {
    /// Lower bound of the value domain (default `0`).
    pub fn min(mut self, v: f64) -> Self {
        self.min = v;
        self
    }
    /// Upper bound of the value domain (default `100`).
    pub fn max(mut self, v: f64) -> Self {
        self.max = v;
        self
    }
    /// Snap increment; `0` for a continuous slider (default `1`).
    pub fn step(mut self, v: f64) -> Self {
        self.step = v.max(0.0);
        self
    }
    /// A single-thumb initial value.
    pub fn value(mut self, v: f64) -> Self {
        self.values = Some(vec![v]);
        self
    }
    /// A two-thumb range selector, seeded with `[lo, hi]`.
    pub fn range(mut self, lo: f64, hi: f64) -> Self {
        self.values = Some(vec![lo.min(hi), lo.max(hi)]);
        self
    }
    /// Orient vertically instead of horizontally.
    pub fn orientation(mut self, axis: Axis) -> Self {
        self.orientation = axis;
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
    /// The accessible name read out by the screen reader (default "Slider").
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    /// Fired with the full value list on every click / drag / key step.
    pub fn on_changed(mut self, f: impl Fn(Vec<f64>) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

struct SliderProps {
    length: f64,
    min: f64,
    max: f64,
    step: f64,
    values: Vec<f64>,
    orientation: Axis,
    disabled: bool,
    autofocus: bool,
    label: Option<String>,
    on_changed: Option<Rc<dyn Fn(Vec<f64>)>>,
}

impl IntoWidget for Slider {
    fn into_widget(self) -> AnyWidget {
        // Resolve the seed values: default to a single mid-domain thumb.
        let values = self.values.unwrap_or_else(|| vec![self.min + (self.max - self.min) * 0.5]);
        component_props(
            render_slider,
            SliderProps {
                length: self.length,
                min: self.min,
                max: self.max,
                step: self.step,
                values,
                orientation: self.orientation,
                disabled: self.disabled,
                autofocus: self.autofocus,
                label: self.label,
                on_changed: self.on_changed,
            },
        )
        .into_widget()
    }
}

fn render_slider(p: &SliderProps) -> AnyWidget {
    let c = theme().colors;
    let (min, max, step, len, orient) = (p.min, p.max, p.step, p.length, p.orientation);
    let domain = (max - min).abs().max(1e-9);

    let vals = create_signal(p.values.clone());
    let active = create_signal(0usize);
    let hovered = create_signal(false);
    let node = create_focus();

    // Commit a new value to thumb `i`, snapped to `step` and clamped between its
    // neighbors, then report the whole list.
    let apply: Rc<dyn Fn(usize, f64)> = {
        let on = p.on_changed.clone();
        Rc::new(move |i: usize, v: f64| {
            vals.update(|xs| {
                let lo = if i > 0 { xs[i - 1] } else { min };
                let hi = if i + 1 < xs.len() { xs[i + 1] } else { max };
                let snapped = {
                    let v = v.clamp(min, max);
                    if step > 0.0 { min + ((v - min) / step).round() * step } else { v }
                };
                xs[i] = snapped.clamp(lo.min(hi), lo.max(hi));
            });
            if let Some(cb) = &on {
                cb(vals.peek());
            }
        })
    };

    // Map a local pointer position to a value and move the nearest thumb to it.
    let move_to: Rc<dyn Fn(f64, f64)> = {
        let apply = apply.clone();
        Rc::new(move |px: f64, py: f64| {
            let frac = match orient {
                Axis::Horizontal => px / len,
                Axis::Vertical => 1.0 - py / len,
            }
            .clamp(0.0, 1.0);
            let v = min + frac * domain;
            let xs = vals.peek();
            let mut i = 0;
            let mut best = f64::MAX;
            for (k, &x) in xs.iter().enumerate() {
                let d = (x - v).abs();
                if d < best {
                    best = d;
                    i = k;
                }
            }
            active.set(i);
            node.request_focus();
            apply(i, v);
        })
    };

    // Keyboard: arrows step the active thumb; Home/End jump to the domain ends.
    if !p.disabled {
        node.register(Rc::new(|| {}), None, p.autofocus);
        let apply_kb = apply.clone();
        node.register_editor(Rc::new(move |k: KeyInput| {
            let i = active.peek();
            let cur = vals.peek()[i];
            let stp = if step > 0.0 { step } else { domain / 100.0 };
            let nv = match k {
                KeyInput::Move { motion, .. } => match motion {
                    Motion::Left | Motion::Down => Some(cur - stp),
                    Motion::Right | Motion::Up => Some(cur + stp),
                    Motion::LineStart | Motion::DocStart => Some(min),
                    Motion::LineEnd | Motion::DocEnd => Some(max),
                    _ => None,
                },
                _ => None,
            };
            if let Some(nv) = nv {
                apply_kb(i, nv);
            }
        }));
    }

    // ----- geometry -----
    let values = vals.get();
    let active_i = active.get();
    let is_hovered = hovered.get();
    let focused = !p.disabled && node.is_focused();
    let frac = |v: f64| ((v - min) / domain).clamp(0.0, 1.0);
    let lo_f = if values.len() > 1 { frac(values[0]) } else { 0.0 };
    let hi_f = frac(*values.last().unwrap());

    let horiz = orient == Axis::Horizontal;
    let cross_off = |thickness: f64| (HIT - thickness) / 2.0;

    // Track + filled range.
    let track = if horiz {
        Positioned::new(rounded(len, TRACK, c.secondary)).left(0.0).top(cross_off(TRACK))
    } else {
        Positioned::new(rounded(TRACK, len, c.secondary)).left(cross_off(TRACK)).top(0.0)
    };
    let fill_len = ((hi_f - lo_f) * len).max(0.0);
    let fill = if horiz {
        Positioned::new(rounded(fill_len, TRACK, c.primary)).left(lo_f * len).top(cross_off(TRACK))
    } else {
        // vertical fill grows from the bottom (top = max)
        Positioned::new(rounded(TRACK, fill_len, c.primary)).left(cross_off(TRACK)).top(len * (1.0 - hi_f))
    };

    // Thumbs.
    let mut kids: Vec<AnyWidget> = vec![track.into_widget(), fill.into_widget()];
    for (i, &v) in values.iter().enumerate() {
        let f = frac(v);
        let ring = if focused && i == active_i {
            Some(Color::new([c.ring.components[0], c.ring.components[1], c.ring.components[2], 0.55]))
        } else if is_hovered {
            Some(Color::new([c.ring.components[0], c.ring.components[1], c.ring.components[2], 0.22]))
        } else {
            None
        };
        let t = mk_thumb(c.background, c.primary, ring);
        let pos = if horiz {
            Positioned::new(t).left((f * len - THUMB / 2.0).clamp(0.0, len - THUMB)).top(cross_off(THUMB))
        } else {
            Positioned::new(t)
                .left(cross_off(THUMB))
                .top((len * (1.0 - f) - THUMB / 2.0).clamp(0.0, len - THUMB))
        };
        kids.push(pos.into_widget());
    }

    let (body_w, body_h) = if horiz { (len, HIT) } else { (HIT, len) };
    let body = Container::new()
        .width(body_w)
        .height(body_h)
        .alignment(Alignment::TOP_LEFT)
        .child(stack(kids).alignment(Alignment::TOP_LEFT));

    let control: AnyWidget = if p.disabled {
        GestureDetector::new(Opacity::new(0.55, body)).cursor(Cursor::NotAllowed).into_widget()
    } else {
        let a_start = move_to.clone();
        let a_move = move_to.clone();
        GestureDetector::new(body)
            .cursor(Cursor::Pointer)
            .on_hover_enter(move || hovered.set(true))
            .on_hover_exit(move || hovered.set(false))
            .on_pan_start(action_event(move |e| a_start(e.position.x, e.position.y)))
            .on_pan_update(action_event(move |e| a_move(e.position.x, e.position.y)))
            // Sliders consume right-clicks by default.
            .on_secondary_tap(|| {})
            .into_widget()
    };

    // Accessibility: the slider announces its name + current values.
    let name = p.label.clone().unwrap_or_else(|| "Slider".to_string());
    let value = vals.get().iter().map(|v| format!("{v:.0}")).collect::<Vec<_>>().join(", ");
    crate::widgets::semantics(pebbles_render::SemanticsRole::Slider, name, control)
        .value(value)
        .disabled(p.disabled)
        .into_widget()
}

/// A rounded, solidly-colored bar.
fn rounded(w: f64, h: f64, color: Color) -> Container {
    Container::new()
        .width(w)
        .height(h)
        .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(999.0)))
}

/// The circular thumb: white fill, 2px accent border, soft drop shadow, plus an
/// optional hover/focus ring rendered as a spread shadow.
fn mk_thumb(bg: Color, accent: Color, ring: Option<Color>) -> Container {
    let mut deco = BoxDecoration::new()
        .color(bg)
        .border(Border::new(accent, 2.0))
        .radius(BorderRadius::all(999.0))
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 30), Offset::new(0.0, 1.0), 2.0, 0.0));
    if let Some(ring) = ring {
        // A 0-blur spread reads as a solid ring around the thumb.
        deco = deco.shadow(BoxShadow::new(ring, Offset::new(0.0, 0.0), 0.0, 4.0));
    }
    Container::new().width(THUMB).height(THUMB).decoration(deco)
}
