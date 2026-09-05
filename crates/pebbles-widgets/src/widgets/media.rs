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

use std::rc::Rc;

use pebbles_foundation::{EdgeInsets, Size};

use crate::overlay::window_size;
use crate::widgets::padding;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Element, component_props, use_bounds};

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
}

/// The current window's [`MediaQueryData`]. See the module docs for the desktop/mobile
/// split of each field.
pub fn media_query() -> MediaQueryData {
    let (w, h) = window_size();
    MediaQueryData {
        size: Size::new(w, h),
        padding: EdgeInsets::ZERO,
        view_insets: EdgeInsets::ZERO,
        orientation: if h >= w { Orientation::Portrait } else { Orientation::Landscape },
        device_pixel_ratio: 1.0,
        text_scale: 1.0,
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
