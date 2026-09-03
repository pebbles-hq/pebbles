//! D2: the reactive text-direction signal — a component that reads `text_direction()`
//! subscribes and re-renders when the direction toggles (mirrors the theme test).

use std::cell::Cell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, TextDirection, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, set_text_direction, text, text_direction};

thread_local! {
    static SEEN: Cell<Option<TextDirection>> = const { Cell::new(None) };
}

fn probe() -> impl IntoWidget {
    let dir = text_direction(); // reading subscribes this component
    SEEN.with(|s| s.set(Some(dir)));
    text(format!("{dir:?}"))
}

#[test]
fn toggling_direction_re_renders_subscribers() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    pebbles_widgets::text_direction::init();
    set_text_direction(TextDirection::Ltr);
    SEEN.with(|s| s.set(None));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.make_current();
    ui.mount_root(View::new(palette::WHITE, component(probe)).into_widget());
    ui.layout(&mut env, Size::new(200.0, 80.0));
    assert_eq!(SEEN.with(Cell::get), Some(TextDirection::Ltr), "reads LTR initially");

    // Toggle → the subscriber re-runs.
    set_text_direction(TextDirection::Rtl);
    ui.rebuild_if_dirty();
    assert_eq!(SEEN.with(Cell::get), Some(TextDirection::Rtl), "subscriber re-ran on toggle");

    set_text_direction(TextDirection::Ltr); // reset for other tests in this binary
}
