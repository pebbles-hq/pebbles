//! Layout-aware text-editing helpers backed by parley's `Selection`.
//!
//! Cursor motion, hit-testing and word selection need shaped text, which lives in
//! [`RenderTextField`](crate::RenderTextField). Each field publishes here (keyed
//! by a stable id) on every `layout()` pass; the widget layer calls the granular
//! helpers to move the caret, extend a selection, or map a click to a byte
//! offset. Byte offsets are into the field's (unobscured) text; password fields
//! do not publish.
//!
//! Two shapes are published:
//! - **Single**: one `Layout` for the whole content (single-line fields, the
//!   placeholder, unbounded-width fields). Motion delegates to parley directly.
//! - **Lines** (P5): a [`LineTable`] — one shaped layout PER SOURCE LINE, so a
//!   keystroke re-shapes one line instead of one document. Motion runs parley
//!   *within* the line's layout (visual/BiDi/grapheme correctness preserved,
//!   including wrapped visual lines inside a source line) and hops source lines
//!   at the boundaries.
//!
//! This module lives in `pebbles-render` (which owns parley) and exposes plain
//! functions rather than the core `Motion` enum, because `pebbles-render` sits
//! *below* `pebbles-core` in the crate graph.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use parley::{Affinity, Cursor, Layout, Selection};
use vello::peniko::Brush;

// ---------------------------------------------------------------------------
// The line table (P5)
// ---------------------------------------------------------------------------

/// One source line of a multi-line field: its byte range in the full text, its
/// laid-out geometry, and its own shaped layout. Empty lines shape a single
/// space so caret/selection geometry exists; local offsets clamp to `len`, so
/// the fake glyph is never addressable.
pub(crate) struct LineSlot {
    /// Byte offset of the line's first char in the full text.
    pub(crate) start: usize,
    /// Line length in bytes, EXCLUDING the trailing newline.
    pub(crate) len: usize,
    /// Local y of the line's top within the field.
    pub(crate) y: f64,
    pub(crate) height: f64,
    pub(crate) layout: Rc<Layout<Brush>>,
    pub(crate) empty: bool,
}

/// The per-line shaped model of a multi-line field (P5): source lines in order,
/// each with its own layout. Built by `RenderTextField`, consumed by the motion
/// helpers below and by the field's own paint.
pub struct LineTable {
    pub(crate) lines: Vec<LineSlot>,
    /// Full text length (a caret may sit at `text_len`).
    pub(crate) text_len: usize,
}

impl LineTable {
    /// Total laid-out height (the field's content height).
    pub fn total_height(&self) -> f64 {
        self.lines.last().map(|l| l.y + l.height).unwrap_or(0.0)
    }

    /// Number of source lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Index of the source line containing byte `b` (a caret at a line's end —
    /// before its newline — belongs to that line).
    pub(crate) fn line_of(&self, b: usize) -> usize {
        match self.lines.partition_point(|l| l.start <= b) {
            0 => 0,
            n => n - 1,
        }
    }

    /// Index of the source line containing local `y` (clamped to the ends).
    pub(crate) fn line_at_y(&self, y: f64) -> usize {
        let n = self.lines.partition_point(|l| l.y + l.height <= y);
        n.min(self.lines.len().saturating_sub(1))
    }

    /// `b` as a local offset within line `i`, clamped to the line's length.
    pub(crate) fn local(&self, i: usize, b: usize) -> usize {
        b.saturating_sub(self.lines[i].start).min(self.lines[i].len)
    }

    /// A local offset in line `i` back to a global byte offset.
    pub(crate) fn global(&self, i: usize, local: usize) -> usize {
        (self.lines[i].start + local.min(self.lines[i].len)).min(self.text_len)
    }

    /// The caret x (local space) of a byte offset within line `i`.
    pub(crate) fn caret_x(&self, i: usize, local: usize) -> f64 {
        let l = &self.lines[i];
        Cursor::from_byte_index(&l.layout, local.min(l.len), Affinity::Downstream)
            .geometry(&l.layout, 1.0)
            .x0
    }

