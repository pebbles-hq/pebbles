//! Gallery: a test bench for checking by eye that every pebbles-widgets component and widget
//! group works. A side list of pages, each showing live samples in every state and variant,
//! with a top-bar theme switcher (design language × light/dark/system) and text direction.
//!
//! [`registry::PAGES`] lists every page; `tools/render-check` imports them (feature
//! `headless`) to render each one deterministically.

pub mod pages;
pub mod registry;
pub mod shell;
pub mod styles;
pub mod ui;

#[cfg(feature = "headless")]
pub mod headless;

pub use registry::{PAGES, Page, page};
pub use shell::app;
