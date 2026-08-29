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
pub mod overlay;
pub mod style;
pub mod theme;
pub mod widgets;

pub use overlay::{OverlayHost, hide_overlay, show_overlay};
pub use style::{Style, StyleExt, style, styled};
pub use theme::{Colors, Theme, theme};
pub use widgets::{
    Align, AspectRatio, ClipRRect, ColoredBox, Column, ConstrainedBox, Container, DecoratedBox,
    EditableText, Expanded, Flexible, GestureDetector, GridView, ListView, Opacity, Padding,
    Positioned, Row, ScrollController, ScrollExt, ScrollbarPolicy, ScrollbarStyle,
    SingleChildScrollView, SizedBox, Spinner, Stack, Text, View, Wrap, aspect_ratio, center,
    column, editable, list_view, row, spacer, spinner, stack, text, use_scroll_controller, wrap,
};

pub use components::*;