    /// The caret rect (field-local space) for a global byte offset — used by the
    /// field's paint and by scroll-to-caret logic.
    pub fn caret_rect(&self, b: usize, width: f64) -> vello::kurbo::Rect {
        if self.lines.is_empty() {
            return vello::kurbo::Rect::new(0.0, 0.0, width, 0.0);
        }
        let i = self.line_of(b);
        let l = &self.lines[i];
        let bb = Cursor::from_byte_index(&l.layout, self.local(i, b), Affinity::Downstream)
            .geometry(&l.layout, width as f32);
        vello::kurbo::Rect::new(bb.x0, bb.y0 + l.y, bb.x1, bb.y1 + l.y)
    }
}

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

enum Published {
    Single(Rc<Layout<Brush>>),
    Lines(Rc<LineTable>),
}

thread_local! {
    static LAYOUTS: RefCell<HashMap<u64, Published>> = RefCell::new(HashMap::new());
}

/// Publish a whole-content `layout` for field `id` (single-line fields).
pub fn store(id: u64, layout: Rc<Layout<Brush>>) {
    LAYOUTS.with(|m| {
        m.borrow_mut().insert(id, Published::Single(layout));
    });
}

/// Publish a per-line table for field `id` (multi-line fields, P5).
pub fn store_lines(id: u64, table: Rc<LineTable>) {
    LAYOUTS.with(|m| {
        m.borrow_mut().insert(id, Published::Lines(table));
    });
}

/// Drop a field's published layout (on unmount / password).
pub fn clear(id: u64) {
    LAYOUTS.with(|m| {
        m.borrow_mut().remove(&id);
    });
}

/// The published single layout for `id`, if any (tests + tooling; `Rc` identity
/// proves cache hits — a caret blink must NOT produce a new shaped layout).
pub fn get(id: u64) -> Option<Rc<Layout<Brush>>> {
    LAYOUTS.with(|r| {
        r.borrow().get(&id).and_then(|p| match p {
            Published::Single(l) => Some(l.clone()),
            Published::Lines(_) => None,
        })
    })
}

/// The published line table for `id`, if any (tests + tooling; `Rc` identity
/// proves a blink reused the table).
pub fn get_lines(id: u64) -> Option<Rc<LineTable>> {
    LAYOUTS.with(|r| {
        r.borrow().get(&id).and_then(|p| match p {
            Published::Lines(t) => Some(t.clone()),
            Published::Single(_) => None,
        })
    })
}

/// Number of live published layouts (debug observability for the lifecycle soak test).
pub fn len() -> usize {
    LAYOUTS.with(|m| m.borrow().len())
}

fn with_single<R>(id: u64, f: impl FnOnce(&Layout<Brush>) -> R) -> Option<R> {
    LAYOUTS.with(|m| {
        m.borrow().get(&id).and_then(|p| match p {
            Published::Single(l) => Some(f(l)),
            Published::Lines(_) => None,
        })
    })
}

