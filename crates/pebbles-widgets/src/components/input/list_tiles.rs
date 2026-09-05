//! Selection-control list rows — Flutter's `CheckboxListTile` / `RadioListTile` /
//! `SwitchListTile`. Thin composers over [`list_tile`](crate::list_tile) with a
//! [`checkbox`](crate::checkbox) / [`radio`](crate::radio) / [`switch`](crate::switch)
//! as the trailing control. The **whole row** is the tap target (the control is
//! display-only, wrapped in `ignore_pointer`), so a tap anywhere fires `on_changed`.

use std::rc::Rc;

use crate::components::{checkbox, list_tile, radio, switch};
use crate::widgets::ignore_pointer;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// Assemble a list tile with a trailing (display-only) control.
fn compose(
    title: String,
    subtitle: Option<String>,
    secondary: Option<AnyWidget>,
    control: AnyWidget,
    on_changed: Option<Rc<dyn Fn()>>,
) -> AnyWidget {
    let mut t = list_tile(title);
    if let Some(s) = subtitle {
        t = t.subtitle(s);
    }
    if let Some(sec) = secondary {
        t = t.leading(sec);
    }
    // The control never handles the tap itself — the row does, so the hit target
    // is the full width (Flutter's ListTile semantics).
    t = t.trailing(ignore_pointer(control));
    if let Some(f) = on_changed {
        t = t.on_tap(move || f());
    }
    t.into_widget()
}

// ===========================================================================
// CheckboxListTile
// ===========================================================================

/// A [`list_tile`](crate::list_tile) with a trailing checkbox — Flutter's
/// `CheckboxListTile`. Built by [`checkbox_list_tile`].
#[derive(Clone, Default)]
pub struct CheckboxListTile {
    title: String,
    subtitle: Option<String>,
    value: bool,
    secondary: Option<AnyWidget>,
    on_changed: Option<Rc<dyn Fn()>>,
}

/// A row whose trailing control is a checkbox reflecting `value`.
pub fn checkbox_list_tile(title: impl Into<String>, value: bool) -> CheckboxListTile {
    CheckboxListTile { title: title.into(), value, ..Default::default() }
}

impl CheckboxListTile {
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// A leading widget (Flutter's `secondary`).
    pub fn secondary(mut self, w: impl IntoWidget) -> Self {
        self.secondary = Some(w.into_widget());
        self
    }
    /// Fired when the row is tapped — the caller flips `value`.
    pub fn on_changed(mut self, f: impl Fn() + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

impl IntoWidget for CheckboxListTile {
    fn into_widget(self) -> AnyWidget {
        compose(
            self.title,
            self.subtitle,
            self.secondary,
            checkbox(self.value).into_widget(),
            self.on_changed,
        )
    }
}

// ===========================================================================
// RadioListTile
// ===========================================================================

/// A [`list_tile`](crate::list_tile) with a trailing radio — Flutter's
/// `RadioListTile`. Built by [`radio_list_tile`].
#[derive(Clone, Default)]
pub struct RadioListTile {
    title: String,
    subtitle: Option<String>,
    selected: bool,
    secondary: Option<AnyWidget>,
    on_changed: Option<Rc<dyn Fn()>>,
}

/// A row whose trailing control is a radio reflecting `selected`.
pub fn radio_list_tile(title: impl Into<String>, selected: bool) -> RadioListTile {
    RadioListTile { title: title.into(), selected, ..Default::default() }
}

impl RadioListTile {
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// A leading widget (Flutter's `secondary`).
    pub fn secondary(mut self, w: impl IntoWidget) -> Self {
        self.secondary = Some(w.into_widget());
        self
    }
    /// Fired when the row is tapped — the caller selects this option.
    pub fn on_changed(mut self, f: impl Fn() + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

impl IntoWidget for RadioListTile {
    fn into_widget(self) -> AnyWidget {
        compose(
            self.title,
            self.subtitle,
            self.secondary,
            radio(self.selected).into_widget(),
            self.on_changed,
        )
    }
}

// ===========================================================================
// SwitchListTile
// ===========================================================================

/// A [`list_tile`](crate::list_tile) with a trailing switch — Flutter's
/// `SwitchListTile`. Built by [`switch_list_tile`].
#[derive(Clone, Default)]
pub struct SwitchListTile {
    title: String,
    subtitle: Option<String>,
    value: bool,
    secondary: Option<AnyWidget>,
    on_changed: Option<Rc<dyn Fn()>>,
}

/// A row whose trailing control is a switch reflecting `value`.
pub fn switch_list_tile(title: impl Into<String>, value: bool) -> SwitchListTile {
    SwitchListTile { title: title.into(), value, ..Default::default() }
}

impl SwitchListTile {
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// A leading widget (Flutter's `secondary`).
    pub fn secondary(mut self, w: impl IntoWidget) -> Self {
        self.secondary = Some(w.into_widget());
        self
    }
    /// Fired when the row is tapped — the caller flips `value`.
    pub fn on_changed(mut self, f: impl Fn() + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

impl IntoWidget for SwitchListTile {
    fn into_widget(self) -> AnyWidget {
        compose(self.title, self.subtitle, self.secondary, switch(self.value).into_widget(), self.on_changed)
    }
}
