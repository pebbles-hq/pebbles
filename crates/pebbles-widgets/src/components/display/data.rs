//! Data-display components: [`ListTile`] (a list row) and [`Table`] (a data grid
//! with optional column sorting, row selection, zebra striping and an empty state).

use std::rc::Rc;

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{BoxDecoration, Cursor, IconKind};

use crate::components::{checkbox, icon};
use crate::theme::{mix, theme};
use crate::widgets::{
    Container, Expanded, GestureDetector, Padding, SizedBox, center, column, gap_h, gap_w, row,
    spacer, text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, component_props, create_signal};

/// A list row: optional leading widget, a title + optional subtitle, optional
/// trailing widget.
#[derive(Clone, Default)]
pub struct ListTile {
    leading: Option<AnyWidget>,
    title: String,
    subtitle: Option<String>,
    trailing: Option<AnyWidget>,
}

/// Create a [`ListTile`] with a title.
pub fn list_tile(title: impl Into<String>) -> ListTile {
    ListTile { title: title.into(), ..Default::default() }
}

impl ListTile {
    pub fn leading(mut self, leading: impl IntoWidget) -> Self {
        self.leading = Some(leading.into_widget());
        self
    }
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
    pub fn trailing(mut self, trailing: impl IntoWidget) -> Self {
        self.trailing = Some(trailing.into_widget());
        self
    }
}


impl IntoWidget for ListTile {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let mut title_col = vec![
            text(std::mem::take(&mut self.title)).size(14.0).weight(500.0).color(th.colors.foreground).into_widget(),
        ];
        if let Some(sub) = self.subtitle.take() {
            title_col.push(gap_h(2.0).into_widget());
            title_col.push(text(sub).size(12.0).color(th.colors.muted_foreground).into_widget());
        }

        let mut items: Vec<AnyWidget> = Vec::new();
        if let Some(leading) = self.leading.take() {
            items.push(leading);
            items.push(gap_w(12.0).into_widget());
        }
        items.push(
            Expanded::new(
                column(title_col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min),
            )
            .into_widget(),
        );
        if let Some(trailing) = self.trailing.take() {
            items.push(trailing);
        } else {
            items.push(spacer().into_widget());
        }

        Padding::new(
            EdgeInsets::symmetric(12.0, 10.0),
            row(items).cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .into_widget()
    }
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

/// The sort direction reported by a sortable [`Table`] header.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortDir {
    /// Smallest first (the header chevron points up).
    Asc,
    /// Largest first (the header chevron points down).
    Desc,
}

/// A data grid: a header row plus data rows of string cells, with optional column
/// sorting, row selection, zebra striping and an empty state.
///
/// Sorting and selection are **controlled** — the active sort and the selected rows
/// come in via builders ([`sort`](Table::sort) / [`selection`](Table::selection)) and
/// changes are reported out ([`on_sort`](Table::on_sort) /
/// [`on_selection`](Table::on_selection)); the table never reorders or stores rows
/// itself.
#[derive(Clone, Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    sortable: Vec<usize>,
    sort: Option<(usize, SortDir)>,
    on_sort: Option<Rc<dyn Fn(usize, SortDir)>>,
    selectable: bool,
    selection: Vec<usize>,
    on_selection: Option<Rc<dyn Fn(&[usize])>>,
    striped: bool,
    empty_state: Option<AnyWidget>,
}

/// Create a [`Table`] with column headers.
pub fn table(headers: Vec<String>) -> Table {
    Table { headers, ..Default::default() }
}

