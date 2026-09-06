//! [`Table`] interaction coverage: sortable headers cycle and report `(col, dir)`;
//! the selection column and header select-all report selections; an empty table
//! with an empty state and a striped table both paint without panicking.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component, create_signal};
use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets, Offset, Size, palette};
use pebbles_render::{Border, RenderClipRRect, RenderDecoratedBox, TextEnv};
use pebbles_widgets::{
    CellOverflow, ColumnWidth, SortDir, View, avatar, badge, cell, column, empty, muted, style, table, text,
};

thread_local! {
    static SORT_EVENTS: RefCell<Vec<(usize, SortDir)>> = const { RefCell::new(Vec::new()) };
    static SELECTION: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

fn sortable_view() -> impl IntoWidget {
    let sort = create_signal(None::<(usize, SortDir)>);
    let mut t = table(vec!["Name".into(), "Role".into(), "Status".into()])
        .row(vec!["Andres", "Engineer", "Away"])
        .row(vec!["Reyco", "Lead", "Active"])
        .column_width_all(ColumnWidth::Flex(1.0)) // equal columns for stable tap coords
        .sortable(0)
        .sortable(1)
        .sortable(2);
    if let Some((c, d)) = sort.get() {
        t = t.sort(c, d);
    }
    t.on_sort(move |c, d| {
        SORT_EVENTS.with(|e| e.borrow_mut().push((c, d)));
        sort.set(Some((c, d)));
    })
}

fn selectable_view() -> impl IntoWidget {
    let selected = create_signal(Vec::<usize>::new());
    table(vec!["Name".into(), "Role".into()])
        .row(vec!["Andres", "Engineer"])
        .row(vec!["Reyco", "Lead"])
        .row(vec!["Joseph", "Engineer"])
        .column_width_all(ColumnWidth::Flex(1.0)) // equal columns for stable tap coords
        .selectable()
        .selection(selected.get())
        .on_selection(move |s| {
            SELECTION.with(|c| *c.borrow_mut() = s.to_vec());
            selected.set(s.to_vec());
        })
}

fn setup<W: IntoWidget + 'static>(view: fn() -> W) -> (Ui, TextEnv, Size) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(500.0, 400.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![component(view).into_widget()]).cross_axis_alignment(CrossAxisAlignment::Stretch),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty(); // the table is now a component — build it before layout
    ui.layout(&mut text_env, window);
    (ui, text_env, window)
}

fn tap(ui: &mut Ui, x: f64, y: f64) {
    let p = Offset::new(x, y);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
}

#[test]
fn sortable_headers_cycle_and_report() {
    SORT_EVENTS.with(|e| e.borrow_mut().clear());
    let (mut ui, mut text_env, window) = setup(sortable_view);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut text_env, &mut scene);
    };
    frame(&mut ui);
    let events = || SORT_EVENTS.with(|e| e.borrow().clone());

    // Three equal header cells across 500px: column 0 spans ~0..166, column 1
    // ~166..333. The header row is ~36px tall, centered on y ≈ 17.
    tap(&mut ui, 83.0, 17.0);
    frame(&mut ui);
    assert_eq!(events(), vec![(0, SortDir::Asc)], "first click sorts ascending");

    tap(&mut ui, 83.0, 17.0);
    frame(&mut ui);
    assert_eq!(events(), vec![(0, SortDir::Asc), (0, SortDir::Desc)], "second click flips to descending");

    tap(&mut ui, 83.0, 17.0);
    frame(&mut ui);
    assert_eq!(
        events(),
        vec![(0, SortDir::Asc), (0, SortDir::Desc), (0, SortDir::Asc)],
        "third click cycles back to ascending"
    );

    tap(&mut ui, 250.0, 17.0);
    frame(&mut ui);
    assert_eq!(*events().last().unwrap(), (1, SortDir::Asc), "a new column starts ascending");
}

#[test]
fn selection_checkboxes_and_select_all_report() {
    SELECTION.with(|c| c.borrow_mut().clear());
    let (mut ui, mut text_env, window) = setup(selectable_view);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut text_env, &mut scene);
    };
    frame(&mut ui);
    let selection = || SELECTION.with(|c| c.borrow().clone());

    // Header select-all: the checkbox column is 40px wide, its box centered on
    // x ≈ 20; the header row is ~36px tall, centered on y ≈ 17.
    tap(&mut ui, 20.0, 17.0);
    frame(&mut ui);
    assert_eq!(selection(), vec![0, 1, 2], "select-all selects every row");

    // Row 0 checkbox: below the header + 1px separator, centered on y ≈ 54.
    tap(&mut ui, 20.0, 54.0);
    frame(&mut ui);
    assert_eq!(selection(), vec![1, 2], "tapping row 0 deselects it");

    tap(&mut ui, 20.0, 17.0);
    frame(&mut ui);
    assert_eq!(selection(), vec![0, 1, 2], "select-all re-selects all rows");
}

