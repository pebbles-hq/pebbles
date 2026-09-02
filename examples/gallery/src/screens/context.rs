use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

pub fn context() -> Element {
    screen("Context & Scopes")
        .description(
            "Render-time context: theme_override swaps the theme for one subtree only, and focus_scope traps Tab-cycling inside it — dialogs and sheets contain focus; preview panes contain their own theme.",
        )
        .body(children![
            scoped_theme(),
            focus_scope_demo(),
        ])
}

fn scoped_theme() -> impl IntoWidget {
    doc("theme_override")
        .description("Inside theme_override(..) every component reads the overridden theme; outside, the global one. The global light/dark toggle still re-renders everything — the override just wins within its subtree.")
        .body(
            row(children![
                card()
                    .title("Global theme")
                    .child(badge("uses theme()").variant(BadgeVariant::Secondary))
                    .into_widget(),
                gap_w(16.0),
                theme_override(
                    Theme::dark(),
                    card()
                        .title("Overridden to dark")
                        .child(badge("same components, dark tokens").variant(BadgeVariant::Secondary)),
                )
                .into_widget(),
                gap_w(16.0),
                theme_override(
                    Theme::light(),
                    card()
                        .title("Overridden to light")
                        .child(badge("even in dark mode").variant(BadgeVariant::Secondary)),
                )
                .into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn focus_scope_demo() -> impl IntoWidget {
    let focus_note = create_signal(String::from("Tab cycles only inside the scoped pane."));
    doc("focus_scope")
        .description("A focus scope turns its subtree into its own Tab cycle: keyboard traversal never leaks out. Click into the pane (or Tab from outside until it is focused), then Tab — focus wraps within the three buttons. Outside buttons form their own cycle.")
        .body(
            column(children![
                row(children![
                    button("Outside 1").on_pressed(|| {}),
                    gap_w(8.0),
                    button("Outside 2").on_pressed(|| {}),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(14.0),
                focus_scope(
                    Container::new()
                        .width(360.0)
                        .padding(EdgeInsets::all(14.0))
                        .decoration(
                            BoxDecoration::new()
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius(BorderRadius::all(theme().radius)),
                        )
                        .child(column(children![
                            muted("SCOPED PANE — focus is trapped in here").size(11.5),
                            gap_h(8.0),
                            row(children![
                                button("Inner A").on_pressed(move || {
                                    focus_note.set("A pressed — focus stays in scope.".to_string())
                                }),
                                gap_w(8.0),
                                button("Inner B").on_pressed(move || {
                                    focus_note.set("B pressed.".to_string())
                                }),
                                gap_w(8.0),
                                button("Inner C").on_pressed(move || {
                                    focus_note.set("C pressed.".to_string())
                                }),
                            ])
                            .main_axis_size(MainAxisSize::Min),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min)),
                ),
                gap_h(10.0),
                muted(focus_note.get().clone()).size(12.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}
