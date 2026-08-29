use pebbles::prelude::*;

use crate::ui::{gap_h, gap_w, hstack, screen, section, stat_card, vstack};

pub fn overview() -> impl IntoWidget {
    screen(
        "Welcome to Pebbles",
        "A Flutter-style, desktop-first GUI framework built on Vello.",
        children![
            section(
                "STATS — props demo (reusable parameterized component)",
                row(children![
                    stat_card("Widgets", "50+", IconKind::Star, palette::INDIGO),
                    gap_w(14.0),
                    stat_card("Screens", "9", IconKind::Menu, palette::TEAL),
                    gap_w(14.0),
                    stat_card("Tests", "green", IconKind::Check, palette::GREEN),
                ])
                .main_axis_min(),
            ),
            Card::new(vstack(
                children![
                    title("This app is built with Pebbles"),
                    body("The sidebar is a SideNav, the bar above a TopPanel, and this content a RouteView. Navigation is a global signal — click the menu."),
                    gap_h(6.0),
                    hstack(
                        children![
                            badge("Scaffold"),
                            badge("SideNav").variant(BadgeVariant::Secondary),
                            badge("RouteView").variant(BadgeVariant::Secondary),
                            badge("Signals").variant(BadgeVariant::Success),
                        ],
                        8.0,
                    ),
                ],
                10.0,
            )),
            gap_h(16.0),
            section(
                "STATE MODEL (SolidJS-style)",
                vstack(
                    children![
                        body("create_signal — local AND global state, one primitive."),
                        body("Function components — fn() -> impl IntoWidget; no structs, no traits."),
                        body("Plain-closure events — on_pressed(action(move || sig.set(x)))."),
                    ],
                    6.0,
                ),
            ),
        ],
    )
}
