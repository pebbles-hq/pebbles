//! Cross-window IPC — but with **no serialization**, because every window runs on
//! the one thread sharing the one reactive runtime. A [`Channel`] is just a global
//! signal holding the latest message; send from any window, react in any other.
//!
//! ```ignore
//! // At app scope (so it's shared, not owned by a component):
//! static SELECTION: Channel<u32> = ...;  // e.g. via a global accessor
//!
//! // In the inspector window's component — re-renders on every send:
//! let picked = SELECTION.latest();
//!
//! // From the main window, on click:
//! SELECTION.send(42);
//! ```
//!
//! Create channels at **app scope** (like a route signal), not inside a component —
//! a channel made inside a component would be local to that window.

use crate::reactive::{Signal, create_effect, create_signal};

/// A typed cross-window channel: the latest message sent on it, read reactively.
/// `Copy` (it's a signal handle), so capture it freely into window closures.
pub struct Channel<T: 'static> {
    signal: Signal<Option<T>>,
}

impl<T: 'static> Clone for Channel<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T: 'static> Copy for Channel<T> {}

/// Create a cross-window [`Channel`]. Call at app scope so every window shares it.
pub fn channel<T: 'static + Clone>() -> Channel<T> {
    Channel { signal: create_signal(None) }
}

impl<T: 'static + Clone> Channel<T> {
    /// Send a message. Every window reading [`latest`](Channel::latest) re-renders,
    /// and every [`on`](Channel::on) handler fires.
    pub fn send(&self, message: T) {
        self.signal.set(Some(message));
    }

    /// The latest message, **subscribing** the caller — read it in a component's
    /// render and that window re-renders whenever a new message arrives. `None`
    /// before the first send.
    pub fn latest(&self) -> Option<T> {
        self.signal.get()
    }

    /// The latest message without subscribing.
    pub fn peek(&self) -> Option<T> {
        self.signal.peek()
    }

    /// Run `handler` for each message (a side-effect subscription). Register it once
    /// at app scope; it fires on every subsequent [`send`](Channel::send).
    pub fn on(&self, handler: impl Fn(T) + 'static) {
        let signal = self.signal;
        create_effect(move || {
            if let Some(message) = signal.get() {
                handler(message);
            }
        });
    }
}
