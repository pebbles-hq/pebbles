//! Reactive ambient [`TextDirection`] (D2) — mirrors [`theme`](fn@crate::theme). Apps read
//! [`text_direction`] (subscribing) to re-render on a direction change; the setter also
//! updates the render-layer global ([`pebbles_render::set_text_direction`]) that layout
//! consults so Rows flip and paragraphs pick the right bidi base.
//!
//! v1 scope is a **single global direction**; physical `EdgeInsets` stay physical
//! (logical start/end insets, `AlignmentDirectional`, and per-subtree overrides are
//! out — roadmap §J).

use std::cell::RefCell;

use pebbles_core::{Signal, create_root_signal};
use pebbles_foundation::TextDirection;

thread_local! {
    static DIR: RefCell<Option<Signal<TextDirection>>> = const { RefCell::new(None) };
}

fn signal() -> Signal<TextDirection> {
    DIR.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(TextDirection::Ltr)))
}

/// Create the direction signal (call once at startup, like [`theme::init`](crate::theme::init))
/// and sync the render-layer global to its initial value.
pub fn init() {
    let sig = signal();
    pebbles_render::set_text_direction(sig.peek());
}

/// The current ambient text direction — reading it **subscribes** the caller, so a
/// component re-renders when the direction toggles.
pub fn text_direction() -> TextDirection {
    signal().get()
}

/// Set the global text direction: updates the render-layer global (so the next layout
/// flips Rows / bidi) and the reactive signal (so subscribers re-render).
pub fn set_text_direction(dir: TextDirection) {
    pebbles_render::set_text_direction(dir);
    signal().set(dir);
}
