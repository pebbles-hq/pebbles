//! # pebbles-widgets
//!
//! The catalog layer — the Flutter-style widgets and shadcn-style components
//! developers actually compose. The runtime that builds and reconciles them lives
//! in [`pebbles_core`] (re-exported through the `pebbles` umbrella's prelude).
//!
//! * The primitives in [`widgets`] — `Text`, `Row`, `Column`, `Container`,
//!   `GestureDetector`, `Padding`, `Align`/`center`, `SizedBox`, `Stack`, …
//! * The higher-level set in [`components`] — buttons, toggles, tabs, tables,
//!   navigation chrome, surfaces — built from those primitives.
//! * The design system: [`Theme`]/[`Colors`] tokens and the general [`Style`] system.

//! ### Crate layout
//!
//! Four groups, each a directory:
//!
//! * [`widgets`] — the Flutter-style primitives (each backed by a render object)
//! * [`components`] — the shadcn-style catalog, by role (input / display /
//!   layout / navigation)
//! * `design` — the look: theme tokens, the style system, modifiers, fonts,
//!   text direction
//! * `services` — the ambient layers above the tree: overlays, dialogs, sheets,
//!   toasts, the global menu
//! * `platform` — OS surfaces: secondary windows, the native menu bar
//!
//! `design`, `services` and `platform` group for navigation only — every module
//! inside them is re-exported here, so the public paths remain flat
//! (`pebbles_widgets::theme`, `pebbles_widgets::overlay`, …).

pub mod components;
pub mod side;
pub mod widgets;

mod design;
mod platform;
mod services;

// The grouped modules, re-exported flat so the public API is unchanged by the
// internal organization.
#[cfg(feature = "image-view")]
pub use components::display::image_view;
pub use design::{fonts, modifiers, style, text_direction, theme};
pub use platform::{native_menu, window};
pub use services::{dialog, global_menu, overlay, sheet, toast};

pub use dialog::{AlertDialog, Dialog, DialogId, alert_dialog, close_dialog, dialog};
pub use fonts::{builtins, families, has, is_builtin};
pub use global_menu::{
    block_context_menu, global_menu_on, is_global_menu_enabled, on_context_menu, reset_global_menu,
    set_global_menu, set_global_menu_enabled, set_global_menu_style, set_global_menu_width,
    show_here as show_global_menu_here,
};
pub use window::{
    MonitorInfo, Window, WindowId, close_window, focus_window, minimize_window, monitors, set_monitors,
    set_window_maximized, set_window_position, set_window_resizable, set_window_title, window,
};
// `window::set_window_size` (OS resize) is intentionally NOT re-exported at the crate
// root to avoid colliding with `overlay::set_window_size` (popover sizing); reach it as
// `window::set_window_size`.
#[cfg(feature = "image-view")]
pub use image_view::ImageView;
pub use modifiers::ModifierExt;
pub use native_menu::{MenuBar, NativeEntry, NativeMenu, menu, menu_bar};
pub use overlay::{OverlayHost, hide_overlay, hide_passive, show_overlay, show_passive};
pub use sheet::{Sheet, SheetId, close_sheet, sheet};
pub use side::Side;
pub use style::{Style, StyleExt, style, styled, styles};
#[cfg(feature = "image-view")]
pub use style::{image_from_bytes, image_from_path};
pub use text_direction::{set_text_direction, text_direction};
pub use theme::{Colors, Theme, set_theme, theme, theme_override, toggle_theme};
pub use toast::{Toast, ToastId, ToastVariant, dismiss_toast, toast};
pub use widgets::{
    Align, AnimatedContainer, AnimatedGrid, AnimatedList, AspectRatio, CanvasWidget, ClipRRect, ColoredBox,
    Column, ConstrainedBox, Container, DecoratedBox, EditableText, Expanded, FittedBox, Flexible,
    FractionallySizedBox, GestureDetector, GridView, IntrinsicHeight, IntrinsicWidth, Keyed, LimitedBox,
    ListView, Opacity, OverflowBox, Padding, Positioned, RepaintBoundary, RichText, Row, ScrollController,
    ScrollExt, ScrollbarPolicy, ScrollbarStyle, Semantics, SemanticsBoundary, SemanticsExt, SemanticsProps,
    SemanticsRole, SingleChildScrollView, SizedBox, Spinner, Stack, Text, TextSpan, Transform, View, Wrap,
    align, animated_container, animated_grid, animated_list, aspect_ratio, block_semantics, canvas, center,
    clip_rrect, colored_box, column, constrained_box, container, editable, exclude_semantics, expanded,
    fitted_box, flexible, focus_scope, fractionally_sized_box, gap_h, gap_w, gesture_detector,
    intrinsic_height, intrinsic_width, keyed, limited_box, list_view, merge_semantics, opacity, overflow_box,
    padding, positioned, repaint_boundary, row, scroll_view, semantics, sized_box, spacer, span, spinner,
    stack, text, text_rich, text_signal, transform, use_scroll_controller, wrap,
};
pub use widgets::{
    AnimatedAlign, AnimatedCrossFade, AnimatedOpacity, AnimatedPadding, AnimatedPositioned, AnimatedRotation,
    AnimatedScale, AnimatedSlide, AnimatedSwitcher, DecoratedBoxTransition, Dismissible, FadeTransition,
    PositionedTransition, RotationTransition, ScaleTransition, SizeTransition, SlideTransition,
    animated_align, animated_cross_fade, animated_opacity, animated_padding, animated_positioned,
    animated_rotation, animated_scale, animated_slide, animated_switcher, decorated_box_transition,
    dismissible, fade_transition, positioned_transition, rotation_transition, scale_transition,
    size_transition, slide_transition,
};
// async builder (Flutter's StreamBuilder, over the reactive Channel)
pub use widgets::{StreamBuilder, stream_builder};
// drag & drop / pointer control (Flutter's Draggable / DragTarget / Ignore/AbsorbPointer
// / ReorderableListView / InteractiveViewer)
pub use widgets::{
    DragTarget, Draggable, InteractiveViewer, PointerControl, ReorderableListView, absorb_pointer,
    drag_target, draggable, ignore_pointer, interactive_viewer, long_press_draggable, reorderable_list_view,
};
// painting & effects (siblings of clip_rrect / opacity)
pub use widgets::{
    ClipOval, ClipPath, ColorFiltered, ShaderMask, clip_oval, clip_path, clip_rect, color_filtered,
    shader_mask,
};
// window metrics + mobile widgets (Flutter's MediaQuery / SafeArea / OrientationBuilder)
pub use widgets::{
    MediaQueryData, Orientation, OrientationBuilder, SafeArea, media_query, orientation_builder, safe_area,
};
// layout (the long-tail Flutter layout widgets)
pub use widgets::{
    Baseline, CustomMultiChildLayout, CustomSingleChildLayout, Flow, FractionalTranslation, LayoutBuilder,
    LayoutTable, Offstage, RotatedBox, SizedOverflowBox, TableColumnWidth, Visibility, baseline,
    custom_multi_child_layout, custom_single_child_layout, flow, fractional_translation, indexed_stack,
    layout_builder, layout_table, offstage, rotated_box, sized_overflow_box, unconstrained_box, visibility,
};
// The immediate-mode drawing surface (H2) a `canvas(..)` painter receives.
pub use pebbles_render::Canvas;

pub use components::*;
