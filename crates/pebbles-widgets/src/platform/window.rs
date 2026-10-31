//! Open a second OS window from app code. Each window is a real top-level window
//! with its own [`Ui`](pebbles_core::Ui), but they **share the one reactive
//! runtime** — so cross-window communication is just a shared signal or a
//! [`Channel`](pebbles_core::Channel), with no serialization (unlike Electron's IPC).
//!
//! Imperative, like the dialog API: `window(content).title("Inspector").size(w, h)
//! .on_close(..).open()` returns a [`WindowId`]; [`close_window`] closes it. `winit`
//! stays hidden — the shell drains these requests and manages the windows.
//!
//! The first (main) window is created by `App::run` in the shell crate. Overlays are
//! per-window: popovers, dropdowns, dialogs, and sheets are keyed by
//! [`current_window`](pebbles_core::reactive::current_window), so each window hosts its
//! own transient-overlay and modal stacks (a dialog opened from a secondary window
//! stays in that window). Escape dismisses the topmost surface of the focused window.

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
/// Tests can drain the queue themselves with [`take_window_commands`] to assert what
/// a widget asked of the window (no OS window needed).
#[derive(Clone, Debug, PartialEq)]
pub enum WindowCommand {
    SetTitle(WindowId, String),
    SetResizable(WindowId, bool),
    SetMaximized(WindowId, bool),
    Minimize(WindowId),
    SetPosition(WindowId, i32, i32),
    SetSize(WindowId, u32, u32),
    Focus(WindowId),
    /// Enter (`true`) or leave borderless full screen on the window's current
    /// monitor (winit `Fullscreen::Borderless`, like a browser's F11).
    SetFullscreen(WindowId, bool),
    /// Show (`true`) or remove (`false`) the OS title bar and frame (winit
    /// `set_decorations`). With them off the app draws its own title bar.
    SetDecorations(WindowId, bool),
    /// Maximize when restored, restore when maximized.
    ToggleMaximized(WindowId),
    /// Hand the held primary press to the window manager to move the window (winit
    /// `drag_window`). Send it from a press/drag on a client-side title bar.
    DragMove(WindowId),
    /// Hand the held primary press to the window manager to resize from `edge`
    /// (winit `drag_resize_window`).
    DragResize(WindowId, ResizeEdge),
    /// Open the window manager's window menu (move / resize / always on top / close)
    /// at `(x, y)` logical px inside the window (winit `show_window_menu`; Windows and
    /// Wayland/X11 where the compositor supports it, otherwise ignored).
    ShowWindowMenu(WindowId, f64, f64),
}

/// The edge or corner a client-side resize starts from (winit `ResizeDirection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

impl ResizeEdge {
    /// All eight, edges first.
    pub const ALL: [ResizeEdge; 8] = [
        ResizeEdge::North,
        ResizeEdge::South,
        ResizeEdge::East,
        ResizeEdge::West,
        ResizeEdge::NorthWest,
        ResizeEdge::NorthEast,
        ResizeEdge::SouthWest,
        ResizeEdge::SouthEast,
    ];

    /// The CSS resize cursor for this edge (what browsers and desktop toolkits show).
    pub fn cursor(self) -> pebbles_render::Cursor {
        use pebbles_render::Cursor;
        match self {
            ResizeEdge::North | ResizeEdge::South => Cursor::NsResize,
            ResizeEdge::East | ResizeEdge::West => Cursor::EwResize,
            ResizeEdge::NorthWest | ResizeEdge::SouthEast => Cursor::NwseResize,
            ResizeEdge::NorthEast | ResizeEdge::SouthWest => Cursor::NeswResize,
        }
    }
}

/// The window-manager state an app-drawn title bar reflects (maximise vs restore icon,
/// whether resize borders apply). Published by the shell; see [`window_state`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowState {
    pub maximized: bool,
    pub fullscreen: bool,
    pub minimized: bool,
    /// Whether the window is focused (title bars dim when inactive).
    pub focused: bool,
}

/// The id that addresses the app's MAIN window in the runtime helpers
/// (`set_window_fullscreen(MAIN_WINDOW, true)`, …). Secondary window ids start
/// at 1, so 0 never collides.
pub const MAIN_WINDOW: WindowId = 0;

