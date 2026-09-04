//! Child-first **modifiers** (SwiftUI-style) that cut Flutter's inside-out wrapper
//! nesting. Instead of
//!
//! ```ignore
//! center(Padding::new(EdgeInsets::all(12.0), ClipRRect::new(BorderRadius::all(8.0), child)))
//! ```
//!
//! write
//!
//! ```ignore
//! child.clipped(8.0).padded(12.0).centered()
//! ```
//!
//! Each modifier returns the wrapping widget (which is itself `IntoWidget`), so they
//! chain. This complements the existing [`ScrollExt`](crate::ScrollExt) /
//! [`StyleExt`](crate::StyleExt) traits.

use pebbles_foundation::EdgeInsets;
use pebbles_render::BorderRadius;

use crate::widgets::{Align, ClipRRect, Expanded, Opacity, Padding, SizedBox, center};
use pebbles_core::widget::IntoWidget;

/// SwiftUI-style child-first modifiers on any widget.
pub trait ModifierExt: IntoWidget + Sized {
    /// Uniform padding on all sides.
    fn padded(self, all: f64) -> Padding {
        Padding::new(EdgeInsets::all(all), self)
    }
    /// Symmetric horizontal / vertical padding.
    fn padded_xy(self, horizontal: f64, vertical: f64) -> Padding {
        Padding::new(EdgeInsets::symmetric(horizontal, vertical), self)
    }
    /// Explicit padding.
    fn padding(self, insets: EdgeInsets) -> Padding {
        Padding::new(insets, self)
    }
    /// Center within the available space.
    fn centered(self) -> Align {
        center(self)
    }
    /// Fill the free space along the enclosing flex's main axis.
    fn expanded(self) -> Expanded {
        Expanded::new(self)
    }
    /// Fix the widget to `width × height`.
    fn sized(self, width: f64, height: f64) -> SizedBox {
        SizedBox::new(Some(width), Some(height), Some(self.into_widget()))
    }
    /// Clip to a uniform corner radius.
    fn clipped(self, radius: f64) -> ClipRRect {
        ClipRRect::new(BorderRadius::all(radius), self)
    }
    /// Apply opacity (`0.0..=1.0`).
    fn opacity(self, opacity: f32) -> Opacity {
        Opacity::new(opacity, self)
    }
}

impl<W: IntoWidget> ModifierExt for W {}
