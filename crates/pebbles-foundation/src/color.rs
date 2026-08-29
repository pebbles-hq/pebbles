//! Color primitives — a thin wrapper over [`peniko::Color`].
//!
//! The built-in palette (the full Tailwind/shadcn color scale) lives in the sibling
//! [`crate::palette`] module.

/// The framework color type. Re-exported from `peniko` so brushes and gradients
/// interoperate without conversions.
pub type Color = peniko::Color;
