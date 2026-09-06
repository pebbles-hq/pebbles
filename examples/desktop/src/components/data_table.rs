//! Reusable data-table chrome shared by the Products / Orders / Customers screens: a
//! search field, the shadcn-style pagination footer, and a table empty state.

use pebbles::prelude::*;

/// A search box that filters a table: binds `search` and resets `page` to the first
/// page on every keystroke.
pub fn search_field(
    search: Signal<String>,
    page: Signal<usize>,
    placeholder: &str,
    width: f64,
) -> impl IntoWidget {
    container().width(width).child(
        text_field()
            .leading(lucide::SEARCH)
            .placeholder(placeholder.to_string())
            .bind(search)
            .on_changed(move |_| page.set(0)),
    )
}

/// The table footer: a rows-per-page selector + "start–end of N results" on the left,
/// compact first/prev/next/last nav on the right. Changing the page size re-paginates
/// and jumps back to the first page.
pub fn table_pager(
    page: Signal<usize>,
    per_page: Signal<usize>,
    cur: usize,
    total_pages: usize,
    total: usize,
) -> impl IntoWidget {
    let size = per_page.get();
    container().padding(EdgeInsets::only(0.0, 14.0, 0.0, 0.0)).child(
        pagination(cur + 1, total_pages)
            .variant(PaginationVariant::Compact)
            .rows_per_page(size, vec![10, 20, 30, 50], move |s| {
                per_page.set(s);
                page.set(0);
            })
            .total_items(total)
            .on_page(move |p| page.set(p - 1)),
    )
}

/// A centered empty state for a table with no matching rows.
pub fn table_empty(msg: &str) -> AnyWidget {
    container()
        .padding(EdgeInsets::all(30.0))
        .alignment(Alignment::CENTER)
        .child(text(msg.to_string()).size(13.5).color(theme().colors.muted_foreground))
        .into_widget()
}
