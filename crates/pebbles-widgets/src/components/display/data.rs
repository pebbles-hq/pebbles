//! Data-display components: [`Table`] (a data grid with optional column sorting,
//! row selection, zebra striping, an empty state and a footer slot). [`ListTile`]
//! lives in [`super::list_tile`].

use std::rc::Rc;

use pebbles_foundation::{Alignment, Color, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, BoxConstraints, BoxDecoration, Cursor, IconKind, TableColumnWidth};

use crate::components::{checkbox, icon};
use crate::style::{Style, styled};
use crate::theme::{mix, theme};
use crate::widgets::{
    Align, Container, GestureDetector, Padding, SingleChildScrollView, center, clip_rect, column,
    constrained_box, gap_w, layout_table, row, spacer, text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animated, component_props, create_signal};

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

/// How a column's cell content behaves when it's wider than the column — the same
/// choices HTML/Flutter give you. Every cell is clipped to its column either way, so
/// content **never** overlaps a neighbor; this only picks how the text is laid out.
/// Set it per column with [`Table::overflow`] (or all columns with
/// [`Table::overflow_all`]). Applies to text cells; widget cells are always clipped.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum CellOverflow {
    /// Wrap onto as many lines as needed, breaking a long word if it can't fit; the
    /// row grows taller to show all of it (HTML's default). This is the default.
    #[default]
    Wrap,
    /// Keep to a single line, truncated with a trailing "…" (CSS `text-overflow:
    /// ellipsis`).
    Ellipsis,
    /// Keep to a single line, hard-clipped at the column edge (no ellipsis).
    Clip,
}

/// How a column is sized. By default (`Auto`) every column sizes to its own widest
/// content — no fixed/equal widths — and the table scrolls horizontally when the
/// columns together don't fit. Set a width per column with [`Table::column_width`]
/// (or all at once with [`Table::column_widths`]); the same choices HTML/Flutter give.
#[derive(Clone, Copy, PartialEq, Default)]
pub enum ColumnWidth {
    /// Size to the column's widest content (default). No wrapping unless capped.
    #[default]
    Auto,
    /// Exactly this many logical pixels (content wraps/ellipsizes to fit).
    Fixed(f64),
    /// Size to content, but never wider than this — content wraps/ellipsizes past it.
    Max(f64),
    /// Take a weighted share of the width left over after the sized columns, filling
    /// the table. Any `Flex` column disables horizontal scrolling (the grid fits).
    Flex(f64),
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
#[derive(Clone)]
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
    overflow: Vec<Option<CellOverflow>>,
    overflow_default: CellOverflow,
    columns: Vec<Option<ColumnWidth>>,
    column_default: ColumnWidth,
    cell_padding: EdgeInsets,
    cell_size: f32,
    cell_color: Option<Color>,
    header_style: Option<Style>,
    selection_column_width: f64,
    row_hover: bool,
    sort_asc_icon: pebbles_render::IconData,
    sort_desc_icon: pebbles_render::IconData,
    sort_idle_icon: pebbles_render::IconData,
    sort_idle_visible: bool,
    sort_icon_color: Option<Color>,
    sort_icon_size: f64,
    style: Option<Style>,
}

