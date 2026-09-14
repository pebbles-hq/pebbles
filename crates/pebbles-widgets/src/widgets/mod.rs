//! The widget catalog — the Flutter-equivalent building blocks. Each widget is a
//! thin, immutable configuration over a render object (or a composite of them).
//!
//! Constructors follow a fluent style: `text("Hi").size(24.0).color(BLUE)`,
//! `column(children![...])`, `Container::new().color(..).padding(..)`.
//!
//! Grouped by concern (each subdir's `mod.rs` is its front door): `layout`,
//! `animation`, `interaction`, `scrolling`, `painting`, `text`. A handful
//! of cross-cutting singletons stay at this level (`view`, `media`, `semantics`,
//! `keyed`, `probe`, `spinner`, `stream_builder`, `mobile_runtime`). Everything is
//! re-exported flat here, so the public path is `pebbles_widgets::<Item>` regardless
//! of which subdir a widget lives in.

// Concern groups.
mod animation;
mod interaction;
mod layout;
mod painting;
mod scrolling;
mod text;

// Cross-cutting singletons (each a distinct, standalone concern).
mod keyed;
mod media;
mod mobile_runtime;
mod probe;
mod semantics;
mod spinner;
mod stream_builder;
mod view;

pub use animation::*;
pub use interaction::*;
pub use layout::*;
pub use painting::*;
pub use scrolling::*;
pub use text::*;

pub use keyed::{Keyed, keyed};
pub use media::{
    Breakpoint, MediaQueryData, Orientation, OrientationBuilder, SafeArea, breakpoint, drop_window_metrics,
    media_query, orientation_builder, safe_area, set_device_pixel_ratio, set_safe_area_padding,
    set_text_scale, set_view_insets,
};
pub use mobile_runtime::{
    PopScope, SystemUiOverlayStyle, back_is_blocked, dispatch_back, pop_scope, set_system_ui_overlay_style,
    system_ui_overlay_style,
};
pub use probe::{ExtentProbe, extent_probe};
pub use semantics::{
    Semantics, SemanticsBoundary, SemanticsExt, block_semantics, exclude_semantics, merge_semantics,
    semantics,
};
pub use spinner::{Spinner, spinner};
pub use stream_builder::{StreamBuilder, stream_builder};
pub use view::View;

// Cross-crate re-exports (render-layer types surfaced through the widget API).
pub use pebbles_render::TableColumnWidth;
pub use pebbles_render::{
    ScrollEvent, ScrollMetrics, ScrollNotification, ScrollbarPolicy, ScrollbarStyle, SemanticsProps,
    SemanticsRole,
};
