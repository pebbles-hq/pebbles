//! The storefront sections — a hero carousel over imagery, shop-by-category tiles, a
//! featured-product grid, a lifestyle split, and a newsletter card on a gradient. Each
//! shows off image-heavy, layered, translucent rendering.

use pebbles::prelude::*;

use crate::data::{self, Product, img};
use crate::ui;

// ---------------------------------------------------------------------------
// Nav (over the hero imagery)
// ---------------------------------------------------------------------------

pub fn nav_bar() -> AnyWidget {
    let link = |s: &str| {
        text(s.to_string()).size(13.5).weight(500.0).color(ui::with_alpha(ui::white(), 0.86)).into_widget()
    };
    container()
        .padding(EdgeInsets::symmetric(30.0, 22.0))
        .child(
            row(children![
                row(children![
                    container()
                        .decoration(BoxDecoration::new().color(ui::white()).radius(BorderRadius::all(8.0)))
                        .padding(EdgeInsets::all(7.0))
                        .child(icon(lucide::LAYERS).size(17.0).color(ui::ink())),
                    gap_w(10.0),
                    text("PEBBLES ATELIER").size(15.0).weight(700.0).letter_spacing(1.0).color(ui::white()),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
                spacer(),
                row(children![
                    link("New In"),
                    gap_w(26.0),
                    link("Women"),
                    gap_w(26.0),
                    link("Men"),
                    gap_w(26.0),
                    link("Collections"),
                    gap_w(26.0),
                    link("Journal"),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
                spacer(),
                row(children![
                    icon(lucide::SEARCH).size(18.0).color(ui::white()),
                    gap_w(18.0),
                    icon(lucide::USER).size(18.0).color(ui::white()),
                    gap_w(16.0),
                    ui::glass(
                        999.0,
                        container().padding(EdgeInsets::symmetric(14.0, 8.0)).child(
                            row(children![
                                icon(lucide::SHOPPING_BAG).size(15.0).color(ui::white()),
                                gap_w(8.0),
                                text("Bag · 2").size(13.0).weight(600.0).color(ui::white()),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .main_axis_size(MainAxisSize::Min),
                        ),
                    ),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .into_widget()
}

// ---------------------------------------------------------------------------
// Hero — a sliding carousel of featured banners, nav overlaid
// ---------------------------------------------------------------------------

pub fn hero() -> AnyWidget {
    let pages: Vec<AnyWidget> =
        data::FEATURES.iter().map(|&(eb, hl, seed)| hero_slide(eb, hl, seed)).collect();
    container()
        .height(660.0)
        .child(stack(children![
            carousel(pages).height(660.0).autoplay(6.0).indicator(true).arrows(false),
            positioned(nav_bar()).left(0.0).right(0.0).top(0.0),
        ]))
        .into_widget()
}

fn hero_slide(eyebrow: &str, headline: &str, seed: &str) -> AnyWidget {
    stack(children![
        Positioned::fill(ui::image_fill(img(seed, 1680, 900))),
        Positioned::fill(ui::scrim(ui::with_alpha(ui::ink(), 0.18), ui::with_alpha(ui::ink(), 0.66))),
        Positioned::fill(center(
            container().width(780.0).child(
                column(children![
                    ui::eyebrow(eyebrow, ui::with_alpha(ui::white(), 0.9)),
                    gap_h(18.0),
                    text(headline.to_string())
                        .size(54.0)
                        .weight(800.0)
                        .line_height(1.06)
                        .align(TextAlign::Center)
                        .color(ui::white()),
                    gap_h(20.0),
                    container().width(540.0).child(
                        text(
                            "An image-heavy storefront drawn entirely in Pebbles — proof the \
                             framework carries real photography, layering and glass, not just forms.",
                        )
                        .size(16.0)
                        .line_height(1.5)
                        .align(TextAlign::Center)
                        .color(ui::with_alpha(ui::white(), 0.82)),
                    ),
                    gap_h(30.0),
                    row(children![
                        button("Shop the collection")
                            .size(ButtonSize::Lg)
                            .color(ui::white())
                            .text_color(ui::ink())
                            .trailing(lucide::ARROW_RIGHT),
                        gap_w(12.0),
                        ui::glass_button("Lookbook"),
                    ])
                    .main_axis_alignment(MainAxisAlignment::Center)
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
            ),
        )),
    ])
    .into_widget()
}

// ---------------------------------------------------------------------------
// Shop by category — image tiles with a bottom scrim
// ---------------------------------------------------------------------------

pub fn categories() -> AnyWidget {
    let mut tiles: Vec<AnyWidget> = Vec::new();
    for (i, &(label, count, seed)) in data::CATEGORIES.iter().enumerate() {
        if i > 0 {
            tiles.push(gap_w(18.0).into_widget());
        }
        tiles.push(Expanded::new(category_tile(label, count, seed)).into_widget());
    }
    ui::section(
        ui::paper(),
        66.0,
        column(children![
            section_head("Shop by category", "Find your silhouette", ui::ink()),
            gap_h(28.0),
            row(tiles).cross_axis_alignment(CrossAxisAlignment::Stretch),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn category_tile(label: &str, count: &str, seed: &str) -> AnyWidget {
    container()
        .decoration(BoxDecoration::new().radius(BorderRadius::all(16.0)))
        .clip()
        .height(340.0)
        .child(stack(children![
            Positioned::fill(ui::image_fill(img(seed, 520, 680))),
            Positioned::fill(ui::scrim(ui::with_alpha(ui::ink(), 0.0), ui::with_alpha(ui::ink(), 0.78))),
            positioned(
                column(children![
                    text(label.to_string()).size(21.0).weight(700.0).color(ui::white()),
                    gap_h(3.0),
                    text(count.to_string()).size(12.5).color(ui::with_alpha(ui::white(), 0.82)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            )
            .left(18.0)
            .right(18.0)
            .bottom(18.0),
        ]))
        .into_widget()
}

// ---------------------------------------------------------------------------
// Featured products — a wrapped grid of image cards
// ---------------------------------------------------------------------------

pub fn products() -> AnyWidget {
    // A responsive grid: 4 columns on desktop, 3 on tablet, 2 on mobile. Cards are
    // `Expanded` so each row fills the width exactly (no ragged right edge).
    let cols = breakpoint().select(2usize, 3, 4);
    let n = data::PRODUCTS.len();

    let mut body: Vec<AnyWidget> = vec![
        row(children![
            Expanded::new(section_head("Featured pieces", "The atelier edit", ui::ink())),
            gap_w(10.0),
            text("View all  →").size(14.0).weight(600.0).color(ui::accent()),
        ])
        .cross_axis_alignment(CrossAxisAlignment::End)
        .into_widget(),
        gap_h(28.0).into_widget(),
    ];

    let mut i = 0;
    while i < n {
        let mut cells: Vec<AnyWidget> = Vec::new();
        for j in 0..cols {
            if j > 0 {
                cells.push(gap_w(22.0).into_widget());
            }
            if i + j < n {
                cells.push(Expanded::new(product_card(&data::PRODUCTS[i + j])).into_widget());
            } else {
                // Empty flex cell keeps the last row's cards the same width as full rows.
                cells.push(Expanded::new(gap_h(0.0)).into_widget());
            }
        }
        body.push(row(cells).cross_axis_alignment(CrossAxisAlignment::Start).into_widget());
        i += cols;
        if i < n {
            body.push(gap_h(30.0).into_widget());
        }
    }

    ui::section(
        ui::paper_dim(),
        66.0,
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
}

fn product_card(p: &Product) -> AnyWidget {
    let tag: AnyWidget = match p.tag {
        Some(t) => positioned(ui::pill(t, ui::white(), ui::ink())).left(12.0).top(12.0).into_widget(),
        None => gap_h(0.0).into_widget(),
    };
    let add = positioned(
        pressable(ui::glass(
            999.0,
            container()
                .width(38.0)
                .height(38.0)
                .alignment(Alignment::CENTER)
                .child(icon(lucide::PLUS).size(18.0).color(ui::white())),
        ))
        .radius(999.0),
    )
    .right(12.0)
    .bottom(12.0);

    let media = container()
        .decoration(BoxDecoration::new().radius(BorderRadius::all(14.0)))
        .clip()
        .height(330.0)
        .child(stack(children![Positioned::fill(ui::image_fill(img(p.seed, 520, 680))), tag, add,]));

    // No fixed width — the grid's `Expanded` cell sizes each card to fill its column.
    column(children![
        media,
        gap_h(14.0),
        row(children![
            Expanded::new(
                column(children![
                    text(p.name.to_string()).size(15.0).semibold().max_lines(1).ellipsis().color(ui::ink()),
                    gap_h(3.0),
                    text(p.category.to_string()).size(12.5).color(ui::ink_muted()),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
            gap_w(8.0),
            text(p.price.to_string()).size(15.0).weight(700.0).color(ui::ink()),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min)
    .into_widget()
}

// ---------------------------------------------------------------------------
// Lifestyle split — a full portrait image beside a copy block (dark section)
// ---------------------------------------------------------------------------

pub fn lifestyle() -> AnyWidget {
    ui::section(
        ui::ink(),
        76.0,
        row(children![
            Expanded::new(
                container()
                    .decoration(BoxDecoration::new().radius(BorderRadius::all(18.0)))
                    .clip()
                    .height(540.0)
                    .child(ui::image_fill(img("peb-model", 900, 1100))),
            ),
            gap_w(56.0),
            Expanded::new(
                column(children![
                    ui::eyebrow("The Winter Atelier", ui::accent()),
                    gap_h(16.0),
                    text("Made to be worn, drawn to be smooth")
                        .size(38.0)
                        .weight(800.0)
                        .line_height(1.1)
                        .color(ui::white()),
                    gap_h(18.0),
                    text(
                        "Every frame here — the photography, the parallax layering, the translucent \
                         chrome — is composed by Pebbles and rendered on the GPU with Vello. The same \
                         widgets that build a data table build this.",
                    )
                    .size(15.5)
                    .line_height(1.6)
                    .color(ui::with_alpha(ui::white(), 0.8)),
                    gap_h(26.0),
                    row(children![
                        stat("40k+", "photos / sec"),
                        gap_w(40.0),
                        stat("120", "fps, GPU"),
                        gap_w(40.0),
                        stat("1", "codebase"),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(30.0),
                    row(children![
                        button("Explore the edit")
                            .size(ButtonSize::Lg)
                            .color(ui::white())
                            .text_color(ui::ink())
                            .trailing(lucide::ARROW_RIGHT),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
}

fn stat(value: &str, label: &str) -> AnyWidget {
    column(children![
        text(value.to_string()).size(26.0).weight(800.0).color(ui::white()),
        gap_h(2.0),
        text(label.to_string()).size(12.5).color(ui::with_alpha(ui::white(), 0.7)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
    .into_widget()
}

// ---------------------------------------------------------------------------
// Newsletter — a glass card on a gradient
// ---------------------------------------------------------------------------

pub fn newsletter() -> AnyWidget {
    let email = create_signal(String::new());
    container()
        .decoration(BoxDecoration::new().gradient(Gradient::linear(
            Alignment::TOP_LEFT,
            Alignment::BOTTOM_RIGHT,
            [ui::ink(), ui::with_alpha(ui::accent(), 0.9)],
        )))
        .padding(EdgeInsets::symmetric(0.0, 82.0))
        .child(center(ui::glass(
            24.0,
            container().width(640.0).padding(EdgeInsets::all(40.0)).child(
                column(children![
                    ui::eyebrow("Members only", ui::with_alpha(ui::white(), 0.9)),
                    gap_h(14.0),
                    text("Join the atelier list")
                        .size(30.0)
                        .weight(800.0)
                        .align(TextAlign::Center)
                        .color(ui::white()),
                    gap_h(10.0),
                    text("Early access to drops, and the occasional note on how this page is built.")
                        .size(14.5)
                        .line_height(1.5)
                        .align(TextAlign::Center)
                        .color(ui::with_alpha(ui::white(), 0.82)),
                    gap_h(24.0),
                    row(children![
                        Expanded::new(text_field().placeholder("Email address").bind(email)),
                        gap_w(12.0),
                        button("Subscribe").size(ButtonSize::Lg).color(ui::white()).text_color(ui::ink()),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
        )))
        .into_widget()
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

pub fn footer() -> AnyWidget {
    ui::section(
        ui::ink(),
        56.0,
        column(children![
            row(children![
                Expanded::new(
                    column(children![
                        text("PEBBLES ATELIER")
                            .size(16.0)
                            .weight(700.0)
                            .letter_spacing(1.0)
                            .color(ui::white()),
                        gap_h(10.0),
                        container().width(240.0).child(
                            text("A demo storefront rendered on the GPU with Pebbles + Vello.")
                                .size(13.0)
                                .line_height(1.5)
                                .color(ui::with_alpha(ui::white(), 0.65)),
                        ),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
                footer_col("Shop", &["New in", "Women", "Men", "Sale"]),
                gap_w(56.0),
                footer_col("Company", &["About", "Journal", "Careers", "Stores"]),
                gap_w(56.0),
                footer_col("Support", &["Help", "Shipping", "Returns", "Contact"]),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start),
            gap_h(34.0),
            container().color(ui::with_alpha(ui::white(), 0.12)).height(1.0),
            gap_h(20.0),
            row(children![
                text("© 2026 Pebbles Atelier").size(12.5).color(ui::with_alpha(ui::white(), 0.6)),
                spacer(),
                text("Built with Pebbles · GPU-rendered · Apache-2.0")
                    .size(12.5)
                    .color(ui::with_alpha(ui::white(), 0.6)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn footer_col(title: &str, links: &[&str]) -> AnyWidget {
    let mut kids: Vec<AnyWidget> = vec![
        text(title.to_string()).size(13.0).weight(700.0).color(ui::white()).into_widget(),
        gap_h(12.0).into_widget(),
    ];
    for l in links {
        kids.push(text(l.to_string()).size(13.0).color(ui::with_alpha(ui::white(), 0.65)).into_widget());
        kids.push(gap_h(9.0).into_widget());
    }
    column(kids)
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn section_head(title: &str, eyebrow: &str, color: Color) -> AnyWidget {
    column(children![
        ui::eyebrow(eyebrow, ui::accent()),
        gap_h(8.0),
        text(title.to_string()).size(31.0).weight(700.0).color(color),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
    .into_widget()
}
