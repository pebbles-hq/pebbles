//! [`sheet`] — an edge-anchored modal panel (shadcn Sheet / Drawer): a full-height
//! (left/right) or full-width (top/bottom) surface over a dimmed scrim. An app service
//! like [`dialog`](fn@crate::dialog); per-window, one at a time. `Side::Bottom` is the
//! "drawer" pattern.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, BoxDecoration};

use crate::overlay::window_size;
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Opacity, Positioned, Transform, column, gap_h, text};
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animation, create_root_signal};

use crate::side::Side;

/// Identifies an open sheet.
pub type SheetId = u64;

/// Enter/exit tween length (seconds) — also the deferred-removal delay (C3).
const MOTION_SECS: f64 = 0.22;

#[derive(Clone)]
struct SheetEntry {
    content: AnyWidget,
    id: SheetId,
    side: Side,
    size: f64,
    title: String,
    background: Option<Color>,
    style: Option<crate::style::Style>,
    dismissible: bool,
    on_close: Option<Rc<dyn Fn()>>,
    /// True once closing started: the panel slides out and the entry is removed after
    /// [`MOTION_SECS`] (deferred removal). `is_open()` reports `false` immediately.
    leaving: bool,
}

thread_local! {
    static NEXT_ID: Cell<SheetId> = const { Cell::new(1) };
    static SHEET: RefCell<HashMap<u32, Signal<Option<SheetEntry>>>> = RefCell::new(HashMap::new());
    /// One reusable open/close progress signal per window (0 = off-screen, 1 = at rest).
    /// Reused across sheets — no per-sheet signal to leak (C3 motion).
    static SHEET_T: RefCell<HashMap<u32, Signal<f64>>> = RefCell::new(HashMap::new());
}

/// The current window's sheet motion progress signal (created once, reused).
fn t_signal() -> Signal<f64> {
    let window = current_window();
    SHEET_T.with(|cell| *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(0.0)))
}

/// The deferred-removal timer key for a window's sheet (distinct namespace).
fn removal_key(window: u32) -> u64 {
    0x7042_0000_0000_0000 ^ (window as u64)
}

/// Create the main window's sheet signal (call once at startup, like dialog/overlay).
pub fn init() {
    let _ = sheet_signal();
}

/// Forget a closed window's sheet state (clear + drop its signal — window ids are
/// never reused).
pub(crate) fn drop_window(window: u32) {
    if let Some(sig) = SHEET.with(|m| m.borrow_mut().remove(&window)) {
        sig.set(None);
    }
    if let Some(t) = SHEET_T.with(|m| m.borrow_mut().remove(&window)) {
        t.set(0.0);
    }
    animation::clear_timeout(removal_key(window));
}

fn sheet_signal() -> Signal<Option<SheetEntry>> {
    let window = current_window();
    SHEET.with(|cell| *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None)))
}

/// Whether a sheet is open (and not already closing) in the current window.
pub fn is_open() -> bool {
    sheet_signal().peek().is_some_and(|e| !e.leaving)
}

/// Begin closing the sheet matching `id` (or any open one when `id == 0`): fires
/// `on_close`, slides the panel out, and removes it after the exit tween.
/// Idempotent — a second call while it's already closing does nothing (so an Escape or
/// scrim tap during the exit is a no-op).
pub fn close_sheet(id: SheetId) {
    let Some(e) = sheet_signal().peek() else { return };
    if (id != 0 && e.id != id) || e.leaving {
        return;
    }
    sheet_signal().update(|s| {
        if let Some(s) = s {
            s.leaving = true;
        }
    });
    if let Some(f) = &e.on_close {
        f();
    }
    animation::animate_to(t_signal(), 0.0, MOTION_SECS);
    let window = current_window();
    let closing = e.id;
    animation::set_timeout(removal_key(window), MOTION_SECS, move || {
        // Only clear if it's still the same (leaving) sheet.
        sheet_signal().update(|s| {
            if s.as_ref().is_some_and(|x| x.id == closing) {
                *s = None;
            }
        });
    });
}

/// Close the open sheet if it's dismissible (Escape / scrim click). A no-op once it's
/// already closing.
pub fn dismiss_top() {
    if let Some(e) = sheet_signal().peek()
        && e.dismissible
        && !e.leaving
    {
        close_sheet(e.id);
    }
}

/// A sheet to open. Build it, then [`open`](Sheet::open).
pub struct Sheet {
    content: AnyWidget,
    side: Side,
    size: f64,
    title: String,
    background: Option<Color>,
    style: Option<crate::style::Style>,
    dismissible: bool,
    on_close: Option<Rc<dyn Fn()>>,
}

/// Create a [`Sheet`] wrapping `content` (a right-side, 360px panel by default).
pub fn sheet(content: impl IntoWidget) -> Sheet {
    Sheet {
        content: content.into_widget(),
        side: Side::Right,
        size: 360.0,
        title: String::new(),
        background: None,
        style: None,
        dismissible: true,
        on_close: None,
    }
}

