use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

/// A labelled date field demonstrating one caption layout.
fn caption_demo(label: &str, layout: CaptionLayout) -> impl IntoWidget {
    column(children![
        muted(label),
        gap_h(6.0),
        date_field().caption(layout).width(210.0)
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
    .spacing(0.0)
}

/// A labelled date field demonstrating one date format.
fn caption_fmt(label: &str, fmt: DateFormat) -> impl IntoWidget {
    column(children![
        muted(label),
        gap_h(6.0),
        date_field().format(fmt).width(200.0)
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
}

/// A labelled time field.
fn caption_time(label: &str, field: TimeField) -> impl IntoWidget {
    column(children![muted(label), gap_h(6.0), field.width(200.0)])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
}

pub fn date_picker() -> Element {
    let picked = create_signal(String::new());
    let range_note = create_signal(String::from("—"));
    let inline = calendar(move |y, m, d| picked.set(format!("{m:02}/{d:02}/{y:04}")))
        .caption(CaptionLayout::Dropdown);

    screen("Date Picker")

        .description("A shadcn-style calendar. Click the month or year in the caption to jump straight to a month grid or year grid — no scrubbing through the arrows. Pick the caption layout that fits.")

        .body(
        children![
            doc("Date input")
                .description("Type digits (they auto-format to MM/DD/YYYY) or click the calendar button to open the picker. Reopening the picker highlights the current value and opens on its month.")
                .body(
                date_field().width(240.0),
            ),
            doc("Range date")
                .description("date_field().range(true): the calendar picks a start + end (order doesn't matter — endpoints sort), the input shows both read-only, on_range_changed reports each pick, and min/max bound the pickable days.")
                .body(
                column(children![
                    date_field()
                        .range(true)
                        .range_value((2026, 1, 1), (2026, 1, 7))
                        .clearable(true)
                        .min(2026, 1, 1)
                        .max(2026, 3, 31)
                        .width(300.0)
                        .on_range_changed(move |s, e| range_note.set(format!("{:02}/{:02}/{} – {:02}/{:02}/{}", s.1, s.2, s.0, e.1, e.2, e.0))),
                    muted(format!("last pick: {}", range_note.get())),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .spacing(10.0),
            ),
            doc("Caption layouts")
                .description("shadcn's captionLayout — a plain label with arrows, or clickable month/year dropdowns. Each dropdown drills into a grid so years are one click away.")
                .body(
                wrap(children![
                    caption_demo("Label", CaptionLayout::Label),
                    caption_demo("Dropdown", CaptionLayout::Dropdown),
                    caption_demo("Dropdown · months", CaptionLayout::DropdownMonths),
                    caption_demo("Dropdown · years", CaptionLayout::DropdownYears),
                ])
                .spacing(22.0)
                .run_spacing(18.0),
            ),
            doc("Custom formats")
                .description("The developer picks the format — order + separator. MDY, DMY, YMD, or your own separator; typing and the picker both honor it.")
                .body(
                wrap(children![
                    caption_fmt("MM/DD/YYYY", DateFormat::MDY),
                    caption_fmt("DD/MM/YYYY", DateFormat::DMY),
                    caption_fmt("YYYY-MM-DD", DateFormat::YMD),
                    caption_fmt("DD.MM.YYYY", DateFormat::DMY.separator('.')),
                ])
                .spacing(22.0)
                .run_spacing(18.0),
            ),
            doc("Time picker — separate from the date picker")
                .description("time_field() is its own widget: type a specific time, or open the dropdown of slots. 24-hour or .hour12() (AM/PM), and a configurable .step().")
                .body(
                wrap(children![
                    caption_time("24-hour · 30m", time_field()),
                    caption_time("12-hour · 15m", time_field().hour12().step(15)),
                ])
                .spacing(22.0)
                .run_spacing(18.0),
            ),
            doc("Styled calendar via Style")
                .description("The calendar popover takes a Style — background, border, radius, shadow — like every other widget. (.style(..) does the same for the input box.)")
                .body(
                date_field().width(240.0).calendar_style(
                    style()
                        .background(theme().colors.card)
                        .radius_all(18.0)
                        .border(Border::new(theme().colors.primary, 1.5)),
                ),
            ),
            doc("Inline calendar — pick month & year")
                .description("The calendar on its own. Click “Month ▾” for the 12-month grid, or the year for a year grid (page decades with the arrows). Selecting a day reports it back.")
                .body(
                column(
                    children![
                        inline,
                        gap_h(12.0),
                        muted(if picked.get().is_empty() {
                            "No date selected — try the month/year dropdowns.".to_string()
                        } else {
                            format!("Selected: {}", picked.get())
                        }),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).spacing(0.0),
            ),
        ],
    )
}
