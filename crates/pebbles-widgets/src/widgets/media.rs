//! Window metrics + the widgets that consume them: [`media_query`] / [`MediaQueryData`]
//! (Flutter's `MediaQuery`), [`safe_area`] (Flutter's `SafeArea`), and
//! [`orientation_builder`] (Flutter's `OrientationBuilder`).
//!
//! [`MediaQueryData`] unifies the window size, orientation, safe-area `padding`, soft-
//! keyboard `view_insets`, device pixel ratio and text scale. On desktop/web `padding`
//! and `view_insets` are zero and `device_pixel_ratio`/`text_scale` are `1.0` — the
//! mobile shell will fill the real values. `size` tracks the window; because it reads a
//! non-reactive metric it isn't guaranteed to rebuild on resize — for a resize-reactive
//! layout use [`orientation_builder`] (which reads its allotted bounds).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::{EdgeInsets, Size};

use crate::overlay::window_size;
use crate::widgets::padding;
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Element, Signal, component_props, create_root_signal, use_bounds};

// ---------------------------------------------------------------------------
// Runtime metrics — the mutable, shell-reported slice of MediaQueryData.
// ---------------------------------------------------------------------------

/// The mutable window metrics the shell (or a test) reports — everything in
/// [`MediaQueryData`] except `size` (which tracks the live window). Reactive:
/// setting any field re-renders the components that read `media_query()`.
#[derive(Clone, Copy, PartialEq, Debug)]
struct MediaMetrics {
    padding: EdgeInsets,
    view_insets: EdgeInsets,
    device_pixel_ratio: f64,
    text_scale: f64,
}

impl Default for MediaMetrics {
    fn default() -> Self {
        // Desktop/web defaults: no insets, unit dpr + text scale.
        MediaMetrics {
            padding: EdgeInsets::ZERO,
            view_insets: EdgeInsets::ZERO,
            device_pixel_ratio: 1.0,
            text_scale: 1.0,
        }
    }
}

thread_local! {
    static METRICS: RefCell<HashMap<u32, Signal<MediaMetrics>>> = RefCell::new(HashMap::new());
}

/// The current window's metrics signal (created once, reused).
fn metrics_signal() -> Signal<MediaMetrics> {
    let window = current_window();
    METRICS.with(|cell| {
        *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(MediaMetrics::default()))
    })
}

/// Report the soft-keyboard insets for the current window (Flutter's
/// `viewInsets`). The mobile shell calls this as the keyboard shows/hides; it
/// drives [`Scaffold::resize_to_avoid_bottom_inset`](crate::Scaffold::resize_to_avoid_bottom_inset).
pub fn set_view_insets(insets: EdgeInsets) {
    metrics_signal().update(|m| m.view_insets = insets);
}

/// Report the safe-area insets (notch / status bar / home indicator) — drives
/// [`safe_area`].
pub fn set_safe_area_padding(padding: EdgeInsets) {
    metrics_signal().update(|m| m.padding = padding);
}

/// Report the device pixel ratio (physical px per logical px).
pub fn set_device_pixel_ratio(dpr: f64) {
    metrics_signal().update(|m| m.device_pixel_ratio = dpr.max(0.1));
}

/// Report the user text-scale factor.
pub fn set_text_scale(scale: f64) {
    metrics_signal().update(|m| m.text_scale = scale.max(0.1));
}

/// Forget a closed window's metrics (the shell calls this on window close).
pub fn drop_window_metrics(window: u32) {
    METRICS.with(|cell| {
        cell.borrow_mut().remove(&window);
    });
}

/// Portrait (taller than wide) or landscape (wider than tall).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Orientation {
    Portrait,
    Landscape,
}

/// Window metrics for the current window (Flutter's `MediaQueryData`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MediaQueryData {
    /// Logical window size.
    pub size: Size,
    /// Safe-area insets (notch / status bar / home indicator). Zero on desktop/web.
    pub padding: EdgeInsets,
    /// Insets covered by system UI — the soft keyboard. Zero on desktop/web.
    pub view_insets: EdgeInsets,
    /// Portrait or landscape, from the window aspect.
    pub orientation: Orientation,
    /// Physical pixels per logical pixel (`1.0` until the shell reports it).
    pub device_pixel_ratio: f64,
    /// User text-scale factor (`1.0` until the shell reports it).
    pub text_scale: f64,
}

impl MediaQueryData {
    /// Whether the window is taller than it is wide.
    pub fn is_portrait(&self) -> bool {
        self.orientation == Orientation::Portrait
    }
    /// The responsive [`Breakpoint`] for the current window width.
    pub fn breakpoint(&self) -> Breakpoint {
        Breakpoint::for_width(self.size.width)
    }
}

/// A responsive size class derived from the window width — the basis for
/// breakpoint-driven layouts (like CSS media queries). Thresholds:
/// `Mobile < 700 ≤ Tablet < 1200 ≤ Desktop`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Breakpoint {
    /// Phones and narrow windows (`< 700` logical px).
    Mobile,
    /// Tablets and split-screen windows (`700..1200`).
    Tablet,
    /// Full desktop windows (`≥ 1200`).
    Desktop,
}

