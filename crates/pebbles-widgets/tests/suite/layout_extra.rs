//! Headless layout tests for the long-tail layout widgets: `Offstage` collapses to
//! zero, `RotatedBox` swaps its extent on an odd turn, and `Table` negotiates column
//! widths (fixed + flex).

use pebbles_core::{AnyWidget, IntoWidget, Ui};
use pebbles_foundation::{Size, palette};
use pebbles_render::{RenderConstrainedBox, RenderOffstage, RenderRotatedBox, TextEnv};
use pebbles_widgets::{SizedBox, TableColumnWidth, View, center, layout_table, offstage, rotated_box};

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
