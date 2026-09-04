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
//! **Lazy materialization (P5.2):** above a size threshold, a fresh table holds
//! only line metadata (byte ranges + height *estimates*) — a line shapes on
//! first *visibility* (the field's paint materializes the visible window through
//! the [`TextEnv`](crate::TextEnv) now carried by `PaintCx`) or on first
//! *need* (the caret window, materialized during layout). Measured heights
//! replace estimates and the field requests a corrective relayout, exactly like
//! the ListView's estimate-then-measure extents. Motion on a line that has never
//! been shaped falls back to char-boundary–safe arithmetic on the table's own
//! copy of the text — approximate (chars, not graphemes; no BiDi) but
//! panic-free, and reachable only when the caret is far outside the window
//! (scroll-to-view + the caret window keep it materialized in practice).
//!
//! This module lives in `pebbles-render` (which owns parley) and exposes plain
//! functions rather than the core `Motion` enum, because `pebbles-render` sits
//! *below* `pebbles-core` in the crate graph.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontWeight, Layout, LineHeight, Selection, StyleProperty,
};
use vello::kurbo::Rect;
use vello::peniko::Brush;

// ---------------------------------------------------------------------------
// The line table (P5)
// ---------------------------------------------------------------------------

/// One source line of a multi-line field: its byte range in the full text, its
/// laid-out geometry, and (once materialized) its own shaped layout. Empty lines
/// shape a single space so caret/selection geometry exists; local offsets clamp
/// to `len`, so the fake glyph is never addressable.
pub(crate) struct LineSlot {
    /// The line's shape-cache key (text + style + wrap width) — the REUSE key: a
    /// table rebuild recycles unchanged lines' layouts from the previous table,
    /// so a keystroke shapes O(changed lines) even if the global cache evicted
    /// them under pressure.
    pub(crate) key: u64,
    /// Byte offset of the line's first char in the full text.
    pub(crate) start: usize,
    /// Line length in bytes, EXCLUDING the trailing newline.
    pub(crate) len: usize,
    /// Local y of the line's top within the field. A `Cell` because measuring a
    /// lazily materialized line reflows every line below it.
    pub(crate) y: Cell<f64>,
    /// Estimated until `measured`; exact after the line first shapes.
    pub(crate) height: Cell<f64>,
    /// Whether `height` came from a real shape (empty lines are exact upfront —
    /// the fake space always lays out to one line box).
    pub(crate) measured: Cell<bool>,
    /// The shaped layout, materialized on first visibility/need (P5.2).
    pub(crate) layout: RefCell<Option<Rc<Layout<Brush>>>>,
    pub(crate) empty: bool,
}

impl LineSlot {
    /// The materialized layout, if this line has ever shaped.
    pub(crate) fn layout(&self) -> Option<Rc<Layout<Brush>>> {
        self.layout.borrow().clone()
    }
}

/// Everything needed to shape any line of the table after construction — the
/// style inputs the owning field shaped with. Lets paint (and the caret window)
/// materialize lines long after `layout_lines` returned.
pub(crate) struct ShapeSpec {
    pub(crate) font_size: f32,
    pub(crate) weight: f32,
    pub(crate) line_height: f32,
    pub(crate) brush: Brush,
    pub(crate) width: f64,
}

/// The per-line shaped model of a multi-line field (P5): source lines in order,
/// each with its own (possibly lazily materialized) layout. Built by
/// `RenderTextField`, consumed by the motion helpers below and by the field's
/// own paint.
pub struct LineTable {
    pub(crate) lines: Vec<LineSlot>,
    /// Full text length (a caret may sit at `text_len`).
    pub(crate) text_len: usize,
    /// The composed display text the slots' byte ranges index into — owned by
    /// the table so lazy materialization (and the motion fallbacks) can slice
    /// line text at any time.
    pub(crate) display: Rc<str>,
    /// The style inputs to shape any line with (P5.2).
    pub(crate) shape: ShapeSpec,
    /// One nominal line box (`font_size × line_height`) — the estimate unit.
    pub(crate) line_px: f64,
    /// Set when a materialization changed some line's height; cleared by
    /// [`reflow`](Self::reflow).
    pub(crate) dirty_geometry: Cell<bool>,
}

