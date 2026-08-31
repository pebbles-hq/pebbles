//! Overlays & feedback: Tooltip (passive hover hint), Popover (click panel), and
//! Toast (transient notifications).

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn overlays() -> Element {
    screen("Overlays & Feedback")
        .description("Floating, layered UI: hover tooltips in the passive layer, click-triggered popovers in the overlay layer, edge-anchored sheets/drawers, stacked toast notifications, and the GLOBAL right-click menu.")
        .body(
        children![global_menu(), tooltips(), popovers(), sheets(), toasts()],
    )
}

fn global_menu() -> impl IntoWidget {
    let enabled = create_signal(true);
    doc("Global right-click")
        .description("Right-click ANYWHERE with no widget claiming it and the standard menu opens — Cut / Copy / Paste / Select All, disabled when no editor holds focus. Disable it app-wide, replace its options, restyle it, or suppress it per area.")
        .body(
        column(children![
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
            gap_h(12.0),
            Container::new()
                .height(120.0)
                .decoration(
                    BoxDecoration::new()
                        .color(theme().colors.secondary)
                        .radius(BorderRadius::all(theme().radius)),
                )
                .alignment(Alignment::CENTER)
                .child(block_context_menu(muted("Right-click here is suppressed (block_context_menu)"))),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn sheets() -> impl IntoWidget {
    doc("Sheet / Drawer")
        .description("An edge-anchored modal panel over a dimmed scrim — Right/Left for a full-height sheet, Bottom for a drawer. Escape or an outside click dismisses.")
        .body(
        row(children![
            button("Open right sheet").variant(ButtonVariant::Outline).on_pressed(|| {
                sheet(
                    column(children![
                        muted("Filter the results by the fields below."),
                        gap_h(14.0),
                        text_field().placeholder("Search").width(280.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                )
                .side(Side::Right)
                .size(340.0)
                .title("Filters")
                .open();
            }),
            gap_w(10.0),
            button("Open bottom drawer").variant(ButtonVariant::Outline).on_pressed(|| {
                sheet(muted("A drawer slides up from the bottom edge."))
                    .side(Side::Bottom)
                    .size(220.0)
                    .title("Details")
                    .open();
            }),
        ])
        .main_axis_size(MainAxisSize::Min),
    )
}

fn tooltips() -> impl IntoWidget {
    doc("Tooltip")
        .description("Hover a trigger; after a short delay a hint appears near the pointer and follows hover-exit to dismiss. Never blocks clicks.")
        .body(
        row(children![
            tooltip("Saved to disk", button("Hover me").variant(ButtonVariant::Outline)),
            gap_w(12.0),
            tooltip("More information", icon_button(IconKind::Info)).delay(0.3),
            gap_w(12.0),
            tooltip("Not yet stable", badge("Beta").variant(BadgeVariant::Secondary)),
        ])
        .main_axis_size(MainAxisSize::Min),
    )
}

fn popovers() -> impl IntoWidget {
    doc("Popover")
        .description("Click a trigger to float arbitrary content in the overlay layer — it flips near edges, follows page scroll, and hosts real inputs. Click outside to dismiss.")
        .body(
        row(children![
            popover(
                column(children![
                    text("Dimensions").size(14.0).semibold(),
                    gap_h(8.0),
                    muted("Set the width and height of the panel."),
                    gap_h(12.0),
                    text_field().placeholder("Width").width(200.0),
                    gap_h(8.0),
                    text_field().placeholder("Height").width(200.0),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
                button("Open popover").variant(ButtonVariant::Outline),
            )
            .width(232.0)
            .height(200.0)
            .trigger_height(38.0),
        ])
        .main_axis_size(MainAxisSize::Min),
    )
}

fn toasts() -> impl IntoWidget {
    doc("Toast")
        .description("Transient notifications stacked bottom-right, auto-dismissed after a few seconds (or manually). Variants carry an icon + accent; an optional action button.")
        .body(
        row(children![
            button("Default").variant(ButtonVariant::Outline).on_pressed(|| {
                toast("Event created").description("Fri, Jan 3 at 5:00 PM").show();
            }),
            gap_w(10.0),
            button("Success").variant(ButtonVariant::Outline).on_pressed(|| {
                toast("Saved").description("Your changes are live.").variant(ToastVariant::Success).show();
            }),
            gap_w(10.0),
            button("Warning").variant(ButtonVariant::Outline).on_pressed(|| {
                toast("Heads up").variant(ToastVariant::Warning).show();
            }),
            gap_w(10.0),
            button("With action").on_pressed(|| {
                toast("Message archived").action("Undo", || {}).duration(6.0).show();
            }),
        ])
        .main_axis_size(MainAxisSize::Min),
    )
}
