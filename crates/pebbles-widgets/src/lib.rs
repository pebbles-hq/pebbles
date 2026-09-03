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

pub mod components;
pub mod dialog;
pub mod fonts;
pub mod global_menu;
#[cfg(feature = "image-view")]
pub mod image_view;
pub mod modifiers;
pub mod native_menu;
pub mod overlay;
pub mod sheet;
pub mod side;
pub mod style;
pub mod text_direction;
pub mod theme;
pub mod toast;
pub mod widgets;
pub mod window;

pub use dialog::{AlertDialog, Dialog, DialogId, alert_dialog, close_dialog, dialog};
pub use fonts::{builtins, families, has, is_builtin};
pub use global_menu::{
    block_context_menu, global_menu_on, is_global_menu_enabled, on_context_menu,
    reset_global_menu, set_global_menu, set_global_menu_enabled, set_global_menu_style,
    set_global_menu_width, show_here as show_global_menu_here,
};
pub use window::{
    MonitorInfo, Window, WindowId, close_window, focus_window, minimize_window, monitors,
    set_monitors, set_window_maximized, set_window_position, set_window_resizable,
    set_window_title, window,
};
// `window::set_window_size` (OS resize) is intentionally NOT re-exported at the crate
// root to avoid colliding with `overlay::set_window_size` (popover sizing); reach it as
// `window::set_window_size`.
#[cfg(feature = "image-view")]
pub use image_view::ImageView;
pub use modifiers::ModifierExt;
pub use native_menu::{MenuBar, NativeEntry, NativeMenu, menu, menu_bar};
pub use overlay::{OverlayHost, hide_overlay, hide_passive, show_overlay, show_passive};
pub use side::Side;
pub use sheet::{Sheet, SheetId, close_sheet, sheet};
pub use text_direction::{set_text_direction, text_direction};
pub use toast::{Toast, ToastId, ToastVariant, dismiss_toast, toast};
pub use style::{Style, StyleExt, style, styled, styles};
#[cfg(feature = "image-view")]
pub use style::{image_from_bytes, image_from_path};
pub use theme::{Colors, Theme, set_theme, theme, theme_override, toggle_theme};
pub use widgets::{
    Align, AnimatedContainer, AspectRatio, CanvasWidget, ClipRRect, ColoredBox, Column, ConstrainedBox,
    Container, DecoratedBox,
    EditableText, Expanded, FittedBox, Flexible, FractionallySizedBox, GestureDetector, GridView,
    IntrinsicHeight, IntrinsicWidth, LimitedBox, ListView, Opacity, OverflowBox, Padding,
    Positioned, Row, ScrollController, ScrollExt, ScrollbarPolicy, ScrollbarStyle, Semantics,
    SemanticsExt, SemanticsProps, SemanticsRole, SingleChildScrollView, SizedBox, Transform, Spinner,
    Stack, Text, View, Wrap, animated_container, aspect_ratio, canvas, center,
    column, editable, fitted_box, focus_scope, fractionally_sized_box, gap_h, gap_w, intrinsic_height,
    intrinsic_width, limited_box, list_view, overflow_box, row, semantics, sized_box, spacer, spinner, stack,
    text, text_signal, transform, use_scroll_controller, wrap,
};
// The immediate-mode drawing surface (H2) a `canvas(..)` painter receives.
pub use pebbles_render::Canvas;

pub use components::*;
