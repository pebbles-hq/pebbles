//! The date input — a masked, calendar-picked date field with a customizable
//! format, an optional time picker, and Style-based theming.
//!
//! Every other text-based input (text, number, email, url, phone, currency,
//! password, search, …) is **not** a separate widget: it's [`text_field`] with a
//! [`kind`](super::TextField::kind), e.g. `text_field().kind(InputKind::Number)`.
//! The date field is its own component only because it drives an overlay calendar.

use pebbles_render::{IconKind, PointerEvent};

use std::rc::Rc;

use super::calendar::{CaptionLayout, Date, calendar};
use super::text_field::text_field;
use super::{ButtonVariant, icon_button};
use crate::overlay::{hide_overlay, show_overlay_guarded, window_size};
use crate::style::{Style, styled};
use crate::widgets::{gap_w, row};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action_event, component_props, create_effect, create_signal};

type DisabledPred = Rc<dyn Fn(i32, u32, u32) -> bool>;

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
    fn widths(self) -> [usize; 3] {
        match self.order {
            DateOrder::Ymd => [4, 2, 2],
            _ => [2, 2, 4],
        }
    }

    /// Reformat raw input to the masked pattern, inserting separators.
    fn mask(self, s: &str) -> String {
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
    fn parse(self, s: &str) -> Option<(i32, u32, u32)> {
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
        // Reject calendar-impossible dates (e.g. Feb 30, Apr 31) — not just 1..=31.
        if (1..=12).contains(&m) && (1..=days_in_month(y, m)).contains(&d) && ys.len() == 4 {
            Some((y, m, d))
        } else {
            None
        }
    }

    /// Format `(year, month, day)` into a string.
    fn format(self, y: i32, m: u32, d: u32) -> String {
        let s = self.separator;
        match self.order {
            DateOrder::Mdy => format!("{m:02}{s}{d:02}{s}{y:04}"),
            DateOrder::Dmy => format!("{d:02}{s}{m:02}{s}{y:04}"),
            DateOrder::Ymd => format!("{y:04}{s}{m:02}{s}{d:02}"),
        }
    }

    /// The placeholder pattern (e.g. `MM/DD/YYYY`).
    fn hint(self) -> String {
        let s = self.separator;
        match self.order {
            DateOrder::Mdy => format!("MM{s}DD{s}YYYY"),
            DateOrder::Dmy => format!("DD{s}MM{s}YYYY"),
            DateOrder::Ymd => format!("YYYY{s}MM{s}DD"),
        }
    }

    fn allows(self, c: char) -> bool {
        c.is_ascii_digit() || c == self.separator
    }
}

/// Days in a given month, honoring the proleptic-Gregorian leap-year rule so the
/// parser can reject impossible dates (Feb 30, Apr 31, …).
fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
            if leap { 29 } else { 28 }
        }
        _ => 0,
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
    range: bool,
    range_value: Option<(Date, Date)>,
    on_changed: Option<Rc<dyn Fn(Date)>>,
    on_range_changed: Option<Rc<dyn Fn(Date, Date)>>,
    clearable: bool,
    min: Option<Date>,
    max: Option<Date>,
    disabled_pred: Option<DisabledPred>,
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
    /// Switch to **range** mode: the calendar picks a start + end date, the input
    /// shows both (read-only — the picker owns the value), and
    /// [`on_range_changed`](DateField::on_range_changed) reports each pick.
    pub fn range(mut self, yes: bool) -> Self {
        self.range = yes;
        self
    }
    /// Reports a completed single-date pick — whether typed to a valid, complete
    /// date or chosen from the calendar (a [`Date`] is `(year, month, day)`).
    /// Range mode uses [`on_range_changed`](DateField::on_range_changed) instead.
    pub fn on_changed(mut self, f: impl Fn(Date) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
    /// The initial range in range mode (a [`Date`] is `(year, month, day)`).
    pub fn range_value(mut self, start: Date, end: Date) -> Self {
        self.range_value = Some((start, end));
        self
    }
    /// Reports a completed range pick (both ends chosen).
    pub fn on_range_changed(mut self, f: impl Fn(Date, Date) + 'static) -> Self {
        self.on_range_changed = Some(Rc::new(f));
        self
    }
    /// Show a ✕ that resets the input to its placeholder (range mode especially:
    /// the picker owns the value).
    pub fn clearable(mut self, yes: bool) -> Self {
        self.clearable = yes;
        self
    }
    /// The earliest pickable date; earlier days are muted + disabled.
    pub fn min(mut self, y: i32, m: u32, d: u32) -> Self {
        self.min = Some((y, m, d));
        self
    }
    /// The latest pickable date; later days are muted + disabled.
    pub fn max(mut self, y: i32, m: u32, d: u32) -> Self {
        self.max = Some((y, m, d));
        self
    }
    /// Days for which this predicate returns `true` are muted + disabled.
    pub fn disabled_dates(mut self, pred: impl Fn(i32, u32, u32) -> bool + 'static) -> Self {
        self.disabled_pred = Some(Rc::new(pred));
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
    range: bool,
    range_value: Option<(Date, Date)>,
    on_changed: Option<Rc<dyn Fn(Date)>>,
    on_range_changed: Option<Rc<dyn Fn(Date, Date)>>,
    clearable: bool,
    min: Option<Date>,
    max: Option<Date>,
    disabled_pred: Option<DisabledPred>,
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
                range: self.range,
                range_value: self.range_value,
                on_changed: self.on_changed,
                on_range_changed: self.on_range_changed,
                clearable: self.clearable,
                min: self.min,
                max: self.max,
                disabled_pred: self.disabled_pred,
            },
        )
        .into_widget()
    }
}

