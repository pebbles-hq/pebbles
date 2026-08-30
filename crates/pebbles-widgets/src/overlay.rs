//! A global **overlay layer** — the substrate for dropdowns, menus, popovers and
//! tooltips. One transient overlay (a widget + a window-space position) lives in a
//! global [`Signal`]; [`OverlayHost`] wraps the app root and paints it on top of
//! everything, with a full-window scrim that dismisses on an outside click.
//!
//! Anything can pop content: [`show_overlay`] from a click handler, [`hide_overlay`]
//! to dismiss. Because the entry is a signal, showing/hiding re-renders the host.

use std::cell::{Cell, RefCell};

use pebbles_foundation::Alignment;

use crate::widgets::{Container, GestureDetector, Positioned, stack};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, action, component_props, create_root_signal};

thread_local! {
    /// The current window's logical size, published by the shell each frame so
    /// popovers can flip/shift to stay on-screen.
    static WINDOW: Cell<(f64, f64)> = const { Cell::new((0.0, 0.0)) };
}

/// Record the window's logical size (called by the shell).
pub fn set_window_size(width: f64, height: f64) {
    WINDOW.with(|w| w.set((width, height)));
}

/// The window's logical size `(width, height)`, or `(0, 0)` before the first frame.
pub fn window_size() -> (f64, f64) {
    WINDOW.with(Cell::get)
}

/// Whether an overlay is currently open.
pub fn is_open() -> bool {
    overlay_signal().peek().is_some()
}

/// A live overlay: its content, window-space top-left, and approximate panel size
/// (used to tell a wheel over the panel from a wheel over the page behind it).
#[derive(Clone)]
pub struct OverlayEntry {
    pub content: AnyWidget,
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

thread_local! {
    static OVERLAY: RefCell<Option<Signal<Option<OverlayEntry>>>> = const { RefCell::new(None) };
}

/// Create the global overlay signal. Call once at startup (before the tree runs)
/// so the signal is global, not owned by whatever component renders first.
pub fn init() {
    let _ = overlay_signal();
}

/// The global overlay signal (reactive).
pub fn overlay_signal() -> Signal<Option<OverlayEntry>> {
    OVERLAY.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_root_signal(None));
        }
        cell.unwrap()
    })
}

/// Show `content` at window position `(left, top)`, replacing any current overlay.
/// `width`/`height` are the panel's approximate size — the shell uses them to keep
/// the popover anchored to its trigger while the page scrolls (see [`shift`]).
pub fn show_overlay(content: AnyWidget, left: f64, top: f64, width: f64, height: f64) {
    overlay_signal().set(Some(OverlayEntry { content, left, top, width, height }));
}

/// Dismiss the current overlay, if any.
pub fn hide_overlay() {
    overlay_signal().set(None);
}

/// Nudge the open overlay by `(dx, dy)` — used to keep it glued to its trigger as
/// the page scrolls underneath. A no-op if nothing is open.
pub fn shift(dx: f64, dy: f64) {
    overlay_signal().update(|e| {
        if let Some(entry) = e {
            entry.left += dx;
            entry.top += dy;
        }
    });
}

/// Whether `(x, y)` (window space) falls within the open overlay's panel rect — so
/// the shell scrolls the popover's own content instead of following the page.
pub fn over_panel(x: f64, y: f64) -> bool {
    match overlay_signal().peek() {
        Some(e) => x >= e.left && x <= e.left + e.width && y >= e.top && y <= e.top + e.height,
        None => false,
    }
}

/// Wraps the app root and renders the active overlay above it. The shell installs
/// one of these around every app automatically.
pub struct OverlayHost {
    child: AnyWidget,
}

impl OverlayHost {
    /// Wrap `child` so overlays pop above it.
    pub fn wrap(child: impl IntoWidget) -> Self {
        OverlayHost { child: child.into_widget() }
    }
}

struct Props {
    child: AnyWidget,
}

impl IntoWidget for OverlayHost {
    fn into_widget(self) -> AnyWidget {
        component_props(render_host, Props { child: self.child }).into_widget()
    }
}

fn render_host(p: &Props) -> crate::widgets::Stack {
    let mut kids: Vec<AnyWidget> = vec![p.child.clone()];
    if let Some(entry) = overlay_signal().get() {
        // Full-window scrim: an outside click dismisses.
        let scrim = Positioned::fill(
            GestureDetector::new(Container::new()).on_tap(action(hide_overlay)),
        )
        .into_widget();
        let panel = Positioned::new(entry.content).left(entry.left).top(entry.top).into_widget();
        kids.push(scrim);
        kids.push(panel);
    }
    // Modal dialogs paint above the popover layer (dim scrim + centered surface).
    kids.extend(crate::dialog::overlay_children());
    stack(kids).alignment(Alignment::TOP_LEFT).expand()
}
