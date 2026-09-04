//! Open a second OS window from app code. Each window is a real top-level window
//! with its own [`Ui`](pebbles_core::Ui), but they **share the one reactive
//! runtime** — so cross-window communication is just a shared signal or a
//! [`Channel`](pebbles_core::Channel), with no serialization (unlike Electron's IPC).
//!
//! Imperative, like the dialog API: `window(content).title("Inspector").size(w, h)
//! .on_close(..).open()` returns a [`WindowId`]; [`close_window`] closes it. `winit`
//! stays hidden — the shell drains these requests and manages the windows.
//!
//! The first (main) window is created by `App::run` in the shell crate. Popovers and
//! modal dialogs currently target the **main** window only; secondary windows host
//! the core widget set.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pebbles_foundation::Color;

use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// Identifies an open secondary window.
pub type WindowId = u64;

/// A decoded window icon: RGBA8 pixels + dimensions.
#[derive(Clone)]
pub struct WindowIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// A queued request to open a window (drained by the shell).
pub struct WindowSpec {
    pub id: WindowId,
    pub root: AnyWidget,
    pub title: String,
    pub width: u32,
    pub height: u32,
    /// Optional lower bound on the resizable size, in logical pixels.
    pub min_size: Option<(u32, u32)>,
    /// Optional upper bound on the resizable size, in logical pixels.
    pub max_size: Option<(u32, u32)>,
    /// Optional initial top-left position, in logical pixels.
    pub position: Option<(i32, i32)>,
    /// Whether the user can resize the window (default `true`).
    pub resizable: bool,
    /// Whether the window opens maximized.
    pub maximized: bool,
    /// Whether the OS draws the title bar / borders (default `true`).
    pub decorations: bool,
    pub icon: Option<WindowIcon>,
    pub background: Color,
    pub on_close: Option<Rc<dyn Fn()>>,
}

/// A runtime change to an already-open window (drained by the shell each turn).
pub enum WindowCommand {
    SetTitle(WindowId, String),
    SetResizable(WindowId, bool),
    SetMaximized(WindowId, bool),
    Minimize(WindowId),
    SetPosition(WindowId, i32, i32),
    SetSize(WindowId, u32, u32),
    Focus(WindowId),
}

thread_local! {
    static NEXT_ID: Cell<WindowId> = const { Cell::new(1) };
    static OPEN_QUEUE: RefCell<Vec<WindowSpec>> = const { RefCell::new(Vec::new()) };
    static CLOSE_QUEUE: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
    static CMD_QUEUE: RefCell<Vec<WindowCommand>> = const { RefCell::new(Vec::new()) };
}

/// A window to open. Build it, then [`open`](Window::open).
pub struct Window {
    root: AnyWidget,
    title: String,
    width: u32,
    height: u32,
    min_size: Option<(u32, u32)>,
    max_size: Option<(u32, u32)>,
    position: Option<(i32, i32)>,
    resizable: bool,
    maximized: bool,
    decorations: bool,
    icon: Option<WindowIcon>,
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
        min_size: None,
        max_size: None,
        position: None,
        resizable: true,
        maximized: false,
        decorations: true,
        icon: None,
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
    /// The smallest the user can resize the window to (logical px).
    pub fn min_size(mut self, width: u32, height: u32) -> Self {
        self.min_size = Some((width, height));
        self
    }
    /// The largest the user can resize the window to (logical px).
    pub fn max_size(mut self, width: u32, height: u32) -> Self {
        self.max_size = Some((width, height));
        self
    }
    /// The window's initial top-left position (logical px).
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }
    /// Whether the user can resize the window (default `true`).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }
    /// Open the window maximized.
    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }
    /// Whether the OS draws the title bar / borders (default `true`). `false` gives a
    /// borderless window.
    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }
    /// The taskbar/title-bar icon, as raw RGBA8 pixels (`width * height * 4` bytes).
    pub fn icon(mut self, rgba: impl Into<Vec<u8>>, width: u32, height: u32) -> Self {
        self.icon = Some(WindowIcon { rgba: rgba.into(), width, height });
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
    /// loop. Returns an id for [`close_window`] and the runtime `set_*` helpers.
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
                min_size: self.min_size,
                max_size: self.max_size,
                position: self.position,
                resizable: self.resizable,
                maximized: self.maximized,
                decorations: self.decorations,
                icon: self.icon,
                background: self.background.unwrap_or_else(|| theme().colors.background),
                on_close: self.on_close,
            })
        });
        id
    }
}

