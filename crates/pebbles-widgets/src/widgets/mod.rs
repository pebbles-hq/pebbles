//! The widget catalog — the Flutter-equivalent building blocks. Each widget is a
//! thin, immutable configuration over a render object (or a composite of them).
//!
//! Constructors follow a fluent style: `text("Hi").size(24.0).color(BLUE)`,
//! `column(children![...])`, `Container::new().color(..).padding(..)`.

mod animated;
mod animated_list;
mod boxes;
mod canvas;
mod container;
mod decorated;
mod default_text_style;
mod dnd;
mod editable;
mod effects;
mod effects_extra;
mod flex;
mod flex_children;
mod focus_scope;
mod gesture;
mod interactive_viewer;
mod keyed;
mod layout;
mod layout_extra;
mod list;
mod media;
mod mobile_runtime;
mod motion;
mod pointer_control;
mod probe;
mod reorderable;
mod scroll;
mod semantics;
mod sizing;
mod spinner;
mod stack;
mod stream_builder;
mod text;
mod view;

mod transform;
pub use animated::{AnimatedContainer, animated_container};
pub use animated_list::{AnimatedGrid, AnimatedList, animated_grid, animated_list};
pub use boxes::{
    Align, ColoredBox, ConstrainedBox, Padding, SizedBox, align, center, colored_box, constrained_box, gap_h,
    gap_w, padding, sized_box,
};
pub use canvas::{CanvasWidget, canvas};
pub use container::{Container, container};
pub use decorated::DecoratedBox;
pub use default_text_style::{DefaultTextStyle, animated_default_text_style, default_text_style};
pub use dnd::{DragTarget, Draggable, drag_target, draggable, long_press_draggable};
pub use editable::{EditableText, editable};
pub use effects::{ClipRRect, Opacity, RepaintBoundary, clip_rrect, opacity, repaint_boundary};
pub use effects_extra::{
    ClipOval, ClipPath, ColorFiltered, ShaderMask, clip_oval, clip_path, clip_rect, color_filtered,
    shader_mask,
};
pub use flex::{Column, Row, column, row};
pub use flex_children::{Expanded, Flexible, expanded, flexible, spacer};
pub use focus_scope::focus_scope;
pub use gesture::{GestureDetector, gesture_detector};
pub use interactive_viewer::{InteractiveViewer, interactive_viewer};
pub use keyed::{Keyed, keyed};
pub use layout::{AspectRatio, Wrap, aspect_ratio, wrap};
pub use layout_extra::{
    Baseline, CustomMultiChildLayout, CustomSingleChildLayout, Flow, FractionalTranslation, LayoutBuilder,
    LayoutTable, ListBody, Offstage, RotatedBox, SizedOverflowBox, Visibility, baseline,
    custom_multi_child_layout, custom_single_child_layout, flow, fractional_translation, indexed_stack,
    layout_builder, layout_table, list_body, offstage, rotated_box, sized_overflow_box, unconstrained_box,
    visibility,
};
pub use list::{GridView, ListView, ScrollController, use_scroll_controller};
pub use media::{
    Breakpoint, MediaQueryData, Orientation, OrientationBuilder, SafeArea, breakpoint, drop_window_metrics,
    media_query, orientation_builder, safe_area, set_device_pixel_ratio, set_safe_area_padding,
    set_text_scale, set_view_insets,
};
pub use mobile_runtime::{
    PopScope, SystemUiOverlayStyle, back_is_blocked, dispatch_back, pop_scope, set_system_ui_overlay_style,
    system_ui_overlay_style,
};
pub use motion::{
    AnimatedAlign, AnimatedCrossFade, AnimatedOpacity, AnimatedPadding, AnimatedPositioned, AnimatedRotation,
    AnimatedScale, AnimatedSlide, AnimatedSwitcher, DecoratedBoxTransition, Dismissible, FadeTransition,
    PositionedTransition, RotationTransition, ScaleTransition, SizeTransition, SlideTransition,
    animated_align, animated_cross_fade, animated_opacity, animated_padding, animated_positioned,
    animated_rotation, animated_scale, animated_slide, animated_switcher, decorated_box_transition,
    dismissible, fade_transition, positioned_transition, rotation_transition, scale_transition,
    size_transition, slide_transition,
};
pub use pebbles_render::TableColumnWidth;
pub use pebbles_render::{
    ScrollEvent, ScrollMetrics, ScrollNotification, ScrollbarPolicy, ScrollbarStyle, SemanticsProps,
    SemanticsRole,
};
pub use pointer_control::{PointerControl, absorb_pointer, ignore_pointer};
pub use probe::{ExtentProbe, extent_probe};
pub use reorderable::{ReorderableListView, reorderable_list_view};
pub use scroll::{ScrollExt, SingleChildScrollView, list_view, scroll_view};
pub use semantics::{
    Semantics, SemanticsBoundary, SemanticsExt, block_semantics, exclude_semantics, merge_semantics,
    semantics,
};
pub use sizing::{
    FittedBox, FractionallySizedBox, IntrinsicHeight, IntrinsicWidth, LimitedBox, OverflowBox, fitted_box,
    fractionally_sized_box, intrinsic_height, intrinsic_width, limited_box, overflow_box,
};
pub use spinner::{Spinner, spinner};
pub use stack::{Positioned, Stack, positioned, stack};
pub use stream_builder::{StreamBuilder, stream_builder};
pub use text::{RichText, Text, TextSpan, span, text, text_rich, text_signal};
pub use transform::{Transform, transform};
pub use view::View;
