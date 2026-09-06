//! A [`button`] must center its label/icon when a parent stretches it wider than its
//! content (e.g. a button in a `CrossAxisAlignment::Stretch` column) — while still
//! shrink-wrapping to its content when it isn't stretched. Guards the regression where
//! stretched buttons left-aligned their icon+label.

use pebbles_core::IntoWidget;
use pebbles_foundation::{CrossAxisAlignment, MainAxisSize};
use pebbles_render::{RenderParagraph, lucide};
use pebbles_testing::Harness;
use pebbles_widgets::{button, column};

/// The (center_x, width) of the widest paragraph in the tree — the button's text label
/// (wider than the icon glyph).
fn widest_label(h: &Harness) -> (f64, f64) {
    h.find_all::<RenderParagraph>()
        .into_iter()
        .map(|id| {
            let off = h.ui.render_tree().absolute_offset(id);
            let sz = h.size_of(id);
            (off.x + sz.width / 2.0, sz.width)
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("a label paragraph is present")
}

fn stretched() -> impl IntoWidget {
    // Stretch makes the button fill the 400px window width — wider than its content.
    column(vec![button("Save changes").leading(lucide::CHECK).on_pressed(|| {}).into_widget()])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
}

fn unstretched() -> impl IntoWidget {
    // Start does NOT stretch the button — it must hug its content on the left.
    column(vec![button("Save changes").leading(lucide::CHECK).on_pressed(|| {}).into_widget()])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
}

#[test]
fn stretched_button_centers_its_content() {
    let mut h = Harness::new().window(400.0, 200.0);
    h.mount(stretched);
    h.draw();
    let (center_x, _) = widest_label(&h);
    assert!(
        (center_x - 200.0).abs() < 40.0,
        "a stretched button's label should be centered (~200 in a 400px window), got {center_x}"
    );
}

#[test]
fn unstretched_button_shrink_wraps_to_content() {
    let mut h = Harness::new().window(800.0, 200.0);
    h.mount(unstretched);
    h.draw();
    let (center_x, width) = widest_label(&h);
    // The button hugs its content at the left; it must NOT balloon to the full window
    // (which would center the label near x≈400). Guards against a fill-when-loose fix.
    assert!(center_x < 150.0, "an unstretched button should hug its content at the left, got {center_x}");
    assert!(width < 300.0, "label width sanity check, got {width}");
}