/// Release everything the framework holds for a closed window in the shared
/// per-window service maps — overlay/passive panels, modal dialog, sheet, toasts.
/// The shell calls this (after `Ui::dispose`) when the OS window is destroyed.
/// Window ids are never reused, so without this every open/close cycle leaks the
/// closed window's entries and whatever widget trees they hold.
pub fn drop_window_state(window: u32) {
    crate::overlay::drop_window(window);
    crate::dialog::drop_window(window);
    crate::sheet::drop_window(window);
    crate::toast::drop_window(window);
}

/// Request that the window with `id` be closed.
pub fn close_window(id: WindowId) {
    CLOSE_QUEUE.with(|q| q.borrow_mut().push(id));
}

/// Change an open window's title at runtime.
pub fn set_window_title(id: WindowId, title: impl Into<String>) {
    push_cmd(WindowCommand::SetTitle(id, title.into()));
}
/// Toggle whether an open window can be resized.
pub fn set_window_resizable(id: WindowId, resizable: bool) {
    push_cmd(WindowCommand::SetResizable(id, resizable));
}
/// Maximize / un-maximize an open window.
pub fn set_window_maximized(id: WindowId, maximized: bool) {
    push_cmd(WindowCommand::SetMaximized(id, maximized));
}
/// Minimize an open window to the taskbar/dock.
pub fn minimize_window(id: WindowId) {
    push_cmd(WindowCommand::Minimize(id));
}
/// Move an open window's top-left to `(x, y)` (logical px).
pub fn set_window_position(id: WindowId, x: i32, y: i32) {
    push_cmd(WindowCommand::SetPosition(id, x, y));
}
/// Resize an open window (logical px).
pub fn set_window_size(id: WindowId, width: u32, height: u32) {
    push_cmd(WindowCommand::SetSize(id, width, height));
}
/// Bring an open window to the front and focus it.
pub fn focus_window(id: WindowId) {
    push_cmd(WindowCommand::Focus(id));
}

fn push_cmd(cmd: WindowCommand) {
    CMD_QUEUE.with(|q| q.borrow_mut().push(cmd));
}

/// Drain pending open requests (shell-only).
pub fn take_open_requests() -> Vec<WindowSpec> {
    OPEN_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain pending close requests (shell-only).
pub fn take_close_requests() -> Vec<WindowId> {
    CLOSE_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// Drain pending runtime window commands (shell-only).
pub fn take_window_commands() -> Vec<WindowCommand> {
    CMD_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

// ---------------------------------------------------------------------------
// F5 — monitor enumeration (a polled snapshot mirrored from the shell)
// ---------------------------------------------------------------------------

/// A connected display. A polled snapshot — no hot-plug events (§J).
#[derive(Clone, Debug, PartialEq)]
pub struct MonitorInfo {
    pub name: String,
    /// Top-left in physical desktop coordinates.
    pub position: (i32, i32),
    /// Resolution in physical pixels.
    pub size: (u32, u32),
    pub scale: f64,
    pub primary: bool,
}

thread_local! {
    static MONITORS: RefCell<Option<pebbles_core::Signal<Vec<MonitorInfo>>>> =
        const { RefCell::new(None) };
}

fn monitors_signal() -> pebbles_core::Signal<Vec<MonitorInfo>> {
    MONITORS.with(|c| *c.borrow_mut().get_or_insert_with(|| pebbles_core::create_root_signal(Vec::new())))
}

/// The connected monitors (reactive — reading it subscribes the caller, so a component
/// re-renders when the shell publishes a changed snapshot). Empty until the first
/// `about_to_wait` refresh.
pub fn monitors() -> Vec<MonitorInfo> {
    monitors_signal().get()
}

/// Shell-only: publish the current monitor snapshot. A no-op when unchanged (so it
/// doesn't wake the frame loop every turn).
pub fn set_monitors(list: Vec<MonitorInfo>) {
    let sig = monitors_signal();
    if sig.peek() != list {
        sig.set(list);
    }
}
