//! F1: the `#[component]` macro expands to a working component + ctor. Lives in the
//! gallery's test target because the macro's expansion references `::pebbles::…`, so it
//! needs the umbrella crate in scope (which the gallery depends on).

use std::cell::Cell;

use pebbles::prelude::*;
use pebbles::render::TextEnv;

thread_local! {
    static RENDERED: Cell<u32> = const { Cell::new(0) };
    static SEEN_LABEL: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[component]
fn labelled_box(width: f64, label: String) -> Element {
    RENDERED.with(|c| c.set(c.get() + 1));
    SEEN_LABEL.with(|s| *s.borrow_mut() = label.clone());
    let _ = label;
    center(SizedBox::new(Some(width), Some(10.0), None)).into_widget()
}

#[test]
fn component_macro_builds_a_working_ctor() {
    RENDERED.with(|c| c.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    // The generated ctor takes the args by value and returns an `Element`.
    ui.mount_root(View::new(palette::WHITE, labelled_box(40.0, "hi".to_string())).into_widget());
    ui.layout(&mut env, Size::new(200.0, 200.0));

    assert_eq!(RENDERED.with(Cell::get), 1, "the macro component rendered once");
    assert_eq!(SEEN_LABEL.with(|s| s.borrow().clone()), "hi", "args reach the body by value");
}
