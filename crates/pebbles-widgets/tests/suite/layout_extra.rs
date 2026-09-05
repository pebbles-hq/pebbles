//! Headless layout tests for the long-tail layout widgets: `Offstage` collapses to
//! zero, `RotatedBox` swaps its extent on an odd turn, and `Table` negotiates column
//! widths (fixed + flex).

use std::cell::Cell;

use pebbles_core::{AnyWidget, IntoWidget, Ui};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{Affine, RenderConstrainedBox, RenderOffstage, RenderRotatedBox, TextEnv};
use pebbles_widgets::{
    SizedBox, TableColumnWidth, View, center, flow, gesture_detector, layout_table, offstage, rotated_box,
};

fn mount(root: impl IntoWidget) -> (Ui, TextEnv) {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    // `center` hands the subject loose constraints so it can size to its content
    // (View alone would stretch it to fill the window).
    ui.mount_root(View::new(palette::WHITE, center(root)).into_widget());
    ui.layout(&mut text, Size::new(200.0, 200.0));
    (ui, text)
}

#[test]
fn offstage_true_collapses_to_zero() {
    let (ui, _t) = mount(offstage(true, SizedBox::new(Some(40.0), Some(40.0), None)));
    let id = ui.render_tree().find::<RenderOffstage>().expect("offstage present");
    assert_eq!(ui.render_tree().size_of(id), Size::new(0.0, 0.0), "offstage=true takes no space");
}

#[test]
fn offstage_false_takes_the_child_size() {
    let (ui, _t) = mount(offstage(false, SizedBox::new(Some(40.0), Some(40.0), None)));
    let id = ui.render_tree().find::<RenderOffstage>().expect("offstage present");
    assert_eq!(ui.render_tree().size_of(id), Size::new(40.0, 40.0), "offstage=false is transparent");
}

#[test]
fn rotated_box_odd_turn_swaps_width_and_height() {
    let (ui, _t) = mount(rotated_box(1, SizedBox::new(Some(40.0), Some(20.0), None)));
    let id = ui.render_tree().find::<RenderRotatedBox>().expect("rotated box present");
    // A single quarter turn rotates the layout box: 40×20 → 20×40.
    assert_eq!(ui.render_tree().size_of(id), Size::new(20.0, 40.0));
}

#[test]
fn table_negotiates_fixed_and_flex_columns() {
    // Cells take their column's width (tight) and a fixed 20 height.
    let cell = || SizedBox::new(None, Some(20.0), None).into_widget();
    let rows: Vec<Vec<AnyWidget>> = vec![vec![cell(), cell()], vec![cell(), cell()]];
    let (ui, _t) = mount(
        layout_table(rows).column_widths(vec![TableColumnWidth::Fixed(50.0), TableColumnWidth::Flex(1.0)]),
    );

    let t = ui.render_tree();
    let mut widths: Vec<f64> =
        t.find_all::<RenderConstrainedBox>().into_iter().map(|id| t.size_of(id).width).collect();
    widths.sort_by(f64::total_cmp);
    // In a 200-wide table: fixed column = 50 (×2 rows), flex column = 150 (×2 rows).
    assert_eq!(widths, vec![50.0, 50.0, 150.0, 150.0]);
}

thread_local! {
    static FLOW_HIT: Cell<i64> = const { Cell::new(0) };
}

#[test]
fn flow_child_hit_tests_at_its_transformed_position() {
    FLOW_HIT.with(|c| c.set(0));
    // A single 40×40 tappable child, flowed to (100, 100) by an affine transform.
    let child = gesture_detector(SizedBox::new(Some(40.0), Some(40.0), None))
        .on_tap(|| FLOW_HIT.with(|c| c.set(c.get() + 1)))
        .into_widget();
    let flowed =
        flow(vec![child]).size(|c| c.biggest()).transform(|_, _, _| Affine::translate((100.0, 100.0)));

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, flowed).into_widget());
    ui.layout(&mut text, Size::new(200.0, 200.0));

    // Miss: the child's LAYOUT position (0,0) is empty — the transform moved it.
    assert!(!ui.dispatch_tap(Offset::new(20.0, 20.0)), "nothing at the untransformed origin");
    assert_eq!(FLOW_HIT.with(Cell::get), 0);

    // Hit: the child lives at its TRANSFORMED position (100..140), proving the
    // parent-applied per-child transform flows into hit-testing.
    assert!(ui.dispatch_tap(Offset::new(120.0, 120.0)), "the flowed child is hittable where it paints");
    assert_eq!(FLOW_HIT.with(Cell::get), 1);
}
