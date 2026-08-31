//! [`Calendar`] — a shadcn-style month-grid date picker shown in the overlay by
//! `date_field`. Self-contained Gregorian date math (no external date crate):
//! today's date comes from `SystemTime`; the grid navigates by month and reports
//! the picked `(year, month, day)`.
//!
//! The caption supports shadcn's layouts ([`CaptionLayout`]): the plain
//! label-with-arrows, or clickable month/year "dropdowns" that drill into a
//! month grid and a year grid so the user can jump years and months directly
//! (all in-panel — no nested overlay).

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use pebbles_foundation::{Alignment, Color, CrossAxisAlignment, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconKind};

use super::{ButtonSize, ButtonVariant, button, icon_button};
use crate::style::Style;
use crate::theme::theme;
use crate::widgets::{
    Container, GestureDetector, Opacity, SizedBox, column, gap_h, gap_w, row, spacer, text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, component_props, create_signal};

// --- date math ---

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        2 => if is_leap(y) { 29 } else { 28 },
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Day of week for `(y, m, d)`, `0 = Sunday`..`6 = Saturday` (Sakamoto).
fn day_of_week(y: i32, m: u32, d: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if m < 3 { y - 1 } else { y };
    (((y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + d as i32) % 7 + 7) % 7) as u32
}

/// Today's `(year, month, day)` from the system clock (Howard Hinnant's algorithm).
fn today() -> (i32, u32, u32) {
    let secs =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    civil_from_days(secs / 86_400)
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (y as i32 + i32::from(m <= 2), m, d)
}

fn month_name(m: u32) -> &'static str {
    [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ][(m.clamp(1, 12) - 1) as usize]
}

fn month_abbr(m: u32) -> &'static str {
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        [(m.clamp(1, 12) - 1) as usize]
}

// --- public API ---

/// How the calendar's month/year caption is presented — mirrors shadcn's
/// `captionLayout`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionLayout {
    /// Month + year as a static label, flanked by prev/next arrows (the default).
    Label,
    /// Month **and** year as clickable dropdowns that open pickers.
    Dropdown,
    /// Month as a dropdown; year stays a label.
    DropdownMonths,
    /// Year as a dropdown; month stays a label.
    DropdownYears,
}

/// Which panel the calendar is showing.
#[derive(Clone, Copy, PartialEq)]
enum View {
    Days,
    Months,
    Years,
}

/// A calendar `(year, month, day)` triple.
pub type Date = (i32, u32, u32);

/// A month-grid date picker. `on_pick(year, month, day)` fires on a day tap.
pub struct Calendar {
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
    caption: CaptionLayout,
    selected: Option<Date>,
    start: Option<(i32, u32)>,
    range: bool,
    range_start: Option<Date>,
    range_end: Option<Date>,
    on_range_changed: Option<Rc<dyn Fn(Date, Date)>>,
    min: Option<Date>,
    max: Option<Date>,
    disabled_pred: Option<Rc<dyn Fn(i32, u32, u32) -> bool>>,
    style: Option<Style>,
}

/// Create a [`Calendar`].
pub fn calendar(on_pick: impl Fn(i32, u32, u32) + 'static) -> Calendar {
    Calendar {
        on_pick: Rc::new(on_pick),
        caption: CaptionLayout::Label,
        selected: None,
        start: None,
        range: false,
        range_start: None,
        range_end: None,
        on_range_changed: None,
        min: None,
        max: None,
        disabled_pred: None,
        style: None,
    }
}

