//! # Pebbles
//!
//! A Flutter-style, desktop-first GUI framework built on [Vello](https://vello.dev).
//!
//! Pebbles keeps Flutter's three-tree architecture — **Widget → Element →
//! RenderObject** — and its box layout protocol (constraints down, sizes up, parent
//! sets position), but implements them in idiomatic Rust: the element and render
//! trees are generational arenas, and `setState` is a borrow-safe, type-erased
//! callback rather than an `Rc<RefCell>` dance.
//!
//! This umbrella crate re-exports the layered crates and offers a [`prelude`] with
//! everything you need to write an app.
//!
//! ```ignore
//! use pebbles::prelude::*;
//!
//! struct Counter;
//! struct CounterState { count: i64 }
//!
//! impl StatefulWidget for Counter {
//!     fn create_state(&self) -> Box<dyn State> {
//!         Box::new(CounterState { count: 0 })
//!     }
//! }
//! stateful_widget!(Counter);
//!
//! impl State for CounterState {
//!     fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
//!     fn build(&mut self, cx: &mut BuildContext) -> AnyWidget {
//!         let inc = cx.callback(|s: &mut CounterState| s.count += 1);
//!         center(column(children![
//!             text(format!("{}", self.count)).size(48.0),
//!             GestureDetector::on_tap(inc, Container::new().color(palette::BLUE)
//!                 .padding(EdgeInsets::all(12.0)).child(text("+").color(palette::WHITE))),
//!         ])).into_widget()
//!     }
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     App::new(Counter).title("Counter").run()
//! }
//! ```

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
        Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconKind, PointerButton,
        PointerEvent, TextFieldStyle,
    };

    // runtime (pebbles-core): reactivity (SolidJS-style) + function components + focus
    pub use pebbles_core::{
        Component, Element, FocusNode, KeyInput, Motion, Signal, Store, action, action_event,
        animate_to, animated, component, component_props, create_effect, create_focus, create_memo,
        create_signal, create_store,
    };

    // runtime (pebbles-core): the widget contract + reconciler handles
    pub use pebbles_core::{
        AnyWidget, Callback, ElementId, IntoWidget, ParentDataWidget, RenderWidget, Ui, Widget,
        WidgetExt,
    };

    // runtime (pebbles-core): legacy class-widget API (being migrated to signals)
    pub use pebbles_core::{BuildContext, State, StatefulWidget, StatelessWidget};

    // theming + the general style system (RN/CSS-like, apply anywhere)
    pub use pebbles_widgets::{Colors, Style, StyleExt, Theme, style, styled, theme};

    // the global overlay layer (dropdowns / menus / popovers)
    pub use pebbles_widgets::{OverlayHost, hide_overlay, show_overlay};

    // widgets: layout primitives + constructors
    pub use pebbles_widgets::{
        Align, AspectRatio, ClipRRect, ColoredBox, Column, ConstrainedBox, Container, DecoratedBox,
        EditableText, Expanded, Flexible, GestureDetector, GridView, ListView, Opacity, Padding,
        Positioned, Row, ScrollController, ScrollExt, ScrollbarPolicy, ScrollbarStyle,
        SingleChildScrollView, SizedBox, Spinner, Stack, Text, View, Wrap, aspect_ratio, center,
        column, editable, list_view, row, spacer, spinner, stack, text, use_scroll_controller,
        wrap,
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