impl LineTable {
    /// Total laid-out height (the field's content height; estimated lines
    /// contribute their estimates until they materialize).
    pub fn total_height(&self) -> f64 {
        self.lines.last().map(|l| l.y.get() + l.height.get()).unwrap_or(0.0)
    }

    /// Number of source lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Number of lines whose shaped layout is materialized (tests/census).
    pub fn materialized_count(&self) -> usize {
        self.lines.iter().filter(|l| l.layout.borrow().is_some()).count()
    }

    /// Whether line `i` has actually been shaped yet (P5.2): `false` means its
    /// geometry is still an estimate. Out-of-range reads `false`.
    pub fn line_is_materialized(&self, i: usize) -> bool {
        self.lines.get(i).is_some_and(|l| l.layout.borrow().is_some())
    }

    /// Line `i`'s height — exact once
    /// [`line_is_materialized`](Self::line_is_materialized), an estimate before.
    pub fn line_height(&self, i: usize) -> f64 {
        self.lines.get(i).map_or(0.0, |l| l.height.get())
    }

    /// Local y of line `i`'s top within the field — what scroll-to-line needs.
    pub fn line_top(&self, i: usize) -> f64 {
        self.lines.get(i).map_or(0.0, |l| l.y.get())
    }

    /// The text of line `i` as shaped: the display slice, or a single space for
    /// an empty line (so caret/selection geometry exists).
    pub(crate) fn slot_text(&self, i: usize) -> &str {
        let l = &self.lines[i];
        if l.empty { " " } else { &self.display[l.start..l.start + l.len] }
    }

