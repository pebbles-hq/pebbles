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
    screen("Media Query")
        .description("Unified window metrics — size, orientation, safe-area padding, keyboard insets, device pixel ratio, text scale. Flutter's MediaQuery. On desktop padding/insets are zero and dpr/textScale are 1.0 (the mobile shell fills the real values).")
        .body(children![
            doc("media_query()")
                .description("Read the current window's metrics anywhere.")
                .body(column(children![
                    muted(&format!("size: {:.0} × {:.0}", m.size.width, m.size.height)),
                    gap_h(6.0),
                    muted(&format!("orientation: {:?}", m.orientation)),
                    gap_h(6.0),
                    muted(&format!("safe-area padding: {:?}", m.padding)),
                    gap_h(6.0),
                    muted(&format!("keyboard view-insets: {:?}", m.view_insets)),
                    gap_h(6.0),
                    muted(&format!("device pixel ratio: {}   ·   text scale: {}", m.device_pixel_ratio, m.text_scale)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// SafeArea
// ===========================================================================

pub fn safe_area_screen() -> Element {
    screen("Safe Area")
        .description("Insets a child past the notch / status bar / home indicator by the MediaQuery padding. Flutter's SafeArea. A no-op on desktop/web (zero insets) — it becomes real with the mobile shell.")
        .body(children![
            doc("safe_area(child)")
                .description("On desktop this adds no inset (padding is zero), so the box below fills its frame edge-to-edge. Toggle edges with .top()/.bottom()/.left()/.right().")
                .body(framed(
                    280.0,
                    140.0,
                    safe_area(Container::new().color(theme().colors.primary).radius(BorderRadius::all(6.0))),
                ))
        ])
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

    screen("Orientation Builder")
        .description("Rebuilds with the orientation of the space it's given — Portrait when taller than wide, else Landscape. Flutter's OrientationBuilder. Reactive to resize (reads its allotted bounds).")
        .body(children![
            doc("orientation_builder(|orientation| ..)")
                .description("The same builder in a wide box and a tall box reports different orientations.")
                .body(row(children![cell(260.0, 150.0), gap_w(20.0), cell(150.0, 260.0)])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Scaffold slots
// ===========================================================================

pub fn scaffold_slots_screen() -> Element {
    let taps = create_signal(0i64);
    screen("Scaffold Slots")
        .description("Beyond top/side/body/bottom, the Scaffold has a .fab() slot (bottom-right FAB overlay) and a .persistent_footer() slot (a pinned action row above the bottom bar, with a top divider). Flutter's floatingActionButton + persistentFooterButtons.")
        .body(children![
            doc("scaffold(body).fab(..).persistent_footer(..)")
                .description("A mini shell: a body, a persistent footer of actions, and a floating action button.")
                .body(framed(
                    380.0,
                    240.0,
                    scaffold(center(muted("body content")))
                        .persistent_footer(
                            row(children![
                                spacer(),
                                button("Cancel").variant(ButtonVariant::Ghost),
                                gap_w(8.0),
                                button("Save").on_pressed(move || taps.update(|n| *n += 1)),
                            ]),
                        )
                        .fab(fab(lucide::PLUS).mini(true).on_pressed(move || taps.update(|n| *n += 1))),
                )),
            doc("drawer / end_drawer")
                .description("A side drawer is the existing sheet(Side::Left / Side::Right) — a modal side panel. (Wiring it as a dedicated Scaffold slot with an auto-hamburger is a follow-up.)")
                .body(muted(&format!("footer/fab presses: {}", taps.get()))),
        ])
}
