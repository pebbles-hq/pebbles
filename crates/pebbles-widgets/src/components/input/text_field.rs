//! [`TextField`] — a themed, editable single-line input, plus [`text_area`] for a
//! multiline variant and `.password()` for obscured entry.
//!
//! State is Solid-style: the component owns `value`, `anchor` and `focus` signals
//! plus undo/redo stacks, registers itself as the focused editor, and mutates them
//! on each [`KeyInput`]. Full standard editing: type/backspace/delete, arrow +
//! Home/End (Shift to select, Ctrl for word/word-delete), Ctrl+A select-all,
//! Ctrl+C/X/V clipboard, Ctrl+Z/Ctrl+Shift+Z undo/redo, and mouse (click to place
//! the caret, drag to select, double-click to select a word, Shift-click to extend).

use std::rc::Rc;

use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets};
use pebbles_render::text_edit as edit;
use pebbles_render::{BorderRadius, BoxDecoration, Cursor, IconKind, PointerEvent, TextFieldStyle};

use crate::components::icon;
use crate::theme::{mix, theme};
use crate::widgets::{
    Container, Expanded, GestureDetector, Opacity, SizedBox, column, editable, row, text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    KeyInput, Motion, Signal, action_event, animated, clipboard, component_props, create_focus,
    create_signal, keyboard,
};

/// One undo/redo snapshot: text + selection.
type Snap = (String, usize, usize);

/// A themed text input. Build with [`text_field`] / [`text_area`].
pub struct TextField {
    placeholder: String,
    initial: String,
    width: Option<f64>,
    multiline: bool,
    lines: u32,
    obscure: Option<char>,
    leading: Option<IconKind>,
    trailing: Option<AnyWidget>,
    char_filter: Option<Rc<dyn Fn(char) -> bool>>,
    max_length: Option<usize>,
    bind: Option<Signal<String>>,
    format: Option<Rc<dyn Fn(&str) -> String>>,
    label: Option<String>,
    helper: Option<String>,
    error: Option<String>,
    disabled: bool,
    autofocus: bool,
    on_changed: Option<Rc<dyn Fn(&str)>>,
    on_submit: Option<Rc<dyn Fn(&str)>>,
    on_editing_complete: Option<Rc<dyn Fn()>>,
    on_tap: Option<Rc<dyn Fn()>>,
    on_focus_change: Option<Rc<dyn Fn(bool)>>,
}

/// A single-line text input.
pub fn text_field() -> TextField {
    TextField {
        placeholder: String::new(),
        initial: String::new(),
        width: None,
        multiline: false,
        lines: 1,
        obscure: None,
        leading: None,
        trailing: None,
        char_filter: None,
        max_length: None,
        bind: None,
        format: None,
        label: None,
        helper: None,
        error: None,
        disabled: false,
        autofocus: false,
        on_changed: None,
        on_submit: None,
        on_editing_complete: None,
        on_tap: None,
        on_focus_change: None,
    }
}

/// A multiline text input (textarea), `lines` rows tall.
pub fn text_area(lines: u32) -> TextField {
    TextField { multiline: true, lines: lines.max(2), ..text_field() }
}