impl Sheet {
    /// Which edge the sheet slides from (default `Right`). `Bottom` = a drawer.
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
    /// The panel's width (left/right) or height (top/bottom), in logical px.
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }
    /// A header title rendered above the content.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Merge a [`Style`](crate::Style) onto the panel surface (bg / border / radius …).
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
    /// Open the sheet (replacing any open one). Returns its [`SheetId`].
    pub fn open(self) -> SheetId {
        let id = NEXT_ID.with(|n| {
            let v = n.get();
            n.set(v + 1);
            v
        });
        sheet_signal().set(Some(SheetEntry {
            content: self.content,
            id,
            side: self.side,
            size: self.size,
            title: self.title,
            background: self.background,
            style: self.style,
            dismissible: self.dismissible,
            on_close: self.on_close,
            leaving: false,
        }));
        // C3: slide in + fade the scrim from off-screen (t: 0 → 1).
        let t = t_signal();
        t.set(0.0);
        animation::clear_timeout(removal_key(current_window()));
        animation::animate_to(t, 1.0, MOTION_SECS);
        id
    }
}

/// Overlay children for the open sheet (scrim + edge-anchored panel), or empty.
/// Rendered by [`OverlayHost`](crate::overlay::OverlayHost).
pub(crate) fn overlay_children() -> Vec<AnyWidget> {
    let Some(e) = sheet_signal().get() else {
        return Vec::new();
    };
    let c = theme().colors;
    // C3 progress: 0 = off-screen (scrim clear), 1 = at rest (scrim at 0.4).
    let t = t_signal().get().clamp(0.0, 1.0);

    let scrim = Positioned::fill(Opacity::new(
        t as f32,
        GestureDetector::new(Container::new().color(Color::new([0.0, 0.0, 0.0, 0.4]))).on_tap(dismiss_top),
    ))
    .into_widget();

    // The panel content: optional title header, then the caller's content.
    let mut kids: Vec<AnyWidget> = Vec::new();
    if !e.title.is_empty() {
        kids.push(text(e.title.clone()).size(17.0).weight(600.0).color(c.foreground).into_widget());
        kids.push(gap_h(14.0).into_widget());
    }
    kids.push(e.content);
    let body = column(kids).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min);

    let horizontal = matches!(e.side, Side::Left | Side::Right);
    let (ww, wh) = window_size();
    // Base surface presentation as a Style; the user's `.style(..)` merges on top.
    let base = crate::style::style()
        .background(e.background.unwrap_or(c.background))
        .border(Border::new(c.border, 1.0))
        .radius_all(0.0);
    let deco =
        base.merge(e.style.clone().unwrap_or_default()).decoration().unwrap_or_else(BoxDecoration::new);
    let mut surface = Container::new().decoration(deco).padding(EdgeInsets::all(22.0)).child(body);
    if horizontal {
        surface = surface.width(e.size);
        if wh > 0.0 {
            surface = surface.height(wh);
        }
    } else {
        surface = surface.height(e.size);
        if ww > 0.0 {
            surface = surface.width(ww);
        }
    }

    // C3: slide the panel in from its edge — off by (1-t)·size, easing to 0.
    let off = (1.0 - t) * e.size;
    let (dx, dy) = match e.side {
        Side::Left => (-off, 0.0),
        Side::Right => (off, 0.0),
        Side::Top => (0.0, -off),
        Side::Bottom => (0.0, off),
    };
    // Consume taps on the panel so they don't fall through to the dismiss scrim
    // behind it — the shell fires a tap on the topmost listener under the point, and
    // the scrim (a full-window GestureDetector) sits behind the panel. Without this,
    // tapping the panel (e.g. focusing a text field, which only handles pointer-down)
    // would reach the scrim and close the sheet. The catcher wraps the Transform so
    // it's the panel's outermost hit layer. Flutter's modal content absorbs the same.
    let slid = GestureDetector::new(Transform::translate(dx, dy, surface)).on_tap(|| {});

    let mut panel = Positioned::new(slid);
    panel = match e.side {
        Side::Left => panel.left(0.0).top(0.0),
        Side::Right => panel.right(0.0).top(0.0),
        Side::Top => panel.left(0.0).top(0.0),
        Side::Bottom => panel.left(0.0).bottom(0.0),
    };

    vec![scrim, panel.into_widget()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::text;

    fn present() -> bool {
        sheet_signal().peek().is_some()
    }

    #[test]
    fn open_slides_in_and_close_defers_removal() {
        animation::reset();
        init();
        sheet_signal().set(None); // isolate from any prior state in this thread

        let id = sheet(text("filters")).side(Side::Right).size(300.0).open();
        assert!(is_open(), "opened");

        // C3: t rises 0 → 1 across MOTION_SECS (panel slides in from the edge).
        animation::tick(0.0);
        assert!(t_signal().peek() <= 0.01, "starts off-screen");
        animation::tick(MOTION_SECS / 2.0);
        let mid = t_signal().peek();
        assert!(mid > 0.0 && mid < 1.0, "panel is between off-screen and rest, got {mid}");
        animation::tick(MOTION_SECS + 0.01);
        assert!((t_signal().peek() - 1.0).abs() < 1e-6, "settled at rest");

        // Close: is_open reports false at once, but the panel survives the exit tween.
        close_sheet(id);
        assert!(!is_open(), "reports closed immediately");
        assert!(present(), "entry still rendered during the exit tween");
        animation::tick(MOTION_SECS + 0.02); // anchor the removal timer
        animation::tick(2.0 * MOTION_SECS + 0.05); // past the exit window
        assert!(!present(), "removed after the exit tween");
    }
}
