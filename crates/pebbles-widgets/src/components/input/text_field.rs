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

use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::text_edit as edit;
use pebbles_render::{
    BoxDecoration, Cursor, IconData, IconKind, PointerEvent, TextFieldStyle, lucide,
};

use super::{ButtonVariant, icon_button};
use crate::components::icon;
use crate::theme::{mix, theme};
use crate::widgets::{Container, Expanded, GestureDetector, Opacity, column, editable, gap_h, gap_w, row, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    KeyInput, Motion, Signal, action_event, animated, clipboard, component_props,
    create_focus, create_signal, keyboard,
};

/// One undo/redo snapshot: text + selection.
type Snap = (String, usize, usize);

/// The kind of a text-based input. This is the **single knob** for every
/// text-based field — number, email, URL, currency, password, search, … — the way
/// Flutter's one `TextField` takes a `keyboardType`. It drives the character
/// filter, leading icon, placeholder and formatting, plus any built-in affordance
/// (a password show/hide toggle, a search clear button). Set with
/// [`TextField::kind`]; explicit `.filter()` / `.leading()` / `.placeholder()` /
/// `.format()` still override it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputKind {
    /// Free text (the default).
    #[default]
    Text,
    /// A signed decimal number (digits, `.`, `-`).
    Number,
    /// A signed whole number (digits, `-`).
    Integer,
    /// An unsigned decimal (digits, `.`).
    Decimal,
    /// An email address — envelope icon, no spaces.
    Email,
    /// A URL — no spaces.
    Url,
    /// A phone number — phone icon, digits and phone punctuation.
    Phone,
    /// A currency amount — `$` icon, digits/`.`, grouped as `$1,234.56`.
    Currency,
    /// Obscured entry — lock icon and a built-in show/hide (eye) toggle.
    Password,
    /// A search box — magnifier icon and a clear (×) button when non-empty.
    Search,
}

/// The character filter a kind imposes (before any explicit `.filter()`).
fn kind_filter(kind: InputKind) -> Option<Rc<dyn Fn(char) -> bool>> {
    let f: fn(char) -> bool = match kind {
        InputKind::Number => |c| c.is_ascii_digit() || c == '.' || c == '-',
        InputKind::Integer => |c| c.is_ascii_digit() || c == '-',
        InputKind::Decimal | InputKind::Currency => |c| c.is_ascii_digit() || c == '.',
        InputKind::Email | InputKind::Url => |c| !c.is_whitespace(),
        InputKind::Phone => |c| c.is_ascii_digit() || "()+- ".contains(c),
        _ => return None,
    };
    Some(Rc::new(f))
}

/// The leading icon a kind adds (before any explicit `.leading()`).
fn kind_leading(kind: InputKind) -> Option<IconData> {
    Some(match kind {
        InputKind::Email => IconKind::Mail.into(),
        InputKind::Phone => IconKind::Phone.into(),
        InputKind::Password => IconKind::Lock.into(),
        InputKind::Search => IconKind::Search.into(),
        InputKind::Currency => lucide::DOLLAR_SIGN,
        _ => return None,
    })
}

/// The default placeholder a kind suggests (used only when none is set).
fn kind_placeholder(kind: InputKind) -> &'static str {
    match kind {
        InputKind::Number | InputKind::Integer => "0",
        InputKind::Decimal => "0.00",
        InputKind::Currency => "$0.00",
        InputKind::Email => "you@example.com",
        InputKind::Url => "https://example.com",
        InputKind::Phone => "(555) 123-4567",
        InputKind::Password => "Password",
        InputKind::Search => "Search…",
        InputKind::Text => "",
    }
}

/// The input mask a kind applies (before any explicit `.format()`).
fn kind_format(kind: InputKind) -> Option<Rc<dyn Fn(&str) -> String>> {
    match kind {
        InputKind::Currency => Some(Rc::new(format_currency)),
        _ => None,
    }
}

