//! A per-window **overlay layer** — the substrate for dropdowns, menus, popovers and
//! tooltips. Each window has one transient overlay (a widget + a window-space
//! position) in its own [`Signal`]; [`OverlayHost`] wraps a window's root and paints
//! that window's overlay on top of everything, with a full-window scrim that dismisses
//! on an outside click.
//!
//! Anything can pop content: [`show_overlay`] from a click handler, [`hide_overlay`]
//! to dismiss. Because the entry is a signal, showing/hiding re-renders the host. All
//! of these resolve to the **current window** (the one rendering, or the one the shell
//! is dispatching input to), so an overlay opened in a secondary window stays there.

use std::cell::RefCell;
use std::collections::HashMap;

use pebbles_foundation::Alignment;

use crate::widgets::{Container, GestureDetector, Positioned, stack};
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, action, component_props, create_root_signal};

thread_local! {
    /// Each window's logical size, published by the shell each frame so popovers can
    /// flip/shift to stay on-screen. Keyed by window id.
    static WINDOW: RefCell<HashMap<u32, (f64, f64)>> = RefCell::new(HashMap::new());
}

/// Record the current window's logical size (called by the shell).
pub fn set_window_size(width: f64, height: f64) {
    WINDOW.with(|w| w.borrow_mut().insert(current_window(), (width, height)));
}

/// The current window's logical size `(width, height)`, or `(0, 0)` before its first
/// frame.
pub fn window_size() -> (f64, f64) {
    WINDOW.with(|w| w.borrow().get(&current_window()).copied().unwrap_or((0.0, 0.0)))
}

/// Whether an overlay is currently open in the current window.
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
    /// One overlay signal per window id (main = 0). Created lazily on first access.
    static OVERLAY: RefCell<HashMap<u32, Signal<Option<OverlayEntry>>>> =
        RefCell::new(HashMap::new());
}

/// Create the main window's overlay signal. Call once at startup (before the tree
/// runs) so the signal is global, not owned by whatever component renders first.
/// Secondary windows create theirs lazily when their [`OverlayHost`] first renders.
pub fn init() {
    let _ = overlay_signal();
}

/// The current window's overlay signal (reactive), created on first access.
pub fn overlay_signal() -> Signal<Option<OverlayEntry>> {
    let window = current_window();
    OVERLAY.with(|cell| {
        *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None))
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

// ---------------------------------------------------------------------------
// Passive layer — non-blocking overlays (tooltips, hover cards). No scrim, no
// outside-click dismiss, never captures clicks; painted above content.
// ---------------------------------------------------------------------------

/// A passive overlay: a widget at a window-space position, painted above the app but
/// click-through (unlike the menu layer, which has a dismissing scrim).
#[derive(Clone)]
pub struct PassiveEntry {
    pub content: AnyWidget,
    pub left: f64,
    pub top: f64,
}

thread_local! {
    static PASSIVE: RefCell<HashMap<u32, Signal<Option<PassiveEntry>>>> =
        RefCell::new(HashMap::new());
}

fn passive_signal() -> Signal<Option<PassiveEntry>> {
    let window = current_window();
    PASSIVE.with(|cell| {
        *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None))
    })
}

/// Show click-through `content` at window position `(left, top)` in the current
/// window's passive layer, replacing any current passive entry.
pub fn show_passive(content: AnyWidget, left: f64, top: f64) {
    passive_signal().set(Some(PassiveEntry { content, left, top }));
}

/// Dismiss the current window's passive overlay, if any.
pub fn hide_passive() {
    passive_signal().set(None);
}

/// Whether a passive overlay is currently showing in the current window.
pub fn passive_is_open() -> bool {
    passive_signal().peek().is_some()
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
    // Passive layer (tooltips / hover cards): above content, click-through, no scrim.
    if let Some(entry) = passive_signal().get() {
        kids.push(Positioned::new(entry.content).left(entry.left).top(entry.top).into_widget());
    }
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
    // Toasts paint topmost (over modals) so notifications are always visible.
    kids.extend(crate::toast::overlay_children());
    stack(kids).alignment(Alignment::TOP_LEFT).expand()
}
