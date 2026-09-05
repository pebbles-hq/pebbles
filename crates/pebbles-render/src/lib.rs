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
pub mod fonts;
pub mod inspect;
pub mod object;
pub mod objects;
pub mod scroll_metrics;
pub mod stats;
pub mod text;
pub mod text_edit;
pub mod tree;

pub use constraints::BoxConstraints;
pub use decoration::{
    BlendMode, Border, BorderRadius, BorderSide, BoxDecoration, BoxShadow, BoxShape, Gradient, Image,
    ImageFit, image_from_rgba8,
};
pub use direction::{set_text_direction, text_direction};
pub use inspect::{InspectNode, format_chain, inspect_at};
// (Image = peniko ImageBrush)
pub use fonts::{
    BUILTIN_FAMILIES, available_families, builtin_families, builtin_fonts, has_family, is_builtin,
    register_user_font,
};
pub use object::{HitBehavior, RenderObject, SemanticsFlag};
pub use objects::{
    Canvas, Cursor, FlexParentData, IconData, IconKind, IconPrim, ParagraphStyle, PointerButton,
    PointerEvent, RefreshState, RenderAlign, RenderAspectRatio, RenderBaseline, RenderBoundary, RenderCanvas,
    RenderClipOval, RenderClipPath, RenderClipRRect, RenderColorFilter, RenderColoredBox,
    RenderConstrainedBox, RenderCustomMultiChild, RenderCustomSingleChild, RenderDecoratedBox,
    RenderFittedBox, RenderFlex, RenderFlow, RenderFractionalTranslation, RenderFractionallySizedBox,
    RenderIcon, RenderIntrinsicHeight, RenderIntrinsicWidth, RenderLimitedBox, RenderList,
    RenderMeasureProbe, RenderOffstage, RenderOpacity, RenderOverflowBox, RenderPadding, RenderParagraph,
    RenderPointerBarrier, RenderPointerListener, RenderRotatedBox, RenderScroll, RenderSemanticsBoundary,
    RenderShaderMask, RenderSizedOverflowBox, RenderSpinner, RenderStack, RenderTable, RenderTextField,
    RenderTransform, RenderView, RenderWrap, ScrollEvent, ScrollMetrics, ScrollNotification, ScrollPhysics,
    ScrollbarPolicy, ScrollbarStyle, SemanticsNode, SemanticsProps, SemanticsRole, SizeFn, StackFit,
    StackParentData, TableColumnWidth, TapCallback, TextFieldStyle, TextSpanStyle, lucide,
};
pub use text::TextEnv;
pub use tree::{IntrinsicCx, LayoutCx, PaintCx, RenderId, RenderNode, RenderTree};

/// Re-export the vello scene type the paint layer targets.
pub use vello::Scene;
/// Re-export the 2D affine transform used by [`RenderTransform`].
pub use vello::kurbo::Affine;
/// Re-export the Bézier path type used by `ClipPath` clip delegates.
pub use vello::kurbo::BezPath;