impl Calendar {
    /// Choose how the month/year caption is presented.
    pub fn caption(mut self, layout: CaptionLayout) -> Self {
        self.caption = layout;
        self
    }
    /// Highlight an already-selected date (and open on its month).
    pub fn selected(mut self, y: i32, m: u32, d: u32) -> Self {
        self.selected = Some((y, m, d));
        self
    }
    /// Open on a specific month (defaults to the selected date's month, else today).
    pub fn month(mut self, y: i32, m: u32) -> Self {
        self.start = Some((y, m));
        self
    }
    /// Switch to **range** mode: the first tap sets the start, the second the end
    /// (endpoints swap if picked out of order), and the span between them is
    /// highlighted. A third tap starts a new range. Fires
    /// [`on_range_changed`](Calendar::on_range_changed) when both ends are set.
    pub fn range(mut self, on: bool) -> Self {
        self.range = on;
        self
    }
    /// Pre-select a range (implies range mode; opens on the start's month).
    pub fn range_value(mut self, start: Date, end: Date) -> Self {
        self.range = true;
        self.range_start = Some(start);
        self.range_end = Some(end);
        self
    }
    /// Fired with `(start, end)` once both ends of a range are chosen.
    pub fn on_range_changed(mut self, f: impl Fn(Date, Date) + 'static) -> Self {
        self.on_range_changed = Some(Rc::new(f));
        self
    }
    /// Earliest selectable date — earlier days are muted and non-interactive, and
    /// month navigation stops at its month.
    pub fn min(mut self, y: i32, m: u32, d: u32) -> Self {
        self.min = Some((y, m, d));
        self
    }
    /// Latest selectable date — later days are muted and non-interactive.
    pub fn max(mut self, y: i32, m: u32, d: u32) -> Self {
        self.max = Some((y, m, d));
        self
    }
    /// Disable individual days by predicate (e.g. weekends, holidays). A day for
    /// which `pred(y, m, d)` is `true` is muted and non-interactive.
    pub fn disabled_dates(mut self, pred: impl Fn(i32, u32, u32) -> bool + 'static) -> Self {
        self.disabled_pred = Some(Rc::new(pred));
        self
    }
    /// Customize the popover's box (background, border, radius, shadow, padding,
    /// width) via a [`Style`] — the same style values used everywhere.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

struct Props {
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
    caption: CaptionLayout,
    selected: Option<Date>,
    start: Option<(i32, u32)>,
    range: bool,
    range_start: Option<Date>,
    range_end: Option<Date>,
    on_range_changed: Option<Rc<dyn Fn(Date, Date)>>,
    min: Option<Date>,
    max: Option<Date>,
    disabled_pred: Option<Rc<dyn Fn(i32, u32, u32) -> bool>>,
    style: Option<Style>,
}

impl IntoWidget for Calendar {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_calendar,
            Props {
                on_pick: self.on_pick,
                caption: self.caption,
                selected: self.selected,
                start: self.start,
                range: self.range,
                range_start: self.range_start,
                range_end: self.range_end,
                on_range_changed: self.on_range_changed,
                min: self.min,
                max: self.max,
                disabled_pred: self.disabled_pred,
                style: self.style,
            },
        )
        .into_widget()
    }
}

/// Whether `d` falls outside the `[min, max]` bounds (tuple order is date order).
fn out_of_bounds(d: Date, min: Option<Date>, max: Option<Date>) -> bool {
    min.is_some_and(|lo| d < lo) || max.is_some_and(|hi| d > hi)
}

const CELL: f64 = 40.0;
const BODY_W: f64 = 7.0 * CELL;

// --- small building blocks ---

/// A square outline nav arrow (prev/next). Disabled arrows stop month navigation
/// at the calendar's bounds.
fn nav_arrow(kind: IconKind, disabled: bool, on_tap: impl Fn() + 'static) -> AnyWidget {
    icon_button(kind)
        .variant(ButtonVariant::Outline)
        .size(15.0)
        .disabled(disabled)
        .on_pressed(on_tap)
        .into_widget()
}

/// A clickable caption "dropdown" — a small outline button with a chevron.
fn caption_chip(label: String, on_tap: impl Fn() + 'static) -> AnyWidget {
    button(label)
        .variant(ButtonVariant::Outline)
        .size(ButtonSize::Sm)
        .trailing(IconKind::ChevronDown)
        .on_pressed(on_tap)
        .into_widget()
}

/// Arrange `cells` into a grid of `cols` columns.
fn grid(cells: Vec<AnyWidget>, cols: usize) -> AnyWidget {
    let mut rows: Vec<AnyWidget> = Vec::new();
    let mut cells = cells;
    while !cells.is_empty() {
        let take = cols.min(cells.len());
        let rest = cells.split_off(take);
        rows.push(row(cells).main_axis_size(MainAxisSize::Min).into_widget());
        cells = rest;
    }
    column(rows).main_axis_size(MainAxisSize::Min).into_widget()
}

