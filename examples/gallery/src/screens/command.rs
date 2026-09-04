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

    // The ⌘K palette binding (B2): a real shortcut hook, auto-unregistered when
    // this screen unmounts — the "app-side recipe" is gone.
    create_shortcut("Mod+K", move || command_palette(groups()).open());

    screen("Command")

        .description("A searchable command list (shadcn's Command) — inline, or centered as the ⌘K palette.")

        .body(
        children![
            doc("Inline command")
                .description("Type to filter across groups; Up/Down move the highlight, Enter runs, Escape clears.")
                .body(
                command(groups()).width(460.0),
            ),
            doc("Command palette")
                .description("The same list centered in a dismissible modal — press ⌘K anywhere (even while typing in a field — editor intents still win for their own keys) to open it. The binding is a real create_shortcut hook, not app-side glue.")
                .body(
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
