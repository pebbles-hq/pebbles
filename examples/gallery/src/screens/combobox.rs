use pebbles::prelude::*;

use crate::ui::{doc, screen};

const FRAMEWORKS: [&str; 10] =
    ["Next.js", "SvelteKit", "Nuxt", "Remix", "Astro", "Angular", "Vue", "Solid", "Qwik", "Ember"];

pub fn combobox_screen() -> Element {
    screen("Combobox")
        .description("A searchable select built on the overlay + a filtered command list — shadcn's Combobox (single choice) and MultiSelect (many). Type to filter, with a check on the current value and a “no results” state.")
        .body(
        children![basic(), preselected(), multi(), narrow()],
    )
}

fn basic() -> impl IntoWidget {
    let combo = create_signal(String::new());
    doc("Combobox — searchable select")
        .description("A single-select whose search box filters the list as you type; picking closes it. Customize the trigger, search and empty text.")
        .body(
        column(children![
            combobox(FRAMEWORKS)
                .width(260.0)
                .placeholder("Select framework…")
                .search_placeholder("Search framework…")
                .empty("No framework found.")
                .on_changed(move |_i, label| combo.set(label.to_string())),
            muted(format!(
                "framework: {}",
                if combo.get().is_empty() { "—".to_string() } else { combo.get() }
            )),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .spacing(10.0),
    )
}

fn preselected() -> impl IntoWidget {
    doc("Preselected value")
        .description(
            "Seed the current choice with .value(index) — the trigger shows it and the menu checks it.",
        )
        .body(combobox(FRAMEWORKS).width(260.0).value(2).placeholder("Select framework…"))
}

fn multi() -> impl IntoWidget {
    let multi = create_signal(String::from("none"));
    doc("Multi-select — choose several")
        .description("Items toggle and the menu stays open; the trigger summarizes the selection. Search filters the same way.")
        .body(
        column(children![
            multi_select(FRAMEWORKS)
                .width(260.0)
                .placeholder("Add frameworks…")
                .values([0, 7])
                .on_changed(move |sel| multi.set(if sel.is_empty() {
                    "none".to_string()
                } else {
                    format!("{} selected", sel.len())
                })),
            muted(format!("count: {}", multi.get())),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .spacing(10.0),
    )
}

fn narrow() -> impl IntoWidget {
    doc("Any width").description("The trigger and popover share a width — set it with .width().").body(
        column(children![
            combobox(["Low", "Medium", "High", "Critical"]).width(160.0).placeholder("Priority…"),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}
