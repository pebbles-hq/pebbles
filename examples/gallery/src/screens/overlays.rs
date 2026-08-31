//! Overlays & feedback: Tooltip (passive hover hint), Popover (click panel), and
//! Toast (transient notifications).

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn overlays() -> Element {
    screen(
        "Overlays & Feedback",
        "Floating, layered UI: hover tooltips in the passive layer, click-triggered popovers in the overlay layer, edge-anchored sheets/drawers, and stacked toast notifications.",
        children![tooltips(), popovers(), sheets(), toasts()],
    )
}

fn sheets() -> impl IntoWidget {
    doc(
        "Sheet / Drawer",
        "An edge-anchored modal panel over a dimmed scrim — Right/Left for a full-height sheet, Bottom for a drawer. Escape or an outside click dismisses.",
        row(children![
            button("Open right sheet").variant(ButtonVariant::Outline).on_pressed(|| {
                sheet(
                    column(children![
                        muted("Filter the results by the fields below."),
                        gap_h(14.0),
                        text_field().placeholder("Search").width(280.0),
                    ])
                    .start()
                    .min(),
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
        .min(),
    )
}

fn tooltips() -> impl IntoWidget {
    doc(
        "Tooltip",
        "Hover a trigger; after a short delay a hint appears near the pointer and follows hover-exit to dismiss. Never blocks clicks.",
        row(children![
            tooltip(button("Hover me").variant(ButtonVariant::Outline), "Saved to disk"),
            gap_w(12.0),
            tooltip(icon_button(IconKind::Info), "More information").delay(0.3),
            gap_w(12.0),
            tooltip(badge("Beta").variant(BadgeVariant::Secondary), "Not yet stable"),
        ])
        .min(),
    )
}

fn popovers() -> impl IntoWidget {
    doc(
        "Popover",
        "Click a trigger to float arbitrary content in the overlay layer — it flips near edges, follows page scroll, and hosts real inputs. Click outside to dismiss.",
        row(children![
            popover(
                button("Open popover").variant(ButtonVariant::Outline),
                column(children![
                    text("Dimensions").size(14.0).semibold(),
                    gap_h(8.0),
                    muted("Set the width and height of the panel."),
                    gap_h(12.0),
                    text_field().placeholder("Width").width(200.0),
                    gap_h(8.0),
                    text_field().placeholder("Height").width(200.0),
                ])
                .start()
                .min(),
            )
            .width(232.0)
            .height(200.0)
            .trigger_height(38.0),
        ])
        .min(),
    )
}

fn gap_h(n: f64) -> impl IntoWidget {
    SizedBox::spacer(0.0, n)
}

fn toasts() -> impl IntoWidget {
    doc(
        "Toast",
        "Transient notifications stacked bottom-right, auto-dismissed after a few seconds (or manually). Variants carry an icon + accent; an optional action button.",
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
        .min(),
    )
}