/// A single day cell: selected/endpoint (primary fill), in-range span (accent
/// band), today (accent), disabled (muted, non-interactive) or plain — with a
/// hover tint on enabled plain cells.
struct DayCellProps {
    y: i32,
    m: u32,
    d: u32,
    today: bool,
    /// A single selection or a range endpoint — filled with `primary`.
    endpoint: bool,
    /// Strictly inside a chosen range — an `accent` band.
    in_range: bool,
    disabled: bool,
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
}

fn day_cell(p: DayCellProps) -> AnyWidget {
    component_props(render_day_cell, p).into_widget()
}

fn render_day_cell(p: &DayCellProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let (y, m, d) = (p.y, p.m, p.d);

    let mut deco = BoxDecoration::new().radius(BorderRadius::all(6.0));
    let fg = if p.disabled {
        c.muted_foreground
    } else if p.endpoint {
        deco = deco.color(c.primary);
        c.primary_foreground
    } else if p.in_range {
        deco = deco.color(c.accent);
        c.accent_foreground
    } else if p.today {
        deco = deco.color(c.accent);
        c.accent_foreground
    } else {
        if !p.disabled && hovered.get() {
            deco = deco.color(c.accent);
        }
        c.foreground
    };
    let mut label = text(format!("{d}")).size(13.0).color(fg);
    if p.endpoint {
        label = label.semibold();
    }
    let cell = Container::new()
        .width(CELL)
        .height(CELL)
        .alignment(Alignment::CENTER)
        .decoration(deco)
        .child(label);

    if p.disabled {
        return GestureDetector::new(Opacity::new(0.4, cell)).cursor(Cursor::NotAllowed).into_widget();
    }
    let on_pick = p.on_pick.clone();
    GestureDetector::new(cell)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_tap(move || on_pick(y, m, d))
        .into_widget()
}

/// A month/year chooser cell (used by the Months and Years panels).
fn choice_cell(label: String, current: bool, on_tap: impl Fn() + 'static) -> AnyWidget {
    let c = theme().colors;
    let mut deco = BoxDecoration::new().radius(BorderRadius::all(8.0));
    let fg = if current {
        deco = deco.color(c.primary);
        c.primary_foreground
    } else {
        c.foreground
    };
    let cell = Container::new()
        .width(BODY_W / 3.0)
        .height(42.0)
        .alignment(Alignment::CENTER)
        .decoration(deco)
        .child(text(label).size(13.0).color(fg));
    GestureDetector::new(cell)
        .cursor(Cursor::Pointer)
        .on_tap(on_tap)
        .into_widget()
}

// --- panels ---

/// Everything the day grid needs to render its cell states (selection, range,
/// bounds, disabled predicate) plus the tap handler.
struct DayGridCtx {
    caption: CaptionLayout,
    selected: Option<Date>,
    range_start: Option<Date>,
    range_end: Option<Date>,
    min: Option<Date>,
    max: Option<Date>,
    pred: Option<Rc<dyn Fn(i32, u32, u32) -> bool>>,
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
}

