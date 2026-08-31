use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn data_tables() -> Element {
    let sort = create_signal(None::<(usize, SortDir)>);
    let page = create_signal(0usize);
    let selected = create_signal(Vec::<usize>::new());

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
            if dir == SortDir::Desc {
                ord.reverse()
            } else {
                ord
            }
        });
        rows
    };
    let per_page = 4;
    let total_pages = shown.len().div_ceil(per_page);
    let start = page.get() * per_page;
    let page_rows = &shown[start..(start + per_page).min(shown.len())];

    screen("Data Table")
        .description("A data grid — a header row plus rows of cells, with sorting, selection, striping, a footer and an empty state. Cells accept any widget; every piece (surface, header, cells, checkbox column, paddings, alignment) is styleable.")
        .body(children![
            doc("Default — sort + select + striped + footer pagination")
                .description("The shadcn look out of the box: sortable headers carry an always-visible glyph at their right edge (chevrons-up-down idle, ▲/▼ when active), checkboxes select rows (indeterminate select-all), zebra striping, and a .footer(..) slot holding the pagination.")
                .body(
                    card().child(
                        {
                            let mut t = table(vec!["Name".into(), "Role".into(), "Status".into()])
                                .sortable(0)
                                .sortable(1)
                                .sortable(2)
                                .selectable()
                                .striped(true)
                                .selection(selected.get())
                                .footer(
                                    Padding::new(
                                        EdgeInsets::symmetric(12.0, 8.0),
                                        row(children![
                                            muted(format!("{} of {} rows", per_page, shown.len())),
                                            spacer(),
                                            pagination(page.get() + 1, total_pages)
                                                .on_prev(move || {
                                                    page.update(|p| *p = p.saturating_sub(1));
                                                    selected.set(Vec::new());
                                                })
                                                .on_next(move || {
                                                    page.update(|p| *p = (*p + 1).min(total_pages - 1));
                                                    selected.set(Vec::new());
                                                }),
                                        ]),
                                    ),
                                );
                            if let Some((c, d)) = sort.get() {
                                t = t.sort(c, d);
                            }
                            for (n, r, s) in page_rows.iter() {
                                t = t.row(vec![n.clone(), r.clone(), s.clone()]);
                            }
                            t.on_sort(move |c, d| sort.set(Some((c, d))))
                                .on_selection(move |s| selected.set(s.to_vec()))
                        },
                    )
                    .padding(EdgeInsets::all(0.0)),
                ),
            doc("Sharp & compact")
                .description("style().radius_all(0.0) + a border gives the sharp enterprise look; .cell_padding(..) tightens the rows.")
                .body(
                    table(vec!["Invoice".into(), "Amount".into(), "Due".into()])
                        .row(vec!["INV-001", "$420.00", "2026-09-12"])
                        .row(vec!["INV-002", "$1,180.00", "2026-09-15"])
                        .row(vec!["INV-003", "$75.00", "2026-10-01"])
                        .align(1, Alignment::CENTER_RIGHT)
                        .align(2, Alignment::CENTER_RIGHT)
                        .cell_padding(EdgeInsets::symmetric(10.0, 8.0))
                        .style(
                            style()
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius_all(0.0),
                        ),
                ),
            doc("Styled header")
                .description("header_style(..) restyles the header row — accent background, white labels, or any brand look.")
                .body(
                    table(vec!["Module".into(), "Coverage".into(), "Bugs".into()])
                        .row(vec!["core", "98%", "0"])
                        .row(vec!["render", "94%", "2"])
                        .row(vec!["shell", "89%", "5"])
                        .align(1, Alignment::CENTER_RIGHT)
                        .align(2, Alignment::CENTER_RIGHT)
                        .header_style(
                            style()
                                .background(theme().colors.primary)
                                .color(theme().colors.primary_foreground)
                                .font_weight(700.0),
                        )
                        .style(style().border(Border::new(theme().colors.border, 1.0))),
                ),
            doc("Custom sort icons")
                .description("The sort glyphs are configurable — arrows instead of chevrons, a colored active icon, or hide the idle glyph entirely.")
                .body(
                    column(children![
                        table(vec!["Task".into(), "Priority".into()])
                            .row(vec!["Refactor table", "High"])
                            .row(vec!["Ship gallery", "Medium"])
                            .sortable(0)
                            .sortable(1)
                            .sort_asc_icon(lucide::ARROW_UP)
                            .sort_desc_icon(lucide::ARROW_DOWN)
                            .sort_idle_icon(lucide::ARROWS_UP_FROM_LINE)
                            .sort_icon_size(14.0)
                            .sort_icon_color(palette::blue::S600)
                            .style(style().border(Border::new(theme().colors.border, 1.0))),
                        gap_h(16.0),
                        table(vec!["Name".into(), "Role".into()])
                            .row(vec!["Reyco", "Lead"])
                            .row(vec!["Andres", "Engineer"])
                            .sortable(0)
                            .sort_idle_visible(false),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Rich cells")
                .description("Cells accept ANY widget via cell(..) — avatars, badges, buttons, icons — mixed with plain text in the same row.")
                .body(
                    table(vec!["User".into(), "Plan".into(), "Status".into(), "Action".into()])
                        .row(vec![
                            cell(row(children![
                                avatar("RS").size(24.0),
                                gap_w(8.0),
                                text("Reyco").size(13.0),
                            ])),
                            cell(badge("Pro")),
                            cell(badge("Active").variant(BadgeVariant::Success)),
                            cell(button("Manage").variant(ButtonVariant::Ghost).size(ButtonSize::Sm)),
                        ])
                        .row(vec![
                            cell(row(children![
                                avatar("AK").size(24.0).color(palette::GREEN),
                                gap_w(8.0),
                                text("Andres").size(13.0),
                            ])),
                            cell(badge("Free").variant(BadgeVariant::Secondary)),
                            cell(badge("Inactive").variant(BadgeVariant::Destructive)),
                            cell(button("Upgrade").variant(ButtonVariant::Ghost).size(ButtonSize::Sm)),
                        ])
                        .style(style().border(Border::new(theme().colors.border, 1.0))),
                ),
            doc("Minimal & quiet")
                .description("Turn the chrome off: .row_hover(false), no striping, generous padding, muted cells — a calm reference table.")
                .body(
                    table(vec!["Key".into(), "Value".into()])
                        .row(vec!["name", "pebbles"])
                        .row(vec!["version", "0.2.0"])
                        .row(vec!["edition", "2024"])
                        .row_hover(false)
                        .cell_padding(EdgeInsets::symmetric(16.0, 12.0))
                        .cell_color(theme().colors.muted_foreground)
                        .style(style().background(theme().colors.card).radius_all(theme().radius)),
                ),
            doc("Empty state")
                .description(".empty(widget) renders under the header when there are no rows.")
                .body(
                    card().child(
                        table(vec!["Name".into(), "Role".into(), "Status".into()]).empty(
                            empty()
                                .icon(IconKind::Search)
                                .title("No results")
                                .description("Try a different search."),
                        ),
                    )
                    .padding(EdgeInsets::all(0.0)),
                ),
        ])
}
