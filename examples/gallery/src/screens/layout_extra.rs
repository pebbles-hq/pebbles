//! The long-tail layout widgets — one screen per widget: `IndexedStack`, `Offstage`,
//! `Visibility`, `Baseline`, `RotatedBox`, `UnconstrainedBox`, `SizedOverflowBox`,
//! `FractionalTranslation`, the layout `Table`, `CustomSingleChildLayout`,
//! `CustomMultiChildLayout`, and `LayoutBuilder`.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

/// A labelled demo tile at a fixed size.
fn tile(label: &str, color: Color, w: f64, h: f64) -> impl IntoWidget {
    Container::new()
        .width(w)
        .height(h)
        .color(color)
        .radius(BorderRadius::all(8.0))
        .child(center(text(label).color(palette::WHITE).size(13.0)))
}

fn frame(w: f64, h: f64, child: impl IntoWidget) -> impl IntoWidget {
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

// ===========================================================================
// IndexedStack
// ===========================================================================

pub fn indexed_stack_screen() -> Element {
    let index = create_signal(0usize);
    // Screen-scope counters prove each panel keeps its state while hidden.
    let counts = [create_signal(0i64), create_signal(0i64), create_signal(0i64)];
    let colors = [theme().colors.primary, palette::green::S500, palette::violet::S500];

    let panels: Vec<AnyWidget> = (0..3)
        .map(|i| {
            let c = counts[i];
            Container::new()
                .color(colors[i])
                .radius(BorderRadius::all(10.0))
                .padding(EdgeInsets::all(20.0))
                .child(center(
                    column(children![
                        text(&format!("Panel {}", i + 1)).color(palette::WHITE).size(15.0).semibold(),
                        gap_h(10.0),
                        button(&format!("count: {}", c.get())).on_pressed(move || c.update(|n| *n += 1)),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ))
                .into_widget()
        })
        .collect();

    screen("Indexed Stack")
        .description("Shows one child at a time but keeps ALL children mounted, so each keeps its state. Flutter's IndexedStack.")
        .body(children![
            doc("indexed_stack(index, children)")
                .description("Bump a panel's counter, switch away, switch back — the count is still there.")
                .body(column(children![
                    row(children![
                        button("Panel 1").on_pressed(move || index.set(0)),
                        gap_w(8.0),
                        button("Panel 2").on_pressed(move || index.set(1)),
                        gap_w(8.0),
                        button("Panel 3").on_pressed(move || index.set(2)),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(16.0),
                    frame(300.0, 150.0, indexed_stack(index.get(), panels)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Offstage
// ===========================================================================

pub fn offstage_screen() -> Element {
    let hidden = create_signal(false);
    screen("Offstage")
        .description("Removes a child from layout, paint, and hit-testing while keeping it mounted — it takes NO space when offstage. Flutter's Offstage.")
        .body(children![
            doc("offstage(offstage, child)")
                .description("Toggle it: the middle tile disappears and the row collapses around the gap.")
                .body(column(children![
                    button(if hidden.get() { "offstage: ON" } else { "offstage: OFF" })
                        .on_pressed(move || hidden.update(|v| *v = !*v)),
                    gap_h(16.0),
                    row(children![
                        tile("A", theme().colors.primary, 70.0, 70.0),
                        gap_w(8.0),
                        offstage(hidden.get(), tile("B", palette::pink::S500, 70.0, 70.0)),
                        gap_w(8.0),
                        tile("C", palette::green::S500, 70.0, 70.0),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Visibility
// ===========================================================================

pub fn visibility_screen() -> Element {
    let show = create_signal(true);
    screen("Visibility")
        .description("Show or hide a child, optionally keeping its space (maintain_size). Flutter's Visibility.")
        .body(children![
            doc("visibility(visible, child).maintain_size(true)")
                .description("With maintain_size, the hidden tile stays invisible but keeps its slot — the row doesn't collapse.")
                .body(column(children![
                    button(if show.get() { "visible: ON" } else { "visible: OFF" })
                        .on_pressed(move || show.update(|v| *v = !*v)),
                    gap_h(16.0),
                    row(children![
                        tile("A", theme().colors.primary, 70.0, 70.0),
                        gap_w(8.0),
                        visibility(show.get(), tile("B", palette::pink::S500, 70.0, 70.0)).maintain_size(true),
                        gap_w(8.0),
                        tile("C", palette::green::S500, 70.0, 70.0),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Baseline
// ===========================================================================

pub fn baseline_screen() -> Element {
    screen("Baseline")
        .description(
            "Positions a child so its text baseline sits a fixed distance below the top. Flutter's Baseline.",
        )
        .body(children![
            doc("baseline(distance, child)")
                .description("The big text's baseline is pinned 60px down, regardless of its own height.")
                .body(frame(
                    260.0,
                    100.0,
                    baseline(60.0, text("Aligned").size(34.0).color(theme().colors.foreground)),
                ))
        ])
}

// ===========================================================================
// RotatedBox
// ===========================================================================

pub fn rotated_box_screen() -> Element {
    let turns = create_signal(1i32);
    screen("Rotated Box")
        .description("Rotates a child by quarter turns AND rotates the layout box (unlike paint-only Transform). Flutter's RotatedBox.")
        .body(children![
            doc("rotated_box(quarter_turns, child)")
                .description("Each press adds a quarter turn; note the 120×40 tile's box swaps to 40×120 on odd turns.")
                .body(column(children![
                    button(&format!("turns: {} (+90°)", turns.get()))
                        .on_pressed(move || turns.update(|t| *t += 1)),
                    gap_h(16.0),
                    frame(
                        200.0,
                        200.0,
                        center(rotated_box(turns.get(), tile("120×40", theme().colors.primary, 120.0, 40.0))),
                    ),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// UnconstrainedBox
// ===========================================================================

pub fn unconstrained_box_screen() -> Element {
    screen("Unconstrained Box")
        .description("Lets a child size itself with no constraints, then shrinks to it (overflow clipped). Flutter's UnconstrainedBox.")
        .body(children![
            doc("unconstrained_box(child)")
                .description("A 260-wide child inside a 140-wide slot: unconstrained, it keeps its natural width (clipped to the slot).")
                .body(frame(
                    140.0,
                    80.0,
                    unconstrained_box(tile("260 wide", palette::violet::S500, 260.0, 50.0)),
                ))
        ])
}

// ===========================================================================
// SizedOverflowBox
// ===========================================================================

pub fn sized_overflow_box_screen() -> Element {
    screen("Sized Overflow Box")
        .description("Reports a fixed size but lays the child out loosely, so it may overflow. Flutter's SizedOverflowBox.")
        .body(children![
            doc("sized_overflow_box(w, h, child)")
                .description("The box claims 80×80 (dashed frame), but its 140×50 child overflows, centered.")
                .body(frame(
                    220.0,
                    160.0,
                    center(sized_overflow_box(80.0, 80.0, tile("140×50", palette::amber::S500, 140.0, 50.0))),
                ))
        ])
}

// ===========================================================================
// FractionalTranslation
// ===========================================================================

pub fn fractional_translation_screen() -> Element {
    let shifted = create_signal(false);
    screen("Fractional Translation")
        .description("Offsets a child by a fraction of its OWN size (paint/hit only; layout unchanged). Flutter's FractionalTranslation.")
        .body(children![
            doc("fractional_translation(dx, dy, child)")
                .description("Toggle to slide the tile by half its own width and height.")
                .body(column(children![
                    button("Toggle").on_pressed(move || shifted.update(|v| *v = !*v)),
                    gap_h(16.0),
                    frame(
                        220.0,
                        160.0,
                        fractional_translation(
                            if shifted.get() { 0.5 } else { 0.0 },
                            if shifted.get() { 0.5 } else { 0.0 },
                            tile("shift", theme().colors.primary, 90.0, 60.0),
                        ),
                    ),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Table (layout)
// ===========================================================================

pub fn table_screen() -> Element {
    let cell = |s: &str, c: Color| {
        Container::new()
            .color(c)
            .padding(EdgeInsets::all(10.0))
            .child(text(s).color(palette::WHITE).size(13.0))
            .into_widget()
    };
    let p = theme().colors.primary;
    let rows: Vec<Vec<AnyWidget>> = vec![
        vec![
            cell("Fixed 80", p),
            cell("Intrinsic", palette::green::S500),
            cell("Flex fills the rest", palette::violet::S500),
        ],
        vec![cell("a", p), cell("wider content", palette::green::S500), cell("b", palette::violet::S500)],
    ];

    screen("Table (layout)")
        .description("A column-negotiating grid: columns size by spec, rows take their tallest cell. Flutter's Table (the layout widget — distinct from the data table).")
        .body(children![
            doc("layout_table(rows).column_widths(..)")
                .description("Column 1 is Fixed(80), column 2 is Intrinsic (fits its widest cell), column 3 is Flex (takes the rest).")
                .body(frame(
                    420.0,
                    120.0,
                    Container::new().padding(EdgeInsets::all(8.0)).child(
                        layout_table(rows).column_widths(vec![
                            TableColumnWidth::Fixed(80.0),
                            TableColumnWidth::Intrinsic,
                            TableColumnWidth::Flex(1.0),
                        ]),
                    ),
                ))
        ])
}

// ===========================================================================
// CustomSingleChildLayout
// ===========================================================================

pub fn custom_single_child_layout_screen() -> Element {
    screen("Custom Single Child Layout")
        .description(
            "Lay out and position a single child with your own functions. Flutter's CustomSingleChildLayout.",
        )
        .body(children![
            doc("custom_single_child_layout(child).position(..)")
                .description("A delegate pins the tile to the bottom-right corner of the box.")
                .body(frame(
                    240.0,
                    160.0,
                    custom_single_child_layout(tile("BR", theme().colors.primary, 70.0, 40.0))
                        .size(|c| c.biggest())
                        .position(|self_size, child| {
                            Offset::new(self_size.width - child.width, self_size.height - child.height)
                        }),
                ))
        ])
}

// ===========================================================================
// CustomMultiChildLayout
// ===========================================================================

pub fn custom_multi_child_layout_screen() -> Element {
    let kids: Vec<AnyWidget> = (0..4)
        .map(|i| {
            let c =
                [theme().colors.primary, palette::green::S500, palette::violet::S500, palette::amber::S500]
                    [i];
            tile(&format!("{}", i + 1), c, 60.0, 40.0).into_widget()
        })
        .collect();

    screen("Custom Multi Child Layout")
        .description("Lay out and position many children by index with your own functions. Flutter's CustomMultiChildLayout.")
        .body(children![
            doc("custom_multi_child_layout(children).position(..)")
                .description("A delegate cascades the four tiles diagonally by index.")
                .body(frame(
                    280.0,
                    200.0,
                    custom_multi_child_layout(kids)
                        .size(|c| c.biggest())
                        .position(|i, _self, _child| Offset::new(i as f64 * 40.0, i as f64 * 40.0)),
                ))
        ])
}

// ===========================================================================
// LayoutBuilder
// ===========================================================================

pub fn layout_builder_screen() -> Element {
    screen("Layout Builder")
        .description("Builds against the available size, rebuilding when it changes. Flutter's LayoutBuilder. (In Pebbles this reads the previous frame's size, so it's one frame behind on resize.)")
        .body(children![
            doc("layout_builder(|size| ..)")
                .description("Resize the window: the builder reports the width it was given and switches its badge past 480px.")
                .body(layout_builder(|size| {
                    let wide = size.width > 480.0;
                    let c = if wide { palette::green::S500 } else { theme().colors.primary };
                    Container::new()
                        .height(70.0)
                        .decoration(BoxDecoration::new().color(c).radius(BorderRadius::all(10.0)))
                        .padding(EdgeInsets::all(16.0))
                        .child(center(
                            text(&format!(
                                "available width: {:.0}px  →  {}",
                                size.width,
                                if wide { "WIDE layout" } else { "narrow layout" }
                            ))
                            .color(palette::WHITE)
                            .size(14.0),
                        ))
                }))
        ])
}
