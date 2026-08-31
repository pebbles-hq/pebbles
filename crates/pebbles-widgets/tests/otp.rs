//! Input OTP: typing digits fills cells left-to-right, Backspace deletes, and
//! `on_complete` fires once all `n` slots are filled. Driven headlessly through the
//! focus/editor channel (autofocus → dispatch_key).

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, input_otp};

thread_local! {
    static CHANGED: RefCell<String> = const { RefCell::new(String::new()) };
    static COMPLETE: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn root() -> impl IntoWidget {
    OverlayHost::wrap(
        input_otp(6)
            .group_size(3)
            .autofocus()
            .on_changed(|s| CHANGED.with(|c| *c.borrow_mut() = s.to_string()))
            .on_complete(|s| COMPLETE.with(|c| *c.borrow_mut() = Some(s.to_string()))),
    )
}

#[test]
fn typing_fills_cells_and_fires_on_complete() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    CHANGED.with(|c| c.borrow_mut().clear());
    COMPLETE.with(|c| *c.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(400.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui); // autofocus grabs the editor

    // Type five digits — not complete yet.
    ui.dispatch_key(KeyInput::Insert("12345".to_string()));
    frame(&mut ui);
    assert_eq!(CHANGED.with(|c| c.borrow().clone()), "12345");
    assert_eq!(COMPLETE.with(|c| c.borrow().clone()), None, "not complete at 5/6");

    // Backspace then type the last two — completing at 6.
    ui.dispatch_key(KeyInput::Backspace);
    frame(&mut ui);
    assert_eq!(CHANGED.with(|c| c.borrow().clone()), "1234");
    ui.dispatch_key(KeyInput::Insert("567".to_string())); // caps at 6 → "123456"
    frame(&mut ui);
    assert_eq!(CHANGED.with(|c| c.borrow().clone()), "123456", "capped at len");
    assert_eq!(COMPLETE.with(|c| c.borrow().clone()), Some("123456".to_string()), "on_complete fired");
}
