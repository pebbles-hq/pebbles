//! [`Slider`] — a draggable value track in the shadcn style: an 8px rounded
//! `secondary` track, a `primary`-filled range, and a 20px thumb (2px primary
//! border on `background`, soft shadow). Click anywhere on the track to jump, or
//! drag the thumb; `value` is `0.0..=1.0`, reported live through `on_changed`.

use std::rc::Rc;

use pebbles_foundation::{Alignment, Color, Offset};
use pebbles_render::{BorderRadius, BoxDecoration, BoxShadow, Cursor};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Positioned, stack};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action_event, children, component_props, create_signal};

/// A draggable value slider. `value` in `0.0..=1.0`.
pub struct Slider {
    width: f64,
    initial: f64,
    on_changed: Option<Rc<dyn Fn(f64)>>,
}

/// Create a [`Slider`] of the given width (starts at the midpoint).
pub fn slider(width: f64) -> Slider {
    Slider { width, initial: 0.5, on_changed: None }
}

impl Slider {
    /// Set the initial value (`0.0..=1.0`).
    pub fn value(mut self, v: f64) -> Self {
        self.initial = v.clamp(0.0, 1.0);
        self
    }
    /// Fired with the new value (`0.0..=1.0`) on every click/drag.
    pub fn on_changed(mut self, f: impl Fn(f64) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

struct SliderProps {
    width: f64,
    initial: f64,
    on_changed: Option<Rc<dyn Fn(f64)>>,
}

impl IntoWidget for Slider {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_slider,
            SliderProps { width: self.width, initial: self.initial, on_changed: self.on_changed },
        )
        .into_widget()
    }
}

const H: f64 = 20.0; // interaction height
const TRACK: f64 = 8.0; // shadcn h-2
const THUMB: f64 = 20.0; // shadcn h-5 w-5

fn render_slider(p: &SliderProps) -> GestureDetector {
    let c = theme().colors;
    let w = p.width;
    let value = create_signal(p.initial);
    let on_changed = p.on_changed.clone();

    let apply = Rc::new(move |x: f64| {
        let t = (x / w).clamp(0.0, 1.0);
        value.set(t);
        if let Some(cb) = &on_changed {
            cb(t);
        }
    });
    let a_start = apply.clone();
    let a_move = apply.clone();

    let v = value.get();
    let filled = (w * v).clamp(0.0, w);

    // Track (secondary) with the primary-filled range, both rounded-full.
    let track = Container::new()
        .width(w)
        .height(TRACK)
        .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(999.0)))
        .alignment(Alignment::CENTER_LEFT)
        .child(
            Container::new()
                .width(filled)
                .height(TRACK)
                .decoration(BoxDecoration::new().color(c.primary).radius(BorderRadius::all(999.0))),
        );

    // Thumb: white, 2px primary border, soft shadow.
    let thumb = Container::new().width(THUMB).height(THUMB).decoration(
        BoxDecoration::new()
            .color(c.background)
            .border(pebbles_render::Border::new(c.primary, 2.0))
            .radius(BorderRadius::all(999.0))
            .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 30), Offset::new(0.0, 1.0), 2.0, 0.0)),
    );

    let thumb_left = (filled - THUMB / 2.0).clamp(0.0, w - THUMB);

    let body = Container::new().width(w).height(H).alignment(Alignment::CENTER_LEFT).child(
        stack(children![track, Positioned::new(thumb).left(thumb_left).top((H - THUMB) / 2.0)])
            .alignment(Alignment::CENTER_LEFT),
    );

    GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_pan_start(action_event(move |e| a_start(e.position.x)))
        .on_pan_update(action_event(move |e| a_move(e.position.x)))
}
