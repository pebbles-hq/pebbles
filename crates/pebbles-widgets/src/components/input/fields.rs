//! The date input — a masked, calendar-picked date field with a customizable
//! format, an optional time picker, and Style-based theming.
//!
//! Every other text-based input (text, number, email, url, phone, currency,
//! password, search, …) is **not** a separate widget: it's [`text_field`] with a
//! [`kind`](super::TextField::kind), e.g. `text_field().kind(InputKind::Number)`.
//! The date field is its own component only because it drives an overlay calendar.

use pebbles_render::{IconKind, PointerEvent};

use super::calendar::{CaptionLayout, calendar};
use super::text_field::text_field;
use super::{ButtonVariant, icon_button};
use crate::overlay::{hide_overlay, show_overlay, window_size};
use crate::style::{Style, styled};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action_event, component_props, create_signal};

// ---------------------------------------------------------------------------
// Date format
// ---------------------------------------------------------------------------

/// The order of the day/month/year fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder {
    /// Month, day, year — `MM/DD/YYYY` (US).
    Mdy,
    /// Day, month, year — `DD/MM/YYYY` (most of the world).
    Dmy,
    /// Year, month, day — `YYYY-MM-DD` (ISO).
    Ymd,
}

/// A customizable date format: field [`order`](DateOrder) + a separator character.
/// Presets: [`DateFormat::MDY`], [`DMY`](DateFormat::DMY), [`YMD`](DateFormat::YMD).
#[derive(Clone, Copy, Debug)]
pub struct DateFormat {
    pub order: DateOrder,
    pub separator: char,
}

impl Default for DateFormat {
    fn default() -> Self {
        DateFormat::MDY
    }
}

impl DateFormat {
    /// `MM/DD/YYYY`.
    pub const MDY: DateFormat = DateFormat { order: DateOrder::Mdy, separator: '/' };
    /// `DD/MM/YYYY`.
    pub const DMY: DateFormat = DateFormat { order: DateOrder::Dmy, separator: '/' };
    /// `YYYY-MM-DD`.
    pub const YMD: DateFormat = DateFormat { order: DateOrder::Ymd, separator: '-' };

    /// Change the separator character (e.g. `.separator('-')`).
    pub fn separator(mut self, sep: char) -> Self {
        self.separator = sep;
        self
    }

    /// Field widths in display order (the 4-digit year varies position).
    fn widths(&self) -> [usize; 3] {
        match self.order {
            DateOrder::Ymd => [4, 2, 2],
            _ => [2, 2, 4],
        }
    }

    /// Reformat raw input to the masked pattern, inserting separators.
    fn mask(&self, s: &str) -> String {
        let w = self.widths();
        let sep_after = [w[0], w[0] + w[1]];
        let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
        let mut out = String::new();
        for (i, ch) in digits.chars().enumerate() {
            if i == sep_after[0] || i == sep_after[1] {
                out.push(self.separator);
            }
            out.push(ch);
        }
        out
    }

    /// Parse a complete formatted date into `(year, month, day)`.
    fn parse(&self, s: &str) -> Option<(i32, u32, u32)> {
        let parts: Vec<&str> = s.split(self.separator).collect();
        if parts.len() != 3 {
            return None;
        }
        let (ys, ms, ds) = match self.order {
            DateOrder::Mdy => (parts[2], parts[0], parts[1]),
            DateOrder::Dmy => (parts[2], parts[1], parts[0]),
            DateOrder::Ymd => (parts[0], parts[1], parts[2]),
        };
        let m: u32 = ms.parse().ok()?;
        let d: u32 = ds.parse().ok()?;
        let y: i32 = ys.parse().ok()?;
        if (1..=12).contains(&m) && (1..=31).contains(&d) && ys.len() == 4 {
            Some((y, m, d))
        } else {
            None
        }
    }

    /// Format `(year, month, day)` into a string.
    fn format(&self, y: i32, m: u32, d: u32) -> String {
        let s = self.separator;
        match self.order {
            DateOrder::Mdy => format!("{m:02}{s}{d:02}{s}{y:04}"),
            DateOrder::Dmy => format!("{d:02}{s}{m:02}{s}{y:04}"),
            DateOrder::Ymd => format!("{y:04}{s}{m:02}{s}{d:02}"),
        }
    }

