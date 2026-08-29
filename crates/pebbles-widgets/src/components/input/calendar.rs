//! [`Calendar`] — a month-grid date picker shown in the overlay by `date_field`.
//! Self-contained Gregorian date math (no external date crate): today's date comes
//! from `SystemTime`; the grid navigates by month and reports the picked
//! `(year, month, day)`.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use pebbles_foundation::{Alignment, Color, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconKind};

use super::ButtonVariant;
use super::icon_button;
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, SizedBox, column, row, spacer, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action, component_props, create_signal};

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

// --- widget ---

/// A month-grid date picker. `on_pick(year, month, day)` fires on a day tap.
pub struct Calendar {
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
}

/// Create a [`Calendar`].
pub fn calendar(on_pick: impl Fn(i32, u32, u32) + 'static) -> Calendar {
    Calendar { on_pick: Rc::new(on_pick) }
}

struct Props {
    on_pick: Rc<dyn Fn(i32, u32, u32)>,
}

impl IntoWidget for Calendar {
    fn into_widget(self) -> AnyWidget {
        component_props(render_calendar, Props { on_pick: self.on_pick }).into_widget()
    }
}

const CELL: f64 = 34.0;

fn day_cell(y: i32, m: u32, d: u32, is_today: bool, on_pick: Rc<dyn Fn(i32, u32, u32)>) -> AnyWidget {
    let c = theme().colors;
    let mut deco = BoxDecoration::new().radius(BorderRadius::all(6.0));
    if is_today {
        deco = deco.color(c.accent);
    }
    let cell = Container::new()
        .width(CELL)
        .height(CELL)
        .alignment(Alignment::CENTER)
        .decoration(deco)
        .child(text(format!("{d}")).size(13.0).color(c.foreground));
    GestureDetector::new(cell)
        .cursor(Cursor::Pointer)
        .on_tap(action(move || on_pick(y, m, d)))
        .into_widget()
}

fn render_calendar(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let (ty, tm, td) = today();
    let disp = create_signal((ty, tm));
    let (y, m) = disp.get();

    // Month navigation.
    let prev = icon_button(IconKind::ChevronLeft).variant(ButtonVariant::Ghost).size(16.0).on_pressed(
        action(move || disp.update(|(yy, mm)| {
            if *mm == 1 {
                *mm = 12;
                *yy -= 1;
            } else {
                *mm -= 1;
            }
        })),
    );
    let next = icon_button(IconKind::ChevronRight).variant(ButtonVariant::Ghost).size(16.0).on_pressed(
        action(move || disp.update(|(yy, mm)| {
            if *mm == 12 {
                *mm = 1;
                *yy += 1;
            } else {
                *mm += 1;
            }
        })),
    );
    let header = row(vec![
        prev.into_widget(),
        spacer().into_widget(),
        text(format!("{} {}", month_name(m), y)).size(14.0).semibold().color(c.foreground).into_widget(),
        spacer().into_widget(),
        next.into_widget(),
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
                Some(d) => day_cell(y, m, *d, *d == td && y == ty && m == tm, p.on_pick.clone()),
                None => SizedBox::spacer(CELL, CELL).into_widget(),
            })
            .collect();
        weeks.push(row(cells).main_axis_min().into_widget());
    }

    let mut body: Vec<AnyWidget> = vec![
        header.into_widget(),
        SizedBox::spacer(0.0, 10.0).into_widget(),
        row(weekdays).main_axis_min().into_widget(),
        SizedBox::spacer(0.0, 4.0).into_widget(),
    ];
    body.extend(weeks);

    Container::new()
        .width(7.0 * CELL + 24.0)
        .decoration(
            BoxDecoration::new()
                .color(c.popover)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius))
                .shadow(BoxShadow::new(
                    Color::from_rgba8(0, 0, 0, 45),
                    Offset::new(0.0, 8.0),
                    22.0,
                    -4.0,
                )),
        )
        .padding(EdgeInsets::all(12.0))
        .child(column(body).main_axis_min())
        .into_widget()
}
