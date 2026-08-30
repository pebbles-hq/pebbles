use pebbles::prelude::*;

use crate::ui::{screen, section};

pub fn data() -> impl IntoWidget {
    let open = create_signal([true, false]);
    let sel = create_signal(1usize);
    let list_ctrl = use_scroll_controller();

    let tree = tree_view(vec![
        tree_node("src")
            .expanded(open.get()[0])
            .on_toggle(move || open.update(|o| o[0] = !o[0]))
            .children(vec![
                tree_node("main.rs").icon(IconKind::Dot).selected(sel.get() == 0).on_select(move || sel.set(0)),
                tree_node("components")
                    .expanded(open.get()[1])
                    .on_toggle(move || open.update(|o| o[1] = !o[1]))
                    .children(vec![
                        tree_node("button.rs").icon(IconKind::Dot).selected(sel.get() == 2).on_select(move || sel.set(2)),
                    ]),
            ]),
        tree_node("Cargo.toml").icon(IconKind::Dot).selected(sel.get() == 1).on_select(move || sel.set(1)),
    ]);

    screen(
        "Data & Desktop",
        "List, table, tree/file-explorer, split view and panels.",
        children![
            section(
                "VIRTUALIZED LIST — 5,000 rows, only visible ones built. Smooth wheel, keyboard (PageDn/Home/End), drag the bar, or the buttons (ScrollController)",
                column(children![
                    row(children![
                        button("Top").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                            .on_pressed(move || list_ctrl.animate_to(0.0)),
                        SizedBox::spacer(8.0, 0.0),
                        button("Jump to #2500").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                            .on_pressed(move || list_ctrl.scroll_to_index(2500, 44.0)),
                        SizedBox::spacer(8.0, 0.0),
                        button("Bottom").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                            .on_pressed(move || list_ctrl.animate_to(5000.0 * 44.0)),
                    ])
                    .main_axis_min(),
                    SizedBox::spacer(0.0, 10.0),
                    Container::new()
                        .decoration(
                            BoxDecoration::new()
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius(BorderRadius::all(theme().radius)),
                        )
                        .height(300.0)
                        .child(ListView::builder(5000, 44.0, |i| {
                            let c = theme().colors;
                            Container::new()
                                .height(44.0)
                                .padding(EdgeInsets::symmetric(14.0, 0.0))
                                .alignment(Alignment::CENTER_LEFT)
                                .decoration(BoxDecoration::new().border(Border::new(c.border, 0.5)))
                                .child(row(children![
                                    icon(IconKind::Dot).size(16.0).color(c.muted_foreground),
                                    SizedBox::spacer(10.0, 0.0),
                                    text(format!("Row {i}")).size(14.0).color(c.foreground),
                                    spacer(),
                                    muted(format!("#{i}")),
                                ]))
                        })
                        .controller(list_ctrl)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_min(),
            ),
            section(
                "VIRTUALIZED GRID — 600 cells, 4 columns, only visible rows built",
                Container::new()
                    .decoration(
                        BoxDecoration::new()
                            .border(Border::new(theme().colors.border, 1.0))
                            .radius(BorderRadius::all(theme().radius)),
                    )
                    .height(260.0)
                    .child(GridView::builder(600, 4, 96.0, |i| {
                        let c = theme().colors;
                        Container::new()
                            .padding(EdgeInsets::all(6.0))
                            .child(
                                Container::new()
                                    .decoration(
                                        BoxDecoration::new()
                                            .color(c.secondary)
                                            .radius(BorderRadius::all(theme().radius)),
                                    )
                                    .alignment(Alignment::CENTER)
                                    .child(text(format!("#{i}")).size(14.0).color(c.foreground)),
                            )
                    })),
            ),
            section(
                "LIST",
                Card::new(
                    column(children![
                        list_tile("Inbox").leading(icon(IconKind::Menu).size(18.0)).subtitle("12 new").trailing(badge("12")),
                        separator(),
                        list_tile("Starred").leading(icon(IconKind::Star).size(18.0)).subtitle("3 items"),
                        separator(),
                        list_tile("Drafts").leading(icon(IconKind::Info).size(18.0)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_min(),
                )
                .padding(EdgeInsets::all(4.0)),
            ),
            section(
                "TABLE",
                Card::new(
                    table(vec!["Name".into(), "Role".into(), "Status".into()])
                        .row(vec!["Reyco".into(), "Lead".into(), "Active".into()])
                        .row(vec!["Andres".into(), "Engineer".into(), "Active".into()])
                        .row(vec!["Joseph".into(), "Engineer".into(), "Away".into()]),
                )
                .padding(EdgeInsets::all(0.0)),
            ),
            section("FILE EXPLORER (TreeView)", Card::new(tree).padding(EdgeInsets::all(4.0))),
            section(
                "SPLIT VIEW + PANELS",
                Container::new()
                    .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                    .height(200.0)
                    .child(
                        split_view(
                            panel("EXPLORER", tree_view(vec![
                                tree_node("app").icon(IconKind::Dot),
                                tree_node("lib").icon(IconKind::Dot),
                                tree_node("README.md").icon(IconKind::Dot),
                            ])),
                            panel("EDITOR", body("// Two resizable panes.")),
                        )
                        .ratio(0.4),
                    ),
            ),
        ],
    )
}
