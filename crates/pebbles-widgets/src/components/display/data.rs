//! Data-display components: [`Table`] (a data grid with optional column sorting,
//! row selection, zebra striping, an empty state and a footer slot). [`ListTile`]
//! lives in [`super::list_tile`].

use std::rc::Rc;

use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{BoxDecoration, Cursor, IconKind};

use crate::components::{checkbox, icon};
use crate::style::{Style, styled};
use crate::theme::{mix, theme};
use crate::widgets::{
    Align, Container, Expanded, GestureDetector, Padding, SizedBox, center, column, gap_w, row,
    text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, component_props, create_signal};

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

/// One table cell: plain text, or any widget (avatars, badges, buttons, icons…).
/// `&str`/`String` convert to text cells; widgets via [`cell`].
#[derive(Clone)]
pub enum Cell {
    Text(String),
    Widget(AnyWidget),
}

/// A rich cell: any widget rendered in the cell's slot.
pub fn cell(w: impl IntoWidget) -> Cell {
    Cell::Widget(w.into_widget())
}

impl From<String> for Cell {
    fn from(s: String) -> Self {
        Cell::Text(s)
    }
}
impl From<&str> for Cell {
    fn from(s: &str) -> Self {
        Cell::Text(s.to_string())
    }
}
impl From<AnyWidget> for Cell {
    fn from(w: AnyWidget) -> Self {
        Cell::Widget(w)
    }
}

/// A data grid: a header row plus data rows of cells, with optional column
/// sorting, row selection, zebra striping, a footer and an empty state — every
/// piece styleable.
///
/// Sorting and selection are **controlled** — the active sort and the selected
/// rows come in via builders ([`sort`](Table::sort) /
/// [`selection`](Table::selection)) and changes are reported out
/// ([`on_sort`](Table::on_sort) / [`on_selection`](Table::on_selection)); the
/// table never reorders or stores rows itself.
#[derive(Clone, Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<Cell>>,
    sortable: Vec<usize>,
    sort: Option<(usize, SortDir)>,
    on_sort: Option<Rc<dyn Fn(usize, SortDir)>>,
    selectable: bool,
    selection: Vec<usize>,
    on_selection: Option<Rc<dyn Fn(&[usize])>>,
    striped: bool,
    empty_state: Option<AnyWidget>,
    footer: Option<AnyWidget>,
    align: Vec<Option<Alignment>>,
    cell_padding: EdgeInsets,
    cell_size: f32,
    cell_color: Option<pebbles_foundation::Color>,
    header_style: Option<Style>,
    selection_column_width: f64,
    row_hover: bool,
    style: Option<Style>,
}

/// Create a [`Table`] with column headers.
pub fn table(headers: Vec<String>) -> Table {
    Table {
        headers,
        cell_padding: EdgeInsets::symmetric(12.0, 10.0),
        cell_size: 13.0,
        selection_column_width: 40.0,
        row_hover: true,
        ..Default::default()
    }
}

