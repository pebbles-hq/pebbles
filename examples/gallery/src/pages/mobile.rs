//! Scaffold slots + mobile-specific widgets: `media_query`, `SafeArea`,
//! `OrientationBuilder`, and the Scaffold `.persistent_footer()` slot. On desktop the
//! safe-area padding and keyboard insets are zero — those fill in with the mobile shell.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

fn framed(w: f64, h: f64, child: impl IntoWidget) -> impl IntoWidget {
    Container::new()
        .width(w)
        .height(h)
        .decoration(
            BoxDecoration::new()
                .color(theme().colors.muted)
                .radius(BorderRadius::all(10.0))
                .border(Border::all(BorderSide::new(theme().colors.border, 1.0))),
        )
        .child(child)
}

fn muted(s: &str) -> impl IntoWidget {
    text(s).size(13.0).color(theme().colors.muted_foreground)
}

// ===========================================================================
// MediaQuery
// ===========================================================================

pub fn media_query_screen() -> Element {
    let m = media_query();
    screen("Media Query").body(children![
        doc("media_query()").body(
            column(children![
                muted(&format!("size: {:.0} × {:.0}", m.size.width, m.size.height)),
                gap_h(6.0),
                muted(&format!("orientation: {:?}", m.orientation)),
                gap_h(6.0),
                muted(&format!("safe-area padding: {:?}", m.padding)),
                gap_h(6.0),
                muted(&format!("keyboard view-insets: {:?}", m.view_insets)),
                gap_h(6.0),
                muted(&format!(
                    "device pixel ratio: {}   ·   text scale: {}",
                    m.device_pixel_ratio, m.text_scale
                )),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
        )
    ])
}

// ===========================================================================
// SafeArea
// ===========================================================================

pub fn safe_area_screen() -> Element {
    screen("Safe Area").body(children![doc("safe_area(child)").body(framed(
        280.0,
        140.0,
        safe_area(Container::new().color(theme().colors.primary).radius(BorderRadius::all(6.0))),
    ))])
}

// ===========================================================================
// OrientationBuilder
// ===========================================================================

pub fn orientation_builder_screen() -> Element {
    let cell = |w: f64, h: f64| {
        framed(
            w,
            h,
            orientation_builder(|o| {
                let (label, color) = match o {
                    Orientation::Landscape => ("Landscape", theme().colors.primary),
                    Orientation::Portrait => ("Portrait", palette::violet::S500),
                };
                center(
                    Container::new()
                        .color(color)
                        .radius(BorderRadius::all(999.0))
                        .padding(EdgeInsets::symmetric(14.0, 6.0))
                        .child(text(label).color(palette::WHITE).size(13.0).semibold()),
                )
            }),
        )
    };

    screen("Orientation Builder").body(children![
        doc("orientation_builder(|orientation| ..)").body(
            row(children![cell(260.0, 150.0), gap_w(20.0), cell(150.0, 260.0)])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
        )
    ])
}

// ===========================================================================
// Scaffold slots
// ===========================================================================

pub fn scaffold_slots_screen() -> Element {
    let taps = create_signal(0i64);
    screen("Scaffold Slots").body(children![
        doc("scaffold(body).fab(..).persistent_footer(..)").body(framed(
            380.0,
            240.0,
            scaffold(center(muted("body content")))
                .persistent_footer(row(children![
                    spacer(),
                    button("Cancel").variant(ButtonVariant::Ghost),
                    gap_w(8.0),
                    button("Save").on_pressed(move || taps.update(|n| *n += 1)),
                ]),)
                .fab(fab(tabler::PLUS).mini(true).on_pressed(move || taps.update(|n| *n += 1))),
        )),
        doc("drawer / end_drawer").body(muted(&format!("footer/fab presses: {}", taps.get()))),
    ])
}
