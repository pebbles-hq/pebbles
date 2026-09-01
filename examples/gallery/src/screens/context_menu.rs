//! Context Menu: the global right-click service and per-widget opt-in/opt-out.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

pub fn context_menu() -> Element {
    screen("Context Menu")
        .description("The GLOBAL right-click menu: disabled by default, opt in globally, per widget, or per area. Buttons and controls consume right-clicks by default (opt in with .context_menu); the File Explorer always owns its own. Menu commands route to the focused editor.")
        .body(children![global_switch(), per_widget()])
}

fn global_switch() -> impl IntoWidget {
    let enabled = create_signal(false);
    doc("Global menu")
        .description("Flip the global switch on, customize the items, restyle it, or restore defaults — then right-click the surfaces below.")
        .body(
            column(children![
                row(children![
                    button(if enabled.get() { "Disable global menu" } else { "Enable global menu" }).variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(move || {
                        let next = !enabled.get();
                        enabled.set(next);
                        set_global_menu_enabled(next);
                    }),
                    gap_w(8.0),
                    button("Custom options").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(|| {
                        set_global_menu(vec![
                            menu_item("Refresh").icon(lucide::REFRESH_CW).on_select(|| {}).into(),
                            menu_sub(
                                "Go to",
                                [menu_item("Overview"), menu_item("Data Table")],
                            ),
                            menu_separator(),
                            menu_item("Settings").icon(lucide::SETTINGS).on_select(|| {}).into(),
                        ]);
                    }),
                    gap_w(8.0),
                    button("Restore defaults").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(reset_global_menu),
                    gap_w(8.0),
                    button("Style it").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(|| {
                        set_global_menu_style(
                            style()
                                .background(theme().colors.card)
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius_all(theme().radius + 2.0),
                        );
                    }),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(14.0),
                wrap(children![
                    demo_box("Right-click this box", "global_menu_on(..) — the default menu on THIS widget, even while global is off", global_menu_on(
                        Container::new()
                            .height(90.0)
                            .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(theme().radius)))
                            .alignment(Alignment::CENTER)
                            .child(muted("global_menu_on(..)")),
                    )),
                    demo_box("Right-click this box", "block_context_menu(..) — suppressed here, always", block_context_menu(
                        Container::new()
                            .height(90.0)
                            .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(theme().radius)))
                            .alignment(Alignment::CENTER)
                            .child(muted("block_context_menu(..)")),
                    )),
                    demo_box("Right-click this button", "buttons consume right-clicks by default", button("Plain button").variant(ButtonVariant::Secondary)),
                    demo_box("Right-click this button", ".context_menu(..) opts the button back in", button("With context menu").variant(ButtonVariant::Secondary).context_menu(|| {
                        toast("Button context menu").show();
                    })),
                    demo_box("Right-click the text", "text surfaces get the global menu when enabled", text("Right-click me when the global menu is on.").size(13.0)),
                ])
                .spacing(12.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn per_widget() -> impl IntoWidget {
    doc("Per-widget menus")
        .description("on_context_menu(child, builder) attaches a fully custom menu to any widget — independent of the global switch.")
        .body(
            row(children![
                on_context_menu(
                    button("Right-click for a custom menu").variant(ButtonVariant::Outline),
                    move |e| {
                        set_global_menu(vec![
                            menu_item("Copy").icon(lucide::COPY).on_select(|| {}).into(),
                            menu_item("Cut").icon(lucide::SCISSORS).on_select(|| {}).into(),
                            menu_separator(),
                            menu_item("Delete").icon(lucide::TRASH).on_select(|| {}).into(),
                        ]);
                        show_global_menu_here(e.global.x, e.global.y);
                        reset_global_menu();
                    },
                ),
                gap_w(10.0),
                muted("set_global_menu(..) + show_global_menu_here(..)").size(12.0),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn demo_box(label: &str, hint: &str, content: impl IntoWidget) -> impl IntoWidget {
    column(children![
        muted(label),
        gap_w(0.0),
        content,
        gap_w(0.0),
        text(hint).size(11.5).color(theme().colors.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
    .spacing(6.0)
}