impl Table {
    /// Append a data row — any mix of text cells (`&str`/`String`) and rich
    /// [`cell`] widgets.
    pub fn row<C>(mut self, cells: impl IntoIterator<Item = C>) -> Self
    where
        C: Into<Cell>,
    {
        self.rows.push(cells.into_iter().map(Into::into).collect());
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
    /// A footer slot under the rows (separated by a hairline) — pagination,
    /// summaries, buttons.
    pub fn footer(mut self, w: impl IntoWidget) -> Self {
        self.footer = Some(w.into_widget());
        self
    }
    /// Align column `col`'s cells (and its header) — e.g. right-align numeric
    /// columns. Defaults to `Alignment::CENTER_LEFT`.
    pub fn align(mut self, col: usize, alignment: Alignment) -> Self {
        if self.align.len() <= col {
            self.align.resize(col + 1, None);
        }
        self.align[col] = Some(alignment);
        self
    }
    /// The cells' padding (default `(12, 10)` — horizontal 12, vertical 10).
    pub fn cell_padding(mut self, insets: EdgeInsets) -> Self {
        self.cell_padding = insets;
        self
    }
    /// The cell text size (default 13).
    pub fn cell_size(mut self, size: f32) -> Self {
        self.cell_size = size;
        self
    }
    /// The cell text color (defaults to the foreground; a [`style`](Table::style)'s
    /// text color wins over this).
    pub fn cell_color(mut self, color: pebbles_foundation::Color) -> Self {
        self.cell_color = Some(color);
        self
    }
    /// Style the header row: box props (background, border) plus text props
    /// (color, size, weight) for the header labels. Default: muted background,
    /// 12px semibold muted text.
    pub fn header_style(mut self, style: Style) -> Self {
        self.header_style = Some(style);
        self
    }
    /// The leading checkbox column's width (default 40).
    pub fn selection_column_width(mut self, width: f64) -> Self {
        self.selection_column_width = width;
        self
    }
    /// Enable/disable row hover feedback (default on).
    pub fn row_hover(mut self, on: bool) -> Self {
        self.row_hover = on;
        self
    }
    /// Merge a [`Style`](crate::Style) over the table surface (background, border,
    /// radius, shadow, width, margin, …) — `style().radius_all(0.0)` gives the
    /// sharp look. Its text props (color/size) also drive the cell text.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

/// Props for one sortable header cell.
struct SortHeaderProps {
    label: String,
    dir: Option<SortDir>,
    on_tap: Option<Rc<dyn Fn()>>,
    color: pebbles_foundation::Color,
    size: f32,
    weight: f32,
    pad: EdgeInsets,
    align: Alignment,
}

/// A sortable header cell: label + active-direction chevron, clickable with hover
/// feedback and a pointer cursor.
fn render_sort_header(p: &SortHeaderProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let bg = mix(c.muted, c.foreground, 0.05 * hv as f32);

    let mut items: Vec<AnyWidget> = vec![
        text(p.label.clone()).size(p.size).weight(p.weight).color(p.color).into_widget(),
    ];
    if let Some(dir) = p.dir {
        items.push(gap_w(4.0).into_widget());
        items.push(
            icon(match dir {
                SortDir::Asc => IconKind::ChevronUp,
                SortDir::Desc => IconKind::ChevronDown,
            })
            .size(12.0)
            .color(p.color)
            .into_widget(),
        );
    }

    let inner = Padding::new(
        p.pad,
        Align::new(
            p.align,
            row(items).cross_axis_alignment(CrossAxisAlignment::Center).main_axis_size(MainAxisSize::Min),
        ),
    );
    let mut g = GestureDetector::new(Container::new().color(bg).child(inner))
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false));
    if let Some(f) = p.on_tap.clone() {
        g = g.on_tap(move || f());
    }
    crate::widgets::semantics(crate::widgets::SemanticsRole::Button, p.label.clone(), g).into_widget()
}

/// Props for one data row.
struct TableRowProps {
    cells: Vec<Cell>,
    striped: bool,
    row_hover: bool,
    checkbox: Option<(bool, Rc<dyn Fn()>)>,
    checkbox_width: f64,
    cell_padding: EdgeInsets,
    cell_size: f32,
    cell_color: pebbles_foundation::Color,
    align: Rc<Vec<Option<Alignment>>>,
}