fn days_panel(disp: Signal<(i32, u32)>, view: Signal<View>, ctx: DayGridCtx) -> AnyWidget {
    let c = theme().colors;
    let (ty, tm, td) = today();
    let (y, m) = disp.get();
    let caption = ctx.caption;

    // Bounds clamp: don't navigate to a month wholly outside [min, max].
    let prev_disabled = ctx.min.is_some_and(|(ly, lm, _)| (y, m) <= (ly, lm));
    let next_disabled = ctx.max.is_some_and(|(hy, hm, _)| (y, m) >= (hy, hm));

    let prev = nav_arrow(IconKind::ChevronLeft, prev_disabled, move || {
        disp.update(|(yy, mm)| {
            if *mm == 1 {
                *mm = 12;
                *yy -= 1;
            } else {
                *mm -= 1;
            }
        })
    });
    let next = nav_arrow(IconKind::ChevronRight, next_disabled, move || {
        disp.update(|(yy, mm)| {
            if *mm == 12 {
                *mm = 1;
                *yy += 1;
            } else {
                *mm += 1;
            }
        })
    });

    // Caption: a "Month Year" label, or clickable month/year "dropdowns".
    let month_lbl = || text(month_name(m).to_string()).size(14.0).semibold().color(c.foreground);
    let year_lbl = || text(format!("{y}")).size(14.0).semibold().color(c.foreground);
    let month_chip = || caption_chip(month_name(m).to_string(), move || view.set(View::Months));
    let year_chip = || caption_chip(format!("{y}"), move || view.set(View::Years));
    let caption_w: AnyWidget = match caption {
        CaptionLayout::Label => text(format!("{} {}", month_name(m), y))
            .size(14.0)
            .semibold()
            .color(c.foreground)
            .into_widget(),
        CaptionLayout::Dropdown => {
            row(vec![month_chip(), gap_w(6.0).into_widget(), year_chip()])
                .main_axis_size(MainAxisSize::Min)
                .into_widget()
        }
        CaptionLayout::DropdownMonths => row(vec![
            month_chip(),
            gap_w(8.0).into_widget(),
            year_lbl().into_widget(),
        ])
        .main_axis_size(MainAxisSize::Min)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .into_widget(),
        CaptionLayout::DropdownYears => row(vec![
            month_lbl().into_widget(),
            gap_w(8.0).into_widget(),
            year_chip(),
        ])
        .main_axis_size(MainAxisSize::Min)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .into_widget(),
    };

    let header = row(vec![
        prev,
        spacer().into_widget(),
        caption_w,
        spacer().into_widget(),
        next,
    ]);

    // Weekday header.
    let weekdays: Vec<AnyWidget> = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
        .into_iter()
        .map(|d| {
            Container::new()
                .width(CELL)
                .alignment(Alignment::CENTER)
                .child(text(d).size(11.0).semibold().color(c.muted_foreground))
                .into_widget()
        })
        .collect();

    // Day grid (leading blanks + days, padded to full weeks).
    let first = day_of_week(y, m, 1);
    let dim = days_in_month(y, m);
    let mut slots: Vec<Option<u32>> = vec![None; first as usize];
    slots.extend((1..=dim).map(Some));
    while slots.len() % 7 != 0 {
        slots.push(None);
    }
    let mut weeks: Vec<AnyWidget> = Vec::new();
    for week in slots.chunks(7) {
        let cells: Vec<AnyWidget> = week
            .iter()
            .map(|slot| match slot {
                Some(d) => {
                    let date = (y, m, *d);
                    let disabled = out_of_bounds(date, ctx.min, ctx.max)
                        || ctx.pred.as_ref().is_some_and(|p| p(y, m, *d));
                    let endpoint = ctx.selected == Some(date)
                        || ctx.range_start == Some(date)
                        || ctx.range_end == Some(date);
                    let in_range = matches!((ctx.range_start, ctx.range_end), (Some(s), Some(e)) if date > s && date < e);
                    day_cell(DayCellProps {
                        y,
                        m,
                        d: *d,
                        today: *d == td && y == ty && m == tm,
                        endpoint,
                        in_range,
                        disabled,
                        on_pick: ctx.on_pick.clone(),
                    })
                }
                None => SizedBox::new(Some(CELL), Some(CELL), None).into_widget(),
            })
            .collect();
        weeks.push(row(cells).main_axis_size(MainAxisSize::Min).into_widget());
    }

    let mut body: Vec<AnyWidget> = vec![
        header.into_widget(),
        gap_h(10.0).into_widget(),
        row(weekdays).main_axis_size(MainAxisSize::Min).into_widget(),
        gap_h(4.0).into_widget(),
    ];
    body.extend(weeks);
    column(body).main_axis_size(MainAxisSize::Min).into_widget()
}

