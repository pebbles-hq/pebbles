//! [`Table`] interaction coverage: sortable headers cycle and report `(col, dir)`;
//! the selection column and header select-all report selections; an empty table
//! with an empty state and a striped table both paint without panicking.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component, create_signal};
use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets, Offset, Size, palette};
use pebbles_render::{Border, RenderDecoratedBox, TextEnv};
use pebbles_widgets::{avatar, badge, cell, column, empty, muted, SortDir, style, table, text, View};

thread_local! {
    static SORT_EVENTS: RefCell<Vec<(usize, SortDir)>> = const { RefCell::new(Vec::new()) };
    static SELECTION: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

fn sortable_view() -> impl IntoWidget {
    let sort = create_signal(None::<(usize, SortDir)>);
    let mut t = table(vec!["Name".into(), "Role".into(), "Status".into()])
        .row(vec!["Andres", "Engineer", "Away"])
        .row(vec!["Reyco", "Lead", "Active"])
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
            column(vec![component(view).into_widget()])
                .cross_axis_alignment(CrossAxisAlignment::Stretch),
        )
        .into_widget(),
    );
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
        ui.paint(&mut scene);
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
    assert_eq!(
        events(),
        vec![(0, SortDir::Asc), (0, SortDir::Desc)],
        "second click flips to descending"
    );

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
        ui.paint(&mut scene);
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
    ui.paint(&mut scene);
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
    ui.paint(&mut scene);

    // The surface style wraps the whole table — its DecoratedBox mounts first,
    // so it is the first decorated box in the tree.
    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("a decorated surface");
    let deco = tree.object_ref(rid).downcast_ref::<RenderDecoratedBox>().expect("decorated box");
    assert_eq!(deco.decoration.color, Some(palette::BLUE), "style background lands on the table");
    assert_eq!(deco.decoration.radius, pebbles_render::BorderRadius::all(0.0), "sharp radius lands on the table");
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
    ui.paint(&mut scene);

    // The table laid out taller than a bare two-row grid (footer present).
    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("surface");
    assert!(tree.size_of(rid).height > 60.0, "footer + rows + header laid out");
}
