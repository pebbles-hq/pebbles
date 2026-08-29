//! # pebbles-foundation
//!
//! The bottom layer of the Pebbles GUI framework. It has no knowledge of widgets,
//! render objects, or the GPU — it provides the vocabulary every other layer speaks:
//!
//! * **geometry** — [`Offset`], [`Size`], [`Rect`], [`EdgeInsets`], [`Alignment`] and the
//!   kurbo re-exports ([`Point`], [`Affine`], [`Vec2`]).
//! * **layout enums** — [`Axis`], [`MainAxisAlignment`], [`CrossAxisAlignment`], … the
//!   Flutter layout vocabulary.
//! * **color** — the [`peniko::Color`] re-export.
//! * **palette** — the full Tailwind/shadcn color scale (every family, shades 50–950).
//!
//! Everything here is `Copy`, allocation-free and cheap to pass by value.

pub mod color;
pub mod geometry;
pub mod layout;
pub mod palette;

pub use color::Color;
pub use geometry::{
    Alignment, EdgeInsets, Offset, Rect, Size,
    // kurbo re-exports
    Affine, Point, Vec2,
};
pub use layout::{
    Axis, CrossAxisAlignment, FlexFit, MainAxisAlignment, MainAxisSize, TextAlign, TextBaseline,
    TextDirection, VerticalDirection,
};

/// Re-export of the underlying 2D geometry crate so downstream code can reach the full API.
pub use kurbo;
/// Re-export of the styling crate (brushes, gradients, colors).
pub use peniko;
