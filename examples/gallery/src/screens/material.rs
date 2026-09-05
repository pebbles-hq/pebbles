//! Material staples with no prior Pebbles equivalent — one screen per widget:
//! `FloatingActionButton` (`fab`) and `GridTile` (+ `GridTileBar`). Both are built in
//! the shadcn/desktop style, not the Material look.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

// ===========================================================================
// FloatingActionButton
// ===========================================================================

pub fn fab_screen() -> Element {
    let taps = create_signal(0i64);

    screen("Floating Action Button")
        .description("A circular, elevated action button. Flutter's FloatingActionButton. Its canonical home is the Scaffold's .fab() slot (bottom-right); it also works standalone in any Stack.")
        .body(children![
            doc("fab(icon).on_pressed(..)")
                .description("Standard (56px), mini (40px), and extended (icon + label) forms. Each press bumps the counter.")
                .body(column(children![
                    row(children![
                        fab(lucide::PLUS).on_pressed(move || taps.update(|n| *n += 1)),
                        gap_w(24.0),
                        fab(IconKind::Search).mini(true).on_pressed(move || taps.update(|n| *n += 1)),
                        gap_w(24.0),
                        fab(IconKind::Mail).label("Compose").on_pressed(move || taps.update(|n| *n += 1)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(18.0),
                    text(&format!("Presses: {}", taps.get()))
                        .size(13.0)
                        .color(theme().colors.muted_foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)),
            doc("scaffold(body).fab(fab(..))")
                .description("The Scaffold overlays a fab at the bottom-right of its body — the standard Material placement, added as a slot.")
                .body(
                    Container::new()
                        .width(360.0)
                        .height(200.0)
                        .decoration(
                            BoxDecoration::new()
                                .radius(BorderRadius::all(12.0))
                                .border(Border::all(BorderSide::new(theme().colors.border, 1.0))),
                        )
                        .child(scaffold(center(text("body").color(theme().colors.muted_foreground)))
                            .fab(fab(lucide::PLUS).on_pressed(move || taps.update(|n| *n += 1)))),
                )
        ])
}

// ===========================================================================
// GridTile
// ===========================================================================

fn photo(color: Color, w: f64, h: f64) -> impl IntoWidget {
    Container::new().width(w).height(h).decoration(BoxDecoration::new().gradient(Gradient::linear(
        Alignment::TOP_LEFT,
        Alignment::BOTTOM_RIGHT,
        [color, theme().colors.muted],
    )))
}

pub fn grid_tile_screen() -> Element {
    let tiles = [
        ("Aurora", "Iceland", palette::violet::S500),
        ("Dunes", "Namibia", palette::amber::S500),
        ("Reef", "Palau", palette::teal::S500),
        ("Canyon", "Arizona", palette::red::S500),
    ];

    screen("Grid Tile")
        .description("A grid cell with a caption bar overlaid on its top and/or bottom edge. Flutter's GridTile + GridTileBar. The classic image-grid tile.")
        .body(children![
            doc("grid_tile(child).footer(grid_tile_bar(title).subtitle(..))")
                .description("Photo tiles with a translucent footer bar (title + subtitle + a trailing action). The first also has a header.")
                .body(wrap(tiles
                    .iter()
                    .enumerate()
                    .map(|(i, (title, place, color))| {
                        let mut tile = grid_tile(clip_rrect(BorderRadius::all(12.0), photo(*color, 190.0, 150.0)))
                            .footer(grid_tile_bar(*title).subtitle(*place).trailing(
                                icon(IconKind::Info).size(16.0).color(palette::WHITE),
                            ));
                        if i == 0 {
                            tile = tile.header(grid_tile_bar("Featured").leading_icon(IconKind::Check));
                        }
                        clip_rrect(BorderRadius::all(12.0), tile).into_widget()
                    })
                    .collect::<Vec<_>>())
                .spacing(14.0)
                .run_spacing(14.0))
        ])
}
