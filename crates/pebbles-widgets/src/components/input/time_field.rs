//! [`TimeField`] — a dedicated time picker, separate from the date picker. Type a
//! specific time (masked `HH:MM`), or open the dropdown and pick a slot. Supports
//! 24-hour or 12-hour (AM/PM) and a configurable step. Styleable like any input.

use std::rc::Rc;
use pebbles_foundation::{MainAxisSize};

use pebbles_render::{IconKind, PointerEvent, lucide};

use super::menu::{ActionRowProps, action_row};
use super::popover::{anchor_below, popover_surface};
use super::text_field::text_field;
use super::{ButtonVariant, icon_button};
use crate::overlay::{hide_overlay, show_overlay};
use crate::style::{Style, styled};
use crate::widgets::{Container, SingleChildScrollView, column};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action_event, component_props, create_signal};

/// Reformat digits as `HH:MM` (first 4 digits, colon after the hour).
fn mask_time(s: &str) -> String {
    let digits: String = s.chars().filter(char::is_ascii_digit).take(4).collect();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i == 2 {
            out.push(':');
        }
        out.push(ch);
    }
    out
}

/// A `12:30 PM`-style label for `(hour24, minute)`.
fn label12(h: u32, m: u32) -> String {
    let ap = if h < 12 { "AM" } else { "PM" };
    let h12 = match h % 12 {
        0 => 12,
        x => x,
    };
    format!("{h12:02}:{m:02} {ap}")
}

/// Every time slot of the day at `step` minutes, formatted per `hour12`.
fn time_options(hour12: bool, step: u32) -> Vec<String> {
    let step = step.max(1);
    let mut out = Vec::new();
    let mut t = 0u32;
    while t < 24 * 60 {
        let (h, m) = (t / 60, t % 60);
        out.push(if hour12 { label12(h, m) } else { format!("{h:02}:{m:02}") });
        t += step;
    }
    out
}

/// A time input with a slot dropdown. Build with [`time_field`].
pub struct TimeField {
    placeholder: Option<String>,
    width: Option<f64>,
    hour12: bool,
    step: u32,
    style: Option<Style>,
    on_changed: Option<Rc<dyn Fn(&str)>>,
}

/// Create a [`TimeField`] (24-hour, 30-minute slots by default).
pub fn time_field() -> TimeField {
    TimeField { placeholder: None, width: None, hour12: false, step: 30, style: None, on_changed: None }
}

impl TimeField {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = Some(s.into());
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = Some(w);
        self
    }
    /// Use 12-hour AM/PM slots instead of 24-hour.
    pub fn hour12(mut self) -> Self {
        self.hour12 = true;
        self
    }
    /// Minutes between dropdown slots (default 30).
    pub fn step(mut self, minutes: u32) -> Self {
        self.step = minutes;
        self
    }
    /// Style the input box (background, border, radius, margin, …).
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
    /// Fired with the time on every edit or pick.
    pub fn on_changed(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

struct Props {
    placeholder: Option<String>,
    width: Option<f64>,
    hour12: bool,
    step: u32,
    style: Option<Style>,
    on_changed: Option<Rc<dyn Fn(&str)>>,
}

impl IntoWidget for TimeField {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_time,
            Props {
                placeholder: self.placeholder,
                width: self.width,
                hour12: self.hour12,
                step: self.step,
                style: self.style,
                on_changed: self.on_changed,
            },
        )
        .into_widget()
    }
}

fn render_time(p: &Props) -> AnyWidget {
    let text = create_signal(String::new());
    let hour12 = p.hour12;
    let width = p.width.unwrap_or(170.0);
    let options = Rc::new(time_options(hour12, p.step));
    let oc = p.on_changed.clone();

    // The dropdown-open button.
    let menu_opts = options.clone();
    let oc_pick = oc.clone();
    let open_btn = icon_button(IconKind::ChevronDown).variant(ButtonVariant::Ghost).size(16.0).on_pressed(
        action_event(move |e: PointerEvent| {
            let inner = width - 8.0;
            let items: Vec<AnyWidget> = menu_opts
                .iter()
                .map(|slot| {
                    let slot = slot.clone();
                    let oc2 = oc_pick.clone();
                    action_row(ActionRowProps {
                        label: slot.clone(),
                        icon: None,
                        shortcut: None,
                        leading_check: None,
                        reserve_gutter: false,
                        destructive: false,
                        disabled: false,
                        highlighted: false,
                        width: inner,
                        on_select: Rc::new(move || {
                            text.set(slot.clone());
                            if let Some(cb) = &oc2 {
                                cb(&slot);
                            }
                            hide_overlay();
                        }),
                    })
                })
                .collect();
            let list = Container::new()
                .height(260.0)
                .child(SingleChildScrollView::vertical(column(items).main_axis_size(MainAxisSize::Min)).scrollbar_thickness(6.0))
                .into_widget();
            let menu = popover_surface(width, 4.0, list);

            // Anchor under the input's left edge (the button sits ~34px from the right).
            let button_left = e.global.x - e.position.x;
            let button_top = e.global.y - e.position.y;
            let input_left = button_left - (width - 34.0);
            let (left, top) = anchor_below(input_left, button_top - 5.0, 38.0, width, 260.0);
            show_overlay(menu, left, top, width, 260.0);
        }),
    );

    let ph = p.placeholder.clone().unwrap_or_else(|| if hour12 { "hh:mm AM".into() } else { "HH:MM".into() });
    let oc_edit = oc.clone();
    let mut tf = text_field()
        .leading(lucide::CLOCK)
        .placeholder(ph)
        .bind(text)
        .trailing(open_btn)
        .on_changed(move |s| {
            if let Some(cb) = &oc_edit {
                cb(s);
            }
        });
    // 24-hour typing is masked to HH:MM; 12-hour allows the AM/PM suffix.
    if hour12 {
        tf = tf.filter(|c| c.is_ascii_digit() || c == ':' || c == ' ' || c.is_ascii_alphabetic());
    } else {
        tf = tf.format(mask_time).filter(|c| c.is_ascii_digit() || c == ':');
    }
    if let Some(w) = p.width {
        tf = tf.width(w);
    } else {
        tf = tf.width(width);
    }
    let field = tf.into_widget();
    match &p.style {
        Some(s) => styled(field, s.clone()),
        None => field,
    }
}
