use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn data_tables() -> Element {
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
        .description("A data grid — a header row plus rows of cells, with optional column sorting, row selection, zebra striping and an empty state. Data stays app-owned; the table reports.")
        .body(children![
            doc("Basic")
                .description("table(headers).row(cells) — the plain grid.")
                .body(
                    card().child(
                        table(vec!["Name".into(), "Role".into(), "Status".into()])
                            .row(vec!["Reyco".into(), "Lead".into(), "Active".into()])
                            .row(vec!["Andres".into(), "Engineer".into(), "Active".into()])
                            .row(vec!["Joseph".into(), "Engineer".into(), "Away".into()]),
                    )
                    .padding(EdgeInsets::all(0.0)),
                ),
            doc("Sort + select + striped + pagination")
                .description("Click a header to sort (▲▼ cycles, controlled via .sort); checkboxes select rows (indeterminate select-all); .striped() zebra-stripes; Pagination pages the app-owned rows.")
                .body(
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
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
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
