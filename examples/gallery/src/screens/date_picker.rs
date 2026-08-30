use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

/// A labelled date field demonstrating one caption layout.
fn caption_demo(label: &str, layout: CaptionLayout) -> impl IntoWidget {
    column(
        children![muted(label), gap_h(6.0), date_field().caption(layout).width(210.0)]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0)
}

/// A labelled date field demonstrating one date format.
fn caption_fmt(label: &str, fmt: DateFormat) -> impl IntoWidget {
    column(children![muted(label), gap_h(6.0), date_field().format(fmt).width(200.0)])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min()
}

/// A labelled time field.
fn caption_time(label: &str, field: TimeField) -> impl IntoWidget {
    column(children![muted(label), gap_h(6.0), field.width(200.0)])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min()
}

pub fn date_picker() -> impl IntoWidget {
    let picked = create_signal(String::new());
    let inline = calendar(move |y, m, d| picked.set(format!("{m:02}/{d:02}/{y:04}")))
        .caption(CaptionLayout::Dropdown);

    screen(
        "Date Picker",
        "A shadcn-style calendar. Click the month or year in the caption to jump straight to a month grid or year grid — no scrubbing through the arrows. Pick the caption layout that fits.",
        children![
            doc(
                "Date input",
                "Type digits (they auto-format to MM/DD/YYYY) or click the calendar button to open the picker. Reopening the picker highlights the current value and opens on its month.",
                date_field().width(240.0),
            ),
            doc(
                "Caption layouts",
                "shadcn's captionLayout — a plain label with arrows, or clickable month/year dropdowns. Each dropdown drills into a grid so years are one click away.",
                wrap(children![
                    caption_demo("Label", CaptionLayout::Label),
                    caption_demo("Dropdown", CaptionLayout::Dropdown),
                    caption_demo("Dropdown · months", CaptionLayout::DropdownMonths),
                    caption_demo("Dropdown · years", CaptionLayout::DropdownYears),
                ])
                .spacing(22.0)
                .run_spacing(18.0),
            ),
            doc(
                "Custom formats",
                "The developer picks the format — order + separator. MDY, DMY, YMD, or your own separator; typing and the picker both honor it.",
                wrap(children![
                    caption_fmt("MM/DD/YYYY", DateFormat::MDY),
                    caption_fmt("DD/MM/YYYY", DateFormat::DMY),
                    caption_fmt("YYYY-MM-DD", DateFormat::YMD),
                    caption_fmt("DD.MM.YYYY", DateFormat::DMY.separator('.')),
                ])
                .spacing(22.0)
                .run_spacing(18.0),
            ),
            doc(
                "Time picker — separate from the date picker",
                "time_field() is its own widget: type a specific time, or open the dropdown of slots. 24-hour or .hour12() (AM/PM), and a configurable .step().",
                wrap(children![
                    caption_time("24-hour · 30m", time_field()),
                    caption_time("12-hour · 15m", time_field().hour12().step(15)),
                ])
                .spacing(22.0)
                .run_spacing(18.0),
            ),
            doc(
                "Styled calendar via Style",
                "The calendar popover takes a Style — background, border, radius, shadow — like every other widget. (.style(..) does the same for the input box.)",
                date_field().width(240.0).calendar_style(
                    style()
                        .background(theme().colors.card)
                        .radius_all(18.0)
                        .border(Border::new(theme().colors.primary, 1.5)),
                ),
            ),
            doc(
                "Inline calendar — pick month & year",
                "The calendar on its own. Click “Month ▾” for the 12-month grid, or the year for a year grid (page decades with the arrows). Selecting a day reports it back.",
                column(
                    children![
                        inline,
                        gap_h(12.0),
                        muted(if picked.get().is_empty() {
                            "No date selected — try the month/year dropdowns.".to_string()
                        } else {
                            format!("Selected: {}", picked.get())
                        }),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
        ],
    )
}
