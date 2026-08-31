//! [`InputOtp`] — a segmented one-time-code input: `n` bordered cells backed by a
//! single hidden editor node owning a `String` signal. Digits append, Backspace
//! deletes, arrows move the active cell, Ctrl+V pastes and fills; `on_complete` fires
//! when all `n` slots are filled. Mirrors shadcn's Input OTP; Flutter-style builder API.

use std::rc::Rc;

use pebbles_foundation::{Alignment, MainAxisSize};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor};

use crate::theme::{mix, theme};
use crate::widgets::{Container, GestureDetector, Opacity, gap_w, row, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{KeyInput, animated, clipboard, component_props, create_signal};
use pebbles_core::focus::create_focus;

/// A one-time-code input. Build with [`input_otp`].
pub struct InputOtp {
    len: usize,
    group_size: usize,
    obscured: bool,
    disabled: bool,
    autofocus: bool,
    on_changed: Option<Rc<dyn Fn(&str)>>,
    on_complete: Option<Rc<dyn Fn(&str)>>,
}

/// Create an [`InputOtp`] with `len` cells (e.g. `input_otp(6)`).
pub fn input_otp(len: usize) -> InputOtp {
    InputOtp {
        len: len.max(1),
        group_size: 0,
        obscured: false,
        disabled: false,
        autofocus: false,
        on_changed: None,
        on_complete: None,
    }
}

impl InputOtp {
    /// Insert a separator dot after every `n` cells (e.g. `3` → `•••‑•••`).
    pub fn group_size(mut self, n: usize) -> Self {
        self.group_size = n;
        self
    }
    /// Render each entered digit as `•` (for secret codes).
    pub fn obscured(mut self, yes: bool) -> Self {
        self.obscured = yes;
        self
    }
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// Called with the current code on every change.
    pub fn on_changed(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
    /// Called once the code reaches `len` characters.
    pub fn on_complete(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_complete = Some(Rc::new(f));
        self
    }
}

struct Props {
    len: usize,
    group_size: usize,
    obscured: bool,
    disabled: bool,
    autofocus: bool,
    on_changed: Option<Rc<dyn Fn(&str)>>,
    on_complete: Option<Rc<dyn Fn(&str)>>,
}

impl IntoWidget for InputOtp {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_otp,
            Props {
                len: self.len,
                group_size: self.group_size,
                obscured: self.obscured,
                disabled: self.disabled,
                autofocus: self.autofocus,
                on_changed: self.on_changed,
                on_complete: self.on_complete,
            },
        )
        .into_widget()
    }
}

const CELL: f64 = 40.0;

fn render_otp(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let len = p.len;
    let code = create_signal(String::new());
    // The active cell = the first empty slot, capped at the last cell.
    let active = create_signal(0usize);
    let focus = create_focus();
    let focused = !p.disabled && focus.is_focused();

    if !p.disabled {
        let on_changed = p.on_changed.clone();
        let on_complete = p.on_complete.clone();
        focus.register(Rc::new(|| {}), None, p.autofocus);
        focus.register_editor(Rc::new(move |k: KeyInput| {
            let mut s: String = code.peek();
            let mut moved = false;
            match k {
                KeyInput::Insert(t) => {
                    for ch in t.chars().filter(|c| !c.is_control()) {
                        if s.chars().count() >= len {
                            break;
                        }
                        s.push(ch);
                    }
                }
                KeyInput::Paste => {
                    for ch in clipboard::read().chars().filter(|c| !c.is_control()) {
                        if s.chars().count() >= len {
                            break;
                        }
                        s.push(ch);
                    }
                }
                KeyInput::Backspace => {
                    s.pop();
                }
                KeyInput::Move { motion, .. } => {
                    use pebbles_core::Motion::*;
                    let cur = active.peek();
                    match motion {
                        Left => active.set(cur.saturating_sub(1)),
                        Right => active.set((cur + 1).min(len - 1)),
                        _ => {}
                    }
                    moved = true;
                }
                _ => {}
            }
            if !moved {
                let n = s.chars().count();
                code.set(s.clone());
                active.set(n.min(len - 1));
                if let Some(cb) = &on_changed {
                    cb(&s);
                }
                if n == len && let Some(cb) = &on_complete {
                    cb(&s);
                }
            }
        }));
    }

    let digits: Vec<char> = code.get().chars().collect();
    let active_i = active.get();
    let ring = animated(if focused { 1.0 } else { 0.0 }, 0.14);

    let mut cells: Vec<AnyWidget> = Vec::with_capacity(len * 2);
    for i in 0..len {
        if p.group_size > 0 && i > 0 && i % p.group_size == 0 {
            cells.push(gap_w(4.0).into_widget());
            cells.push(
                Container::new()
                    .width(8.0)
                    .height(CELL)
                    .alignment(Alignment::CENTER)
                    .child(text("·").size(18.0).color(c.muted_foreground))
                    .into_widget(),
            );
            cells.push(gap_w(4.0).into_widget());
        } else if i > 0 {
            cells.push(gap_w(6.0).into_widget());
        }

        let is_active = focused && i == active_i;
        let border_color = if is_active { c.ring } else { mix(c.input, c.ring, ring as f32) };
        let border_w = if is_active { 2.0 } else { 1.0 };
        let glyph: Option<AnyWidget> = digits.get(i).map(|ch| {
            let shown = if p.obscured { '•' } else { *ch };
            text(shown.to_string()).size(18.0).weight(500.0).color(c.foreground).into_widget()
        });
        let cell = Container::new()
            .width(CELL)
            .height(CELL)
            .alignment(Alignment::CENTER)
            .decoration(
                BoxDecoration::new()
                    .color(c.background)
                    .border(Border::new(border_color, border_w))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .child(glyph.unwrap_or_else(|| gap_w(0.0).into_widget()));
        cells.push(cell.into_widget());
    }

    let field = row(cells).main_axis_size(MainAxisSize::Min);
    let field: AnyWidget = if p.disabled {
        Opacity::new(0.55, field).into_widget()
    } else {
        GestureDetector::new(field)
            .cursor(Cursor::Text)
            .on_tap(move || focus.request_focus())
            .into_widget()
    };
    field
}