thread_local! {
    static NEXT_ID: Cell<WindowId> = const { Cell::new(1) };
    static OPEN_QUEUE: RefCell<Vec<WindowSpec>> = const { RefCell::new(Vec::new()) };
    static CLOSE_QUEUE: RefCell<Vec<WindowId>> = const { RefCell::new(Vec::new()) };
    static CMD_QUEUE: RefCell<Vec<WindowCommand>> = const { RefCell::new(Vec::new()) };
    static CLOSE_HANDLERS: RefCell<std::collections::HashMap<WindowId, CloseHandler>> =
        RefCell::new(std::collections::HashMap::new());
}

// ---------------------------------------------------------------------------
// Close / exit requests (Flutter `AppLifecycleListener.onExitRequested`)
// ---------------------------------------------------------------------------

/// What a close-request handler decides when the user asks to close a window
/// (title-bar close button, Alt+F4, the window manager's close).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitResponse {
    /// Close the window (for the main window: exit the app).
    Exit,
    /// Keep the window open.
    Cancel,
    /// Keep the window open for now; the app answers later (e.g. after a Save /
    /// Don't Save / Cancel dialog) by calling [`confirm_exit`] (main window) or
    /// [`close_window`] (secondary window). Behaves like [`Cancel`](Self::Cancel)
    /// if the app never answers.
    Defer,
}

/// A close-request handler.
pub type CloseHandler = Rc<dyn Fn() -> ExitResponse>;

/// Register the handler consulted when the user asks to close the MAIN window
/// (and so exit the app) — Flutter's `onExitRequested` / `didRequestAppExit`.
/// Replaces any previous handler. Without one, a close request exits at once.
/// Programmatic exits ([`confirm_exit`]) bypass it.
pub fn on_exit_requested(f: impl Fn() -> ExitResponse + 'static) {
    set_close_handler(MAIN_WINDOW, Some(Rc::new(f)));
}

/// Remove the main window's exit handler (close requests exit at once again).
pub fn clear_exit_handler() {
    set_close_handler(MAIN_WINDOW, None);
}

/// Exit the app now, without consulting the exit handler — the answer to a
/// [`ExitResponse::Defer`]red request (also usable as a plain "quit").
pub fn confirm_exit() {
    CLOSE_QUEUE.with(|q| q.borrow_mut().push(MAIN_WINDOW));
}

fn set_close_handler(id: WindowId, f: Option<CloseHandler>) {
    CLOSE_HANDLERS.with(|h| {
        let mut h = h.borrow_mut();
        match f {
            Some(f) => h.insert(id, f),
            None => h.remove(&id),
        };
    });
}

/// Shell-only: the user asked to close window `id` (pass [`MAIN_WINDOW`] for the
/// main window). Runs its close-request handler, if any; `true` means close now.
/// The handler is called outside the registry borrow, so it may freely register
/// handlers, open dialogs, or call [`confirm_exit`].
pub fn close_requested(id: WindowId) -> bool {
    let handler = CLOSE_HANDLERS.with(|h| h.borrow().get(&id).cloned());
    match handler {
        None => true,
        Some(f) => f() == ExitResponse::Exit,
    }
}

/// Shell-only: forget a closed secondary window's close-request handler.
pub fn forget_close_handler(id: WindowId) {
    if id != MAIN_WINDOW {
        set_close_handler(id, None);
    }
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
    on_close_requested: Option<CloseHandler>,
}

