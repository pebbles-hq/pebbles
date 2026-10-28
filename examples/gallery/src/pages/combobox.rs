use pebbles::prelude::*;

use crate::ui::{doc, screen};

const FRAMEWORKS: [&str; 10] =
    ["Apple", "Apricot", "Banana", "Blueberry", "Cherry", "Grape", "Lemon", "Mango", "Peach", "Plum"];

pub fn combobox_screen() -> Element {
    screen("Combobox").body(children![basic(), preselected(), multi(), narrow()])
}

fn basic() -> impl IntoWidget {
    let combo = create_signal(String::new());
    doc("Combobox — searchable select").body(
        column(children![
            combobox(FRAMEWORKS)
                .width(260.0)
                .placeholder("Select fruit…")
                .search_placeholder("Search fruit…")
                .empty("No fruit found.")
                .on_changed(move |_i, label| combo.set(label.to_string())),
            muted(format!("fruit: {}", if combo.get().is_empty() { "—".to_string() } else { combo.get() })),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .spacing(10.0),
    )
}

fn preselected() -> impl IntoWidget {
    doc("Preselected value").body(combobox(FRAMEWORKS).width(260.0).value(2).placeholder("Select fruit…"))
}

fn multi() -> impl IntoWidget {
    let multi = create_signal(String::from("none"));
    doc("Multi-select — choose several").body(
        column(children![
            multi_select(FRAMEWORKS).width(260.0).placeholder("Add fruits…").values([0, 7]).on_changed(
                move |sel| multi.set(if sel.is_empty() {
                    "none".to_string()
                } else {
                    format!("{} selected", sel.len())
                })
            ),
            muted(format!("count: {}", multi.get())),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .spacing(10.0),
    )
}

fn narrow() -> impl IntoWidget {
    doc("Any width").body(
        column(children![
            combobox(["Low", "Medium", "High", "Critical"]).width(160.0).placeholder("Priority…"),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}