    /// Materialize line `i`'s shaped layout (through the window's shape cache),
    /// measuring its real height. A no-op when already materialized. Marks the
    /// table's geometry dirty when the measured height differs from the
    /// estimate — the caller reflows + requests a corrective relayout.
    pub(crate) fn ensure_line(&self, i: usize, env: &mut crate::TextEnv) -> Rc<Layout<Brush>> {
        if let Some(rc) = &*self.lines[i].layout.borrow() {
            return rc.clone();
        }
        let l = &self.lines[i];
        let text = if l.empty { " " } else { &self.display[l.start..l.start + l.len] };
        let rc = match env.cached_layout(l.key) {
            Some((rc, _, _)) => rc,
            None => {
                let s = &self.shape;
                let mut builder = env.layout.ranged_builder(&mut env.fonts, text, 1.0, true);
                builder.push_default(StyleProperty::FontSize(s.font_size));
                builder.push_default(StyleProperty::FontWeight(FontWeight::new(s.weight)));
                builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(s.line_height)));
                builder.push_default(StyleProperty::Brush(s.brush.clone()));
                let mut layout: Layout<Brush> = builder.build(text);
                layout.break_all_lines(Some(s.width as f32));
                layout.align(Alignment::Start, AlignmentOptions::default());
                let rc = Rc::new(layout);
                env.store_layout(l.key, rc.clone(), rc.width() as f64, rc.height() as f64);
                rc
            }
        };
        let h = (rc.height() as f64).max(self.line_px);
        if (h - l.height.get()).abs() > 0.1 {
            l.height.set(h);
            self.dirty_geometry.set(true);
        }
        l.measured.set(true);
        *l.layout.borrow_mut() = Some(rc.clone());
        rc
    }

    /// Materialize every line in `lo..=hi` (indices clamped to the table).
    pub(crate) fn materialize_span(&self, lo: usize, hi: usize, env: &mut crate::TextEnv) {
        if self.lines.is_empty() {
            return;
        }
        let hi = hi.min(self.lines.len() - 1);
        for i in lo.min(hi)..=hi {
            self.ensure_line(i, env);
        }
    }

    /// Re-stack the `y` prefix after measurements changed heights. Returns
    /// `true` when any line moved (the caller then requests a corrective
    /// relayout so the field's reported size catches up).
    pub(crate) fn reflow(&self) -> bool {
        if !self.dirty_geometry.replace(false) {
            return false;
        }
        let mut y = 0.0_f64;
        let mut moved = false;
        for l in &self.lines {
            if (l.y.get() - y).abs() > 0.01 {
                l.y.set(y);
                moved = true;
            }
            y += l.height.get();
        }
        moved
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
        let n = self.lines.partition_point(|l| l.y.get() + l.height.get() <= y);
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

    /// The caret x (local space) of a byte offset within line `i` — `0.0` when
    /// the line has never materialized (only reachable off-window).
    pub(crate) fn caret_x(&self, i: usize, local: usize) -> f64 {
        let l = &self.lines[i];
        match &*l.layout.borrow() {
            Some(layout) => {
                Cursor::from_byte_index(layout, local.min(l.len), Affinity::Downstream)
                    .geometry(layout, 1.0)
                    .x0
            }
            None => 0.0,
        }
    }

    /// The caret rect (field-local space) for a global byte offset — used by the
    /// field's paint and by scroll-to-caret logic. An unmaterialized line yields
    /// a synthetic rect at the line's start (its `y`/height are the estimates,
    /// which is exactly what scroll-to-view needs to bring it on screen — the
    /// next paint then materializes it and the rect becomes exact).
    pub fn caret_rect(&self, b: usize, width: f64) -> Rect {
        if self.lines.is_empty() {
            return Rect::new(0.0, 0.0, width, 0.0);
        }
        let i = self.line_of(b);
        let l = &self.lines[i];
        match &*l.layout.borrow() {
            Some(layout) => {
                let bb = Cursor::from_byte_index(layout, self.local(i, b), Affinity::Downstream)
                    .geometry(layout, width as f32);
                Rect::new(bb.x0, bb.y0 + l.y.get(), bb.x1, bb.y1 + l.y.get())
            }
            None => Rect::new(0.0, l.y.get(), width, l.y.get() + self.line_px),
        }
    }
}

// ---------------------------------------------------------------------------
// Char-boundary–safe fallbacks (motion on a never-materialized line)
// ---------------------------------------------------------------------------

/// Previous char boundary within `s` (0 at the start).
fn prev_char(s: &str, i: usize) -> usize {
    let i = i.min(s.len());
    s[..i].char_indices().next_back().map(|(j, _)| j).unwrap_or(0)
}

/// Next char boundary within `s` (`s.len()` at the end).
fn next_char(s: &str, i: usize) -> usize {
    let i = i.min(s.len());
    s[i..].chars().next().map(|c| i + c.len_utf8()).unwrap_or(s.len())
}