/// A data row: optional leading checkbox plus one expanded cell per column, with
/// hover feedback and optional zebra striping.
fn render_table_row(p: &TableRowProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let hv = if p.row_hover { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) } else { 0.0 };
    let base = if p.striped { mix(c.background, c.muted, 0.5) } else { c.background };
    let bg = mix(base, c.foreground, 0.05 * hv as f32);

    let mut cells: Vec<AnyWidget> = Vec::new();
    if let Some((checked, toggle)) = p.checkbox.clone() {
        cells.push(
            SizedBox::new(Some(p.checkbox_width), None, Some(center(checkbox(checked).on_changed(move || toggle())).into_widget()))
                .into_widget(),
        );
    }
    for (i, cell) in p.cells.iter().enumerate() {
        let content: AnyWidget = match cell {
            Cell::Text(s) => text(s.clone()).size(p.cell_size).color(p.cell_color).into_widget(),
            Cell::Widget(w) => w.clone(),
        };
        let alignment = p.align.get(i).copied().flatten().unwrap_or(Alignment::CENTER_LEFT);
        cells.push(
            Expanded::new(Padding::new(p.cell_padding, Align::new(alignment, content))).into_widget(),
        );
    }

    let mut g = GestureDetector::new(
        Container::new().color(bg).child(row(cells).cross_axis_alignment(CrossAxisAlignment::Center)),
    );
    if p.row_hover {
        g = g.on_hover_enter(move || hovered.set(true)).on_hover_exit(move || hovered.set(false));
    }
    g.into_widget()
}

impl IntoWidget for Table {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let headers = std::mem::take(&mut self.headers);
        let rows = std::mem::take(&mut self.rows);
        let n_rows = rows.len();
        let all_selected = n_rows > 0 && self.selection.len() == n_rows;
        let some_selected = !self.selection.is_empty() && !all_selected;

        // Surface style: transparent base, user wins; its text props drive cells.
        let base = crate::style::style();
        let merged = base.merge(self.style.clone().unwrap_or_default());
        let cell_color = merged
            .color
            .or(self.cell_color)
            .unwrap_or(th.colors.foreground);
        let cell_size = merged.font_size.unwrap_or(self.cell_size);

        // Header style: muted base; user's box + text props win.
        let hbase = crate::style::style()
            .background(th.colors.muted)
            .color(th.colors.muted_foreground)
            .font_size(12.0)
            .font_weight(600.0);
        let hstyle = hbase.merge(self.header_style.clone().unwrap_or_default());
        let header_bg = hstyle.background.unwrap_or(th.colors.muted);
        let header_color = hstyle.color.unwrap_or(th.colors.muted_foreground);
        let header_size = hstyle.font_size.unwrap_or(12.0);
        let header_weight = hstyle.font_weight.unwrap_or(600.0);

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
                SizedBox::new(Some(self.selection_column_width), None, Some(center(cb).into_widget()))
                    .into_widget(),
            );
        }
        for (i, h) in headers.into_iter().enumerate() {
            let alignment = self.align.get(i).copied().flatten().unwrap_or(Alignment::CENTER_LEFT);
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
                        SortHeaderProps {
                            label: h,
                            dir,
                            on_tap,
                            color: header_color,
                            size: header_size,
                            weight: header_weight,
                            pad: self.cell_padding,
                            align: alignment,
                        },
                    ))
                    .into_widget(),
                );
            } else {
                header_cells.push(
                    Expanded::new(Padding::new(
                        self.cell_padding,
                        Align::new(
                            alignment,
                            text(h).size(header_size).weight(header_weight).color(header_color),
                        ),
                    ))
                    .into_widget(),
                );
            }
        }
        let mut header_deco = BoxDecoration::new().color(header_bg);
        if let Some(border) = hstyle.border {
            header_deco = header_deco.border(border);
        }
        body.push(
            Container::new()
                .decoration(header_deco)
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
            let align = Rc::new(self.align.clone());
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
                            row_hover: self.row_hover,
                            checkbox: checkbox_col,
                            checkbox_width: self.selection_column_width,
                            cell_padding: self.cell_padding,
                            cell_size,
                            cell_color,
                            align: align.clone(),
                        },
                    )
                    .into_widget(),
                );
            }
        }

        // --- footer --------------------------------------------------------
        if let Some(footer) = self.footer.take() {
            body.push(Container::new().color(th.colors.border).height(1.0).into_widget());
            body.push(footer);
        }

        let content = column(body)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min);
        styled(content, merged).into_widget()
    }
}