    /// The placeholder pattern (e.g. `MM/DD/YYYY`).
    fn hint(&self) -> String {
        let s = self.separator;
        match self.order {
            DateOrder::Mdy => format!("MM{s}DD{s}YYYY"),
            DateOrder::Dmy => format!("DD{s}MM{s}YYYY"),
            DateOrder::Ymd => format!("YYYY{s}MM{s}DD"),
        }
    }

    fn allows(&self, c: char) -> bool {
        c.is_ascii_digit() || c == self.separator
    }
}

// ---------------------------------------------------------------------------
// DateField — dedicated to dates. For time, use `time_field`.
// ---------------------------------------------------------------------------

/// A date input: type it (auto-formatted to the chosen [`DateFormat`]) or pick it
/// from the calendar popover. Customize the caption, the format, and both the
/// input's and the calendar's [`Style`]. Date-only by design — for time use
/// [`time_field`](super::time_field).
#[derive(Clone, Default)]
pub struct DateField {
    placeholder: Option<String>,
    width: Option<f64>,
    caption: CaptionLayout,
    format: DateFormat,
    style: Option<Style>,
    calendar_style: Option<Style>,
}

/// Create a [`DateField`].
pub fn date_field() -> DateField {
    DateField { caption: CaptionLayout::Dropdown, ..Default::default() }
}

impl DateField {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = Some(s.into());
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = Some(w);
        self
    }
    /// Choose the picker's month/year caption layout (default: `Dropdown`).
    pub fn caption(mut self, layout: CaptionLayout) -> Self {
        self.caption = layout;
        self
    }
    /// The date format — order + separator (default: `DateFormat::MDY`).
    pub fn format(mut self, format: DateFormat) -> Self {
        self.format = format;
        self
    }
    /// Style the input box (background, border, radius, margin, …).
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
    /// Style the calendar popover box.
    pub fn calendar_style(mut self, style: Style) -> Self {
        self.calendar_style = Some(style);
        self
    }
}

struct DateProps {
    placeholder: Option<String>,
    width: Option<f64>,
    caption: CaptionLayout,
    format: DateFormat,
    style: Option<Style>,
    calendar_style: Option<Style>,
}

impl IntoWidget for DateField {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_date,
            DateProps {
                placeholder: self.placeholder,
                width: self.width,
                caption: self.caption,
                format: self.format,
                style: self.style,
                calendar_style: self.calendar_style,
            },
        )
        .into_widget()
    }
}

/// Default popover width (7 × 40px cell + 24 padding), for anchoring.
const POP_W: f64 = 7.0 * 40.0 + 24.0;

fn render_date(p: &DateProps) -> AnyWidget {
    let text = create_signal(String::new());
    let caption = p.caption;
    let fmt = p.format;
    let width = p.width;
    let cal_style = p.calendar_style.clone();

    let cal_btn = icon_button(IconKind::Calendar).variant(ButtonVariant::Ghost).size(16.0).on_pressed(
        action_event(move |e: PointerEvent| {
            let current = fmt.parse(&text.peek());
            let mut cal = calendar(move |y, m, d| {
                text.set(fmt.format(y, m, d));
                hide_overlay();
            })
            .caption(caption);
            if let Some((y, m, d)) = current {
                cal = cal.selected(y, m, d).month(y, m);
            }
            if let Some(cs) = &cal_style {
                cal = cal.style(cs.clone());
            }

            // Left-align the popover to the input's LEFT edge, opening below. The
            // calendar button sits ~34px from the input's right edge.
            let pop_w = cal_style.as_ref().and_then(|s| s.width).unwrap_or(POP_W);
            let button_left = e.global.x - e.position.x;
            let button_top = e.global.y - e.position.y;
            let (ww, _) = window_size();
            let input_left = match width {
                Some(w) => button_left - (w - 34.0),
                None => button_left - pop_w + 34.0,
            };
            let max_left = if ww > 0.0 { ww - pop_w - 8.0 } else { 8.0 };
            let left = input_left.clamp(8.0, max_left.max(8.0));
            show_overlay(cal.into_widget(), left, button_top + 42.0, pop_w, 340.0);
        }),
    );

    let ph = p.placeholder.clone().unwrap_or_else(|| fmt.hint());
    let mut tf = text_field()
        .placeholder(ph)
        .bind(text)
        .format(move |s| fmt.mask(s))
        .filter(move |c| fmt.allows(c))
        .trailing(cal_btn);
    if let Some(w) = width {
        tf = tf.width(w);
    }
    let field = tf.into_widget();
    match &p.style {
        Some(s) => styled(field, s.clone()),
        None => field,
    }
}
