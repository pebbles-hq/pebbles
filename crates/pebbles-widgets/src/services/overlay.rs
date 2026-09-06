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
use pebbles_render::StackFit;

use crate::widgets::{Container, GestureDetector, Positioned, stack};
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, component_props, create_root_signal};

thread_local! {
    /// Each window's logical size as a **reactive** signal, published by the shell each
    /// frame so popovers flip/shift to stay on-screen and layouts respond to resizes.
    /// Keyed by window id.
    static WINDOW: RefCell<HashMap<u32, Signal<(f64, f64)>>> = RefCell::new(HashMap::new());
}

/// The current window's size signal, created on first access.
fn window_signal() -> Signal<(f64, f64)> {
    let window = current_window();
    WINDOW.with(|m| *m.borrow_mut().entry(window).or_insert_with(|| create_root_signal((0.0, 0.0))))
}

/// Record the current window's logical size (called by the shell each frame). Only
/// writes on an actual change, so the per-frame call doesn't thrash subscribers —
/// reactive readers ([`window_size`], [`media_query`](crate::media_query)) re-render
/// only when the window really resizes.
pub fn set_window_size(width: f64, height: f64) {
    let sig = window_signal();
    if sig.peek() != (width, height) {
        sig.set((width, height));
    }
}

/// The current window's logical size `(width, height)`, or `(0, 0)` before its first
/// frame. **Reactive** — reading it in a component re-renders on resize.
pub fn window_size() -> (f64, f64) {
    window_signal().get()
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
    /// An optional second panel rendered beside the content (a submenu).
    pub child: Option<OverlayChild>,
    /// Aliveness probe from the opener ([`show_overlay_guarded`]): once it reports
    /// false (the opening component unmounted — e.g. navigation while a dropdown
    /// was up), the shell's per-frame [`gc_dead`] tears the overlay down BEFORE
    /// its content can re-render against the opener's disposed signals.
    pub alive: Option<std::rc::Rc<dyn Fn() -> bool>>,
}

/// The overlay's optional nested panel (dropdown submenus).
#[derive(Clone)]
pub struct OverlayChild {
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
    OVERLAY.with(|cell| *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None)))
}

/// Show `content` at window position `(left, top)`, replacing any current overlay.
/// `width`/`height` are the panel's approximate size — the shell uses them to keep
/// the popover anchored to its trigger while the page scrolls (see [`shift`]).
pub fn show_overlay(content: AnyWidget, left: f64, top: f64, width: f64, height: f64) {
    if pebbles_core::log::dev_mode() {
        pebbles_core::log::debug(
            pebbles_core::log::Cat::Overlay,
            format!("overlay opened {width:.0}×{height:.0} @ {left:.0},{top:.0}"),
        );
    }
    overlay_signal().set(Some(OverlayEntry { content, left, top, width, height, child: None, alive: None }));
}

/// [`show_overlay`] with an aliveness probe. Widgets whose overlay content captures
/// component-scoped signals (select, menus, pickers…) MUST use this: pass a probe
/// over a signal created in the opener's render (e.g. `move || token.alive()`), so
/// the panel is garbage-collected the frame its owner unmounts instead of
/// re-rendering against disposed signals.
pub fn show_overlay_guarded(
    content: AnyWidget,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    alive: impl Fn() -> bool + 'static,
) {
    if pebbles_core::log::dev_mode() {
        pebbles_core::log::debug(
            pebbles_core::log::Cat::Overlay,
            format!("overlay opened (guarded) {width:.0}×{height:.0} @ {left:.0},{top:.0}"),
        );
    }
    overlay_signal().set(Some(OverlayEntry {
        content,
        left,
        top,
        width,
        height,
        child: None,
        alive: Some(std::rc::Rc::new(alive)),
    }));
}

/// Tear down the current window's overlay if its opener unmounted (probe reports
/// false). The shell calls this every frame BEFORE reconciliation, so a dead
/// overlay's content is never rebuilt against disposed signals.
pub fn gc_dead() {
    let sig = overlay_signal();
    let dead = sig.peek().is_some_and(|e| e.alive.as_ref().is_some_and(|f| !f()));
    if dead {
        sig.set(None);
    }
}

/// Attach (or replace) the open overlay's child panel at window position
/// `(left, top)`. A no-op if no overlay is open.
pub fn set_child(content: AnyWidget, left: f64, top: f64, width: f64, height: f64) {
    overlay_signal().update(|e| {
        if let Some(entry) = e {
            entry.child = Some(OverlayChild { content, left, top, width, height });
        }
    });
}

/// Dismiss the open overlay's child panel (the overlay itself stays).
pub fn clear_child() {
    overlay_signal().update(|e| {
        if let Some(entry) = e {
            entry.child = None;
        }
    });
}

/// Whether the open overlay has a child panel (a submenu is showing).
pub fn child_is_open() -> bool {
    overlay_signal().peek().is_some_and(|e| e.child.is_some())
}

/// Dismiss the current overlay, if any.
pub fn hide_overlay() {
    if pebbles_core::log::dev_mode() && overlay_signal().peek().is_some() {
        pebbles_core::log::debug(pebbles_core::log::Cat::Overlay, "overlay closed".to_string());
    }
    overlay_signal().set(None);
}

