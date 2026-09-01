//! Tooltip & HoverCard: the passive hover layer.

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn tooltips() -> Element {
    screen("Tooltip & Hover Card")
        .description(
            "Passive-layer hover hints that never block clicks: Tooltip for one-line labels, HoverCard for persistent rich cards. Both appear after a configurable delay and dismiss on hover-exit.",
        )
        .body(children![basic(), delayed_rich(), styled(), hover_cards()])
}

fn basic() -> impl IntoWidget {
    doc("Tooltip")
        .description("Hover any trigger — a button, an icon button, a badge, even plain text — and the label floats near the pointer.")
        .body(
            row(children![
                tooltip("Saved to disk", button("Hover me").variant(ButtonVariant::Outline)),
                gap_w(12.0),
                tooltip("More information", icon_button(IconKind::Info)),
                gap_w(12.0),
                tooltip("Not yet stable", badge("Beta").variant(BadgeVariant::Secondary)),
                gap_w(12.0),
                tooltip("Plain text triggers work too", text("hover this text").size(13.5)),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn delayed_rich() -> impl IntoWidget {
    doc("Delay & rich content")
        .description(".delay(secs) tunes how long the hover must hold (the icon button below waits 0.6s). .rich(..) replaces the label with arbitrary content.")
        .body(
            row(children![
                tooltip("Quick — default delay", button("Fast").variant(ButtonVariant::Outline)),
                gap_w(12.0),
                tooltip("Slow — 0.6s hold", button("Slow").variant(ButtonVariant::Outline)).delay(0.6),
                gap_w(12.0),
                tooltip("Plain", button("Rich").variant(ButtonVariant::Outline)).rich(
                    column(children![
                        text("Rich tooltip").semibold().size(13.0),
                        muted("Any widget tree, not just a label.").size(11.5),
                        gap_w(0.0),
                        badge("beta").variant(BadgeVariant::Secondary),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(6.0),
                ),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn styled() -> impl IntoWidget {
    doc("Styled")
        .description(".style(..) restyles the bubble — colors, border, radius — while keeping the hover behavior.")
        .body(
            row(children![
                tooltip("Accent styled", button("Accent").variant(ButtonVariant::Outline)).style(
                    style()
                        .background(theme().colors.primary)
                        .color(palette::WHITE)
                        .radius_all(10.0),
                ),
                gap_w(12.0),
                tooltip("Inverted card", icon_button(IconKind::Info)).style(
                    style()
                        .background(theme().colors.card)
                        .border(Border::new(theme().colors.border, 1.0))
                        .radius_all(6.0),
                ),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn hover_cards() -> impl IntoWidget {
    doc("HoverCard")
        .description("A delayed card that stays open while you hover its content — the pattern for profile previews, image previews, and context cards. .width(..) and .delay(..) tune it.")
        .body(
            row(children![
                hover_card(
                    column(children![
                        row(children![
                            Container::new()
                                .width(36.0)
                                .height(36.0)
                                .decoration(
                                    BoxDecoration::new()
                                        .color(theme().colors.primary)
                                        .shape(BoxShape::Circle),
                                )
                                .child(center(text("RK").color(palette::WHITE).semibold().size(12.0))),
                            gap_w(10.0),
                            column(children![
                                text("reyco").semibold().size(13.0),
                                muted("contributor · 12 badges").size(11.0),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_size(MainAxisSize::Min),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                        gap_w(0.0),
                        muted("Hover the card itself — it stays open while the pointer is inside.").size(11.5),
                        gap_w(0.0),
                        row(children![
                            button("Follow").size(ButtonSize::Sm).on_click(|| {}),
                            button("Message").size(ButtonSize::Sm).variant(ButtonVariant::Secondary).on_click(|| {}),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(6.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(6.0),
                    text("@reyco").size(13.5).color(theme().colors.primary),
                )
                .width(260.0)
                .delay(0.2),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}
