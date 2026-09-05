//! [`stream_builder`]: a widget that re-renders on each [`Channel`] send, passing the
//! latest value to its builder (`None` before the first send). Driven headlessly.

use std::cell::RefCell;

use pebbles_core::{Channel, IntoWidget, Ui, channel, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, stream_builder, text};

thread_local! {
    static CH: RefCell<Option<Channel<u32>>> = const { RefCell::new(None) };
    static SEEN: RefCell<Vec<Option<u32>>> = const { RefCell::new(Vec::new()) };
}

fn ch() -> Channel<u32> {
    CH.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(channel::<u32>());
        }
        c.as_ref().unwrap().clone()
    })
}

fn root() -> impl IntoWidget {
    stream_builder(ch(), |v| {
        SEEN.with(|s| s.borrow_mut().push(v));
        text("x")
    })
}

#[test]
fn stream_builder_rebuilds_on_send() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    CH.with(|c| *c.borrow_mut() = None);
    SEEN.with(|s| s.borrow_mut().clear());
    let _ = ch(); // create the channel at app scope, before mount

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(200.0, 100.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);

    // First build: no message yet.
    assert_eq!(SEEN.with(|s| s.borrow().clone()), vec![None], "builds with None before any send");

    // A send re-renders the builder with the latest value.
    ch().send(7);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    assert_eq!(
        SEEN.with(|s| s.borrow().clone()),
        vec![None, Some(7)],
        "the send rebuilt the widget with the new value"
    );
}
