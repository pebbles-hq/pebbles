//! The built-in color palette — the full Tailwind/shadcn scale, rendered as swatches
//! so every family (50–950) is visible at a glance. Devs reference these as
//! `palette::blue::S500`, or define their own `Color`s / themes.

use pebbles::prelude::*;

use crate::ui::{gap_h, screen, section};

/// One color chip with its shade number underneath.
fn swatch(color: Color, shade: &str) -> impl IntoWidget {
    column(children![
        Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(color)
                    .radius(BorderRadius::all(6.0))
                    .border(Border::new(theme().colors.border, 1.0)),
            )
            .child(SizedBox::spacer(46.0, 40.0)),
        gap_h(4.0),
        text(shade.to_string()).size(10.0).color(theme().colors.muted_foreground),
    ])
    .center_cross()
    .min()
}

/// One family: its name plus the 11 shades 50→950.
fn scale(name: &str, shades: [Color; 11]) -> impl IntoWidget {
    const LABELS: [&str; 11] =
        ["50", "100", "200", "300", "400", "500", "600", "700", "800", "900", "950"];
    let mut chips: Vec<AnyWidget> = Vec::new();
    for (i, c) in shades.into_iter().enumerate() {
        chips.push(swatch(c, LABELS[i]).into_widget());
        chips.push(SizedBox::spacer(6.0, 0.0).into_widget());
    }
    column(children![
        text(name.to_string()).size(12.0).semibold().color(theme().colors.foreground),
        gap_h(6.0),
        row(chips).min(),
        gap_h(14.0),
    ])
    .start()
    .min()
}

macro_rules! fam {
    ($m:ident) => {
        [
            palette::$m::S50,
            palette::$m::S100,
            palette::$m::S200,
            palette::$m::S300,
            palette::$m::S400,
            palette::$m::S500,
            palette::$m::S600,
            palette::$m::S700,
            palette::$m::S800,
            palette::$m::S900,
            palette::$m::S950,
        ]
    };
}

pub fn colors() -> Element {
    screen(
        "Colors",
        "The built-in palette is the full Tailwind/shadcn scale — 22 families × shades 50–950. Reference any as palette::<family>::S<shade>.",
        children![
            section(
                "NEUTRALS",
                column(children![
                    scale("slate", fam!(slate)),
                    scale("gray", fam!(gray)),
                    scale("zinc", fam!(zinc)),
                    scale("neutral", fam!(neutral)),
                    scale("stone", fam!(stone)),
                ])
                .start()
                .min(),
            ),
            section(
                "WARM",
                column(children![
                    scale("red", fam!(red)),
                    scale("orange", fam!(orange)),
                    scale("amber", fam!(amber)),
                    scale("yellow", fam!(yellow)),
                    scale("lime", fam!(lime)),
                ])
                .start()
                .min(),
            ),
            section(
                "GREENS & CYANS",
                column(children![
                    scale("green", fam!(green)),
                    scale("emerald", fam!(emerald)),
                    scale("teal", fam!(teal)),
                    scale("cyan", fam!(cyan)),
                    scale("sky", fam!(sky)),
                ])
                .start()
                .min(),
            ),
            section(
                "BLUES & PURPLES",
                column(children![
                    scale("blue", fam!(blue)),
                    scale("indigo", fam!(indigo)),
                    scale("violet", fam!(violet)),
                    scale("purple", fam!(purple)),
                    scale("fuchsia", fam!(fuchsia)),
                    scale("pink", fam!(pink)),
                    scale("rose", fam!(rose)),
                ])
                .start()
                .min(),
            ),
            section(
                "YOUR OWN COLORS — devs are never limited to the built-ins",
                column(children![
                    text("Any Color::from_rgba8(..) works, and a custom theme is a copied base with overridden fields:")
                        .size(13.0)
                        .color(theme().colors.muted_foreground),
                    gap_h(10.0),
                    row(children![
                        swatch(Color::from_rgba8(0xFF, 0x5A, 0x1F, 255), "brand"),
                        SizedBox::spacer(6.0, 0.0),
                        swatch(palette::violet::S600, "primary"),
                        SizedBox::spacer(6.0, 0.0),
                        swatch(palette::emerald::S500, "success"),
                    ])
                    .min(),
                ])
                .start()
                .min(),
            ),
        ],
    )
}