fn months_panel(disp: Signal<(i32, u32)>, view: Signal<View>) -> AnyWidget {
    let (y, cur_m) = disp.get();

    let prev = nav_arrow(IconKind::ChevronLeft, false, move || disp.update(|(yy, _)| *yy -= 1));
    let next = nav_arrow(IconKind::ChevronRight, false, move || disp.update(|(yy, _)| *yy += 1));
    let title = button(format!("{y}"))
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Sm)
        .on_pressed(move || view.set(View::Years))
        .into_widget();
    let header = row(vec![prev, spacer().into_widget(), title, spacer().into_widget(), next]);

    let cells: Vec<AnyWidget> = (1..=12u32)
        .map(|mm| {
            choice_cell(month_abbr(mm).to_string(), mm == cur_m, move || {
                disp.update(|(_, m)| *m = mm);
                view.set(View::Days);
            })
        })
        .collect();

    column(vec![header.into_widget(), gap_h(8.0).into_widget(), grid(cells, 3)])
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn years_panel(disp: Signal<(i32, u32)>, view: Signal<View>) -> AnyWidget {
    let c = theme().colors;
    let (y, _) = disp.get();
    // A 12-year block aligned to multiples of 12.
    let block = y - y.rem_euclid(12);

    let prev = nav_arrow(IconKind::ChevronLeft, false, move || disp.update(|(yy, _)| *yy -= 12));
    let next = nav_arrow(IconKind::ChevronRight, false, move || disp.update(|(yy, _)| *yy += 12));
    let title =
        text(format!("{} – {}", block, block + 11)).size(14.0).semibold().color(c.foreground);
    let header =
        row(vec![prev, spacer().into_widget(), title.into_widget(), spacer().into_widget(), next]);

    let cells: Vec<AnyWidget> = (0..12i32)
        .map(|i| {
            let yy = block + i;
            choice_cell(format!("{yy}"), yy == y, move || {
                disp.update(|(yr, _)| *yr = yy);
                view.set(View::Days);
            })
        })
        .collect();

    column(vec![header.into_widget(), gap_h(8.0).into_widget(), grid(cells, 3)])
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn render_calendar(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let (ty, tm, _) = today();
    let init = p
        .start
        .or(p.selected.map(|(y, m, _)| (y, m)))
        .or(p.range_start.map(|(y, m, _)| (y, m)))
        .unwrap_or((ty, tm));
    let disp = create_signal(init);
    let view = create_signal(View::Days);

    // Range state (only meaningful in range mode). The tap handler advances it:
    // 1st tap → start (end cleared), 2nd → end (ordered), 3rd → new start.
    let rstart: Signal<Option<Date>> = create_signal(p.range_start);
    let rend: Signal<Option<Date>> = create_signal(p.range_end);

    let on_pick: Rc<dyn Fn(i32, u32, u32)> = if p.range {
        let user = p.on_range_changed.clone();
        Rc::new(move |y, m, d| {
            let date = (y, m, d);
            if rstart.peek().is_none() || rend.peek().is_some() {
                // Begin a fresh range.
                rstart.set(Some(date));
                rend.set(None);
            } else {
                // Close the range, ordering the endpoints.
                let s = rstart.peek().unwrap();
                let (lo, hi) = if date < s { (date, s) } else { (s, date) };
                rstart.set(Some(lo));
                rend.set(Some(hi));
                if let Some(cb) = &user {
                    cb(lo, hi);
                }
            }
        })
    } else {
        p.on_pick.clone()
    };

    let ctx = DayGridCtx {
        caption: p.caption,
        selected: if p.range { None } else { p.selected },
        range_start: rstart.get(),
        range_end: rend.get(),
        min: p.min,
        max: p.max,
        pred: p.disabled_pred.clone(),
        on_pick: on_pick.clone(),
    };

    let panel = match view.get() {
        View::Days => days_panel(disp, view, ctx),
        View::Months => months_panel(disp, view),
        View::Years => years_panel(disp, view),
    };

    // The popover box: sensible defaults, each overridable via the caller's Style.
    let s = p.style.clone().unwrap_or_default();
    let mut deco = BoxDecoration::new()
        .color(s.background.unwrap_or(c.popover))
        .border(s.border.unwrap_or(Border::new(c.border, 1.0)))
        .radius(s.radius.unwrap_or(BorderRadius::all(theme().radius)));
    if s.shadows.is_empty() {
        deco = deco.shadow(BoxShadow::new(
            Color::from_rgba8(0, 0, 0, 45),
            Offset::new(0.0, 8.0),
            22.0,
            -4.0,
        ));
    } else {
        for sh in &s.shadows {
            deco = deco.shadow(*sh);
        }
    }
    Container::new()
        .width(s.width.unwrap_or(BODY_W + 24.0))
        .decoration(deco)
        .padding(s.padding.unwrap_or(EdgeInsets::all(12.0)))
        .child(panel)
        .into_widget()
}
