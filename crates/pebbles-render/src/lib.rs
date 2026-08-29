//! # pebbles-render
//!
//! The render layer: the tree of [`RenderObject`]s that actually performs layout
//! and painting. It sits between the widget layer (which builds and reconciles it)
//! and the GPU shell (which rasterizes the [`vello::Scene`] it paints into).
//!
//! ## The box layout protocol
//! Layout follows Flutter's model exactly: a parent passes each child
//! [`BoxConstraints`]; the child returns a [`Size`](pebbles_foundation::Size) that
//! satisfies them; the parent positions the child. See [`RenderObject::layout`].
//!
//! ## The trees are arenas
//! All render objects live in a single [`RenderTree`] arena keyed by generational
//! [`RenderId`]s, traversed through [`LayoutCx`] / [`PaintCx`]. See [`tree`] for why.

pub mod constraints;
pub mod decoration;
pub mod object;
pub mod objects;
pub mod scroll_metrics;
pub mod text;
pub mod text_edit;
pub mod tree;

#[cfg(test)]
mod tests;

pub use constraints::BoxConstraints;
pub use decoration::{Border, BorderRadius, BoxDecoration, BoxShadow};
pub use object::RenderObject;
pub use objects::{
    Cursor, FlexParentData, IconKind, ParagraphStyle, PointerButton, PointerEvent, RenderAlign,
    RenderAspectRatio,
    RenderClipRRect, RenderColoredBox, RenderConstrainedBox, RenderDecoratedBox, RenderFlex,
    RenderIcon, RenderOpacity, RenderPadding, RenderParagraph, RenderPointerListener, RenderScroll,
    RenderList, RenderSpinner, RenderStack, RenderTextField, RenderView, RenderWrap,
    ScrollbarPolicy, ScrollbarStyle, StackFit, StackParentData, TapCallback, TextFieldStyle,
};
pub use text::TextEnv;
pub use tree::{LayoutCx, PaintCx, RenderId, RenderNode, RenderTree};

/// Re-export the vello scene type the paint layer targets.
pub use vello::Scene;
