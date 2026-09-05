//! Window metrics + mobile widgets: `media_query()` reports size / orientation / zero
//! insets (desktop), `safe_area` is a passthrough when insets are zero, and
//! `orientation_builder` calls its builder.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{EdgeInsets, Size, palette};
use pebbles_render::{RenderConstrainedBox, TextEnv};
use pebbles_widgets::{
    Orientation, SizedBox, View, center, media_query, orientation_builder, safe_area, text,
};

thread_local! {
    static ORI: RefCell<Vec<Orientation>> = const { RefCell::new(Vec::new()) };
}

#[test]
fn media_query_reports_size_and_orientation() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    pebbles_widgets::overlay::set_window_size(800.0, 600.0);
    let m = media_query();
    assert_eq!(m.size, Size::new(800.0, 600.0), "size tracks the window");
    assert_eq!(m.orientation, Orientation::Landscape, "wider than tall = landscape");
    assert_eq!(m.padding, EdgeInsets::ZERO, "no safe-area insets on desktop");
    assert_eq!(m.view_insets, EdgeInsets::ZERO, "no keyboard insets on desktop");
    assert_eq!(m.device_pixel_ratio, 1.0);

    pebbles_widgets::overlay::set_window_size(400.0, 900.0);
    assert_eq!(media_query().orientation, Orientation::Portrait, "taller than wide = portrait");
    assert!(media_query().is_portrait());
}

#[test]
fn safe_area_is_a_passthrough_when_insets_are_zero() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| center(safe_area(SizedBox::new(Some(40.0), Some(40.0), None)))),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 200.0));

    // Desktop insets are zero → the child keeps its exact size (no padding added).
    let t = ui.render_tree();
    let id = t.find::<RenderConstrainedBox>().expect("the SizedBox is present");
    assert_eq!(t.size_of(id), Size::new(40.0, 40.0), "safe_area added no inset on desktop");
}

#[test]
fn orientation_builder_calls_its_builder() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    ORI.with(|o| o.borrow_mut().clear());

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                orientation_builder(|o| {
                    ORI.with(|v| v.borrow_mut().push(o));
                    text("x")
                })
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));

    // The builder ran with an orientation (bounds are one frame behind, so the exact
    // value depends on the shell publishing bounds — here we assert it built at all).
    assert!(!ORI.with(|o| o.borrow().is_empty()), "the builder was called with an orientation");
}