/// Default popover width (7 × 40px cell + 24 padding), for anchoring.
const POP_W: f64 = 7.0 * 40.0 + 24.0;

fn render_date(p: &DateProps) -> AnyWidget {
    // Range mode seeds the display from the initial range (else starts empty).
    let text = create_signal(if p.range {
        p.range_value.map(|(s, e)| joined(p.format, s, e)).unwrap_or_default()
    } else {
        String::new()
    });
    let caption = p.caption;
    let fmt = p.format;
    let width = p.width;
    let cal_style = p.calendar_style.clone();
    let range_mode = p.range;
    let range_initial = p.range_value;
    let on_range_changed = p.on_range_changed.clone();
    let (cmin, cmax, cpred) = (p.min, p.max, p.disabled_pred.clone());
    let clearable = p.clearable;

    // Single-date reporting: fire `on_changed` whenever the input settles on a new,
    // valid, complete date — covering both a typed date and a calendar pick (the
    // pick writes the formatted string into `text`, which this effect observes).
    // A `last` latch dedups so the same date isn't reported twice, and the initial
    // empty/partial state never fires. Range mode reports via `on_range_changed`.
    if !p.range
        && let Some(on_changed) = p.on_changed.clone()
    {
        let last = create_signal::<Option<Date>>(None);
        create_effect(move || {
            let parsed = fmt.parse(&text.get());
            if let Some(date) = parsed
                && last.peek() != Some(date)
            {
                last.set(Some(date));
                on_changed(date);
            }
        });
    }

    let cal_btn = icon_button(IconKind::Calendar).variant(ButtonVariant::Ghost).size(16.0).on_pressed(
        action_event(move |e: PointerEvent| {
            let current = fmt.parse(&text.peek());
            let content: AnyWidget = if range_mode {
                // Range calendar: both ends picked in the popover, then reported
                // and written into the (read-only) input display.
                let on_range = on_range_changed.clone();
                let mut cal = calendar(move |_, _, _| {}).caption(caption).range(true);
                if let Some((s, e2)) = range_initial {
                    cal = cal.range_value(s, e2);
                }
                if let Some((y, m, d)) = cmin {
                    cal = cal.min(y, m, d);
                }
                if let Some((y, m, d)) = cmax {
                    cal = cal.max(y, m, d);
                }
                if let Some(pred) = &cpred {
                    let pred = pred.clone();
                    cal = cal.disabled_dates(move |y, m, d| pred(y, m, d));
                }
                cal = cal.on_range_changed(move |s, e2| {
                    text.set(joined(fmt, s, e2));
                    if let Some(cb) = &on_range {
                        cb(s, e2);
                    }
                    hide_overlay();
                });
                if let Some(cs) = &cal_style {
                    cal = cal.style(cs.clone());
                }
                cal.into_widget()
            } else {
                let mut cal = calendar(move |y, m, d| {
                    text.set(fmt.format(y, m, d));
                    hide_overlay();
                })
                .caption(caption);
                if let Some((y, m, d)) = current {
                    cal = cal.selected(y, m, d).month(y, m);
                }
                if let Some((y, m, d)) = cmin {
                    cal = cal.min(y, m, d);
                }
                if let Some((y, m, d)) = cmax {
                    cal = cal.max(y, m, d);
                }
                if let Some(pred) = &cpred {
                    let pred = pred.clone();
                    cal = cal.disabled_dates(move |y, m, d| pred(y, m, d));
                }
                if let Some(cs) = &cal_style {
                    cal = cal.style(cs.clone());
                }
                cal.into_widget()
            };

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
            show_overlay_guarded(content, left, button_top + 42.0, pop_w, 340.0, move || text.alive());
        }),
    );

    let ph = p
        .placeholder
        .clone()
        .unwrap_or_else(|| if range_mode { "Pick a range".to_string() } else { fmt.hint() });
    let mut tf = text_field().placeholder(ph).bind(text);
    if clearable && !text.get().is_empty() {
        // ✕ resets to the placeholder; the calendar button stays to the right.
        let clear = icon_button(IconKind::Close)
            .variant(ButtonVariant::Ghost)
            .size(15.0)
            .on_pressed(move || text.set(String::new()));
        tf = tf.trailing(row(vec![clear.into_widget(), gap_w(2.0).into_widget(), cal_btn.into_widget()]));
    } else {
        tf = tf.trailing(cal_btn);
    }
    if range_mode {
        // Read-only display: the picker owns the value — no characters may land,
        // and any edit (incl. deletions) is reverted to the current display.
        tf = tf.filter(|_c| false).format(move |_s| text.peek());
    } else {
        tf = tf.format(move |s| fmt.mask(s)).filter(move |c| fmt.allows(c));
    }
    if let Some(w) = width {
        tf = tf.width(w);
    }
    let field = tf.into_widget();
    match &p.style {
        Some(s) => styled(field, s.clone()),
        None => field,
    }
}

