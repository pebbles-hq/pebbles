use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn command_screen() -> Element {
    let picked = create_signal(String::from("Nothing picked yet."));

    let groups = move || {
        [
            command_group(
                "Suggestions",
                [
                    command_item("New File")
                        .icon(lucide::FILE_PLUS)
                        .shortcut("⌘N")
                        .on_select(move || picked.set("Picked: New File".into())),
                    command_item("Open Project")
                        .icon(lucide::FOLDER_OPEN)
                        .on_select(move || picked.set("Picked: Open Project".into())),
                ],
            ),
            command_group(
                "Settings",
                [
                    command_item("Toggle Theme").icon(lucide::MOON).on_select(toggle_theme),
                    command_item("Toggle Fullscreen")
                        .icon(lucide::MAXIMIZE)
                        .on_select(move || picked.set("Picked: Toggle Fullscreen".into())),
                ],
            ),
        ]
    };

    screen(
        "Command",
        "A searchable command list (shadcn's Command) — inline, or centered as the ⌘K palette.",
        children![
            doc(
                "Inline command",
                "Type to filter across groups; Up/Down move the highlight, Enter runs, Escape clears.",
                command(groups()).width(460.0),
            ),
            doc(
                "Command palette",
                "The same list centered in a dismissible modal — call .open() from a key handler (the ⌘K binding is app-side).",
                column(children![
                    row(children![
                        button("Open palette").on_pressed(move || command_palette(groups()).open()),
                        gap_w(10.0),
                        kbd("⌘K"),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                    gap_h(10.0),
                    muted(picked.get()),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
        ],
    )
}
