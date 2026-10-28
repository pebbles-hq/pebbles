use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn kbd_screen() -> Element {
    screen("Kbd").body(children![
        doc("Keys").body(
            wrap(children![
                kbd("⌘K"),
                kbd("Ctrl+C"),
                kbd("⇧⌘P"),
                kbd("Esc"),
                kbd("Tab"),
                kbd("Enter"),
                kbd("Space"),
            ])
            .spacing(8.0),
        ),
        doc("Combos").body(
            column(children![
                combo(&["⌘", "K"]),
                gap_h(8.0),
                combo(&["Ctrl", "Shift", "P"]),
                gap_h(8.0),
                combo(&["⌥", "⇧", "F"]),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Shortcut list").body(
            card().child(
                column(children![
                    shortcut_row("Command palette", "⌘K"),
                    separator(),
                    shortcut_row("Quick open", "⌘P"),
                    separator(),
                    shortcut_row("Save", "⌘S"),
                    separator(),
                    shortcut_row("Close window", "⌘W"),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
        ),
        doc("Inline").body(
            row(children![
                text("Press").size(14.0),
                gap_w(6.0),
                kbd("Enter"),
                gap_w(6.0),
                text("to confirm, or").size(14.0),
                gap_w(6.0),
                kbd("Esc"),
                gap_w(6.0),
                text("to cancel.").size(14.0),
            ])
            .main_axis_size(MainAxisSize::Min),
        ),
    ])
}

fn combo(keys: &[&str]) -> impl IntoWidget {
    let mut kids: Vec<AnyWidget> = Vec::new();
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            kids.push(text("+").size(12.0).color(theme().colors.muted_foreground).into_widget());
            kids.push(gap_w(4.0).into_widget());
        }
        kids.push(kbd(*key).into_widget());
    }
    row(kids).main_axis_size(MainAxisSize::Min)
}

fn shortcut_row(label: &str, keys: &str) -> impl IntoWidget {
    padding(
        EdgeInsets::symmetric(12.0, 10.0),
        row(children![expanded(text(label.to_string()).size(14.0)), kbd(keys.to_string()),]),
    )
}
