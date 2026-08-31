use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn list_view() -> Element {
    let list_ctrl = use_scroll_controller();

    screen("List View")
        .description("A virtualized, fixed-extent list — only the visible rows are built, so any count stays smooth. Flutter's ListView shape: .builder, .separated, .horizontal, scrollbars, and a ScrollController.")
        .body(children![
            doc("5,000 rows")
                .description("Only the visible rows exist. Smooth wheel, keyboard (PageDn/Home/End), the scrollbar, or a ScrollController (the buttons).")
                .body(
                    column(children![
                        row(children![
                            button("Top").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                                .on_pressed(move || list_ctrl.animate_to(0.0)),
                            gap_w(8.0),
                            button("Jump to #2500").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                                .on_pressed(move || list_ctrl.scroll_to_index(2500, 44.0)),
                            gap_w(8.0),
                            button("Bottom").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                                .on_pressed(move || list_ctrl.animate_to(5000.0 * 44.0)),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                        gap_h(10.0),
                        Container::new()
                            .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                            .height(280.0)
                            .child(ListView::builder(5000, 44.0, |i| {
                                let c = theme().colors;
                                Container::new()
                                    .height(44.0)
                                    .padding(EdgeInsets::symmetric(14.0, 0.0))
                                    .alignment(Alignment::CENTER_LEFT)
                                    .decoration(BoxDecoration::new().border(Border::new(c.border, 0.5)))
                                    .child(row(children![
                                        icon(IconKind::Dot).size(16.0).color(c.muted_foreground),
                                        gap_w(10.0),
                                        text(format!("Row {i}")).size(14.0).color(c.foreground),
                                        spacer(),
                                        muted(format!("#{i}")),
                                    ]))
                            })
                            .controller(list_ctrl)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Separated")
                .description("ListView::separated(..) — items AND separators virtualized with their own extents (Flutter's ListView.separated): great for settings rows with hairlines.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(240.0)
                        .child(ListView::separated(
                            30,
                            44.0,
                            1.0,
                            |i| {
                                Padding::new(
                                    EdgeInsets::symmetric(14.0, 0.0),
                                    row(children![
                                        icon(IconKind::Dot).size(16.0).color(theme().colors.muted_foreground),
                                        gap_w(10.0),
                                        text(format!("Setting {i}")).size(14.0),
                                    ])
                                    .main_axis_size(MainAxisSize::Min),
                                )
                            },
                            |_| Container::new().height(1.0).color(theme().colors.border),
                        )),
                ),
            doc("Horizontal")
                .description(".horizontal() flips the axis — a strip of chips, galleries, timelines.")
                .body(
                    Container::new()
                        .height(56.0)
                        .child(ListView::builder(12, 92.0, |i| {
                            Container::new()
                                .margin(EdgeInsets::symmetric(4.0, 0.0))
                                .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(999.0)))
                                .alignment(Alignment::CENTER)
                                .child(text(format!("Chip {i}")).size(13.0))
                        })
                        .horizontal()),
                ),
            doc("Rich items")
                .description("Items are widgets — avatars, badges, buttons; the list only handles the scrolling.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(220.0)
                        .child(ListView::builder(8, 56.0, |i| {
                            Padding::new(
                                EdgeInsets::symmetric(12.0, 6.0),
                                row(children![
                                    avatar(["RS", "AK", "JB", "MK"][i % 4]).size(32.0),
                                    gap_w(10.0),
                                    Expanded::new(column(children![
                                        text(format!("Member {i}")).size(14.0).weight(500.0),
                                        muted(if i % 2 == 0 { "Online" } else { "Away" }),
                                    ])
                                    .cross_axis_alignment(CrossAxisAlignment::Start)
                                    .main_axis_size(MainAxisSize::Min)),
                                    badge(if i % 2 == 0 { "Active" } else { "Idle" })
                                        .variant(if i % 2 == 0 { BadgeVariant::Success } else { BadgeVariant::Secondary }),
                                ])
                                .main_axis_size(MainAxisSize::Min),
                            )
                        })),
                ),
            doc("Variable extents")
                .description("ListView::variable(..) — each item declares its OWN extent (Flutter's variable-extent delegate, Rust-style): a feed of mixed card heights. Virtualized by prefix sums.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(240.0)
                        .child(ListView::variable(
                            20,
                            |i| if i % 3 == 0 { 72.0 } else if i % 2 == 0 { 48.0 } else { 56.0 },
                            |i| {
                                Container::new()
                                    .margin(EdgeInsets::symmetric(8.0, 4.0))
                                    .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(theme().radius)))
                                    .padding(EdgeInsets::all(12.0))
                                    .child(muted(format!("post {i} — a {} tall card", if i % 3 == 0 { 72 } else if i % 2 == 0 { 48 } else { 56 })))
                            },
                        )),
                ),
            doc("Reversed")
                .description(".reverse() — item 0 sits at the END and the list starts scrolled there (chat logs, consoles, terminals).")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(200.0)
                        .child(ListView::builder(40, 36.0, |i| {
                            Padding::new(
                                EdgeInsets::symmetric(14.0, 9.0),
                                row(children![
                                    muted(format!("{i:02}:")),
                                    gap_w(8.0),
                                    text(if i % 5 == 0 { "system: all systems nominal" } else { "log line" }).size(13.0),
                                ])
                                .main_axis_size(MainAxisSize::Min),
                            )
                        })
                        .reverse()),
                ),
            doc("Padded")
                .description(".padding(EdgeInsets) — outer padding that scrolls with the content, so the first/last items never hug the edges.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(160.0)
                        .child(ListView::builder(20, 44.0, |i| {
                            Container::new()
                                .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(theme().radius)))
                                .alignment(Alignment::CENTER)
                                .child(text(format!("Row {i}")).size(13.0))
                        })
                        .padding(EdgeInsets::all(12.0))),
                ),
            doc("Scrollbar styles")
                .description(".scrollbar(..) — hidden, overlay, or always-visible with a thickness.")
                .body(
                    column(children![
                        Container::new()
                            .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                            .height(120.0)
                            .child(
                                ListView::builder(30, 40.0, |i| {
                                    Padding::new(EdgeInsets::symmetric(12.0, 10.0), text(format!("Row {i}")).size(13.0))
                                })
                                .scrollbar(ScrollbarStyle { policy: ScrollbarPolicy::Always, thickness: 6.0, ..ScrollbarStyle::default() }),
                            ),
                        gap_h(10.0),
                        Container::new()
                            .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                            .height(120.0)
                            .child(
                                ListView::builder(30, 40.0, |i| {
                                    Padding::new(EdgeInsets::symmetric(12.0, 10.0), text(format!("Row {i}")).size(13.0))
                                })
                                .scrollbar(ScrollbarStyle { policy: ScrollbarPolicy::Hidden, ..ScrollbarStyle::default() }),
                            ),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