/// Create a [`Table`] with column headers.
pub fn table(headers: Vec<String>) -> Table {
    Table {
        headers,
        rows: Vec::new(),
        cell_padding: EdgeInsets::symmetric(12.0, 10.0),
        cell_size: 13.0,
        selection_column_width: 40.0,
        row_hover: true,
        sort_asc_icon: IconKind::ChevronUp.into(),
        sort_desc_icon: IconKind::ChevronDown.into(),
        sort_idle_icon: IconKind::ChevronsUpDown.into(),
        sort_idle_visible: true,
        sort_icon_color: None,
        sort_icon_size: 12.0,
        style: None,
        striped: false,
        empty_state: None,
        footer: None,
        align: Vec::new(),
        overflow: Vec::new(),
        overflow_default: CellOverflow::Wrap,
        columns: Vec::new(),
        column_default: ColumnWidth::Auto,
        cell_color: None,
        header_style: None,
        sortable: Vec::new(),
        sort: None,
        on_sort: None,
        selectable: false,
        selection: Vec::new(),
        on_selection: None,
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
    /// How column `col` is sized ([`ColumnWidth`]). Columns default to
    /// [`ColumnWidth::Auto`] (fit content); no column has a fixed/equal width unless
    /// you set one here.
    pub fn column_width(mut self, col: usize, width: ColumnWidth) -> Self {
        if self.columns.len() <= col {
            self.columns.resize(col + 1, None);
        }
        self.columns[col] = Some(width);
        self
    }
    /// Set every column's [`ColumnWidth`] at once (index = column).
    pub fn column_widths(mut self, widths: Vec<ColumnWidth>) -> Self {
        self.columns = widths.into_iter().map(Some).collect();
        self
    }
    /// The default [`ColumnWidth`] for columns that don't set their own
    /// (default [`ColumnWidth::Auto`]).
    pub fn column_width_all(mut self, width: ColumnWidth) -> Self {
        self.column_default = width;
        self
    }
    /// How column `col`'s cells behave when content is wider than the column
    /// (wrap / ellipsis / clip). Every cell is clipped to its column regardless, so
    /// content never overlaps a neighbor — this just chooses the layout. Defaults to
    /// [`CellOverflow::Wrap`]; change the table-wide default with [`overflow_all`](Self::overflow_all).
    pub fn overflow(mut self, col: usize, mode: CellOverflow) -> Self {
        if self.overflow.len() <= col {
            self.overflow.resize(col + 1, None);
        }
        self.overflow[col] = Some(mode);
        self
    }
    /// The overflow mode for every column that doesn't set its own (default
    /// [`CellOverflow::Wrap`]).
    pub fn overflow_all(mut self, mode: CellOverflow) -> Self {
        self.overflow_default = mode;
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
    pub fn cell_color(mut self, color: Color) -> Self {
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
    /// The active ascending sort glyph (default `ChevronUp`).
    pub fn sort_asc_icon(mut self, glyph: impl Into<pebbles_render::IconData>) -> Self {
        self.sort_asc_icon = glyph.into();
        self
    }
    /// The active descending sort glyph (default `ChevronDown`).
    pub fn sort_desc_icon(mut self, glyph: impl Into<pebbles_render::IconData>) -> Self {
        self.sort_desc_icon = glyph.into();
        self
    }
    /// The idle glyph shown on sortable columns with no active sort
    /// (default `ChevronsUpDown`).
    pub fn sort_idle_icon(mut self, glyph: impl Into<pebbles_render::IconData>) -> Self {
        self.sort_idle_icon = glyph.into();
        self
    }
    /// Whether unsorted sortable columns show the idle glyph (default true).
    pub fn sort_idle_visible(mut self, visible: bool) -> Self {
        self.sort_idle_visible = visible;
        self
    }
    /// The active sort glyph's color (defaults to the header label color).
    pub fn sort_icon_color(mut self, color: Color) -> Self {
        self.sort_icon_color = Some(color);
        self
    }
    /// The sort glyph size (default 12).
    pub fn sort_icon_size(mut self, size: f64) -> Self {
        self.sort_icon_size = size;
        self
    }
}

/// Props for one sortable header cell.
struct SortHeaderProps {
    label: String,
    dir: Option<SortDir>,
    on_tap: Option<Rc<dyn Fn()>>,
    color: Color,
    size: f32,
    weight: f32,
    pad: EdgeInsets,
    align: Alignment,
    asc_icon: pebbles_render::IconData,
    desc_icon: pebbles_render::IconData,
    idle_icon: pebbles_render::IconData,
    idle_visible: bool,
    icon_color: Option<Color>,
    icon_size: f64,
}

/// A sortable header cell: label + active-direction chevron, clickable with hover
/// feedback and a pointer cursor.
fn render_sort_header(p: &SortHeaderProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let bg = mix(c.muted, c.foreground, 0.05 * hv as f32);

    // The sort glyph sits at the RIGHTMOST edge of the cell (shadcn), always
    // visible: directional when this column is the active sort, the idle glyph
    // otherwise.
    let (glyph, glyph_color) = match p.dir {
        Some(SortDir::Asc) => (p.asc_icon, p.icon_color.unwrap_or(p.color)),
        Some(SortDir::Desc) => (p.desc_icon, p.icon_color.unwrap_or(p.color)),
        None => (p.idle_icon, c.muted_foreground),
    };
    let label = text(p.label.clone()).size(p.size).weight(p.weight).color(p.color);
    let mut items: Vec<AnyWidget> = if p.align.x > 0.0 {
        // Right-aligned column: the text hugs the rightmost sort glyph.
        vec![spacer().into_widget(), label.into_widget()]
    } else {
        vec![label.into_widget(), spacer().into_widget()]
    };
    if p.dir.is_some() || p.idle_visible {
        items.push(gap_w(6.0).into_widget());
        items.push(icon(glyph).size(p.icon_size).color(glyph_color).into_widget());
    }

    let inner = Padding::new(
        p.pad,
        row(items).cross_axis_alignment(CrossAxisAlignment::Center).main_axis_size(MainAxisSize::Max),
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

impl IntoWidget for Table {
    fn into_widget(self) -> AnyWidget {
        // Rendered in a component so its hover state re-renders only the table.
        component_props(render_data_table, self).into_widget()
    }
}

/// Map a [`ColumnWidth`] to a grid column spec + an optional max-width cap (`Max`).
fn map_col(w: ColumnWidth) -> (TableColumnWidth, Option<f64>) {
    match w {
        ColumnWidth::Auto => (TableColumnWidth::Intrinsic, None),
        ColumnWidth::Fixed(px) => (TableColumnWidth::Fixed(px.max(0.0)), None),
        ColumnWidth::Max(px) => (TableColumnWidth::Intrinsic, Some(px.max(0.0))),
        ColumnWidth::Flex(weight) => (TableColumnWidth::Flex(weight.max(0.0)), None),
    }
}

/// A header cell surface: the header background (+ optional border) filling the slot.
fn header_slot(bg: Color, border: Option<Border>, child: AnyWidget) -> AnyWidget {
    let mut deco = BoxDecoration::new().color(bg);
    if let Some(b) = border {
        deco = deco.border(b);
    }
    Container::new().decoration(deco).child(child).into_widget()
}

/// A data cell surface: its striped/hover background filling the row slot, wired for
/// row hover.
fn data_slot(
    row_idx: usize,
    striped: bool,
    row_hover: bool,
    hovered: Signal<Option<usize>>,
    child: AnyWidget,
) -> AnyWidget {
    let c = theme().colors;
    let base = if striped { mix(c.background, c.muted, 0.5) } else { c.background };
    let bg = if row_hover && hovered.get() == Some(row_idx) { mix(base, c.foreground, 0.05) } else { base };
    let container = Container::new().color(bg).child(child);
    if row_hover {
        GestureDetector::new(container)
            .on_hover_enter(move || hovered.set(Some(row_idx)))
            .on_hover_exit(move || {
                if hovered.peek() == Some(row_idx) {
                    hovered.set(None);
                }
            })
            .into_widget()
    } else {
        container.into_widget()
    }
}

/// Build the whole data table as a single column-negotiating grid: columns size by
/// [`ColumnWidth`] (content by default), rows share those widths, and the grid scrolls
/// horizontally when content-sized columns don't fit.
fn render_data_table(t: &Table) -> AnyWidget {
    let c = theme().colors;

    // Surface + cell styles (transparent base; user's style wins; text props drive cells).
    let merged = crate::style::style().merge(t.style.clone().unwrap_or_default());
    let cell_color = merged.color.or(t.cell_color).unwrap_or(c.foreground);
    let cell_size = merged.font_size.unwrap_or(t.cell_size);

    // Header style: muted base; user's box + text props win.
    let hstyle = crate::style::style()
        .background(c.muted)
        .color(c.muted_foreground)
        .font_size(12.0)
        .font_weight(600.0)
        .merge(t.header_style.clone().unwrap_or_default());
    let header_bg = hstyle.background.unwrap_or(c.muted);
    let header_color = hstyle.color.unwrap_or(c.muted_foreground);
    let header_size = hstyle.font_size.unwrap_or(12.0);
    let header_weight = hstyle.font_weight.unwrap_or(600.0);
    let header_border = hstyle.border;

    let hovered = create_signal(Option::<usize>::None);

    let n_user = t.headers.len();
    let sel = t.selectable;
    let n_rows = t.rows.len();
    let all_selected = n_rows > 0 && t.selection.len() == n_rows;
    let some_selected = !t.selection.is_empty() && !all_selected;

    // Column specs (+ max-width caps for `Max`), preceded by a fixed checkbox column.
    let mut specs: Vec<TableColumnWidth> = Vec::new();
    if sel {
        specs.push(TableColumnWidth::Fixed(t.selection_column_width));
    }
    let mut caps: Vec<Option<f64>> = Vec::with_capacity(n_user);
    let mut any_flex = false;
    for i in 0..n_user {
        let cw = t.columns.get(i).copied().flatten().unwrap_or(t.column_default);
        let (spec, cap) = map_col(cw);
        any_flex |= matches!(spec, TableColumnWidth::Flex(_) | TableColumnWidth::Fraction(_));
        specs.push(spec);
        caps.push(cap);
    }

    let mut grid_rows: Vec<Vec<AnyWidget>> = Vec::new();

    // --- header row -------------------------------------------------------
    let mut hrow: Vec<AnyWidget> = Vec::new();
    if sel {
        let on_selection = t.on_selection.clone();
        let n = n_rows;
        let all = all_selected;
        let mut cb = checkbox(all).indeterminate(some_selected);
        if let Some(f) = on_selection {
            cb = cb.on_changed(move || {
                let next: Vec<usize> = if all { Vec::new() } else { (0..n).collect() };
                f(&next);
            });
        }
        hrow.push(header_slot(header_bg, header_border, center(cb).into_widget()));
    }
    for (i, h) in t.headers.iter().enumerate() {
        let alignment = t.align.get(i).copied().flatten().unwrap_or(Alignment::CENTER_LEFT);
        if t.sortable.contains(&i) {
            let dir = t.sort.and_then(|(cc, d)| if cc == i { Some(d) } else { None });
            let on_tap: Option<Rc<dyn Fn()>> = t.on_sort.clone().map(|f| {
                let next = match t.sort {
                    Some((cc, SortDir::Asc)) if cc == i => SortDir::Desc,
                    _ => SortDir::Asc,
                };
                let cb: Rc<dyn Fn()> = Rc::new(move || f(i, next));
                cb
            });
            // The sortable header brings its own background + hover; just clip it.
            hrow.push(
                clip_rect(component_props(
                    render_sort_header,
                    SortHeaderProps {
                        label: h.clone(),
                        dir,
                        on_tap,
                        color: header_color,
                        size: header_size,
                        weight: header_weight,
                        pad: t.cell_padding,
                        align: alignment,
                        asc_icon: t.sort_asc_icon,
                        desc_icon: t.sort_desc_icon,
                        idle_icon: t.sort_idle_icon,
                        idle_visible: t.sort_idle_visible,
                        icon_color: t.sort_icon_color,
                        icon_size: t.sort_icon_size,
                    },
                ))
                .into_widget(),
            );
        } else {
            let label = Padding::new(
                t.cell_padding,
                Align::new(
                    alignment,
                    text(h.clone()).size(header_size).weight(header_weight).color(header_color),
                ),
            );
            hrow.push(header_slot(header_bg, header_border, clip_rect(label).into_widget()));
        }
    }
    grid_rows.push(hrow);

    // --- data rows --------------------------------------------------------
    for (r, cells) in t.rows.iter().enumerate() {
        let striped = t.striped && r % 2 == 1;
        let mut drow: Vec<AnyWidget> = Vec::new();
        if sel {
            let checked = t.selection.contains(&r);
            let selection = t.selection.clone();
            let on_selection = t.on_selection.clone();
            let toggle: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(f) = on_selection.clone() {
                    let mut next: Vec<usize> = selection.iter().copied().filter(|&v| v != r).collect();
                    if !selection.contains(&r) {
                        next.push(r);
                        next.sort_unstable();
                    }
                    f(&next);
                }
            });
            let cb = center(checkbox(checked).on_changed(move || toggle())).into_widget();
            drow.push(data_slot(r, striped, t.row_hover, hovered, cb));
        }
        // `i` indexes align/overflow/cells/caps together, so a range loop is clearest.
        #[allow(clippy::needless_range_loop)]
        for i in 0..n_user {
            let alignment = t.align.get(i).copied().flatten().unwrap_or(Alignment::CENTER_LEFT);
            let mode = t.overflow.get(i).copied().flatten().unwrap_or(t.overflow_default);
            let content: AnyWidget = match cells.get(i) {
                Some(Cell::Text(s)) => {
                    let txt = text(s.clone()).size(cell_size).color(cell_color);
                    match mode {
                        CellOverflow::Wrap => txt,
                        CellOverflow::Ellipsis => txt.max_lines(1).ellipsis().soft_wrap(false),
                        CellOverflow::Clip => txt.max_lines(1).soft_wrap(false),
                    }
                    .into_widget()
                }
                Some(Cell::Widget(w)) => w.clone(),
                None => gap_w(0.0).into_widget(),
            };
            // A `Max` column caps its content width so its intrinsic width is bounded.
            let content = match caps[i] {
                Some(px) => constrained_box(
                    BoxConstraints {
                        min_width: 0.0,
                        max_width: px,
                        min_height: 0.0,
                        max_height: f64::INFINITY,
                    },
                    content,
                )
                .into_widget(),
                None => content,
            };
            let inner = clip_rect(Padding::new(t.cell_padding, Align::new(alignment, content))).into_widget();
            drow.push(data_slot(r, striped, t.row_hover, hovered, inner));
        }
        grid_rows.push(drow);
    }

    let grid = layout_table(grid_rows).column_widths(specs).stretch_rows(true).divider(c.border, 1.0);

    // Content-sized columns scroll horizontally when they don't fit; a Flex/Fraction
    // column instead fills the available width, so no horizontal scroll.
    let grid_widget: AnyWidget =
        if any_flex { grid.into_widget() } else { SingleChildScrollView::horizontal(grid).into_widget() };

    let mut body: Vec<AnyWidget> = vec![grid_widget];
    if n_rows == 0
        && let Some(es) = t.empty_state.clone()
    {
        body.push(Container::new().padding(EdgeInsets::all(24.0)).child(center(es)).into_widget());
    }
    if let Some(footer) = t.footer.clone() {
        body.push(Container::new().color(c.border).height(1.0).into_widget());
        body.push(footer);
    }

    let content =
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min);
    styled(content, merged).into_widget()
}
