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
pub mod direction;
pub mod inspect;
pub mod object;
pub mod objects;
pub mod scroll_metrics;
pub mod fonts;
pub mod stats;
pub mod text;
pub mod text_edit;
pub mod tree;

#[cfg(test)]
mod tests;

pub use constraints::BoxConstraints;
pub use direction::{set_text_direction, text_direction};
pub use inspect::{InspectNode, format_chain, inspect_at};
pub use decoration::{BlendMode, Border, BorderRadius, BorderSide, BoxDecoration, BoxShadow, BoxShape, Gradient, Image, ImageFit, image_from_rgba8};
// (Image = peniko ImageBrush)
pub use object::RenderObject;
pub use objects::{
    Canvas, Cursor, FlexParentData, IconData, IconKind, IconPrim, ParagraphStyle, PointerButton,
    PointerEvent, RenderAlign,
    RenderAspectRatio, RenderCanvas,
    RenderBoundary, RenderClipRRect, RenderColoredBox, RenderConstrainedBox, RenderDecoratedBox, RenderFlex,
    RenderFittedBox, RenderFractionallySizedBox, RenderIcon, RenderIntrinsicHeight,
    RenderIntrinsicWidth, RenderLimitedBox, RenderMeasureProbe, RenderOpacity, RenderOverflowBox, RenderPadding,
    RenderParagraph, RenderPointerListener, RenderScroll,
    RenderList, RenderSpinner, RenderTransform, RenderStack, RenderTextField, RenderView, RenderWrap, TextSpanStyle,
    RefreshState, ScrollPhysics, ScrollbarPolicy, ScrollbarStyle, SemanticsNode, SemanticsProps, SemanticsRole, StackFit,
    StackParentData, TapCallback, TextFieldStyle, lucide,
};
pub use fonts::{available_families, builtin_families, builtin_fonts, has_family, is_builtin, register_user_font, BUILTIN_FAMILIES};
pub use text::TextEnv;
pub use tree::{IntrinsicCx, LayoutCx, PaintCx, RenderId, RenderNode, RenderTree};

/// Re-export the vello scene type the paint layer targets.
pub use vello::Scene;
/// Re-export the 2D affine transform used by [`RenderTransform`].
pub use vello::kurbo::Affine;
