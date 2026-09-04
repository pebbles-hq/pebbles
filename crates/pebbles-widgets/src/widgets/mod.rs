//! The widget catalog — the Flutter-equivalent building blocks. Each widget is a
//! thin, immutable configuration over a render object (or a composite of them).
//!
//! Constructors follow a fluent style: `text("Hi").size(24.0).color(BLUE)`,
//! `column(children![...])`, `Container::new().color(..).padding(..)`.

mod animated;
mod boxes;
mod canvas;
mod container;
mod decorated;
mod editable;
mod effects;
mod flex;
mod flex_children;
mod focus_scope;
mod gesture;
mod layout;
mod list;
mod probe;
mod scroll;
mod semantics;
mod sizing;
mod spinner;
mod stack;
mod text;
mod view;

mod transform;
pub use transform::{Transform, transform};
pub use animated::{AnimatedContainer, animated_container};
pub use boxes::{Align, ColoredBox, ConstrainedBox, Padding, SizedBox, center, gap_h, gap_w, sized_box};
pub use canvas::{CanvasWidget, canvas};
pub use container::Container;
pub use decorated::DecoratedBox;
pub use editable::{EditableText, editable};
pub use effects::{ClipRRect, Opacity, RepaintBoundary, repaint_boundary};
pub use flex::{Column, Row, column, row};
pub use flex_children::{Expanded, Flexible, spacer};
pub use focus_scope::focus_scope;
pub use gesture::GestureDetector;
pub use layout::{AspectRatio, Wrap, aspect_ratio, wrap};
pub use list::{GridView, ListView, ScrollController, use_scroll_controller};
pub use probe::{ExtentProbe, extent_probe};
pub use scroll::{ScrollExt, SingleChildScrollView, list_view};
pub use semantics::{Semantics, SemanticsExt, semantics};
pub use pebbles_render::{ScrollbarPolicy, ScrollbarStyle, SemanticsProps, SemanticsRole};
pub use sizing::{
    FittedBox, FractionallySizedBox, IntrinsicHeight, IntrinsicWidth, LimitedBox, OverflowBox,
    fitted_box, fractionally_sized_box, intrinsic_height, intrinsic_width, limited_box,
    overflow_box,
};
pub use spinner::{Spinner, spinner};
pub use stack::{Positioned, Stack, stack};
pub use text::{RichText, Text, TextSpan, span, text, text_rich, text_signal};
pub use view::View;
