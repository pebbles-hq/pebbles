use pebbles::prelude::*;

use crate::ui::{doc, screen};

const IMG: &str = "https://github.com/shadcn.png";

pub fn avatars() -> Element {
    screen("Avatar")
        .description("An image with an initials fallback (shadcn's Avatar), plus sizes, shapes, a status dot, an overlapping group, and a hover card.")
        .body(
        children![default(), fallback(), sizes(), shapes(), colors(), status_dots(), group(), hover_card_demo()],
    )
}

fn default() -> impl IntoWidget {
    doc("Image with fallback")
        .description("Give .src(url) an image; the initials show while it loads and if it fails. Loads on a background thread.")
        .body(
        wrap(children![
            avatar("CN").src(IMG),
            avatar("PB").src("https://example.invalid/missing.png"),
        ])
        .spacing(14.0),
    )
}

fn fallback() -> impl IntoWidget {
    doc("Initials fallback")
        .description("With no image, an avatar renders initials on a colored background.")
        .body(
            wrap(children![
                avatar("RS"),
                avatar("AK"),
                avatar("JB"),
                avatar("CV")
            ])
            .spacing(14.0),
        )
}

fn sizes() -> impl IntoWidget {
    doc("Sizes")
        .description("Scale with .size(); the initials scale with it.")
        .body(
            wrap(children![
                avatar("SM").size(28.0),
                avatar("MD").size(40.0),
                avatar("LG").size(56.0),
                avatar("XL").size(72.0),
            ])
            .spacing(14.0),
        )
}

fn shapes() -> impl IntoWidget {
    doc("Shapes")
        .description("Circle (default), rounded-square, or square via .shape().")
        .body(
            wrap(children![
                avatar("CI").shape(AvatarShape::Circle),
                avatar("RO").shape(AvatarShape::Rounded),
                avatar("SQ").shape(AvatarShape::Square),
                avatar("CN").src(IMG).shape(AvatarShape::Rounded),
            ])
            .spacing(14.0),
        )
}

fn colors() -> impl IntoWidget {
    doc("Colors")
        .description("Tint the fallback background with .color().")
        .body(
            wrap(children![
                avatar("EM").color(palette::emerald::S600),
                avatar("BL").color(palette::blue::S600),
                avatar("RO").color(palette::rose::S600),
                avatar("VI").color(palette::violet::S600),
                avatar("AM").color(palette::amber::S500),
            ])
            .spacing(14.0),
        )
}

fn status_dots() -> impl IntoWidget {
    doc("Status")
        .description(
            "A small dot at the bottom-right via .status(color) — online, away, busy, offline.",
        )
        .body(
            wrap(children![
                avatar("ON").status(palette::emerald::S500),
                avatar("AW").status(palette::amber::S500),
                avatar("BU").status(palette::rose::S500),
                avatar("OF").status(palette::zinc::S400),
                avatar("CN").src(IMG).status(palette::emerald::S500),
            ])
            .spacing(16.0),
        )
}

fn hover_card_demo() -> impl IntoWidget {
    doc("Hover card")
        .description("Hover the trigger: a rich card appears after a short delay and stays open while the pointer moves onto it (classic @user preview).")
        .body(
        row(children![
            hover_card(
                column(children![
                    row(children![
                        avatar("RS"),
                        gap_w(12.0),
                        column(children![
                            text("Reyco Seguma").size(14.0).semibold(),
                            gap_h(2.0),
                            muted("@xreyc"),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(10.0),
                    text("Building Pebbles — a Flutter-style GUI framework.").size(13.0),
                    gap_h(10.0),
                    button("Follow").size(ButtonSize::Sm).variant(ButtonVariant::Secondary),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
                avatar("RS"),
            )
            .width(300.0)
            .delay(0.3),
            gap_w(20.0),
            hover_card(text("Andres — Engineer").size(13.0), avatar("AK")),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
}

fn group() -> impl IntoWidget {
    doc("Group")
        .description("Overlap several avatars and cap the rest into a “+N” bubble with avatar_group(...).max(n).")
        .body(
        column(children![
            avatar_group(vec![
                avatar("RS").color(palette::emerald::S600),
                avatar("AK").color(palette::blue::S600),
                avatar("JB").color(palette::rose::S600),
                avatar("CN").src(IMG),
            ]),
            gap_h(18.0),
            avatar_group(vec![
                avatar("RS").color(palette::emerald::S600),
                avatar("AK").color(palette::blue::S600),
                avatar("JB").color(palette::rose::S600),
                avatar("CV").color(palette::violet::S600),
                avatar("MK").color(palette::amber::S500),
                avatar("TL").color(palette::teal::S600),
            ])
            .max(3),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}
