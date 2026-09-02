//! # Pebbles
//!
//! A Flutter-style, desktop-first GUI framework built on [Vello](https://vello.dev).
//!
//! Pebbles pairs **Flutter's widget model** for building UI with **Solid's signals**
//! for state. It keeps Flutter's three-tree architecture — **Widget → Element →
//! RenderObject** — and box layout protocol (constraints down, sizes up, parent sets
//! position), but state is a `create_signal` you read and write directly: reading a
//! signal inside a component subscribes it, writing re-renders only the components
//! that read it. No `StatefulWidget`, no `setState`, no `Rc<RefCell>` dance.
//!
//! A component is a plain function; local state is a signal; a handler is a closure:
//!
//! ```ignore
//! use pebbles::prelude::*;
//!
//! fn counter() -> Element {
//!     let count = create_signal(0);
//!     center(column(children![
//!         text(format!("{}", count.get())).size(48.0),
//!         button("+").on_pressed(move || count.update(|c| *c += 1)),
//!     ]))
//!     .into_widget()
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     App::new(component(counter)).title("Counter").run()
//! }
//! ```
//!
//! Global state (shared across the whole app, even across windows) is the *same*
//! `create_signal` primitive created at app scope — that's the Solid model. This
//! umbrella crate re-exports the layered crates and offers a [`prelude`] with
//! everything you need.

pub use pebbles_core as core;
pub use pebbles_foundation as foundation;
pub use pebbles_render as render;
pub use pebbles_shell as shell;
pub use pebbles_widgets as widgets;

/// Everything you need to build a Pebbles app, in one glob import.
pub mod prelude {
    // foundation
    pub use pebbles_foundation::{
        Alignment, Axis, BoxFit, Color, CrossAxisAlignment, EdgeInsets, FlexFit,
        MainAxisAlignment, MainAxisSize, Offset, Rect, Size, TextAlign, TextBaseline,
        TextDirection, VerticalDirection, WrapAlignment, palette,
    };

    // render-level styling primitives (for advanced/custom decoration) + events
    pub use pebbles_render::{
        Affine, BlendMode, Border, BorderRadius, BorderSide, BoxConstraints, BoxDecoration, BoxShadow, BoxShape,
        Cursor, Gradient, IconData, IconKind, IconPrim, Image, ImageFit, PointerButton, PointerEvent,
        RefreshState, ScrollPhysics, StackFit, lucide,
    };

    // runtime (pebbles-core): reactivity (SolidJS-style) + function components + focus
    pub use pebbles_core::{
        Channel, Component, Curve, Element, FocusNode, KeyInput, Motion, Resource, ScopeTag,
        Signal, Spring, Store, Transition, action, action_event, animate_spring, animate_to,
        animate_to_with, animated, animated_spring, animated_with, channel, component,
        component_props, consume_context, create_effect, create_focus, create_focus_scope,
        create_memo, create_resource, create_signal, create_store, create_timeout, provide_context,
        spawn, transition,
    };

    // runtime (pebbles-core): the widget contract + reconciler handles
    pub use pebbles_core::{
        AnyWidget, Callback, ElementId, IntoChildren, IntoWidget, ParentDataWidget, RenderWidget,
        Ui, Widget,
    };

    // font discovery: bundled families + everything installed on the host
    pub use pebbles_widgets::{builtins, families, has, is_builtin};

    // theming + the general style system (RN/CSS-like, apply anywhere)
    pub use pebbles_widgets::{Colors, ModifierExt, Style, StyleExt, Theme, image_from_bytes, image_from_path, set_text_direction, set_theme, style, styled, styles, text_direction, theme, theme_override, toggle_theme};

    // accessibility semantics (screen-reader roles/labels/state)
    pub use pebbles_widgets::{Semantics, SemanticsExt, SemanticsProps, SemanticsRole, semantics};

    // the global overlay layer (dropdowns / menus / popovers) + the passive layer
    // (tooltips / hover cards) + toasts
    pub use pebbles_widgets::{
        OverlayHost, Toast, ToastId, ToastVariant, dismiss_toast, hide_overlay, hide_passive,
        show_overlay, show_passive, toast,
    };

    // the global right-click menu (the fallback when nothing claims a right-click)
    pub use pebbles_widgets::{
        block_context_menu, global_menu_on, is_global_menu_enabled, on_context_menu,
        reset_global_menu, set_global_menu, set_global_menu_enabled, set_global_menu_style,
        set_global_menu_width, show_global_menu_here,
    };

    // modal dialogs (main-window overlay) + the AlertDialog preset + Sheet/Drawer
    pub use pebbles_widgets::{
        AlertDialog, Dialog, DialogId, Sheet, SheetId, Side, alert_dialog, close_dialog,
        close_sheet, dialog, sheet,
    };

    // secondary OS windows (share the runtime; talk via signals / Channel) + monitors
    pub use pebbles_widgets::{
        MonitorInfo, Window, WindowId, close_window, focus_window, minimize_window, monitors,
        set_window_maximized, set_window_position, set_window_resizable, set_window_title, window,
    };

    // widgets: layout primitives + constructors
    pub use pebbles_widgets::{
        Align, AnimatedContainer, AspectRatio, Canvas, CanvasWidget, ClipRRect, ColoredBox, Column,
        ConstrainedBox, Container, DecoratedBox,
        EditableText, Expanded, FittedBox, Flexible, FractionallySizedBox, GestureDetector,
        GridView, IntrinsicHeight, IntrinsicWidth, LimitedBox, ListView, Opacity, OverflowBox,
        Padding, Positioned, Row, ScrollController, ScrollExt, ScrollbarPolicy, ScrollbarStyle,
        ImageView, SingleChildScrollView, SizedBox, Spinner, Stack, Text, Transform, View, Wrap,
        animated_container, aspect_ratio, canvas, center, column, editable, fitted_box, focus_scope,
        fractionally_sized_box, gap_h, gap_w, intrinsic_height, intrinsic_width, limited_box,
        list_view, overflow_box, row, sized_box, spacer, spinner, stack, text, transform,
        use_scroll_controller, wrap,
    };

    // the shadcn-style component catalog
    pub use pebbles_widgets::components::*;

    // B3 native OS menu bar spec (attached via `App::menu`; needs the `native-menus`
    // feature to take effect — the in-window `menubar(..)` is the default form)
    pub use pebbles_widgets::{MenuBar, NativeEntry, NativeMenu, menu, menu_bar};

    // widget-impl macros (pebbles-core)
    pub use pebbles_core::{children, parent_data_widget, render_widget};

    // the #[component] authoring macro (F1). Shares the name with the `component(fn)`
    // helper above — they live in different namespaces (attribute macro vs fn).
    pub use pebbles_macros::component;

    // the app runner + B4 global hotkeys (graceful Err without the `global-hotkeys`
    // feature)
    pub use pebbles_shell::{App, HotkeyId, register_global_hotkey, unregister_global_hotkey};
}