impl TextField {
    /// Placeholder shown while empty.
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    /// Initial text.
    pub fn value(mut self, s: impl Into<String>) -> Self {
        self.initial = s.into();
        self
    }
    /// Fixed width (else it fills its parent).
    pub fn width(mut self, w: f64) -> Self {
        self.width = Some(w);
        self
    }
    /// Obscure input (password). Renders each character as `•`.
    pub fn password(mut self) -> Self {
        self.obscure = Some('•');
        self
    }
    /// Toggle obscuring (for a password show/hide control).
    pub fn obscured(mut self, on: bool) -> Self {
        self.obscure = on.then_some('•');
        self
    }
    /// An icon inside the field on the left.
    pub fn leading(mut self, icon: IconKind) -> Self {
        self.leading = Some(icon);
        self
    }
    /// A widget inside the field on the right (a clear button, a toggle, …).
    pub fn trailing(mut self, w: impl IntoWidget) -> Self {
        self.trailing = Some(w.into_widget());
        self
    }
    /// Only accept characters matching `f` (e.g. digits for a number field).
    pub fn filter(mut self, f: impl Fn(char) -> bool + 'static) -> Self {
        self.char_filter = Some(Rc::new(f));
        self
    }
    /// Cap the number of characters.
    pub fn max_length(mut self, n: usize) -> Self {
        self.max_length = Some(n);
        self
    }
    /// Bind the value to an external signal (controlled — lets callers read/reset it,
    /// e.g. a search field's clear button).
    pub fn bind(mut self, value: Signal<String>) -> Self {
        self.bind = Some(value);
        self
    }
    /// Reformat the value after every edit (input masking — e.g. a date to
    /// `MM/DD/YYYY`). The caret moves to the end after formatting.
    pub fn format(mut self, f: impl Fn(&str) -> String + 'static) -> Self {
        self.format = Some(Rc::new(f));
        self
    }
    /// A label rendered above the field (shadcn form field).
    pub fn label(mut self, s: impl Into<String>) -> Self {
        self.label = Some(s.into());
        self
    }
    /// Helper text rendered below the field (hidden when an error is shown).
    pub fn helper(mut self, s: impl Into<String>) -> Self {
        self.helper = Some(s.into());
        self
    }
    /// Put the field into the error state: a destructive border + this message below.
    pub fn error(mut self, s: impl Into<String>) -> Self {
        self.error = Some(s.into());
        self
    }
    /// Conditionally set the error state (`None` clears it).
    pub fn error_opt(mut self, e: Option<String>) -> Self {
        self.error = e;
        self
    }
    /// Disable the field — dimmed, non-interactive, not focusable.
    pub fn disabled(mut self, on: bool) -> Self {
        self.disabled = on;
        self
    }
    /// Grab keyboard focus on mount.
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// Fired with the full text on every edit (Flutter's `onChanged`).
    pub fn on_changed(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
    /// Fired with the text when Enter is pressed in a single-line field
    /// (Flutter's `onSubmitted`).
    pub fn on_submit(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }
    /// Fired when editing completes — Enter in a single-line field (Flutter's
    /// `onEditingComplete`). Runs before `on_submit`.
    pub fn on_editing_complete(mut self, f: impl Fn() + 'static) -> Self {
        self.on_editing_complete = Some(Rc::new(f));
        self
    }
    /// Fired when the field is tapped (Flutter's `onTap`).
    pub fn on_tap(mut self, f: impl Fn() + 'static) -> Self {
        self.on_tap = Some(Rc::new(f));
        self
    }
    /// Fired with `true`/`false` when the field gains/loses focus.
    pub fn on_focus_change(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus_change = Some(Rc::new(f));
        self
    }
}

struct Props {
    placeholder: String,
    initial: String,
    width: Option<f64>,
    multiline: bool,
    lines: u32,
    obscure: Option<char>,
    leading: Option<IconKind>,
    trailing: Option<AnyWidget>,
    char_filter: Option<Rc<dyn Fn(char) -> bool>>,
    max_length: Option<usize>,
    bind: Option<Signal<String>>,
    format: Option<Rc<dyn Fn(&str) -> String>>,
    label: Option<String>,
    helper: Option<String>,
    error: Option<String>,
    disabled: bool,
    autofocus: bool,
    on_changed: Option<Rc<dyn Fn(&str)>>,
    on_submit: Option<Rc<dyn Fn(&str)>>,
    on_editing_complete: Option<Rc<dyn Fn()>>,
    on_tap: Option<Rc<dyn Fn()>>,
    on_focus_change: Option<Rc<dyn Fn(bool)>>,
}

impl IntoWidget for TextField {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_field,
            Props {
                placeholder: self.placeholder,
                initial: self.initial,
                width: self.width,
                multiline: self.multiline,
                lines: self.lines,
                obscure: self.obscure,
                leading: self.leading,
                trailing: self.trailing,
                char_filter: self.char_filter,
                max_length: self.max_length,
                bind: self.bind,
                format: self.format,
                label: self.label,
                helper: self.helper,
                error: self.error,
                disabled: self.disabled,
                autofocus: self.autofocus,
                on_changed: self.on_changed,
                on_submit: self.on_submit,
                on_editing_complete: self.on_editing_complete,
                on_tap: self.on_tap,
                on_focus_change: self.on_focus_change,
            },
        )
        .into_widget()
    }
}