impl Table {
    /// Append a data row (cells matched to headers by position).
    pub fn row(mut self, cells: Vec<String>) -> Self {
        self.rows.push(cells);
        self
    }
    /// Make column `col` sortable (call once per column): its header becomes
    /// clickable (pointer cursor, hover feedback) and reports through
    /// [`on_sort`](Table::on_sort). Sorting the data is the caller's job.
    pub fn sortable(mut self, col: usize) -> Self {
        self.sortable.push(col);
        self
    }
    /// The active sort — column and direction — shown as a chevron in that column's
    /// header. Controlled: pass the same value back that [`on_sort`](Table::on_sort)
    /// reported.
    pub fn sort(mut self, col: usize, dir: SortDir) -> Self {
        self.sort = Some((col, dir));
        self
    }
    /// Reports a sortable-header click: the column and the direction it should next
    /// sort by (cycles `Asc` → `Desc` → `Asc`).
    pub fn on_sort(mut self, f: impl Fn(usize, SortDir) + 'static) -> Self {
        self.on_sort = Some(Rc::new(f));
        self
    }
    /// Add a leading checkbox column: one checkbox per data row plus a header
    /// select-all (with the indeterminate state when only some rows are selected).
    pub fn selectable(mut self) -> Self {
        self.selectable = true;
        self
    }
    /// The selected row indices. Controlled: pass the value back that
    /// [`on_selection`](Table::on_selection) reported.
    pub fn selection(mut self, selection: impl Into<Vec<usize>>) -> Self {
        self.selection = selection.into();
        self
    }
    /// Reports the new selection whenever a row checkbox or the select-all is toggled.
    pub fn on_selection(mut self, f: impl Fn(&[usize]) + 'static) -> Self {
        self.on_selection = Some(Rc::new(f));
        self
    }
    /// Zebra-stripe the data rows (odd rows on a muted background).
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = striped;
        self
    }
    /// Shown centered under the header when the table has no rows.
    pub fn empty(mut self, w: impl IntoWidget) -> Self {
        self.empty_state = Some(w.into_widget());
        self
    }
}

/// Props for one sortable header cell.
struct SortHeaderProps {
    label: String,
    dir: Option<SortDir>,
    on_tap: Option<Rc<dyn Fn()>>,
}

/// A sortable header cell: label + active-direction chevron, clickable with hover
/// feedback and a pointer cursor.
fn render_sort_header(p: &SortHeaderProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let bg = mix(c.muted, c.foreground, 0.05 * hv as f32);

    let mut items: Vec<AnyWidget> = vec![
        text(p.label.clone()).size(12.0).semibold().color(c.muted_foreground).into_widget(),
    ];
    if let Some(dir) = p.dir {
        items.push(gap_w(4.0).into_widget());
        items.push(
            icon(match dir {
                SortDir::Asc => IconKind::ChevronUp,
                SortDir::Desc => IconKind::ChevronDown,
            })
            .size(12.0)
            .color(c.foreground)
            .into_widget(),
        );
    }

    let inner = Padding::new(
        EdgeInsets::symmetric(12.0, 10.0),
        row(items).cross_axis_alignment(CrossAxisAlignment::Center).main_axis_size(MainAxisSize::Min),
    );
    let mut g = GestureDetector::new(Container::new().color(bg).child(inner))
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false));
    if let Some(f) = p.on_tap.clone() {
        g = g.on_tap(move || f());
    }
    g.into_widget()
}

/// Props for one data row.
struct TableRowProps {
    cells: Vec<String>,
    striped: bool,
    checkbox: Option<(bool, Rc<dyn Fn()>)>,
}

/// A data row: optional leading checkbox plus one expanded text cell per column,
/// with hover feedback and optional zebra striping.
fn render_table_row(p: &TableRowProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let base = if p.striped { mix(c.background, c.muted, 0.5) } else { c.background };
    let bg = mix(base, c.foreground, 0.05 * hv as f32);

    let mut cells: Vec<AnyWidget> = Vec::new();
    if let Some((checked, toggle)) = p.checkbox.clone() {
        cells.push(
            SizedBox::new(Some(40.0), None, Some(center(checkbox(checked).on_changed(move || toggle())).into_widget()))
                .into_widget(),
        );
    }
    for cell in &p.cells {
        cells.push(
            Expanded::new(Padding::new(
                EdgeInsets::symmetric(12.0, 10.0),
                text(cell.clone()).size(13.0).color(c.foreground),
            ))
            .into_widget(),
        );
    }

    GestureDetector::new(
        Container::new().color(bg).child(
            row(cells).cross_axis_alignment(CrossAxisAlignment::Center),
        ),
    )
    .on_hover_enter(move || hovered.set(true))
    .on_hover_exit(move || hovered.set(false))
    .into_widget()
}


