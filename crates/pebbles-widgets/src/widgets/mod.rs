//! The widget catalog — the Flutter-equivalent building blocks. Each widget is a
//! thin, immutable configuration over a render object (or a composite of them).
//!
//! Constructors follow a fluent style: `text("Hi").size(24.0).color(BLUE)`,
//! `column(children![...])`, `Container::new().color(..).padding(..)`.

mod boxes;
mod container;
mod decorated;
mod editable;
mod effects;
mod flex;
mod flex_children;
mod gesture;
mod layout;
mod list;
mod scroll;
mod semantics;
mod spinner;
mod stack;
mod text;
mod view;

mod transform;
pub use transform::{Transform, transform};
pub use boxes::{Align, ColoredBox, ConstrainedBox, Padding, SizedBox, center, sized_box};
pub use container::Container;
pub use decorated::DecoratedBox;
pub use editable::{EditableText, editable};
pub use effects::{ClipRRect, Opacity};
pub use flex::{Column, Row, column, row};
pub use flex_children::{Expanded, Flexible, spacer};
pub use gesture::GestureDetector;
pub use layout::{AspectRatio, Wrap, aspect_ratio, wrap};
pub use list::{GridView, ListView, ScrollController, use_scroll_controller};
pub use scroll::{ScrollExt, SingleChildScrollView, list_view};
pub use semantics::{Semantics, SemanticsExt, semantics};
pub use pebbles_render::{ScrollbarPolicy, ScrollbarStyle, SemanticsProps, SemanticsRole};
pub use spinner::{Spinner, spinner};
pub use stack::{Positioned, Stack, stack};
pub use text::{Text, text};
pub use view::View;