/// Forget a closed window's overlay state: clear + drop its overlay/passive signals
/// and its recorded size. Window ids are never reused, so skipping this would grow
/// the maps — and pin the dropped panels' widget trees — on every open/close cycle.
pub(crate) fn drop_window(window: u32) {
    if let Some(sig) = OVERLAY.with(|m| m.borrow_mut().remove(&window)) {
        sig.set(None);
    }
    if let Some(sig) = PASSIVE.with(|m| m.borrow_mut().remove(&window)) {
        sig.set(None);
    }
    if let Some(sig) = WINDOW.with(|m| m.borrow_mut().remove(&window)) {
        sig.set((0.0, 0.0));
    }
}

/// Nudge the open overlay (and its child panel) by `(dx, dy)` — used to keep it
/// glued to its trigger as the page scrolls underneath. A no-op if nothing is open.
pub fn shift(dx: f64, dy: f64) {
    overlay_signal().update(|e| {
        if let Some(entry) = e {
            entry.left += dx;
            entry.top += dy;
            if let Some(child) = &mut entry.child {
                child.left += dx;
                child.top += dy;
            }
        }
    });
}

/// Whether `(x, y)` (window space) falls within the open overlay's panel rect or
/// its child panel — so the shell scrolls the popover's own content instead of
/// following the page.
pub fn over_panel(x: f64, y: f64) -> bool {
    let in_rect =
        |left: f64, top: f64, w: f64, h: f64| x >= left && x <= left + w && y >= top && y <= top + h;
    match overlay_signal().peek() {
        Some(e) => {
            in_rect(e.left, e.top, e.width, e.height)
                || e.child.as_ref().is_some_and(|c| in_rect(c.left, c.top, c.width, c.height))
        }
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
    PASSIVE.with(|cell| *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None)))
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

/// Number of windows with an open (menu/popover) overlay (debug-only).
#[cfg(debug_assertions)]
pub fn census_overlays() -> usize {
    OVERLAY.with(|o| o.borrow().values().filter(|s| s.peek().is_some()).count())
}

/// Number of windows with an open passive overlay (debug-only).
#[cfg(debug_assertions)]
pub fn census_passive() -> usize {
    PASSIVE.with(|p| p.borrow().values().filter(|s| s.peek().is_some()).count())
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

struct GuardProps {
    content: AnyWidget,
    alive: Option<std::rc::Rc<dyn Fn() -> bool>>,
}

/// Wrap overlay panel content so the aliveness probe is re-checked at the
/// panel's own inflate/render time (see the comment in [`render_host`]).
fn guarded(content: AnyWidget, alive: Option<std::rc::Rc<dyn Fn() -> bool>>) -> AnyWidget {
    match alive {
        None => content,
        Some(_) => component_props(render_panel_guard, GuardProps { content, alive }).into_widget(),
    }
}

fn render_panel_guard(p: &GuardProps) -> AnyWidget {
    if p.alive.as_ref().is_some_and(|f| !f()) {
        return Container::new().into_widget();
    }
    p.content.clone()
}

fn render_host(p: &Props) -> crate::widgets::Stack {
    let mut kids: Vec<AnyWidget> = vec![p.child.clone()];
    // Passive layer (tooltips / hover cards): above content, click-through, no scrim.
    if let Some(entry) = passive_signal().get() {
        kids.push(Positioned::new(entry.content).left(entry.left).top(entry.top).into_widget());
    }
    // Skip (and let the shell's gc_dead drop) an overlay whose opener unmounted
    // EARLIER IN THIS SAME REBUILD PASS — building its content here would read the
    // opener's just-disposed signals. The frame-start gc_dead can't catch that
    // ordering; this check is the mid-pass half of the same guard.
    if let Some(entry) = overlay_signal().get().filter(|e| e.alive.as_ref().is_none_or(|f| f())) {
        // Full-window scrim: an outside click dismisses.
        let scrim =
            Positioned::fill(GestureDetector::new(Container::new()).on_tap(hide_overlay)).into_widget();
        // The probe travels INTO a guard component around the panel content: the
        // host can pass its own check and the opener still unmount later in the
        // SAME rebuild pass, before the panel child inflates — the guard re-checks
        // at that exact moment and inflates nothing instead of reading disposed
        // signals. (The filter above + the shell's frame gc_dead handle the other
        // two orderings.)
        let panel = Positioned::new(guarded(entry.content, entry.alive.clone()))
            .left(entry.left)
            .top(entry.top)
            .into_widget();
        kids.push(scrim);
        kids.push(panel);
        if let Some(child) = &entry.child {
            kids.push(
                Positioned::new(guarded(child.content.clone(), entry.alive.clone()))
                    .left(child.left)
                    .top(child.top)
                    .into_widget(),
            );
        }
    }
    // Modal dialogs paint above the popover layer (dim scrim + centered surface).
    kids.extend(crate::dialog::overlay_children());
    // Sheets / drawers — edge-anchored modal panels, above dialogs.
    kids.extend(crate::sheet::overlay_children());
    // Toasts paint topmost (over modals) so notifications are always visible.
    kids.extend(crate::toast::overlay_children());
    stack(kids).alignment(Alignment::TOP_LEFT).fit(StackFit::Expand)
}