/// The range display: `Jan 5, 2026 – Feb 2, 2026`.
fn joined(_fmt: DateFormat, s: Date, e: Date) -> String {
    format!("{} – {}", readable(s), readable(e))
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `Jan 5, 2026`.
fn readable(d: Date) -> String {
    // Guard: clamp month to 1..=12 so an out-of-range month can't index OOB.
    format!("{} {}, {}", &MONTHS[(d.1.clamp(1, 12) - 1) as usize][..3], d.2, d.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_in_month_handles_leap_years() {
        assert_eq!(days_in_month(2024, 2), 29); // divisible by 4
        assert_eq!(days_in_month(2025, 2), 28); // not a leap year
        assert_eq!(days_in_month(2000, 2), 29); // divisible by 400
        assert_eq!(days_in_month(1900, 2), 28); // divisible by 100, not 400
        assert_eq!(days_in_month(2026, 4), 30);
        assert_eq!(days_in_month(2026, 1), 31);
        assert_eq!(days_in_month(2026, 13), 0); // out of range
    }

    #[test]
    fn parse_rejects_calendar_impossible_dates() {
        let f = DateFormat::MDY;
        // Feb 30 / Apr 31 / month 13 must not parse, even though day <= 31.
        assert_eq!(f.parse("02/30/2026"), None);
        assert_eq!(f.parse("04/31/2026"), None);
        assert_eq!(f.parse("13/01/2026"), None);
        // Valid dates, including a leap-day, still parse.
        assert_eq!(f.parse("02/29/2024"), Some((2024, 2, 29)));
        assert_eq!(f.parse("02/28/2025"), Some((2025, 2, 28)));
        assert_eq!(f.parse("12/31/2026"), Some((2026, 12, 31)));
        // Feb 29 on a non-leap year is rejected.
        assert_eq!(f.parse("02/29/2025"), None);
        // Incomplete / non-4-digit year is rejected.
        assert_eq!(f.parse("1/1/26"), None);
    }

    #[test]
    fn parse_honors_field_order() {
        assert_eq!(DateFormat::DMY.parse("31/12/2026"), Some((2026, 12, 31)));
        assert_eq!(DateFormat::YMD.parse("2026-12-31"), Some((2026, 12, 31)));
        // DMY with an impossible day is still rejected.
        assert_eq!(DateFormat::DMY.parse("30/02/2026"), None);
    }
}
