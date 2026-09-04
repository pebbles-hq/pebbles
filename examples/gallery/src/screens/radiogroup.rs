use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn radio_groups() -> Element {
    screen("Radio Group")
        .description("A set of mutually-exclusive options (shadcn's RadioGroup). Self-managing — seed the choice, and picking one reports its index.")
        .body(
        children![basic(), horizontal(), with_descriptions(), sizes(), colors(), disabled()],
    )
}

fn basic() -> impl IntoWidget {
    let choice = create_signal(0usize);
    doc("Basic").description("A vertical group; picking an option selects it and reports the index.").body(
        column(children![
            radio_group(["Default", "Comfortable", "Compact"]).value(0).on_changed(move |i| choice.set(i)),
            gap_h(10.0),
            muted(format!("selected index: {}", choice.get())),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn horizontal() -> impl IntoWidget {
    doc("Horizontal")
        .description("Lay the options out in a row with .orientation(Axis::Horizontal).")
        .body(radio_group(["Small", "Medium", "Large"]).value(1).orientation(Axis::Horizontal))
}

fn with_descriptions() -> impl IntoWidget {
    doc("With descriptions")
        .description("Add a muted line under an option with .description(index, …).")
        .body(
            radio_group(["Starter", "Pro"])
                .value(1)
                .description(0, "Everything you need to get going.")
                .description(1, "Advanced features and priority support."),
        )
}

fn sizes() -> impl IntoWidget {
    doc("Sizes").description("Scale the whole group with .size().").body(
        row(children![
            radio_group(["One", "Two"]).size(ToggleSize::Sm),
            radio_group(["One", "Two"]).size(ToggleSize::Md),
            radio_group(["One", "Two"]).size(ToggleSize::Lg),
        ])
        .main_axis_size(MainAxisSize::Min)
        .spacing(40.0),
    )
}

fn colors() -> impl IntoWidget {
    doc("Colors").description("Tint the selected dot with .color().").body(
        row(children![
            radio_group(["A", "B"]).color(palette::emerald::S600),
            radio_group(["A", "B"]).color(palette::blue::S600),
            radio_group(["A", "B"]).color(palette::rose::S600),
        ])
        .main_axis_size(MainAxisSize::Min)
        .spacing(40.0),
    )
}

fn disabled() -> impl IntoWidget {
    doc("Disabled")
        .description("Disable the entire group with .disabled(true).")
        .body(radio_group(["Enabled looks like this", "But you can't pick me"]).value(0).disabled(true))
}
