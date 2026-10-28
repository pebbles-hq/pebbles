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
                        .icon(tabler::FILE_PLUS)
                        .shortcut("⌘N")
                        .on_select(move || picked.set("Picked: New File".into())),
                    command_item("Open Project")
                        .icon(tabler::FOLDER_OPEN)
                        .on_select(move || picked.set("Picked: Open Project".into())),
                ],
            ),
            command_group(
                "Settings",
                [
                    command_item("Toggle Theme").icon(tabler::MOON).on_select(toggle_theme),
                    command_item("Toggle Fullscreen")
                        .icon(tabler::MAXIMIZE)
                        .on_select(move || picked.set("Picked: Toggle Fullscreen".into())),
                ],
            ),
        ]
    };

    // The ⌘K palette binding (B2): a real shortcut hook, auto-unregistered when
    // this screen unmounts — the "app-side recipe" is gone.
    create_shortcut("Mod+K", move || command_palette(groups()).open());

    screen("Command").body(children![
        doc("Inline command").body(command(groups()).width(460.0),),
        doc("Command palette").body(
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
    ])
}
