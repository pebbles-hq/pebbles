use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn menubars() -> Element {
    let note = create_signal(String::from("—"));

    screen("Menubar")
        .description("A desktop menu strip — click a trigger to open its menu below; while one is open, hovering a sibling switches to it. Esc or an outside click dismisses.")
        .body(children![
            doc("Classic")
                .description("File / Edit / View with icons, shortcuts, a disabled item and a destructive action.")
                .body(
                    column(children![
                        menubar()
                            .menu(
                                "File",
                                [
                                    menu_item("New").shortcut("⌘N").on_select(move || note.set("New".into())),
                                    menu_item("Open").shortcut("⌘O").on_select(move || note.set("Open".into())),
                                    menu_item("Save").shortcut("⌘S").on_select(move || note.set("Save".into())),
                                ],
                            )
                            .menu(
                                "Edit",
                                [
                                    menu_item("Undo").shortcut("⌘Z"),
                                    menu_item("Redo").shortcut("⇧⌘Z"),
                                ],
                            )
                            .menu(
                                "View",
                                [
                                    menu_item("Toggle Sidebar").on_select(move || note.set("Sidebar".into())),
                                    menu_item("Zen Mode").disabled(true),
                                ],
                            )
                            .menu(
                                "Help",
                                [menu_item("Report issue").destructive()],
                            ),
                        gap_h(8.0),
                        muted(format!("last action: {}", note.get())),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("With submenus")
                .description("menu_sub(..) adds a hover submenu — a second panel opens to the right.")
                .body(
                    menubar()
                        .menu(
                            "File",
                            [
                                menu_item("New").shortcut("⌘N").into(),
                                menu_sub(
                                    "Open Recent",
                                    [
                                        menu_item("pebbles-widgets"),
                                        menu_item("gallery"),
                                        menu_item("counter"),
                                    ],
                                ),
                            ],
                        )
                        .menu(
                            "Share",
                            [
                                menu_sub(
                                    "Copy as…",
                                    [
                                        menu_item("Markdown"),
                                        menu_item("HTML"),
                                    ],
                                ),
                                menu_item("Email link").into(),
                            ],
                        ),
                ),
            doc("Styled strip")
                .description("A Style covers the bar — background, border, radius — and its text props style the trigger labels.")
                .body(
                    column(children![
                        menubar()
                            .menu("File", [menu_item("New"), menu_item("Open")])
                            .menu("Edit", [menu_item("Undo"), menu_item("Redo")])
                            .style(
                                style()
                                    .background(theme().colors.card)
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius_all(theme().radius),
                            ),
                        gap_h(16.0),
                        menubar()
                            .menu("File", [menu_item("New"), menu_item("Open")])
                            .menu("Edit", [menu_item("Undo"), menu_item("Redo")])
                            .style(
                                style()
                                    .color(palette::blue::S600)
                                    .font_weight(600.0),
                            ),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