/// Group a run of digits into thousands: `"1234567" → "1,234,567"`.
fn group_thousands(digits: &str) -> String {
    let d: String = digits.chars().filter(char::is_ascii_digit).collect();
    let n = d.len();
    let mut out = String::with_capacity(n + n / 3);
    for (i, ch) in d.chars().enumerate() {
        if i > 0 && (n - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Format a currency amount: strip to digits/`.`, group the integer part and
/// prefix `$` (`"1234.5" → "$1,234.5"`).
fn format_currency(s: &str) -> String {
    let cleaned: String = s.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    if cleaned.is_empty() {
        return String::new();
    }
    let mut it = cleaned.splitn(2, '.');
    let int_part = it.next().unwrap_or("");
    let frac_part = it.next();
    let grouped = group_thousands(int_part);
    let int_str = if grouped.is_empty() { "0".to_string() } else { grouped };
    match frac_part {
        Some(f) => {
            let f: String = f.chars().filter(char::is_ascii_digit).take(2).collect();
            format!("${int_str}.{f}")
        }
        None => format!("${int_str}"),
    }
}

/// A themed text input. Build with [`text_field`] / [`text_area`].
#[derive(Clone, Default)]
pub struct TextField {
    kind: InputKind,
    placeholder: String,
    initial: String,
    width: Option<f64>,
    multiline: bool,
    lines: u32,
    obscure: Option<char>,
    leading: Option<IconData>,
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
    on_nav: Option<Rc<dyn Fn(KeyInput) -> bool>>,
    style: Option<crate::style::Style>,
}

/// A single-line text input.
pub fn text_field() -> TextField {
    TextField { kind: InputKind::Text, ..Default::default() }
}

/// A multiline text input (textarea), `lines` rows tall.
pub fn text_area(lines: u32) -> TextField {
    TextField { multiline: true, lines: lines.max(2), ..text_field() }
}

impl TextField {
    /// Set the input **type** — the one control for number/email/url/currency/
    /// password/search/… It applies that type's character filter, leading icon,
    /// placeholder, formatting and any built-in affordance (password eye toggle,
    /// search clear button). Explicit `.filter()`, `.leading()`, `.placeholder()`
    /// and `.format()` still take precedence.
    pub fn kind(mut self, kind: InputKind) -> Self {
        self.kind = kind;
        self
    }
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
    /// Merge a [`Style`](crate::Style) onto the field box (bg / border / radius /
    /// shadow / width overrides). The label and helper/error text stay themed.
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
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
    pub fn leading(mut self, icon: impl Into<IconData>) -> Self {
        self.leading = Some(icon.into());
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
    /// Intercept keyboard intents before the field edits: `f(key)` runs first and,
    /// if it returns `true`, the key is consumed (the field does not edit). Used to
    /// drive list navigation (Up/Down/Enter/Escape) from a search field — see the
    /// combobox and command palette.
    pub fn on_nav(mut self, f: impl Fn(KeyInput) -> bool + 'static) -> Self {
        self.on_nav = Some(Rc::new(f));
        self
    }
}

struct Props {
    kind: InputKind,
    placeholder: String,
    initial: String,
    width: Option<f64>,
    multiline: bool,
    lines: u32,
    obscure: Option<char>,
    leading: Option<IconData>,
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
    on_nav: Option<Rc<dyn Fn(KeyInput) -> bool>>,
    style: Option<crate::style::Style>,
}

impl IntoWidget for TextField {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_field,
            Props {
                kind: self.kind,
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
                on_nav: self.on_nav,
                style: self.style,
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
    /// IME composition text (empty when not composing). Rendered underlined at the
    /// caret; committed via an `Insert` when the IME finishes.
    preedit: Signal<String>,
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
        // IME composition update: stash the preedit and stop — it isn't committed yet,
        // and nothing else in `value`/selection changes. The render reads `preedit`.
        if let KeyInput::Preedit(t) = cmd {
            self.preedit.set(t);
            return (false, false);
        }
        // Any real edit ends composition — drop a stale preedit before applying it.
        if !self.preedit.peek().is_empty() {
            self.preedit.set(String::new());
        }

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
            KeyInput::Preedit(_) => unreachable!("handled before the match"),
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
        preedit: create_signal(String::new()),
        undo: create_signal(Vec::new()),
        redo: create_signal(Vec::new()),
        id: focus.raw_id(),
        multiline: p.multiline,
    };
    let focused = !disabled && focus.is_focused();

    // Type-driven defaults — the kind's filter, icon, placeholder, format and
    // built-in affordances. Each is overridden by an explicit builder call.
    let kind = p.kind;
    let visible = create_signal(false); // password show/hide
    let eff_filter = p.char_filter.clone().or_else(|| kind_filter(kind));
    let eff_format = p.format.clone().or_else(|| kind_format(kind));
    let eff_leading = p.leading.or_else(|| kind_leading(kind));
    let eff_placeholder = if p.placeholder.is_empty() {
        kind_placeholder(kind).to_string()
    } else {
        p.placeholder.clone()
    };
    let eff_obscure =
        if kind == InputKind::Password { (!visible.get()).then_some('•') } else { p.obscure };

    // Only a live (enabled) field is focusable + edits.
    if !disabled {
        focus.register(Rc::new(|| {}), p.on_focus_change.clone(), p.autofocus);
        let on_changed = p.on_changed.clone();
        let on_submit = p.on_submit.clone();
        let on_editing = p.on_editing_complete.clone();
        let filter = eff_filter.clone();
        let max_length = p.max_length;
        let format = eff_format.clone();
        let on_nav = p.on_nav.clone();
        focus.register_editor(Rc::new(move |k: KeyInput| {
            // A consumer (list navigation) gets first refusal on each key.
            if let Some(nav) = &on_nav
                && nav(k.clone())
            {
                return;
            }
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
        .placeholder(eff_placeholder.clone())
        .selection(ed.anchor.get().min(vlen), ed.focus.get().min(vlen))
        .preedit(ed.preedit.get())
        .focused(focused)
        .obscure(eff_obscure)
        .multiline(p.multiline)
        .field_id(ed.id)
        .style(style);

    // Built-in trailing affordance for password/search (unless one was set).
    let eff_trailing: Option<AnyWidget> = if let Some(t) = p.trailing.clone() {
        Some(t)
    } else if disabled {
        None
    } else {
        match kind {
            InputKind::Password => Some(
                icon_button(if visible.get() { IconKind::EyeOff } else { IconKind::Eye })
                    .variant(ButtonVariant::Ghost)
                    .size(16.0)
                    .on_pressed(move || visible.update(|v| *v = !*v))
                    .into_widget(),
            ),
            InputKind::Search if !ed.value.get().is_empty() => {
                let oc = p.on_changed.clone();
                Some(
                    icon_button(IconKind::Close)
                        .variant(ButtonVariant::Ghost)
                        .size(15.0)
                        .on_pressed(move || {
                            ed.value.set(String::new());
                            ed.anchor.set(0);
                            ed.focus.set(0);
                            if let Some(cb) = &oc {
                                cb("");
                            }
                        })
                        .into_widget(),
                )
            }
            _ => None,
        }
    };

    // Compose with an optional leading icon and/or trailing widget.
    let content: AnyWidget = if (eff_leading.is_some() || eff_trailing.is_some()) && !p.multiline {
        let mut kids: Vec<AnyWidget> = Vec::new();
        if let Some(lead) = eff_leading {
            kids.push(icon(lead).size(16.0).color(c.muted_foreground).into_widget());
            kids.push(gap_w(8.0).into_widget());
        }
        kids.push(Expanded::new(inner).into_widget());
        if let Some(trail) = eff_trailing {
            kids.push(gap_w(6.0).into_widget());
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
    let lead_off = if eff_leading.is_some() { 24.0 } else { 0.0 };
    let (cl, ct) = if p.multiline { (11.0, 11.0) } else { (13.0 + lead_off, 10.0) };

    // The field box's presentation as a base Style; the user's `.style(..)` merges on
    // top (bg / border / radius / shadow overrides), user wins.
    let base = crate::style::style()
        .background(bg)
        .border(pebbles_render::Border::new(border_color, border_w))
        .radius_all(theme().radius);
    let merged = base.merge(p.style.clone().unwrap_or_default());
    let deco = merged.decoration().unwrap_or_else(BoxDecoration::new);
    let mut field = Container::new()
        .decoration(deco)
        .padding(padding)
        .height(height)
        .alignment(align)
        .child(content);
    if let Some(w) = merged.width.or(p.width) {
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

    // Accessibility: a text input announcing its name (label, else placeholder), its
    // current value and disabled state.
    let a11y_name = p.label.clone().unwrap_or_else(|| eff_placeholder.clone());
    let field_box = crate::widgets::semantics(
        crate::widgets::SemanticsRole::TextInput,
        a11y_name,
        field_box,
    )
    .value(ed.value.peek())
    .disabled(disabled)
    .into_widget();

    // Wrap with an optional label above and helper/error below (shadcn form field).
    if p.label.is_none() && p.helper.is_none() && p.error.is_none() {
        return field_box;
    }
    let mut col: Vec<AnyWidget> = Vec::new();
    if let Some(lbl) = &p.label {
        col.push(text(lbl.clone()).size(13.5).weight(500.0).color(c.foreground).into_widget());
        col.push(gap_h(7.0).into_widget());
    }
    col.push(field_box);
    if let Some(err) = &p.error {
        col.push(gap_h(6.0).into_widget());
        col.push(text(err.clone()).size(12.5).color(c.destructive).into_widget());
    } else if let Some(help) = &p.helper {
        col.push(gap_h(6.0).into_widget());
        col.push(text(help.clone()).size(12.5).color(c.muted_foreground).into_widget());
    }
    column(col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).into_widget()
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