fn with_lines<R>(id: u64, f: impl FnOnce(&LineTable) -> R) -> Option<R> {
    LAYOUTS.with(|m| {
        m.borrow().get(&id).and_then(|p| match p {
            Published::Lines(t) => Some(f(t)),
            Published::Single(_) => None,
        })
    })
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

// ---------------------------------------------------------------------------
// Line-table motion (parley within a line, manual hops across lines)
// ---------------------------------------------------------------------------

fn apply(a: usize, nf: usize, extend: bool) -> (usize, usize) {
    if extend { (a, nf) } else { (nf, nf) }
}

fn lines_left(t: &LineTable, a: usize, f: usize, extend: bool) -> (usize, usize) {
    if !extend && a != f {
        let m = a.min(f);
        return (m, m);
    }
    let i = t.line_of(f);
    let local = t.local(i, f);
    let nf = if local > 0 && !t.lines[i].empty {
        let l = &t.lines[i];
        let s = selection(&l.layout, local, local).previous_visual(&l.layout, false);
        t.global(i, s.focus().index())
    } else if i > 0 {
        t.lines[i - 1].start + t.lines[i - 1].len
    } else {
        0
    };
    apply(a, nf, extend)
}

fn lines_right(t: &LineTable, a: usize, f: usize, extend: bool) -> (usize, usize) {
    if !extend && a != f {
        let m = a.max(f);
        return (m, m);
    }
    let i = t.line_of(f);
    let local = t.local(i, f);
    let l = &t.lines[i];
    let nf = if local < l.len {
        let s = selection(&l.layout, local, local).next_visual(&l.layout, false);
        t.global(i, s.focus().index())
    } else if i + 1 < t.lines.len() {
        t.lines[i + 1].start
    } else {
        t.text_len
    };
    apply(a, nf, extend)
}

fn lines_word_left(t: &LineTable, a: usize, f: usize, extend: bool) -> (usize, usize) {
    let i = t.line_of(f);
    let local = t.local(i, f);
    let nf = if local > 0 && !t.lines[i].empty {
        let l = &t.lines[i];
        let s = selection(&l.layout, local, local).previous_visual_word(&l.layout, false);
        t.global(i, s.focus().index())
    } else if i > 0 {
        t.lines[i - 1].start + t.lines[i - 1].len
    } else {
        0
    };
    apply(a, nf, extend)
}

fn lines_word_right(t: &LineTable, a: usize, f: usize, extend: bool) -> (usize, usize) {
    let i = t.line_of(f);
    let local = t.local(i, f);
    let l = &t.lines[i];
    let nf = if local < l.len {
        let s = selection(&l.layout, local, local).next_visual_word(&l.layout, false);
        t.global(i, s.focus().index())
    } else if i + 1 < t.lines.len() {
        t.lines[i + 1].start
    } else {
        t.text_len
    };
    apply(a, nf, extend)
}

fn lines_line_start(t: &LineTable, a: usize, f: usize, extend: bool) -> (usize, usize) {
    let i = t.line_of(f);
    let local = t.local(i, f);
    let l = &t.lines[i];
    // Visual-line start WITHIN the (possibly wrapped) source line.
    let nf = if l.empty {
        l.start
    } else {
        let s = selection(&l.layout, local, local).line_start(&l.layout, false);
        t.global(i, s.focus().index())
    };
    apply(a, nf, extend)
}

fn lines_line_end(t: &LineTable, a: usize, f: usize, extend: bool) -> (usize, usize) {
    let i = t.line_of(f);
    let local = t.local(i, f);
    let l = &t.lines[i];
    let nf = if l.empty {
        l.start
    } else {
        let s = selection(&l.layout, local, local).line_end(&l.layout, false);
        t.global(i, s.focus().index())
    };
    apply(a, nf, extend)
}

fn lines_vertical(t: &LineTable, a: usize, f: usize, extend: bool, up: bool) -> (usize, usize) {
    let i = t.line_of(f);
    let l = &t.lines[i];
    let local = t.local(i, f);
    // First try parley WITHIN the source line — a soft-wrapped line has several
    // visual lines and vertical motion must walk them before hopping. The move
    // counts only if the caret actually changed VISUAL line (its y moved):
    // parley clamps to the line's start/end at the layout's boundary lines,
    // which must fall through to the source-line hop instead.
    if !l.empty {
        let y_before =
            Cursor::from_byte_index(&l.layout, local, Affinity::Downstream)
                .geometry(&l.layout, 1.0)
                .y0;
        let sel = selection(&l.layout, local, local);
        let moved =
            if up { sel.previous_line(&l.layout, false) } else { sel.next_line(&l.layout, false) };
        let y_after = moved.focus().geometry(&l.layout, 1.0).y0;
        let crossed = if up { y_after < y_before - 0.5 } else { y_after > y_before + 0.5 };
        if crossed {
            let mf = moved.focus().index().min(l.len);
            return apply(a, t.global(i, mf), extend);
        }
    }
    // At the source line's boundary visual line: hop, preserving the caret x.
    let x = (if l.empty { 0.0 } else { t.caret_x(i, local) }) as f32;
    let nf = if up {
        if i == 0 {
            f
        } else {
            let target = &t.lines[i - 1];
            let y = (target.layout.height() - 0.1).max(0.0);
            let s = Selection::from_point(&target.layout, x, y);
            t.global(i - 1, s.focus().index())
        }
    } else if i + 1 >= t.lines.len() {
        f
    } else {
        let target = &t.lines[i + 1];
        let s = Selection::from_point(&target.layout, x, 0.1);
        t.global(i + 1, s.focus().index())
    };
    apply(a, nf, extend)
}

fn lines_hit(t: &LineTable, x: f64, y: f64) -> usize {
    if t.lines.is_empty() {
        return 0;
    }
    let i = t.line_at_y(y);
    let l = &t.lines[i];
    let s = Selection::from_point(&l.layout, x as f32, (y - l.y) as f32);
    t.global(i, s.focus().index())
}

// ---------------------------------------------------------------------------
// The public motion API (shape-agnostic)
// ---------------------------------------------------------------------------

/// Move one grapheme left; `extend` grows the selection instead of collapsing it.
pub fn left(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).previous_visual(l, extend)))
        .or_else(|| with_lines(id, |t| lines_left(t, a, f, extend)))
}
/// Move one grapheme right.
pub fn right(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).next_visual(l, extend)))
        .or_else(|| with_lines(id, |t| lines_right(t, a, f, extend)))
}
/// Move one word left.
pub fn word_left(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).previous_visual_word(l, extend)))
        .or_else(|| with_lines(id, |t| lines_word_left(t, a, f, extend)))
}
/// Move one word right.
pub fn word_right(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).next_visual_word(l, extend)))
        .or_else(|| with_lines(id, |t| lines_word_right(t, a, f, extend)))
}
/// Move to the start of the visual line.
pub fn line_start(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).line_start(l, extend)))
        .or_else(|| with_lines(id, |t| lines_line_start(t, a, f, extend)))
}
/// Move to the end of the visual line.
pub fn line_end(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).line_end(l, extend)))
        .or_else(|| with_lines(id, |t| lines_line_end(t, a, f, extend)))
}
/// Move up one (visual) line.
pub fn line_up(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).previous_line(l, extend)))
        .or_else(|| with_lines(id, |t| lines_vertical(t, a, f, extend, true)))
}
/// Move down one (visual) line.
pub fn line_down(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).next_line(l, extend)))
        .or_else(|| with_lines(id, |t| lines_vertical(t, a, f, extend, false)))
}

/// Map a point (in the text's local space) to a caret byte offset.
pub fn hit(id: u64, x: f64, y: f64) -> Option<usize> {
    with_single(id, |l| Selection::from_point(l, x as f32, y as f32).focus().index())
        .or_else(|| with_lines(id, |t| lines_hit(t, x, y)))
}
/// The word (anchor, focus) under a point — for double-click select.
pub fn word_at(id: u64, x: f64, y: f64) -> Option<(usize, usize)> {
    with_single(id, |l| out(Selection::word_from_point(l, x as f32, y as f32))).or_else(|| {
        with_lines(id, |t| {
            if t.lines.is_empty() {
                return (0, 0);
            }
            let i = t.line_at_y(y);
            let l = &t.lines[i];
            let s = Selection::word_from_point(&l.layout, x as f32, (y - l.y) as f32);
            let (a, f) = out(s);
            (t.global(i, a), t.global(i, f))
        })
    })
}
/// Extend the current selection's focus to a point — for drag-select.
pub fn extend_to(id: u64, a: usize, f: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).extend_to_point(l, x as f32, y as f32)))
        .or_else(|| with_lines(id, |t| (a, lines_hit(t, x, y))))
}
