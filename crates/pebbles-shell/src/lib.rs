//! # pebbles-shell
//!
//! The desktop shell: the only crate that touches the GPU and the OS window. It
//! wires [winit] windowing to a [wgpu] surface and the Vello GPU
//! renderer, and drives the [`Ui`](pebbles_core::Ui) engine each frame.
//!
//! The public surface is deliberately tiny — build a widget tree and hand it to
//! [`App`]:
//!
//! ```ignore
//! use pebbles_shell::App;
//! App::new(my_root()).title("Hello").size(480, 320).run()?;
//! ```

// AccessKit has no web adapter yet, so wasm gets a no-op bridge with the same
// public surface (so the runner needs no per-call-site cfg). See
// documentations/web-support.md §4.4.
#[cfg(not(target_family = "wasm"))]
mod a11y;
#[cfg(target_family = "wasm")]
#[path = "a11y_stub.rs"]
mod a11y;
mod app;
mod hotkeys;
#[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
mod native_menu;

pub use app::App;
pub use hotkeys::{HotkeyId, register_global_hotkey, unregister_global_hotkey};
