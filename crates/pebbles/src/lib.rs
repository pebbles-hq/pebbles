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
        Alignment, Axis, Color, CrossAxisAlignment, EdgeInsets, FlexFit, MainAxisAlignment,
        MainAxisSize, Offset, Rect, Size, TextAlign, VerticalDirection, palette,
    };

    // render-level styling primitives (for advanced/custom decoration) + events
    pub use pebbles_render::{
        Affine, BlendMode, Border, BorderRadius, BorderSide, BoxDecoration, BoxShadow, BoxShape,
        Cursor, Gradient, IconData, IconKind, IconPrim, Image, ImageFit, PointerButton, PointerEvent, TextFieldStyle,
        lucide,
    };

    // runtime (pebbles-core): reactivity (SolidJS-style) + function components + focus
    pub use pebbles_core::{
        Channel, Component, Element, FocusNode, KeyInput, Motion, Signal, Store, action,
        action_event, animate_to, animated, channel, component, component_props, create_effect,
        create_focus, create_memo, create_signal, create_store,
    };

    // runtime (pebbles-core): the widget contract + reconciler handles
    pub use pebbles_core::{
        AnyWidget, Callback, ElementId, IntoWidget, ParentDataWidget, RenderWidget, Ui, Widget,
        WidgetExt,
    };

    // runtime (pebbles-core): legacy class-widget API (being migrated to signals)
    pub use pebbles_core::{BuildContext, State, StatefulWidget, StatelessWidget};

    // theming + the general style system (RN/CSS-like, apply anywhere)
    pub use pebbles_widgets::{Colors, Style, StyleExt, Theme, image_from_bytes, image_from_path, style, styled, theme};

    // the global overlay layer (dropdowns / menus / popovers)
    pub use pebbles_widgets::{OverlayHost, hide_overlay, show_overlay};

    // modal dialogs (main-window overlay)
    pub use pebbles_widgets::{Dialog, DialogId, close_dialog, dialog};

    // secondary OS windows (share the runtime; talk via signals / Channel)
    pub use pebbles_widgets::{Window, WindowId, close_window, window};

    // widgets: layout primitives + constructors
    pub use pebbles_widgets::{
        Align, AspectRatio, ClipRRect, ColoredBox, Column, ConstrainedBox, Container, DecoratedBox,
        EditableText, Expanded, Flexible, GestureDetector, GridView, ListView, Opacity, Padding,
        Positioned, Row, ScrollController, ScrollExt, ScrollbarPolicy, ScrollbarStyle,
        ImageView, SingleChildScrollView, SizedBox, Spinner, Stack, Text, Transform, View, Wrap, aspect_ratio,
        center, column, editable, list_view, row, sized_box, spacer, spinner, stack, text, transform,
        use_scroll_controller, wrap,
    };

    // the shadcn-style component catalog
    pub use pebbles_widgets::components::*;

    // widget-impl macros (pebbles-core)
    pub use pebbles_core::{
        children, parent_data_widget, render_widget, stateful_widget, stateless_widget,
    };

    // the app runner
    pub use pebbles_shell::App;
}