/// Snap `i` back to a char boundary of `s`.
fn snap_char(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Pure char-class word-left within a line (no shaping): skip whitespace, then
/// the word.
fn pure_word_left(s: &str, mut i: usize) -> usize {
    while i > 0 {
        let p = prev_char(s, i);
        if s[p..i].chars().next().is_some_and(char::is_whitespace) { i = p } else { break }
    }
    while i > 0 {
        let p = prev_char(s, i);
        if s[p..i].chars().next().is_some_and(|c| !c.is_whitespace()) { i = p } else { break }
    }
    i
}

fn pure_word_right(s: &str, mut i: usize) -> usize {
    while i < s.len() {
        let n = next_char(s, i);
        if s[i..n].chars().next().is_some_and(char::is_whitespace) { i = n } else { break }
    }
    while i < s.len() {
        let n = next_char(s, i);
        if s[i..n].chars().next().is_some_and(|c| !c.is_whitespace()) { i = n } else { break }
    }
    i
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
        match t.lines[i].layout() {
            Some(layout) => {
                let s = selection(&layout, local, local).previous_visual(&layout, false);
                t.global(i, s.focus().index())
            }
            // Never materialized (off-window): char-boundary step, no shaping.
            None => t.global(i, prev_char(t.slot_text(i), local)),
        }
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
        match l.layout() {
            Some(layout) => {
                let s = selection(&layout, local, local).next_visual(&layout, false);
                t.global(i, s.focus().index())
            }
            None => t.global(i, next_char(t.slot_text(i), local)),
        }
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
        match t.lines[i].layout() {
            Some(layout) => {
                let s = selection(&layout, local, local).previous_visual_word(&layout, false);
                t.global(i, s.focus().index())
            }
            None => t.global(i, pure_word_left(t.slot_text(i), local)),
        }
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
        match l.layout() {
            Some(layout) => {
                let s = selection(&layout, local, local).next_visual_word(&layout, false);
                t.global(i, s.focus().index())
            }
            None => t.global(i, pure_word_right(t.slot_text(i), local)),
        }
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
    // Visual-line start WITHIN the (possibly wrapped) source line; the
    // unmaterialized fallback is the source-line start (exact when unwrapped).
    let nf = if l.empty {
        l.start
    } else {
        match l.layout() {
            Some(layout) => {
                let s = selection(&layout, local, local).line_start(&layout, false);
                t.global(i, s.focus().index())
            }
            None => l.start,
        }
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
        match l.layout() {
            Some(layout) => {
                let s = selection(&layout, local, local).line_end(&layout, false);
                t.global(i, s.focus().index())
            }
            None => l.start + l.len,
        }
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
    if !l.empty
        && let Some(layout) = l.layout()
    {
        let y_before =
            Cursor::from_byte_index(&layout, local, Affinity::Downstream).geometry(&layout, 1.0).y0;
        let sel = selection(&layout, local, local);
        let moved = if up { sel.previous_line(&layout, false) } else { sel.next_line(&layout, false) };
        let y_after = moved.focus().geometry(&layout, 1.0).y0;
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
            match target.layout() {
                Some(layout) => {
                    let y = (layout.height() - 0.1).max(0.0);
                    let s = Selection::from_point(&layout, x, y);
                    t.global(i - 1, s.focus().index())
                }
                // Off-window target: land at a char-snapped ~same byte column.
                None => t.global(i - 1, snap_char(t.slot_text(i - 1), local)),
            }
        }
    } else if i + 1 >= t.lines.len() {
        f
    } else {
        let target = &t.lines[i + 1];
        match target.layout() {
            Some(layout) => {
                let s = Selection::from_point(&layout, x, 0.1);
                t.global(i + 1, s.focus().index())
            }
            None => t.global(i + 1, snap_char(t.slot_text(i + 1), local)),
        }
    };
    apply(a, nf, extend)
}

fn lines_hit(t: &LineTable, x: f64, y: f64) -> usize {
    if t.lines.is_empty() {
        return 0;
    }
    let i = t.line_at_y(y);
    let l = &t.lines[i];
    match l.layout() {
        Some(layout) => {
            let s = Selection::from_point(&layout, x as f32, (y - l.y.get()) as f32);
            t.global(i, s.focus().index())
        }
        // A hit on a never-painted line (drag auto-scroll racing paint): the
        // line's start — the next frame materializes it and the drag corrects.
        None => l.start,
    }
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
            match l.layout() {
                Some(layout) => {
                    let s = Selection::word_from_point(&layout, x as f32, (y - l.y.get()) as f32);
                    let (a, f) = out(s);
                    (t.global(i, a), t.global(i, f))
                }
                None => (l.start, l.start),
            }
        })
    })
}
/// Extend the current selection's focus to a point — for drag-select.
pub fn extend_to(id: u64, a: usize, f: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    with_single(id, |l| out(selection(l, a, f).extend_to_point(l, x as f32, y as f32)))
        .or_else(|| with_lines(id, |t| (a, lines_hit(t, x, y))))
}