impl Breakpoint {
    /// The breakpoint for a given logical window width.
    pub fn for_width(width: f64) -> Breakpoint {
        if width < 700.0 {
            Breakpoint::Mobile
        } else if width < 1200.0 {
            Breakpoint::Tablet
        } else {
            Breakpoint::Desktop
        }
    }
    /// Pick a value per breakpoint (`mobile` / `tablet` / `desktop`).
    pub fn select<T>(self, mobile: T, tablet: T, desktop: T) -> T {
        match self {
            Breakpoint::Mobile => mobile,
            Breakpoint::Tablet => tablet,
            Breakpoint::Desktop => desktop,
        }
    }
}

/// The current window's responsive [`Breakpoint`] — **reactive**, so a component that
/// reads it re-renders when the window crosses a breakpoint. The one call most
/// responsive layouts need.
pub fn breakpoint() -> Breakpoint {
    Breakpoint::for_width(window_size().0)
}

/// The current window's [`MediaQueryData`]. Reactive in the shell-reported fields
/// (padding / view insets / dpr / text scale) — reading it subscribes the calling
/// component, so a `set_view_insets` re-renders it. See the module docs for the
/// desktop/mobile split of each field.
pub fn media_query() -> MediaQueryData {
    let (w, h) = window_size();
    let m = metrics_signal().get();
    MediaQueryData {
        size: Size::new(w, h),
        padding: m.padding,
        view_insets: m.view_insets,
        orientation: if h >= w { Orientation::Portrait } else { Orientation::Landscape },
        device_pixel_ratio: m.device_pixel_ratio,
        text_scale: m.text_scale,
    }
}

// ===========================================================================
// SafeArea
// ===========================================================================

/// Insets a child past the system UI (notch, status bar, home indicator) by the
/// [`MediaQueryData::padding`]. Flutter's `SafeArea`. A no-op on desktop/web (zero
/// padding); real once the mobile shell reports insets. Built by [`safe_area`].
#[derive(Clone)]
pub struct SafeArea {
    child: Option<AnyWidget>,
    left: bool,
    top: bool,
    right: bool,
    bottom: bool,
}

/// See [`SafeArea`]. All four edges are inset by default.
pub fn safe_area(child: impl IntoWidget) -> SafeArea {
    SafeArea { child: Some(child.into_widget()), left: true, top: true, right: true, bottom: true }
}

impl SafeArea {
    /// Inset the top edge (default `true`).
    pub fn top(mut self, on: bool) -> Self {
        self.top = on;
        self
    }
    /// Inset the bottom edge (default `true`).
    pub fn bottom(mut self, on: bool) -> Self {
        self.bottom = on;
        self
    }
    /// Inset the left edge (default `true`).
    pub fn left(mut self, on: bool) -> Self {
        self.left = on;
        self
    }
    /// Inset the right edge (default `true`).
    pub fn right(mut self, on: bool) -> Self {
        self.right = on;
        self
    }
}

impl IntoWidget for SafeArea {
    fn into_widget(mut self) -> AnyWidget {
        let p = media_query().padding;
        let insets = EdgeInsets {
            left: if self.left { p.left } else { 0.0 },
            top: if self.top { p.top } else { 0.0 },
            right: if self.right { p.right } else { 0.0 },
            bottom: if self.bottom { p.bottom } else { 0.0 },
        };
        let child = self.child.take().unwrap_or_else(|| crate::widgets::gap_h(0.0).into_widget());
        padding(insets, child).into_widget()
    }
}

// ===========================================================================
// OrientationBuilder
// ===========================================================================

/// Rebuilds with the current [`Orientation`] of the space it's given. Flutter's
/// `OrientationBuilder`. Reactive to resize (it reads its allotted bounds, one frame
/// behind). Built by [`orientation_builder`].
#[derive(Clone)]
pub struct OrientationBuilder {
    builder: Rc<dyn Fn(Orientation) -> AnyWidget>,
}

/// See [`OrientationBuilder`]. `builder(orientation)` is called with `Portrait` when the
/// allotted box is taller than wide, else `Landscape`.
pub fn orientation_builder<W: IntoWidget>(
    builder: impl Fn(Orientation) -> W + 'static,
) -> OrientationBuilder {
    OrientationBuilder { builder: Rc::new(move |o| builder(o).into_widget()) }
}

impl IntoWidget for OrientationBuilder {
    fn into_widget(self) -> AnyWidget {
        component_props(render_orientation_builder, self).into_widget()
    }
}

fn render_orientation_builder(b: &OrientationBuilder) -> Element {
    let bounds = use_bounds(); // this widget's laid-out box, one frame behind
    let o = if bounds.height() >= bounds.width() { Orientation::Portrait } else { Orientation::Landscape };
    (b.builder)(o)
}