fn prev_boundary(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map(|(j, _)| j).unwrap_or(0)
}
fn next_boundary(s: &str, i: usize) -> usize {
    s[i..].char_indices().nth(1).map(|(j, _)| i + j).unwrap_or(s.len())
}
fn ordered(a: usize, f: usize) -> (usize, usize) {
    (a.min(f), a.max(f))
}

/// Pure word-left (char-class), used when no shaped layout is available (empty or
/// password fields). Skips trailing whitespace, then the word.
fn pure_word_left(s: &str, mut i: usize) -> usize {
    while i > 0 {
        let p = prev_boundary(s, i);
        if s[p..i].chars().next().is_some_and(char::is_whitespace) { i = p } else { break }
    }
    while i > 0 {
        let p = prev_boundary(s, i);
        if s[p..i].chars().next().is_some_and(|c| !c.is_whitespace()) { i = p } else { break }
    }
    i
}
fn pure_word_right(s: &str, mut i: usize) -> usize {
    while i < s.len() {
        let n = next_boundary(s, i);
        if s[i..n].chars().next().is_some_and(char::is_whitespace) { i = n } else { break }
    }
    while i < s.len() {
        let n = next_boundary(s, i);
        if s[i..n].chars().next().is_some_and(|c| !c.is_whitespace()) { i = n } else { break }
    }
    i
}

/// Resolve a caret/selection motion, preferring the layout (parley handles bidi,
/// clusters and multiline) and falling back to pure logic when none is published.
fn resolve_motion(
    id: u64,
    v: &str,
    a: usize,
    f: usize,
    motion: Motion,
    extend: bool,
    multiline: bool,
) -> (usize, usize) {
    let laid = match motion {
        Motion::Left => edit::left(id, a, f, extend),
        Motion::Right => edit::right(id, a, f, extend),
        Motion::WordLeft => edit::word_left(id, a, f, extend),
        Motion::WordRight => edit::word_right(id, a, f, extend),
        Motion::LineStart if multiline => edit::line_start(id, a, f, extend),
        Motion::LineEnd if multiline => edit::line_end(id, a, f, extend),
        Motion::Up => edit::line_up(id, a, f, extend),
        Motion::Down => edit::line_down(id, a, f, extend),
        _ => None,
    };
    if let Some(r) = laid {
        return r;
    }
    let target = match motion {
        Motion::Left => prev_boundary(v, f),
        Motion::Right => next_boundary(v, f),
        Motion::WordLeft => pure_word_left(v, f),
        Motion::WordRight => pure_word_right(v, f),
        Motion::LineStart | Motion::DocStart | Motion::Up => 0,
        Motion::LineEnd | Motion::DocEnd | Motion::Down => v.len(),
    };
    if extend { (a, target) } else { (target, target) }
}

/// State bundle threaded into the editor handler (all `Copy` signals).
#[derive(Clone, Copy)]
struct Editor {
    value: Signal<String>,
    anchor: Signal<usize>,
    focus: Signal<usize>,
    undo: Signal<Vec<Snap>>,
    redo: Signal<Vec<Snap>>,
    id: u64,
    multiline: bool,
}

impl Editor {
    fn push_undo(&self, v: &str, a: usize, f: usize) {
        self.undo.update(|st| {
            st.push((v.to_string(), a, f));
            if st.len() > 200 {
                st.remove(0);
            }
        });
        self.redo.set(Vec::new());
    }