#[test]
fn empty_state_and_striped_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(500.0, 400.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    table(vec!["A".into(), "B".into()])
                        .selectable()
                        .striped(true)
                        .row(vec!["x", "y"])
                        .row(vec!["p", "q"])
                        .into_widget(),
                    table(vec!["Only".into()]).empty(empty().title("Nothing here")).into_widget(),
                ])
            })
            .into_widget(),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut text_env, window);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut text_env, &mut scene);
}

// A long single value that must wrap over several lines in a narrow (fixed) column.
const LONG: &str = "This is an intentionally very long product description that must wrap across several lines \
     when the column is narrow instead of overflowing into the next column.";

fn table_size(ui: &Ui) -> Size {
    let tree = ui.render_tree();
    let rid = tree.find::<pebbles_render::RenderTable>().expect("a table grid");
    tree.size_of(rid)
}

#[test]
fn columns_size_to_content_and_overflow_scrolls() {
    // All-`Auto` columns size to their own content — a wide value makes its column
    // wide while a short one stays narrow (not equal-width). When the columns together
    // exceed the viewport, the grid is wider than the window and gets a scroll view.
    let (ui, _e, _w) = setup(|| table(vec!["Desc".into(), "N".into()]).row(vec![LONG, "1"]));
    let grid = table_size(&ui);
    assert!(grid.width > 500.0, "content-sized columns overflow the 500px window (w={})", grid.width);
    assert!(
        ui.render_tree().find::<pebbles_render::RenderScroll>().is_some(),
        "an overflowing table is wrapped in a horizontal scroll view",
    );
    assert!(
        ui.render_tree().find::<RenderClipRRect>().is_some(),
        "every cell is clipped to its column, so content can't bleed into a neighbor",
    );
}

#[test]
fn flex_column_fills_width_without_scrolling() {
    // A `Flex` column makes the grid fill the available width instead of scrolling.
    let (ui, _e, _w) = setup(|| {
        table(vec!["Desc".into(), "N".into()]).row(vec![LONG, "1"]).column_width(0, ColumnWidth::Flex(1.0))
    });
    assert!(
        ui.render_tree().find::<pebbles_render::RenderScroll>().is_none(),
        "a flex column fills the width, so there's no horizontal scroll",
    );
    let grid = table_size(&ui);
    assert!((grid.width - 500.0).abs() < 1.0, "the grid fills the 500px window (w={})", grid.width);
}

#[test]
fn fixed_column_wraps_or_ellipsizes() {
    // A fixed-width column forces the long value to lay out inside 200px: `Wrap` grows
    // the row over several lines; `Ellipsis` keeps it to one.
    let (ui_wrap, _e, _w) = setup(|| {
        table(vec!["Desc".into(), "N".into()]).row(vec![LONG, "1"]).column_width(0, ColumnWidth::Fixed(200.0))
    });
    let wrap_h = table_size(&ui_wrap).height;

    let (ui_ellipsis, _e2, _w2) = setup(|| {
        table(vec!["Desc".into(), "N".into()])
            .row(vec![LONG, "1"])
            .column_width(0, ColumnWidth::Fixed(200.0))
            .overflow(0, CellOverflow::Ellipsis)
    });
    let ellipsis_h = table_size(&ui_ellipsis).height;

    assert!(
        wrap_h > ellipsis_h + 40.0,
        "Wrap grows the row (h={wrap_h}) while Ellipsis stays one line (h={ellipsis_h})",
    );
}

#[test]
fn surface_style_lands_on_the_table() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                table(vec!["A".into(), "B".into()])
                    .row(vec!["1", "2"])
                    .style(style().background(palette::BLUE).radius_all(0.0))
                    .into_widget()
            }),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);

    // The surface style wraps the whole table — its DecoratedBox mounts first,
    // so it is the first decorated box in the tree.
    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("a decorated surface");
    let deco = tree.object_ref(rid).downcast_ref::<RenderDecoratedBox>().expect("decorated box");
    assert_eq!(deco.decoration.color, Some(palette::BLUE), "style background lands on the table");
    assert_eq!(
        deco.decoration.radius,
        pebbles_render::BorderRadius::all(0.0),
        "sharp radius lands on the table"
    );
}

#[test]
fn rich_cells_footer_and_customizations_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 400.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    table(vec!["User".into(), "Role".into()])
                        .row(vec![cell(avatar("RS")), cell(badge("Lead"))])
                        .row(vec![cell(text("Andres")), "Engineer".into()])
                        .sortable(0)
                        .selectable()
                        .striped(true)
                        .row_hover(true)
                        .cell_padding(EdgeInsets::symmetric(8.0, 6.0))
                        .align(1, Alignment::CENTER_RIGHT)
                        .header_style(style().background(palette::VIOLET).color(palette::WHITE))
                        .footer(muted("footer slot"))
                        .selection_column_width(56.0)
                        .style(style().border(Border::new(palette::BLUE, 1.0)).radius_all(4.0))
                        .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);

    // The table laid out taller than a bare two-row grid (footer present).
    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("surface");
    assert!(tree.size_of(rid).height > 60.0, "footer + rows + header laid out");
}
