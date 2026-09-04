//! The consolidated pebbles-widgets integration suite.
//!
//! Every widget/component test lives here as a module of ONE test harness so the
//! workspace links a single test binary instead of one per file (each binary
//! statically embeds the full vello/parley dep graph — 56 of them once filled a
//! disk). Add new test files to this directory and list them below.

mod accordion;
mod animated_container;
mod breadcrumb;
mod calendar;
mod canvas;
mod caret;
mod carousel;
mod catalog_batch;
mod command;
mod components;
mod context_scope;
mod date_field_range;
mod engine;
mod field_lazy;
mod file_explorer;
mod fonts;
mod forms;
mod global_menu;
mod hovercard_menubar;
mod ime;
mod interactions;
mod lifecycle;
mod list_auto;
mod list_tile;
#[cfg(feature = "markdown")]
mod markdown;
#[cfg(feature = "markdown")]
mod perf;
#[cfg(feature = "markdown")]
mod storm;
mod memo;
mod monitors;
mod multi_window;
mod native_menu;
mod navigation;
mod nav_style;
mod notifications;
mod otp;
mod overlay_menus;
mod pagination;
mod per_window_overlay;
mod popover;
mod property_parity;
mod reactive;
mod scrollbar;
mod scroll_physics;
mod select_menus;
mod semantics_menus;
mod semantics;
mod shortcuts;
mod sizing_boxes;
mod slider;
mod sticky;
mod store_select;
mod stress;
mod styling;
mod table;
mod tabs;
mod text_direction;
mod time_field;
mod transform;
mod virtualization;
mod window_knobs;