    /// Apply one command; returns (text_changed, submitted). `filter` restricts
    /// insertable characters; `max_length` caps the total character count.
    fn apply(
        &self,
        cmd: KeyInput,
        filter: Option<&dyn Fn(char) -> bool>,
        max_length: Option<usize>,
        format: Option<&dyn Fn(&str) -> String>,
    ) -> (bool, bool) {
        let mut v = self.value.peek();
        let mut a = self.anchor.peek().min(v.len());
        let mut f = self.focus.peek().min(v.len());
        let (s0, s1) = ordered(a, f);
        let has_sel = s0 != s1;
        let mut changed = false;
        let mut submit = false;

        match cmd {
            KeyInput::Insert(t) => {
                let mut t = if self.multiline { t } else { t.replace(['\n', '\r'], "") };
                if let Some(keep) = filter {
                    t = t.chars().filter(|ch| keep(*ch)).collect();
                }
                if let Some(max) = max_length {
                    let remaining = max
                        .saturating_sub(v.chars().count() - v[s0..s1].chars().count());
                    t = t.chars().take(remaining).collect();
                }
                if t.is_empty() {
                    return (false, false);
                }
                self.push_undo(&v, a, f);
                v.replace_range(s0..s1, &t);
                let c = s0 + t.len();
                a = c;
                f = c;
                changed = true;
            }
            KeyInput::Enter => {
                if self.multiline {
                    self.push_undo(&v, a, f);
                    v.replace_range(s0..s1, "\n");
                    let c = s0 + 1;
                    a = c;
                    f = c;
                    changed = true;
                } else {
                    submit = true;
                }
            }
            KeyInput::Backspace => {
                self.push_undo(&v, a, f);
                if has_sel {
                    v.replace_range(s0..s1, "");
                    a = s0;
                    f = s0;
                    changed = true;
                } else if f > 0 {
                    let p = prev_boundary(&v, f);
                    v.replace_range(p..f, "");
                    a = p;
                    f = p;
                    changed = true;
                }
            }
            KeyInput::Delete => {
                self.push_undo(&v, a, f);
                if has_sel {
                    v.replace_range(s0..s1, "");
                    a = s0;
                    f = s0;
                    changed = true;
                } else if f < v.len() {
                    let n = next_boundary(&v, f);
                    v.replace_range(f..n, "");
                    changed = true;
                }
            }
            KeyInput::DeleteWordBack => {
                self.push_undo(&v, a, f);
                if has_sel {
                    v.replace_range(s0..s1, "");
                    a = s0;
                    f = s0;
                } else {
                    let t = resolve_motion(self.id, &v, f, f, Motion::WordLeft, false, self.multiline).1;
                    let (lo, hi) = ordered(t, f);
                    v.replace_range(lo..hi, "");
                    a = lo;
                    f = lo;
                }
                changed = true;
            }
            KeyInput::DeleteWordForward => {
                self.push_undo(&v, a, f);
                if has_sel {
                    v.replace_range(s0..s1, "");
                    a = s0;
                    f = s0;
                } else {
                    let t = resolve_motion(self.id, &v, f, f, Motion::WordRight, false, self.multiline).1;
                    let (lo, hi) = ordered(f, t);
                    v.replace_range(lo..hi, "");
                    a = lo;
                    f = lo;
                }
                changed = true;
            }
            KeyInput::Move { motion, extend } => {
                let (na, nf) = resolve_motion(self.id, &v, a, f, motion, extend, self.multiline);
                a = na;
                f = nf;
            }
            KeyInput::SelectAll => {
                a = 0;
                f = v.len();
            }
            KeyInput::Copy => {
                if has_sel {
                    clipboard::write(&v[s0..s1]);
                }
            }
            KeyInput::Cut => {
                if has_sel {
                    clipboard::write(&v[s0..s1]);
                    self.push_undo(&v, a, f);
                    v.replace_range(s0..s1, "");
                    a = s0;
                    f = s0;
                    changed = true;
                }
            }
            KeyInput::Paste => {
                let mut p = clipboard::read();
                if !self.multiline {
                    p = p.replace(['\n', '\r'], "");
                }
                if !p.is_empty() {
                    self.push_undo(&v, a, f);
                    v.replace_range(s0..s1, &p);
                    let c = s0 + p.len();
                    a = c;
                    f = c;
                    changed = true;
                }
            }
            KeyInput::Undo => {
                if let Some((pv, pa, pf)) = self.undo.peek().last().cloned() {
                    self.redo.update(|r| r.push((v.clone(), a, f)));
                    self.undo.update(|u| {
                        u.pop();
                    });
                    v = pv;
                    a = pa;
                    f = pf;
                    changed = true;
                }
            }
            KeyInput::Redo => {
                if let Some((nv, na, nf)) = self.redo.peek().last().cloned() {
                    self.undo.update(|u| u.push((v.clone(), a, f)));
                    self.redo.update(|r| {
                        r.pop();
                    });
                    v = nv;
                    a = na;
                    f = nf;
                    changed = true;
                }
            }
            KeyInput::Escape => {
                pebbles_core::focus::set_focus(None);
                return (false, false);
            }
        }

        // Input masking: reformat after an edit and drop the caret at the end.
        if changed && let Some(fmt) = format {
            let formatted = fmt(&v);
            if formatted != v {
                v = formatted;
                a = v.len();
                f = v.len();
            }
        }

        a = a.min(v.len());
        f = f.min(v.len());
        self.value.set(v);
        self.anchor.set(a);
        self.focus.set(f);
        (changed, submit)
    }
}

