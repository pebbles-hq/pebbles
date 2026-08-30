//! # pebbles-shell
//!
//! The desktop shell: the only crate that touches the GPU and the OS window. It
//! wires [winit](winit) windowing to a [wgpu](wgpu) surface and the Vello GPU
//! renderer, and drives the [`Ui`](pebbles_core::Ui) engine each frame.
//!
//! The public surface is deliberately tiny — build a widget tree and hand it to
//! [`App`]:
//!
//! ```ignore
//! use pebbles_shell::App;
//! App::new(my_root()).title("Hello").size(480, 320).run()?;
//! ```

mod a11y;
mod app;

pub use app::App;
