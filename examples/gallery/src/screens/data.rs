use pebbles::prelude::*;

use crate::ui::{screen, section};

pub fn data() -> Element {
    let open = create_signal([true, false]);
    let sel = create_signal(1usize);
    let list_ctrl = use_scroll_controller();

    let people: Vec<(String, String, String)> = vec![
        ("Reyco", "Lead", "Active"),
        ("Andres", "Engineer", "Active"),
        ("Joseph", "Engineer", "Away"),
        ("Marvin", "Design", "Active"),
        ("Kat", "Support", "Away"),
        ("Leo", "Engineer", "Active"),
        ("Sam", "QA", "Active"),
        ("Nina", "PM", "Away"),
        ("Owen", "Ops", "Active"),
    ]
    .into_iter()
    .map(|(n, r, s)| (n.into(), r.into(), s.into()))
    .collect();
    let sort = create_signal(None::<(usize, SortDir)>);
    let page = create_signal(0usize);
    let selected = create_signal(Vec::<usize>::new());

    // App-owned sort: order the rows per the reported (col, dir).
    let shown = {
        let mut rows = people.clone();
        let (col, dir) = sort.get().unwrap_or((0, SortDir::Asc));
        rows.sort_by(|a, b| {
            let key = |row: &(String, String, String)| match col {
                0 => row.0.to_lowercase(),
                1 => row.1.to_lowercase(),
                _ => row.2.to_lowercase(),
            };
            let ord = key(a).cmp(&key(b));
            if dir == SortDir::Desc { ord.reverse() } else { ord }
        });
        rows
    };
    let per_page = 4;
    let total_pages = shown.len().div_ceil(per_page);
    let start = page.get() * per_page;
    let page_rows = &shown[start..(start + per_page).min(shown.len())];

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
                card().child(
                    column(children![
                        list_tile("Inbox").leading(icon(IconKind::Menu).size(18.0)).subtitle("12 new").trailing(badge("12")),
                        separator(),
                        list_tile("Starred").leading(icon(IconKind::Star).size(18.0)).subtitle("3 items"),
                        separator(),
                        list_tile("Drafts").leading(icon(IconKind::Info).size(18.0)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                )
                .padding(EdgeInsets::all(4.0)),
            ),
            section(
                "TABLE",
                column(children![
                    card()
                        .child({
                            let mut t = table(vec!["Name".into(), "Role".into(), "Status".into()])
                                .sortable(0)
                                .sortable(1)
                                .sortable(2)
                                .selectable()
                                .striped(true)
                                .selection(selected.get());
                            if let Some((c, d)) = sort.get() {
                                t = t.sort(c, d);
                            }
                            for (n, r, s) in page_rows.iter() {
                                t = t.row(vec![n.clone(), r.clone(), s.clone()]);
                            }
                            t.on_sort(move |c, d| sort.set(Some((c, d))))
                                .on_selection(move |s| selected.set(s.to_vec()))
                        })
                        .padding(EdgeInsets::all(0.0)),
                    gap_h(12.0),
                    pagination(page.get() + 1, total_pages)
                        .on_prev(move || {
                            page.update(|p| *p = p.saturating_sub(1));
                            selected.set(Vec::new());
                        })
                        .on_next(move || {
                            page.update(|p| *p = (*p + 1).min(total_pages - 1));
                            selected.set(Vec::new());
                        }),
                    gap_h(20.0),
                    card()
                        .child({
                            table(vec!["Name".into(), "Role".into(), "Status".into()]).empty(
                                empty()
                                    .icon(IconKind::Search)
                                    .title("No results")
                                    .description("Try a different search."),
                            )
                        })
                        .padding(EdgeInsets::all(0.0)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
            section("FILE EXPLORER (TreeView)", card().child(tree).padding(EdgeInsets::all(4.0))),
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
