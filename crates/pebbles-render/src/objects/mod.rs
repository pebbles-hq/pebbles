//! The built-in render objects. Each corresponds to one low-level layout or paint
//! behavior; the widget layer composes them into the familiar widget catalog.

mod aspect;
mod basic;
mod boundary;
mod canvas;
mod decorated;
mod effects;
mod fitted;
mod flex;
mod fractional;
mod icon;
mod intrinsic;
mod layout_extra;
mod list;
mod measure;
mod overflow;
mod paragraph;
mod pointer;
mod scroll;
mod semantics;
mod spinner;
mod stack;
mod text_field;
mod transform;
mod view;
mod wrap;

pub use aspect::RenderAspectRatio;
pub use basic::{RenderAlign, RenderColoredBox, RenderConstrainedBox, RenderPadding, RenderPointerBarrier};
pub use boundary::RenderBoundary;
pub use canvas::{Canvas, RenderCanvas};
pub use decorated::RenderDecoratedBox;
pub use effects::{RenderClipRRect, RenderOpacity};
pub use fitted::RenderFittedBox;
pub use flex::{FlexParentData, RenderFlex};
pub use fractional::RenderFractionallySizedBox;
pub use icon::{IconData, IconKind, IconPrim, RenderIcon, lucide};
pub use intrinsic::{RenderIntrinsicHeight, RenderIntrinsicWidth};
pub use layout_extra::{
    RenderBaseline, RenderCustomMultiChild, RenderCustomSingleChild, RenderFlow, RenderFractionalTranslation,
    RenderOffstage, RenderRotatedBox, RenderSizedOverflowBox, RenderTable, SizeFn, TableColumnWidth,
};
pub use list::RenderList;
pub use measure::RenderMeasureProbe;
pub use overflow::{RenderLimitedBox, RenderOverflowBox};
pub use paragraph::{ParagraphStyle, RenderParagraph, TextSpanStyle};
#[cfg(debug_assertions)]
pub use paragraph::{reset_shape_count, shape_count};
pub use pointer::{Cursor, PointerButton, PointerEvent, RenderPointerListener, TapCallback};
pub use scroll::{RefreshState, RenderScroll, ScrollPhysics, ScrollbarPolicy, ScrollbarStyle};
pub use semantics::{SemanticsNode, SemanticsProps, SemanticsRole};
pub use spinner::RenderSpinner;
pub use stack::{RenderStack, StackFit, StackParentData};
pub use text_field::{RenderTextField, TextFieldStyle};
pub use transform::RenderTransform;
pub use view::RenderView;
pub use wrap::RenderWrap;
