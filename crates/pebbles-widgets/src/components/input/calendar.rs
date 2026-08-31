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
use crate::widgets::{Container, GestureDetector, SizedBox, column, gap_h, gap_w, row, spacer, text};
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

/// A month-grid date picker. `on_pick(year, month, day)` fires on a day tap.
pub struct Calendar {
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
    caption: CaptionLayout,
    selected: Option<(i32, u32, u32)>,
    start: Option<(i32, u32)>,
    style: Option<Style>,
}

/// Create a [`Calendar`].
pub fn calendar(on_pick: impl Fn(i32, u32, u32) + 'static) -> Calendar {
    Calendar {
        on_pick: Rc::new(on_pick),
        caption: CaptionLayout::Label,
        selected: None,
        start: None,
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
    selected: Option<(i32, u32, u32)>,
    start: Option<(i32, u32)>,
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
                style: self.style,
            },
        )
        .into_widget()
    }
}

const CELL: f64 = 40.0;
const BODY_W: f64 = 7.0 * CELL;

// --- small building blocks ---

/// A square outline nav arrow (prev/next).
fn nav_arrow(kind: IconKind, on_tap: impl Fn() + 'static) -> AnyWidget {
    icon_button(kind)
        .variant(ButtonVariant::Outline)
        .size(15.0)
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

/// A single day cell — highlights the selected day, then today.
fn day_cell(
    y: i32,
    m: u32,
    d: u32,
    is_today: bool,
    is_selected: bool,
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
) -> AnyWidget {
    let c = theme().colors;
    let mut deco = BoxDecoration::new().radius(BorderRadius::all(6.0));
    let fg = if is_selected {
        deco = deco.color(c.primary);
        c.primary_foreground
    } else if is_today {
        deco = deco.color(c.accent);
        c.accent_foreground
    } else {
        c.foreground
    };
    let mut label = text(format!("{d}")).size(13.0).color(fg);
    if is_selected {
        label = label.semibold();
    }
    let cell = Container::new()
        .width(CELL)
        .height(CELL)
        .alignment(Alignment::CENTER)
        .decoration(deco)
        .child(label);
    GestureDetector::new(cell)
        .cursor(Cursor::Pointer)
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

fn days_panel(
    disp: Signal<(i32, u32)>,
    view: Signal<View>,
    caption: CaptionLayout,
    selected: Option<(i32, u32, u32)>,
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
) -> AnyWidget {
    let c = theme().colors;
    let (ty, tm, td) = today();
    let (y, m) = disp.get();

    let prev = nav_arrow(IconKind::ChevronLeft, move || {
        disp.update(|(yy, mm)| {
            if *mm == 1 {
                *mm = 12;
                *yy -= 1;
            } else {
                *mm -= 1;
            }
        })
    });
    let next = nav_arrow(IconKind::ChevronRight, move || {
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
                Some(d) => day_cell(
                    y,
                    m,
                    *d,
                    *d == td && y == ty && m == tm,
                    selected == Some((y, m, *d)),
                    on_pick.clone(),
                ),
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

    let prev = nav_arrow(IconKind::ChevronLeft, move || disp.update(|(yy, _)| *yy -= 1));
    let next = nav_arrow(IconKind::ChevronRight, move || disp.update(|(yy, _)| *yy += 1));
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

    let prev = nav_arrow(IconKind::ChevronLeft, move || disp.update(|(yy, _)| *yy -= 12));
    let next = nav_arrow(IconKind::ChevronRight, move || disp.update(|(yy, _)| *yy += 12));
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
    let init = p.start.or(p.selected.map(|(y, m, _)| (y, m))).unwrap_or((ty, tm));
    let disp = create_signal(init);
    let view = create_signal(View::Days);

    let panel = match view.get() {
        View::Days => days_panel(disp, view, p.caption, p.selected, p.on_pick.clone()),
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
