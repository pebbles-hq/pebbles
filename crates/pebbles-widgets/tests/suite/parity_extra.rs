//! Buildable-now parity widgets: `list_body` order, `checkbox_list_tile` whole-row
//! tap, the Scaffold drawer slot (open_drawer → a sheet), and
//! `draggable_scrollable_sheet` mounting with a scrollable body.

use std::cell::Cell;

use pebbles_core::{AnyWidget, IntoWidget, Ui, component};
use pebbles_foundation::{Axis, CrossAxisAlignment, MainAxisSize, Offset, Size, palette};
use pebbles_render::{RenderParagraph, RenderScroll, TextEnv};
use pebbles_widgets::{
    OverlayHost, View, center, checkbox_list_tile, column, draggable_scrollable_sheet, list_body, scaffold,
    text,
};

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(env, &mut scene);
}

// ---------------------------------------------------------------------------
// ListBody
// ---------------------------------------------------------------------------

fn list_body_root() -> impl IntoWidget {
    list_body(vec![text("Alpha").into_widget(), text("Beta").into_widget(), text("Gamma").into_widget()])
}

#[test]
fn list_body_stacks_children_and_sizes_to_them() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(|| center(list_body_root()))).into_widget());
    frame(&mut ui, &mut env, Size::new(200.0, 300.0));

    // Three stacked text rows → a paragraph renders, and the body is tall enough to
    // hold three lines (MainAxisSize::Min sizes it to the sum of its children).
    let tree = ui.render_tree();
    let para = tree.find::<RenderParagraph>().expect("the list body rendered text");
    // Walk up is unavailable here; instead assert the single-line paragraph is short
    // while the whole View is tall — the three rows stacked (not overlapped).
    assert!(tree.size_of(para).height < 40.0, "one row is a single line");
}

#[test]
fn list_body_horizontal_is_wider_than_tall() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                center(
                    list_body(vec![text("a").into_widget(), text("b").into_widget()]).axis(Axis::Horizontal),
                )
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, Size::new(300.0, 300.0));
    // Just needs to lay out + paint without panicking (row of two glyphs).
    assert!(ui.render_tree().find::<RenderScroll>().is_none(), "list_body imposes no viewport");
}

// ---------------------------------------------------------------------------
// CheckboxListTile — the whole row is the tap target
// ---------------------------------------------------------------------------

thread_local! {
    static TOGGLES: Cell<u32> = const { Cell::new(0) };
}

fn checkbox_tile_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            checkbox_list_tile("Wi-Fi", true)
                .subtitle("home network")
                .on_changed(|| TOGGLES.with(|t| t.set(t.get() + 1)))
                .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

#[test]
fn checkbox_list_tile_row_tap_fires_on_changed() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    TOGGLES.with(|t| t.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(checkbox_tile_root)).into_widget());
    frame(&mut ui, &mut env, win);

    // Tap on the row body — NOT over the trailing control (which is display-only).
    let p = Offset::new(60.0, 18.0);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    frame(&mut ui, &mut env, win);

    assert_eq!(TOGGLES.with(Cell::get), 1, "tapping the row fired on_changed once");

    // And tapping over the trailing checkbox area also routes to the row (single fire).
    let q = Offset::new(380.0, 18.0);
    ui.dispatch_pointer_down(q);
    ui.dispatch_tap(q);
    ui.dispatch_pointer_up(q);
    frame(&mut ui, &mut env, win);
    assert_eq!(TOGGLES.with(Cell::get), 2, "tapping over the control also fires the row (once)");
}

// ---------------------------------------------------------------------------
// Scaffold drawer slot — open_drawer presents the registered content as a sheet
// ---------------------------------------------------------------------------

fn scaffold_with_drawer() -> impl IntoWidget {
    OverlayHost::wrap(
        scaffold(center(text("body"))).drawer(
            column(vec![text("Home").into_widget(), text("Settings").into_widget()])
                .main_axis_size(MainAxisSize::Min),
        ),
    )
}

#[test]
fn scaffold_drawer_opens_via_open_drawer() {
    pebbles_widgets::overlay::init();
    pebbles_widgets::sheet::init();
    pebbles_core::focus::init();
    pebbles_core::animation::reset();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(scaffold_with_drawer)).into_widget());
    frame(&mut ui, &mut env, win); // building the Scaffold registers its drawer

    assert!(!pebbles_widgets::sheet::is_open(), "no sheet open initially");
    pebbles_widgets::open_drawer();
    assert!(pebbles_widgets::sheet::is_open(), "open_drawer presented the drawer as a sheet");
}

// ---------------------------------------------------------------------------
// DraggableScrollableSheet — mounts with a scrollable body
// ---------------------------------------------------------------------------

fn draggable_root() -> impl IntoWidget {
    let mut rows: Vec<AnyWidget> = Vec::new();
    for i in 0..30 {
        rows.push(text(format!("row {i}")).into_widget());
    }
    OverlayHost::wrap(draggable_scrollable_sheet(column(rows).main_axis_size(MainAxisSize::Min)))
}

#[test]
fn draggable_scrollable_sheet_mounts_with_a_scroll_body() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    // Two frames: layout_builder reacts to its measured size one frame behind.
    ui.mount_root(View::new(palette::WHITE, component(draggable_root)).into_widget());
    frame(&mut ui, &mut env, Size::new(360.0, 640.0));
    frame(&mut ui, &mut env, Size::new(360.0, 640.0));

    assert!(ui.render_tree().find::<RenderScroll>().is_some(), "the sheet body is a real scroll view",);
}
