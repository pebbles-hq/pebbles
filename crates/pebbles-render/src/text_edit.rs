//! Layout-aware text-editing helpers backed by parley's `Selection`.
//!
//! Cursor motion, hit-testing and word selection all need the shaped `Layout`,
//! which lives in [`RenderTextField`](crate::RenderTextField). Each field publishes
//! its current layout here (keyed by a stable id) on every `layout()` pass; the
//! widget layer then calls these granular helpers to move the caret, extend a
//! selection, or map a click to a byte offset. Byte offsets are into the field's
//! (unobscured) text; password fields do not publish a layout.
//!
//! This module lives in `pebbles-render` (which owns parley) and exposes plain
//! functions rather than the core `Motion` enum, because `pebbles-render` sits
//! *below* `pebbles-core` in the crate graph.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use parley::{Affinity, Cursor, Layout, Selection};
use vello::peniko::Brush;

thread_local! {
    static LAYOUTS: RefCell<HashMap<u64, Rc<Layout<Brush>>>> = RefCell::new(HashMap::new());
}

/// Publish `layout` for field `id` (called each layout pass).
pub fn store(id: u64, layout: Rc<Layout<Brush>>) {
    LAYOUTS.with(|m| {
        m.borrow_mut().insert(id, layout);
    });
}

/// Drop a field's published layout (on unmount / password).
pub fn clear(id: u64) {
    LAYOUTS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

fn with_layout<R>(id: u64, f: impl FnOnce(&Layout<Brush>) -> R) -> Option<R> {
    LAYOUTS.with(|m| m.borrow().get(&id).map(|l| f(l)))
}

fn cursor(layout: &Layout<Brush>, byte: usize) -> Cursor {
    Cursor::from_byte_index(layout, byte, Affinity::Downstream)
}

fn selection(layout: &Layout<Brush>, anchor: usize, focus: usize) -> Selection {
    Selection::new(cursor(layout, anchor), cursor(layout, focus))
}

fn out(s: Selection) -> (usize, usize) {
    (s.anchor().index(), s.focus().index())
}

/// Move one grapheme left; `extend` grows the selection instead of collapsing it.
pub fn left(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).previous_visual(l, extend)))
}
/// Move one grapheme right.
pub fn right(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).next_visual(l, extend)))
}
/// Move one word left.
pub fn word_left(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).previous_visual_word(l, extend)))
}
/// Move one word right.
pub fn word_right(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).next_visual_word(l, extend)))
}
/// Move to the start of the visual line.
pub fn line_start(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).line_start(l, extend)))
}
/// Move to the end of the visual line.
pub fn line_end(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).line_end(l, extend)))
}
/// Move up one line.
pub fn line_up(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).previous_line(l, extend)))
}
/// Move down one line.
pub fn line_down(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).next_line(l, extend)))
}

/// Map a point (in the text's local space) to a caret byte offset.
pub fn hit(id: u64, x: f64, y: f64) -> Option<usize> {
    with_layout(id, |l| Selection::from_point(l, x as f32, y as f32).focus().index())
}
/// The word (anchor, focus) under a point — for double-click select.
pub fn word_at(id: u64, x: f64, y: f64) -> Option<(usize, usize)> {
    with_layout(id, |l| out(Selection::word_from_point(l, x as f32, y as f32)))
}
/// Extend the current selection's focus to a point — for drag-select.
pub fn extend_to(id: u64, a: usize, f: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    with_layout(id, |l| out(selection(l, a, f).extend_to_point(l, x as f32, y as f32)))
}
