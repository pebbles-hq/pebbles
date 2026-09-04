//! No-op accessibility bridge for targets without an AccessKit adapter (wasm).
//!
//! Mirrors the public surface of [`crate::a11y`] exactly, so the runner needs no
//! per-call-site `cfg`. The platform-neutral semantics tree is still built in the
//! render layer (it is cheap); it is simply not published anywhere, because there
//! is no AccessKit web adapter yet. Swap this out when one ships.

use pebbles_core::Ui;
use pebbles_render::SemanticsNode;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// No AT-driven actions to apply without an adapter.
pub(crate) fn drain_actions(_ui: &mut Ui, _window: u32) -> bool {
    false
}

/// A zero-cost stand-in for the real AccessKit bridge.
pub struct Bridge;

impl Bridge {
    pub fn new(_event_loop: &ActiveEventLoop, _window: &Window) -> Self {
        Bridge
    }
    pub fn process_event(&mut self, _window: &Window, _event: &WindowEvent) {}
    pub fn update(&mut self, _nodes: &[SemanticsNode], _focus: Option<u64>) {}
}