/// Create a [`Window`] hosting `content` (opens ~640×480 by default).
///
/// # Platform support
/// **Desktop only** (Linux/macOS/Windows). Web has a single canvas and mobile a
/// single scene, so opening a secondary window is ignored there (a warning is
/// logged) — the app keeps running. See `PLATFORMS.md`.
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
        on_close_requested: None,
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
    /// Consulted when the user asks to close this window (its close control,
    /// Alt+F4, the window manager): return [`ExitResponse::Exit`] to close,
    /// `Cancel` to keep it, or `Defer` and later call [`close_window`].
    /// [`close_window`] itself never consults it.
    pub fn on_close_requested(mut self, f: impl Fn() -> ExitResponse + 'static) -> Self {
        self.on_close_requested = Some(Rc::new(f));
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
        if let Some(f) = self.on_close_requested {
            set_close_handler(id, Some(f));
        }
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
    crate::widgets::drop_window_metrics(window);
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
/// Enter or leave borderless full screen on the window's current monitor.
/// Pass [`MAIN_WINDOW`] for the app's main window. Ignored on web/mobile.
pub fn set_window_fullscreen(id: WindowId, fullscreen: bool) {
    push_cmd(WindowCommand::SetFullscreen(id, fullscreen));
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

/// Remove (`false`) or restore (`true`) the OS title bar and frame of an open window
/// at runtime. For the main window prefer `App::decorations(false)` at start-up.
pub fn set_window_decorations(id: WindowId, decorations: bool) {
    push_cmd(WindowCommand::SetDecorations(id, decorations));
}
/// Maximize a restored window, restore a maximized one (title-bar double-click).
pub fn toggle_window_maximized(id: WindowId) {
    push_cmd(WindowCommand::ToggleMaximized(id));
}
/// Start a window-manager move with the held primary press (client-side title bar).
pub fn drag_window(id: WindowId) {
    push_cmd(WindowCommand::DragMove(id));
}
/// Start a window-manager resize from `edge` with the held primary press.
pub fn drag_resize_window(id: WindowId, edge: ResizeEdge) {
    push_cmd(WindowCommand::DragResize(id, edge));
}
/// Show the window manager's window menu at `(x, y)` (logical px in the window).
pub fn show_window_menu(id: WindowId, x: f64, y: f64) {
    push_cmd(WindowCommand::ShowWindowMenu(id, x, y));
}

thread_local! {
    static STATES: RefCell<std::collections::HashMap<WindowId, pebbles_core::Signal<WindowState>>> =
        RefCell::new(std::collections::HashMap::new());
}

fn state_signal(id: WindowId) -> pebbles_core::Signal<WindowState> {
    STATES.with(|m| {
        *m.borrow_mut().entry(id).or_insert_with(|| {
            pebbles_core::create_root_signal(WindowState { focused: true, ..Default::default() })
        })
    })
}

/// The window's maximised / full-screen / minimised / focused state (reactive: reading
/// it subscribes the caller). Defaults to restored + focused until the shell reports.
pub fn window_state(id: WindowId) -> WindowState {
    state_signal(id).get()
}

/// Shell (and tests): publish a window's state. A no-op when unchanged.
pub fn set_window_state(id: WindowId, state: WindowState) {
    let sig = state_signal(id);
    if sig.peek() != state {
        sig.set(state);
    }
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

#[cfg(test)]
mod close_request_tests {
    use super::*;

    #[test]
    fn no_handler_closes_immediately() {
        clear_exit_handler();
        assert!(close_requested(MAIN_WINDOW));
        assert!(close_requested(4242));
    }

    #[test]
    fn exit_handler_can_cancel_exit_or_defer() {
        let answer = Rc::new(Cell::new(ExitResponse::Cancel));
        let calls = Rc::new(Cell::new(0));
        let (a, c) = (answer.clone(), calls.clone());
        on_exit_requested(move || {
            c.set(c.get() + 1);
            a.get()
        });
        assert!(!close_requested(MAIN_WINDOW));
        answer.set(ExitResponse::Defer);
        assert!(!close_requested(MAIN_WINDOW));
        answer.set(ExitResponse::Exit);
        assert!(close_requested(MAIN_WINDOW));
        assert_eq!(calls.get(), 3);
        clear_exit_handler();
        assert!(close_requested(MAIN_WINDOW));
    }

    #[test]
    fn deferred_exit_is_confirmed_through_the_close_queue() {
        on_exit_requested(|| {
            // A handler may answer synchronously from inside itself, too.
            confirm_exit();
            ExitResponse::Defer
        });
        let _ = take_close_requests();
        assert!(!close_requested(MAIN_WINDOW));
        assert_eq!(take_close_requests(), vec![MAIN_WINDOW]);
        clear_exit_handler();
    }

    #[test]
    fn secondary_window_handler_is_per_window_and_forgotten() {
        let id = window(crate::spacer()).on_close_requested(|| ExitResponse::Cancel).open();
        let other = window(crate::spacer()).open();
        let _ = take_open_requests();
        assert!(!close_requested(id));
        assert!(close_requested(other));
        forget_close_handler(id);
        assert!(close_requested(id));
    }
}
