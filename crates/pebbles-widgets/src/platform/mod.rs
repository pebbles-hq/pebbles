//! The OS-facing surfaces — secondary [`window`]s (each its own `Ui` sharing the
//! reactive runtime) and the [`native_menu`] menu-bar spec the shell attaches to
//! the platform menu.
//!
//! Grouped for navigation only: both modules are re-exported at the crate root
//! (`pebbles_widgets::window`, `pebbles_widgets::native_menu`).

pub mod native_menu;
pub mod window;
