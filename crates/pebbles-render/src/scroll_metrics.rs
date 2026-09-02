//! Viewport metrics published by controlled viewports ([`RenderList`]) each layout,
//! so the widget layer can clamp the offset and compute the visible item window.
//! Keyed by the viewport's stable id.

use std::cell::RefCell;
use std::collections::HashMap;

/// Measured extents for one viewport. `viewport`/`content` are along the scroll
/// axis; `cross` is the cross-axis extent (used by grids to size cells).
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    pub viewport: f64,
    pub content: f64,
    pub cross: f64,
}

thread_local! {
    static METRICS: RefCell<HashMap<u64, Metrics>> = RefCell::new(HashMap::new());
}

pub fn store(id: u64, viewport: f64, content: f64, cross: f64) {
    METRICS.with(|m| {
        m.borrow_mut().insert(id, Metrics { viewport, content, cross });
    });
}

pub fn get(id: u64) -> Option<Metrics> {
    METRICS.with(|m| m.borrow().get(&id).copied())
}

pub fn clear(id: u64) {
    METRICS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

/// Number of live metric entries (debug observability for the lifecycle soak test).
#[cfg(debug_assertions)]
pub fn len() -> usize {
    METRICS.with(|m| m.borrow().len())
}
