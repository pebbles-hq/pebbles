//! Input & forms additions — one screen per new widget: `ChoiceChip`, `FilterChip`,
//! `ActionChip`, and `Stepper`. (Pebbles already ships the rest of the input set:
//! TextField, Checkbox, Radio, Switch, Slider — incl. two-thumb `.range()` — Select,
//! Combobox, ToggleGroup, DateField/Calendar/TimeField, Command, and `field(..)` for
//! labelled/validated fields.)

use std::collections::HashSet;

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

// ===========================================================================
// ChoiceChip — single-select
// ===========================================================================

pub fn choice_chip_screen() -> Element {
    let selected = create_signal(1usize);
    let sizes = ["Small", "Medium", "Large", "X-Large"];

    screen("Choice Chip")
        .description("A single-select chip — one of a set is chosen at a time. Flutter's ChoiceChip.")
        .body(children![
            doc("choice_chip(label).selected(..).on_pressed(..)")
                .description("Pick a size; the owner keeps a single selected index.")
                .body(
                    column(children![
                        row(sizes
                            .iter()
                            .enumerate()
                            .flat_map(|(i, s)| {
                                vec![
                                    choice_chip(*s)
                                        .selected(selected.get() == i)
                                        .on_pressed(move || selected.set(i))
                                        .into_widget(),
                                    gap_w(8.0).into_widget(),
                                ]
                            })
                            .collect::<Vec<_>>())
                        .main_axis_size(MainAxisSize::Min),
                        gap_h(14.0),
                        text(&format!("Selected: {}", sizes[selected.get()]))
                            .size(13.0)
                            .color(theme().colors.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                )
        ])
}

// ===========================================================================
// FilterChip — multi-select with a check
// ===========================================================================

pub fn filter_chip_screen() -> Element {
    let selected = create_signal(HashSet::<usize>::new());
    let tags = ["Rust", "Vello", "wgpu", "GUI", "Reactive"];

    screen("Filter Chip")
        .description("A multi-select toggle chip that shows a leading check when on. Flutter's FilterChip.")
        .body(children![
            doc("filter_chip(label).selected(..).on_pressed(..)")
                .description("Toggle any number of tags; each flips its membership in the set.")
                .body(
                    column(children![
                        wrap(
                            tags.iter()
                                .enumerate()
                                .map(|(i, t)| {
                                    filter_chip(*t)
                                        .selected(selected.get().contains(&i))
                                        .on_pressed(move || {
                                            selected.update(|s| {
                                                if !s.remove(&i) {
                                                    s.insert(i);
                                                }
                                            })
                                        })
                                        .into_widget()
                                })
                                .collect::<Vec<_>>()
                        )
                        .spacing(8.0)
                        .run_spacing(8.0),
                        gap_h(14.0),
                        text(&format!("{} selected", selected.get().len()))
                            .size(13.0)
                            .color(theme().colors.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                )
        ])
}

// ===========================================================================
// ActionChip — tappable action
// ===========================================================================

pub fn action_chip_screen() -> Element {
    let last = create_signal(String::from("—"));
    let actions = [("Email", IconKind::Mail), ("Search", IconKind::Search), ("Details", IconKind::Info)];

    screen("Action Chip")
        .description("A tappable chip that triggers an action (no selected state). Flutter's ActionChip.")
        .body(children![
            doc("action_chip(label).icon(..).on_pressed(..)")
                .description("Each chip fires its action on tap.")
                .body(
                    column(children![
                        row(actions
                            .iter()
                            .flat_map(|(label, ic)| {
                                let label = *label;
                                vec![
                                    action_chip(label)
                                        .icon(*ic)
                                        .on_pressed(move || last.set(format!("{label} pressed")))
                                        .into_widget(),
                                    gap_w(8.0).into_widget(),
                                ]
                            })
                            .collect::<Vec<_>>())
                        .main_axis_size(MainAxisSize::Min),
                        gap_h(14.0),
                        text(&format!("Last action: {}", last.get()))
                            .size(13.0)
                            .color(theme().colors.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                )
        ])
}

// ===========================================================================
// Stepper
// ===========================================================================

fn step_body(text_line: &str) -> impl IntoWidget {
    Container::new()
        .decoration(
            BoxDecoration::new()
                .color(theme().colors.muted)
                .radius(BorderRadius::all(8.0))
                .border(Border::all(BorderSide::new(theme().colors.border, 1.0))),
        )
        .padding(EdgeInsets::all(14.0))
        .child(text(text_line).size(13.0).color(theme().colors.foreground))
}

pub fn stepper_screen() -> Element {
    let current = create_signal(0usize);
    let n = 3usize;

    screen("Stepper")
        .description("A numbered step flow; earlier steps complete (check), the current one reveals its content. Flutter's Stepper.")
        .body(children![
            doc("stepper(steps).current(..).on_step_tapped(..)")
                .description("Use Back/Next, or tap a step header, to move through the flow.")
                .body(column(children![
                    row(children![
                        button("Back").on_pressed(move || current.update(|i| *i = i.saturating_sub(1))),
                        gap_w(8.0),
                        button("Next").on_pressed(move || current.update(|i| *i = (*i + 1).min(n - 1))),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(20.0),
                    stepper(vec![
                        step("Account", step_body("Enter your name and email.")).subtitle("Your details"),
                        step("Payment", step_body("Add a billing method.")).subtitle("Billing info"),
                        step("Confirm", step_body("Review everything, then submit.")).subtitle("Review & submit"),
                    ])
                    .current(current.get())
                    .on_step_tapped(move |i| current.set(i)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}
