//! Open a second OS window from app code. Each window is a real top-level window
//! with its own [`Ui`](pebbles_core::Ui), but they **share the one reactive
//! runtime** — so cross-window communication is just a shared signal or a
//! [`Channel`](pebbles_core::Channel), with no serialization (unlike Electron's IPC).
//!
//! Imperative, like the dialog API: `window(content).title("Inspector").size(w, h)
//! .on_close(..).open()` returns a [`WindowId`]; [`close_window`] closes it. `winit`
//! stays hidden — the shell drains these requests and manages the windows.
//!
//! The first (main) window is created by [`App::run`](pebbles_shell). Popovers and
//! modal dialogs currently target the **main** window only; secondary windows host
//! the core widget set.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pebbles_foundation::Color;

use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// Identifies an open secondary window.
pub type WindowId = u64;

/// A queued request to open a window (drained by the shell).
pub struct WindowSpec {
    pub id: WindowId,
    pub root: AnyWidget,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub background: Color,
    pub on_close: Option<Rc<dyn Fn()>>,
}

thread_local! {
    static NEXT_ID: Cell<WindowId> = const { Cell::new(1) };
    static OPEN_QUEUE: RefCell<Vec<WindowSpec>> = const { RefCell::new(Vec::new()) };
    static CLOSE_QUEUE: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
}

/// A window to open. Build it, then [`open`](Window::open).
pub struct Window {
    root: AnyWidget,
    title: String,
    width: u32,
    height: u32,
    background: Option<Color>,
    on_close: Option<Rc<dyn Fn()>>,
}

/// Create a [`Window`] hosting `content` (opens ~640×480 by default).
pub fn window(content: impl IntoWidget) -> Window {
    Window {
        root: content.into_widget(),
        title: "Window".to_string(),
        width: 640,
        height: 480,
        background: None,
        on_close: None,
    }
}

impl Window {
    /// The OS window title.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    /// The window's logical size.
    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }
    /// The window background (defaults to the theme background).
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Called when the window is closed (its control, or [`close_window`]).
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
    /// Queue the window to open; the shell spawns it on the next turn of the event
    /// loop. Returns an id for [`close_window`].
    pub fn open(self) -> WindowId {
        let id = NEXT_ID.with(|n| {
            let v = n.get();
            n.set(v + 1);
            v
        });
        OPEN_QUEUE.with(|q| {
            q.borrow_mut().push(WindowSpec {
                id,
                root: self.root,
                title: self.title,
                width: self.width,
                height: self.height,
                background: self.background.unwrap_or_else(|| theme().colors.background),
                on_close: self.on_close,
            })
        });
        id
    }
}

/// Request that the window with `id` be closed.
pub fn close_window(id: WindowId) {
    CLOSE_QUEUE.with(|q| q.borrow_mut().push(id));
}

/// Drain pending open requests (shell-only).
pub fn take_open_requests() -> Vec<WindowSpec> {
    OPEN_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain pending close requests (shell-only).
pub fn take_close_requests() -> Vec<WindowId> {
    CLOSE_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}
