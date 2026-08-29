//! Specialized single-line input types, all built on [`TextField`] + its
//! leading-icon / trailing-widget / input-filter support. Simple types
//! (`number_field`, `email_field`, `url_field`, `phone_field`, `date_field`) return
//! a configured `TextField` so callers can keep chaining (`.width()`, `.on_changed()`,
//! …). Stateful types (`password_field` with a show/hide toggle, `search_field` with
//! a clear button) are components.

use std::rc::Rc;

use pebbles_render::{IconKind, PointerEvent};

use super::calendar::calendar;
use super::text_field::{TextField, text_field};
use super::{ButtonVariant, icon_button};
use crate::overlay::{hide_overlay, show_overlay, window_size};
use crate::widgets::SizedBox;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action, action_event, component_props, create_signal};

// ---------------------------------------------------------------------------
// Simple types — a configured TextField.
// ---------------------------------------------------------------------------

/// A numeric input (digits, `.` and `-` only).
pub fn number_field() -> TextField {
    text_field().filter(|c| c.is_ascii_digit() || c == '.' || c == '-').placeholder("0")
}

/// An email input — envelope icon, no spaces.
pub fn email_field() -> TextField {
    text_field()
        .leading(IconKind::Mail)
        .filter(|c| !c.is_whitespace())
        .placeholder("you@example.com")
}

/// A URL input — no spaces.
pub fn url_field() -> TextField {
    text_field().filter(|c| !c.is_whitespace()).placeholder("https://example.com")
}

/// A phone input — digits and phone punctuation.
pub fn phone_field() -> TextField {
    text_field()
        .leading(IconKind::Phone)
        .filter(|c| c.is_ascii_digit() || "()+- ".contains(c))
        .placeholder("(555) 123-4567")
}

// ---------------------------------------------------------------------------
// Date — masked MM/DD/YYYY input with a calendar-popover picker.
// ---------------------------------------------------------------------------

/// Reformat any input as `MM/DD/YYYY` (keeps the first 8 digits, inserts slashes).
fn mask_date(s: &str) -> String {
    let digits: String = s.chars().filter(char::is_ascii_digit).take(8).collect();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i == 2 || i == 4 {
            out.push('/');
        }
        out.push(ch);
    }
    out
}

/// A date input: type digits and they auto-format to `MM/DD/YYYY`, or click the
/// calendar button to pick from a month grid.
pub struct DateField {
    placeholder: String,
    width: Option<f64>,
}

/// Create a [`DateField`].
pub fn date_field() -> DateField {
    DateField { placeholder: "MM/DD/YYYY".to_string(), width: None }
}

impl DateField {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = Some(w);
        self
    }
}

struct DateProps {
    placeholder: String,
    width: Option<f64>,
}

impl IntoWidget for DateField {
    fn into_widget(self) -> AnyWidget {
        component_props(render_date, DateProps { placeholder: self.placeholder, width: self.width })
            .into_widget()
    }
}

fn render_date(p: &DateProps) -> AnyWidget {
    let text = create_signal(String::new());
    let cal_btn = icon_button(IconKind::Calendar).variant(ButtonVariant::Ghost).size(16.0).on_pressed(
        action_event(move |e: PointerEvent| {
            let picker = calendar(move |y, m, d| {
                text.set(format!("{m:02}/{d:02}/{y:04}"));
                hide_overlay();
            });
            // Anchor the picker's right edge near the calendar button, below it.
            const CAL_W: f64 = 262.0;
            let button_left = e.global.x - e.position.x;
            let button_top = e.global.y - e.position.y;
            let (ww, _) = window_size();
            let max_left = if ww > 0.0 { ww - CAL_W - 8.0 } else { 8.0 };
            let left = (button_left - CAL_W + 34.0).clamp(8.0, max_left.max(8.0));
            show_overlay(picker.into_widget(), left, button_top + 42.0);
        }),
    );
    let mut tf = text_field()
        .placeholder(p.placeholder.clone())
        .bind(text)
        .format(mask_date)
        .filter(|c| c.is_ascii_digit() || c == '/')
        .trailing(cal_btn);
    if let Some(w) = p.width {
        tf = tf.width(w);
    }
    tf.into_widget()
}

// ---------------------------------------------------------------------------
// Password — obscured with a show/hide toggle.
// ---------------------------------------------------------------------------

/// A password input with a show/hide (eye) toggle.
pub struct PasswordField {
    placeholder: String,
    width: Option<f64>,
}

/// Create a [`PasswordField`].
pub fn password_field() -> PasswordField {
    PasswordField { placeholder: "Password".to_string(), width: None }
}

impl PasswordField {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = Some(w);
        self
    }
}

struct PwProps {
    placeholder: String,
    width: Option<f64>,
}

impl IntoWidget for PasswordField {
    fn into_widget(self) -> AnyWidget {
        component_props(render_password, PwProps { placeholder: self.placeholder, width: self.width })
            .into_widget()
    }
}

fn render_password(p: &PwProps) -> AnyWidget {
    let visible = create_signal(false);
    let eye = icon_button(if visible.get() { IconKind::EyeOff } else { IconKind::Eye })
        .variant(ButtonVariant::Ghost)
        .size(16.0)
        .on_pressed(action(move || visible.update(|v| *v = !*v)));
    let mut tf = text_field()
        .leading(IconKind::Lock)
        .placeholder(p.placeholder.clone())
        .obscured(!visible.get())
        .trailing(eye);
    if let Some(w) = p.width {
        tf = tf.width(w);
    }
    tf.into_widget()
}

// ---------------------------------------------------------------------------
// Search — leading search icon + a clear button when non-empty.
// ---------------------------------------------------------------------------

/// A search input with a leading magnifier and a clear (×) button.
pub struct SearchField {
    placeholder: String,
    width: Option<f64>,
    on_changed: Option<Rc<dyn Fn(&str)>>,
}

/// Create a [`SearchField`].
pub fn search_field() -> SearchField {
    SearchField { placeholder: "Search…".to_string(), width: None, on_changed: None }
}

impl SearchField {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = Some(w);
        self
    }
    /// Fired with the query on every edit and on clear.
    pub fn on_changed(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

struct SearchProps {
    placeholder: String,
    width: Option<f64>,
    on_changed: Option<Rc<dyn Fn(&str)>>,
}

impl IntoWidget for SearchField {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_search,
            SearchProps { placeholder: self.placeholder, width: self.width, on_changed: self.on_changed },
        )
        .into_widget()
    }
}

fn render_search(p: &SearchProps) -> AnyWidget {
    let text = create_signal(String::new());
    let oc_clear = p.on_changed.clone();
    let oc_edit = p.on_changed.clone();

    let clear: AnyWidget = if text.get().is_empty() {
        SizedBox::spacer(0.0, 0.0).into_widget()
    } else {
        icon_button(IconKind::Close)
            .variant(ButtonVariant::Ghost)
            .size(15.0)
            .on_pressed(action(move || {
                text.set(String::new());
                if let Some(cb) = &oc_clear {
                    cb("");
                }
            }))
            .into_widget()
    };

    let mut tf = text_field()
        .leading(IconKind::Search)
        .placeholder(p.placeholder.clone())
        .bind(text)
        .trailing(clear)
        .on_changed(move |s| {
            if let Some(cb) = &oc_edit {
                cb(s);
            }
        });
    if let Some(w) = p.width {
        tf = tf.width(w);
    }
    tf.into_widget()
}
