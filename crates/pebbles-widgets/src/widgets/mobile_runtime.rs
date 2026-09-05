//! Mobile-runtime hooks that need the shell to report platform events — built here
//! as reactive, desktop-testable APIs; the mobile shell drives them on-device.
//!
//! - [`pop_scope`] (Flutter's `PopScope`/`WillPopScope`) — intercept the Android
//!   hardware back button. The shell calls [`dispatch_back`] on a back press.
//! - [`set_system_ui_overlay_style`] (Flutter's `SystemChrome`) — request status /
//!   navigation-bar styling; the shell reads it (a no-op on desktop).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::Color;

use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{component_props, create_cleanup, create_signal};

// ===========================================================================
// PopScope — intercept the hardware back button
// ===========================================================================

type PopHandler = Rc<dyn Fn()>;
/// One registered pop_scope: `(id, blocking, on_pop)`.
type PopEntry = (u64, bool, PopHandler);

thread_local! {
    /// Per-window stack of pop_scopes — last registered is topmost.
    static POP_HANDLERS: RefCell<HashMap<u32, Vec<PopEntry>>> = RefCell::new(HashMap::new());
    static NEXT_POP_ID: Cell<u64> = const { Cell::new(1) };
}

fn next_pop_id() -> u64 {
    NEXT_POP_ID.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}

fn register_pop(window: u32, id: u64, blocking: bool, handler: PopHandler) {
    POP_HANDLERS.with(|h| {
        let mut map = h.borrow_mut();
        let list = map.entry(window).or_default();
        // Replace an existing entry for this id (re-render), else push (mount order).
        if let Some(slot) = list.iter_mut().find(|(hid, _, _)| *hid == id) {
            slot.1 = blocking;
            slot.2 = handler;
        } else {
            list.push((id, blocking, handler));
        }
    });
}

fn unregister_pop(window: u32, id: u64) {
    POP_HANDLERS.with(|h| {
        if let Some(list) = h.borrow_mut().get_mut(&window) {
            list.retain(|(hid, _, _)| *hid != id);
        }
    });
}

/// Handle a hardware/back-gesture "pop" for the current window. Runs the topmost
/// **blocking** [`pop_scope`]'s `on_pop` and returns `true` (the back was consumed —
/// don't navigate). Returns `false` when nothing is blocking, so the shell performs
/// its default back action. The mobile shell calls this on the Android back button.
pub fn dispatch_back() -> bool {
    let window = current_window();
    let top = POP_HANDLERS.with(|h| {
        h.borrow().get(&window).and_then(|list| {
            list.iter().rev().find(|(_, blocking, _)| *blocking).map(|(_, _, cb)| cb.clone())
        })
    });
    match top {
        Some(cb) => {
            cb();
            true
        }
        None => false,
    }
}

/// Whether a [`pop_scope`] is currently intercepting the back button.
pub fn back_is_blocked() -> bool {
    let window = current_window();
    POP_HANDLERS.with(|h| h.borrow().get(&window).is_some_and(|l| l.iter().any(|(_, b, _)| *b)))
}

/// Intercept the hardware back button while mounted (Flutter's `PopScope`). While
/// `.blocking(true)`, a back press runs `on_pop` and does **not** navigate; while
/// `.blocking(false)` (the default) it's transparent. Build with [`pop_scope`].
#[derive(Clone)]
pub struct PopScope {
    child: AnyWidget,
    blocking: bool,
    on_pop: Option<PopHandler>,
}

/// Wrap `child` in a [`PopScope`].
pub fn pop_scope(child: impl IntoWidget) -> PopScope {
    PopScope { child: child.into_widget(), blocking: true, on_pop: None }
}

impl PopScope {
    /// Whether to intercept the back button (default `true`). Set `false` to make
    /// it transparent (like Flutter's `canPop: true`).
    pub fn blocking(mut self, blocking: bool) -> Self {
        self.blocking = blocking;
        self
    }
    /// Called when the back button is pressed while `blocking`.
    pub fn on_pop(mut self, f: impl Fn() + 'static) -> Self {
        self.on_pop = Some(Rc::new(f));
        self
    }
}

impl IntoWidget for PopScope {
    fn into_widget(self) -> AnyWidget {
        component_props(render_pop_scope, self).into_widget()
    }
}

fn render_pop_scope(p: &PopScope) -> AnyWidget {
    // A stable per-instance id (assigned once, persists by hook order).
    let id_sig = create_signal(0u64);
    if id_sig.peek() == 0 {
        id_sig.set(next_pop_id());
    }
    let id = id_sig.peek();
    let window = current_window();

    let handler: PopHandler = p.on_pop.clone().unwrap_or_else(|| Rc::new(|| {}));
    register_pop(window, id, p.blocking, handler);
    // Unregister on unmount (cleanups are re-registered fresh each render).
    create_cleanup(move || unregister_pop(window, id));

    p.child.clone()
}

// ===========================================================================
// SystemChrome — status / navigation-bar styling
// ===========================================================================

/// The desired system-UI overlay styling (Flutter's `SystemUiOverlayStyle`). The
/// mobile shell reads it and styles the status / navigation bars; a no-op on desktop.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct SystemUiOverlayStyle {
    /// Status-bar background color (where the platform supports it).
    pub status_bar_color: Option<Color>,
    /// Draw the status-bar icons dark (for a light background).
    pub status_bar_dark_icons: bool,
    /// Navigation-bar background color (Android).
    pub nav_bar_color: Option<Color>,
    /// Draw the navigation-bar icons dark.
    pub nav_bar_dark_icons: bool,
}

thread_local! {
    static CHROME: RefCell<HashMap<u32, SystemUiOverlayStyle>> = RefCell::new(HashMap::new());
}

/// Request the system-UI overlay styling for the current window (Flutter's
/// `SystemChrome.setSystemUIOverlayStyle`). The mobile shell reads it via
/// [`system_ui_overlay_style`]; desktop ignores it.
pub fn set_system_ui_overlay_style(style: SystemUiOverlayStyle) {
    let window = current_window();
    CHROME.with(|c| {
        c.borrow_mut().insert(window, style);
    });
}

/// The currently-requested [`SystemUiOverlayStyle`] for the current window (read by
/// the shell).
pub fn system_ui_overlay_style() -> SystemUiOverlayStyle {
    let window = current_window();
    CHROME.with(|c| c.borrow().get(&window).copied().unwrap_or_default())
}