fn render_field(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let disabled = p.disabled;
    let has_error = p.error.is_some();
    let focus = create_focus();
    // Controlled (bound to an external signal) or uncontrolled (internal).
    let value = match p.bind {
        Some(sig) => sig,
        None => create_signal(p.initial.clone()),
    };
    let start = value.peek().len();
    let ed = Editor {
        value,
        anchor: create_signal(start),
        focus: create_signal(start),
        undo: create_signal(Vec::new()),
        redo: create_signal(Vec::new()),
        id: focus.raw_id(),
        multiline: p.multiline,
    };
    let focused = !disabled && focus.is_focused();

    // Only a live (enabled) field is focusable + edits.
    if !disabled {
        focus.register(Rc::new(|| {}), p.on_focus_change.clone(), p.autofocus);
        let on_changed = p.on_changed.clone();
        let on_submit = p.on_submit.clone();
        let on_editing = p.on_editing_complete.clone();
        let filter = p.char_filter.clone();
        let max_length = p.max_length;
        let format = p.format.clone();
        focus.register_editor(Rc::new(move |k: KeyInput| {
            let (changed, submit) =
                ed.apply(k, filter.as_deref(), max_length, format.as_deref());
            if changed && let Some(cb) = &on_changed {
                cb(&ed.value.peek());
            }
            if submit {
                if let Some(cb) = &on_editing {
                    cb();
                }
                if let Some(cb) = &on_submit {
                    cb(&ed.value.peek());
                }
            }
        }));
    }

    let fg = if disabled { c.muted_foreground } else { c.foreground };
    let style = TextFieldStyle {
        font_size: 14.0,
        weight: 400.0,
        line_height: 1.3,
        color: fg,
        placeholder_color: c.muted_foreground,
        caret_color: c.foreground,
        selection_color: {
            let [r, g, b, _] = c.primary.components;
            pebbles_foundation::Color::new([r, g, b, 0.25])
        },
    };

    let val = ed.value.get();
    let vlen = val.len();
    let inner = editable(val)
        .placeholder(p.placeholder.clone())
        .selection(ed.anchor.get().min(vlen), ed.focus.get().min(vlen))
        .focused(focused)
        .obscure(p.obscure)
        .multiline(p.multiline)
        .field_id(ed.id)
        .style(style);

    // Compose with an optional leading icon and/or trailing widget.
    let content: AnyWidget = if (p.leading.is_some() || p.trailing.is_some()) && !p.multiline {
        let mut kids: Vec<AnyWidget> = Vec::new();
        if let Some(lead) = p.leading {
            kids.push(icon(lead).size(16.0).color(c.muted_foreground).into_widget());
            kids.push(SizedBox::spacer(8.0, 0.0).into_widget());
        }
        kids.push(Expanded::new(inner).into_widget());
        if let Some(trail) = p.trailing.clone() {
            kids.push(SizedBox::spacer(6.0, 0.0).into_widget());
            kids.push(trail);
        }
        row(kids).cross_axis_alignment(CrossAxisAlignment::Center).into_widget()
    } else {
        inner.into_widget()
    };

    // Border: destructive when in error, else input→ring cross-fade on focus.
    let fr = animated(if focused { 1.0 } else { 0.0 }, 0.14);
    let (border_color, border_w) =
        if has_error { (c.destructive, 1.5) } else { (mix(c.input, c.ring, fr as f32), 1.0 + fr) };
    let bg = if disabled { c.muted } else { c.background };

    let (height, padding, align) = if p.multiline {
        (p.lines as f64 * 20.0 + 20.0, EdgeInsets::all(10.0), Alignment::TOP_LEFT)
    } else {
        (38.0, EdgeInsets::symmetric(12.0, 0.0), Alignment::CENTER_LEFT)
    };
    let lead_off = if p.leading.is_some() { 24.0 } else { 0.0 };
    let (cl, ct) = if p.multiline { (11.0, 11.0) } else { (13.0 + lead_off, 10.0) };

    let mut field = Container::new()
        .decoration(
            BoxDecoration::new()
                .color(bg)
                .border(pebbles_render::Border::new(border_color, border_w))
                .radius(BorderRadius::all(theme().radius)),
        )
        .padding(padding)
        .height(height)
        .alignment(align)
        .child(content);
    if let Some(w) = p.width {
        field = field.width(w);
    }

    // The interactive (or disabled) field box.
    let field_box: AnyWidget = if disabled {
        GestureDetector::new(Opacity::new(0.6, field)).cursor(Cursor::NotAllowed).into_widget()
    } else {
        let on_tap = p.on_tap.clone();
        GestureDetector::new(field)
            .cursor(Cursor::Text)
            .on_pan_start(action_event(move |e: PointerEvent| {
                focus.request_focus();
                place_caret(ed, e, cl, ct, keyboard::shift_held());
                if let Some(cb) = &on_tap {
                    cb();
                }
            }))
            .on_pan_update(action_event(move |e: PointerEvent| {
                let (tx, ty) = (e.position.x - cl, e.position.y - ct);
                if let Some((a, f)) =
                    edit::extend_to(ed.id, ed.anchor.peek(), ed.focus.peek(), tx, ty)
                {
                    ed.anchor.set(a);
                    ed.focus.set(f);
                }
            }))
            .on_double_tap(action_event(move |e: PointerEvent| {
                let (tx, ty) = (e.position.x - cl, e.position.y - ct);
                if let Some((a, f)) = edit::word_at(ed.id, tx, ty) {
                    ed.anchor.set(a);
                    ed.focus.set(f);
                }
            }))
            .into_widget()
    };

    // Wrap with an optional label above and helper/error below (shadcn form field).
    if p.label.is_none() && p.helper.is_none() && p.error.is_none() {
        return field_box;
    }
    let mut col: Vec<AnyWidget> = Vec::new();
    if let Some(lbl) = &p.label {
        col.push(text(lbl.clone()).size(13.5).weight(500.0).color(c.foreground).into_widget());
        col.push(SizedBox::spacer(0.0, 7.0).into_widget());
    }
    col.push(field_box);
    if let Some(err) = &p.error {
        col.push(SizedBox::spacer(0.0, 6.0).into_widget());
        col.push(text(err.clone()).size(12.5).color(c.destructive).into_widget());
    } else if let Some(help) = &p.helper {
        col.push(SizedBox::spacer(0.0, 6.0).into_widget());
        col.push(text(help.clone()).size(12.5).color(c.muted_foreground).into_widget());
    }
    column(col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().into_widget()
}

/// Place the caret from a pointer press. With `extend`, keep the anchor (Shift-click).
fn place_caret(ed: Editor, e: PointerEvent, cl: f64, ct: f64, extend: bool) {
    let (tx, ty) = (e.position.x - cl, e.position.y - ct);
    let byte = edit::hit(ed.id, tx, ty).unwrap_or(0).min(ed.value.peek().len());
    if extend {
        ed.focus.set(byte);
    } else {
        ed.anchor.set(byte);
        ed.focus.set(byte);
    }
}
