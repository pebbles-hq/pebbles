//! Layout primitives — boxes, flex, stack, sizing, and multi-child layout.

mod boxes;
mod container;
mod flex;
mod flex_children;
#[allow(clippy::module_inception)] // the group's core layout widgets (AspectRatio, Wrap)
mod layout;
mod layout_extra;
mod sizing;
mod stack;

pub use boxes::{
    Align, ColoredBox, ConstrainedBox, Padding, SizedBox, align, center, colored_box, constrained_box, gap_h,
    gap_w, padding, sized_box,
};
pub use container::{Container, container};
pub use flex::{Column, Row, column, row};
pub use flex_children::{Expanded, Flexible, expanded, flexible, spacer};
pub use layout::{AspectRatio, Wrap, aspect_ratio, wrap};
pub use layout_extra::{
    Baseline, CustomMultiChildLayout, CustomSingleChildLayout, Flow, FractionalTranslation, LayoutBuilder,
    LayoutTable, ListBody, Offstage, RotatedBox, SizedOverflowBox, Visibility, baseline,
    custom_multi_child_layout, custom_single_child_layout, flow, fractional_translation, indexed_stack,
    layout_builder, layout_table, list_body, offstage, rotated_box, sized_overflow_box, unconstrained_box,
    visibility,
};
pub use sizing::{
    FittedBox, FractionallySizedBox, IntrinsicHeight, IntrinsicWidth, LimitedBox, OverflowBox, fitted_box,
    fractionally_sized_box, intrinsic_height, intrinsic_width, limited_box, overflow_box,
};
pub use stack::{Positioned, Stack, positioned, stack};