impl IntoWidget for Table {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let headers = std::mem::take(&mut self.headers);
        let rows = std::mem::take(&mut self.rows);
        let n_rows = rows.len();
        let all_selected = n_rows > 0 && self.selection.len() == n_rows;
        let some_selected = !self.selection.is_empty() && !all_selected;

        let mut body: Vec<AnyWidget> = Vec::new();

        // --- header row ---------------------------------------------------
        let mut header_cells: Vec<AnyWidget> = Vec::new();
        if self.selectable {
            let on_selection = self.on_selection.clone();
            let n = n_rows;
            let all = all_selected;
            let mut cb = checkbox(all).indeterminate(some_selected);
            if let Some(f) = on_selection {
                cb = cb.on_changed(move || {
                    let next: Vec<usize> = if all { Vec::new() } else { (0..n).collect() };
                    f(&next);
                });
            }
            header_cells.push(
                SizedBox::new(Some(40.0), None, Some(center(cb).into_widget())).into_widget(),
            );
        }
        for (i, h) in headers.into_iter().enumerate() {
            let sortable = self.sortable.contains(&i);
            if sortable {
                let dir = self.sort.and_then(|(c, d)| if c == i { Some(d) } else { None });
                let on_tap: Option<Rc<dyn Fn()>> = self.on_sort.clone().map(|f| {
                    let next = match self.sort {
                        Some((c, SortDir::Asc)) if c == i => SortDir::Desc,
                        _ => SortDir::Asc,
                    };
                    let cb: Rc<dyn Fn()> = Rc::new(move || f(i, next));
                    cb
                });
                header_cells.push(
                    Expanded::new(component_props(
                        render_sort_header,
                        SortHeaderProps { label: h, dir, on_tap },
                    ))
                    .into_widget(),
                );
            } else {
                header_cells.push(
                    Expanded::new(Padding::new(
                        EdgeInsets::symmetric(12.0, 10.0),
                        text(h).size(12.0).semibold().color(th.colors.muted_foreground),
                    ))
                    .into_widget(),
                );
            }
        }
        body.push(
            Container::new()
                .decoration(BoxDecoration::new().color(th.colors.muted))
                .child(row(header_cells).cross_axis_alignment(CrossAxisAlignment::Center))
                .into_widget(),
        );

        // --- data rows ----------------------------------------------------
        if rows.is_empty() {
            if let Some(empty_state) = self.empty_state.take() {
                body.push(
                    Container::new()
                        .padding(EdgeInsets::all(24.0))
                        .child(center(empty_state))
                        .into_widget(),
                );
            }
        } else {
            for (idx, cells) in rows.into_iter().enumerate() {
                body.push(Container::new().color(th.colors.border).height(1.0).into_widget());
                let checkbox_col = self.selectable.then(|| {
                    let current = Rc::new(self.selection.clone());
                    let checked = current.contains(&idx);
                    let on_selection = self.on_selection.clone();
                    let i = idx;
                    let toggle: Rc<dyn Fn()> = Rc::new(move || {
                        if let Some(f) = on_selection.clone() {
                            let mut next: Vec<usize> =
                                current.iter().copied().filter(|&v| v != i).collect();
                            if !current.contains(&i) {
                                next.push(i);
                                next.sort_unstable();
                            }
                            f(&next);
                        }
                    });
                    (checked, toggle)
                });
                body.push(
                    component_props(
                        render_table_row,
                        TableRowProps {
                            cells,
                            striped: self.striped && idx % 2 == 1,
                            checkbox: checkbox_col,
                        },
                    )
                    .into_widget(),
                );
            }
        }

        column(body)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .into_widget()
    }
}
