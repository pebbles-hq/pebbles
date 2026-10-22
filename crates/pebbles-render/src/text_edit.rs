//! Layout-aware text-editing helpers over Pebbles' cosmic-text
//! [`TextLayout`](crate::text::TextLayout) (03b).
//!
//! Cursor motion, hit-testing and word selection need laid-out text, which lives in the text
//! render objects ([`RenderTextField`](crate::RenderTextField), selectable paragraphs). Each
//! publishes here (keyed by a stable id) on every `layout()` pass; the widget layer calls the
//! granular helpers to move the caret, extend a selection, or map a click to a byte offset.
//! Byte offsets are into the publisher's (unobscured) text; password fields do not publish.
//!
//! Two shapes are published:
//! - **Text**: one [`TextLayout`](crate::text::TextLayout) for the whole content (single-line
//!   fields, the placeholder, unbounded-width fields, selectable paragraphs). Motion runs on
//!   `text::motion` / `TextLayout` geometry directly.
//! - **Lines** (P5): a [`LineTable`] — one layout PER SOURCE LINE, so a keystroke re-shapes one
//!   line instead of one document. Motion runs within the line's layout (visual / bidi /
//!   grapheme correctness, including wrapped visual lines inside a source line) and hops source
//!   lines at the boundaries.
//!
//! **Lazy materialization (P5.2):** above a size threshold, a fresh table holds only line
//! metadata (byte ranges + height *estimates*) — a line lays out on first *visibility* (the
//! field's paint materializes the visible window through the [`TextEnv`](crate::TextEnv)
//! carried by `PaintCx`) or on first *need* (the caret window, materialized during layout).
//! Measured heights replace estimates and the field requests a corrective relayout, exactly
//! like the ListView's estimate-then-measure extents. Motion on a line that has never been laid
//! out falls back to char-boundary–safe arithmetic on the table's own copy of the text —
//! approximate (chars, not graphemes; no bidi) but panic-free, and reachable only when the caret
//! is far outside the window.
//!
//! This module exposes plain functions rather than the core `Motion` enum, because
//! `pebbles-render` sits *below* `pebbles-core` in the crate graph.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use kurbo::Rect;

use crate::text::layout::LayoutParams;
use crate::text::style::TextStyleSpec;
use crate::text::{Affinity, TextLayout, TextRequest, motion};

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
    /// The line's layout, materialized on first visibility/need (P5.2).
    pub(crate) layout: RefCell<Option<Rc<TextLayout>>>,
    pub(crate) empty: bool,
}

impl LineSlot {
    /// The materialized layout, if this line has ever shaped.
    pub(crate) fn layout(&self) -> Option<Rc<TextLayout>> {
        self.layout.borrow().clone()
    }
}

/// Everything needed to shape any line of the table after construction — the
/// style inputs the owning field shaped with. Lets paint (and the caret window)
/// materialize lines long after `layout_lines` returned.
pub(crate) struct ShapeSpec {
    /// The field's text style (colors are painted, never shaped).
    pub(crate) style: TextStyleSpec,
    /// The wrap width of every line.
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
    pub(crate) fn ensure_line(&self, i: usize, env: &mut crate::TextEnv) -> Rc<TextLayout> {
        if let Some(rc) = &*self.lines[i].layout.borrow() {
            return rc.clone();
        }
        let l = &self.lines[i];
        let text = if l.empty { " " } else { &self.display[l.start..l.start + l.len] };
        // Paragraph-direction start alignment (the pre-swap `Alignment::Start` behavior, 03 §3.5.1).
        let params = LayoutParams {
            width: Some(self.shape.width as f32),
            wrap: true,
            align: None,
            max_lines: None,
            ellipsis: false,
        };
        let req = TextRequest { text, style: &self.shape.style, spans: &[], params };
        let key = env.text_key(&req);
        let rc = env.layout_text(&req, key);
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
            Some(layout) => layout.caret_rect(local.min(l.len), Affinity::Downstream, 1.0).x0,
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
                let local = if l.empty { 0 } else { self.local(i, b) };
                let bb = layout.caret_rect(local, Affinity::Downstream, width as f32);
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
    Text(Rc<TextLayout>),
    Lines(Rc<LineTable>),
}

thread_local! {
    static LAYOUTS: RefCell<HashMap<u64, Published>> = RefCell::new(HashMap::new());
}

/// Publish a whole-content `layout` for `id` (single-line fields, selectable paragraphs).
pub fn store(id: u64, layout: Rc<TextLayout>) {
    LAYOUTS.with(|m| {
        m.borrow_mut().insert(id, Published::Text(layout));
    });
}

/// Former name of [`store`] during the text-engine swap (03b E11).
#[deprecated(note = "use `text_edit::store`")]
pub fn store_text(id: u64, layout: Rc<TextLayout>) {
    store(id, layout);
}

/// Publish a per-line table for field `id` (multi-line fields, P5).
pub fn store_lines(id: u64, table: Rc<LineTable>) {
    LAYOUTS.with(|m| {
        m.borrow_mut().insert(id, Published::Lines(table));
    });
}

/// Drop a field's published layout (on unmount / password) and its IME caret.
pub fn clear(id: u64) {
    LAYOUTS.with(|m| {
        m.borrow_mut().remove(&id);
    });
    clear_ime_caret(id);
}

// ---------------------------------------------------------------------------
// IME candidate-window position (03b §3.10, E17)
// ---------------------------------------------------------------------------

/// The focused editor's caret in window coordinates (logical px), and whether it changed since
/// the shell last consumed it.
struct ImeCaret {
    id: u64,
    rect: Rect,
    dirty: bool,
}

thread_local! {
    static IME_CARET: RefCell<Option<ImeCaret>> = const { RefCell::new(None) };
}

/// Publish the focused editor `id`'s caret rect in window coordinates (logical px). A rect equal
/// to the published one is not a change.
pub fn set_ime_caret(id: u64, rect: Rect) {
    IME_CARET.with(|c| {
        let mut c = c.borrow_mut();
        let changed = c.as_ref().is_none_or(|cur| cur.id != id || cur.rect != rect);
        if changed {
            *c = Some(ImeCaret { id, rect, dirty: true });
        }
    });
}

/// Drop `id`'s IME caret (blur / unmount); another editor's caret is left alone.
pub fn clear_ime_caret(id: u64) {
    IME_CARET.with(|c| {
        let mut c = c.borrow_mut();
        if c.as_ref().is_some_and(|cur| cur.id == id) {
            *c = None;
        }
    });
}

/// The published IME caret rect (window coordinates, logical px), if an editor is focused.
pub fn ime_caret() -> Option<Rect> {
    IME_CARET.with(|c| c.borrow().as_ref().map(|cur| cur.rect))
}

/// The IME caret if it changed since the last call (the shell positions the OS candidate window
/// only then, right after painting the window the editor lives in).
pub fn take_ime_caret_update() -> Option<Rect> {
    IME_CARET.with(|c| {
        c.borrow_mut().as_mut().and_then(|cur| std::mem::replace(&mut cur.dirty, false).then_some(cur.rect))
    })
}

/// Debug census: number of published IME carets (0 or 1).
#[cfg(debug_assertions)]
pub fn ime_caret_census() -> usize {
    IME_CARET.with(|c| usize::from(c.borrow().is_some()))
}

/// The published whole-content layout for `id`, if any (tests + tooling; `Rc` identity
/// proves cache hits — a caret blink must NOT produce a new layout).
pub fn get(id: u64) -> Option<Rc<TextLayout>> {
    LAYOUTS.with(|r| {
        r.borrow().get(&id).and_then(|p| match p {
            Published::Text(l) => Some(l.clone()),
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
            Published::Text(_) => None,
        })
    })
}

/// Number of live published layouts (debug observability for the lifecycle soak test).
pub fn len() -> usize {
    LAYOUTS.with(|m| m.borrow().len())
}

/// Caret rect (local space of the publishing object) of `byte`, Downstream affinity, 1 px wide.
/// Out-of-range bytes clamp to the end of the text.
pub fn caret_rect(id: u64, byte: usize) -> Option<Rect> {
    with_text(id, |l| l.caret_rect(byte, Affinity::Downstream, 1.0))
        .or_else(|| with_lines(id, |t| t.caret_rect(byte, 1.0)))
}

fn with_text<R>(id: u64, f: impl FnOnce(&TextLayout) -> R) -> Option<R> {
    LAYOUTS.with(|m| {
        m.borrow().get(&id).and_then(|p| match p {
            Published::Text(l) => Some(f(l)),
            Published::Lines(_) => None,
        })
    })
}

fn with_lines<R>(id: u64, f: impl FnOnce(&LineTable) -> R) -> Option<R> {
    LAYOUTS.with(|m| {
        m.borrow().get(&id).and_then(|p| match p {
            Published::Lines(t) => Some(f(t)),
            Published::Text(_) => None,
        })
    })
}

// ---------------------------------------------------------------------------
// Line-table motion (TextLayout motion within a line, manual hops across lines)
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
            Some(layout) => t.global(i, motion::left(&layout, local, local, false).1),
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
            Some(layout) => t.global(i, motion::right(&layout, local, local, false).1),
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
            Some(layout) => t.global(i, motion::word_left(&layout, local, local, false).1),
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
            Some(layout) => t.global(i, motion::word_right(&layout, local, local, false).1),
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
            Some(layout) => t.global(i, motion::line_start(&layout, local, local, false).1),
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
            Some(layout) => t.global(i, motion::line_end(&layout, local, local, false).1),
            None => l.start + l.len,
        }
    };
    apply(a, nf, extend)
}

fn lines_vertical(t: &LineTable, a: usize, f: usize, extend: bool, up: bool) -> (usize, usize) {
    let i = t.line_of(f);
    let l = &t.lines[i];
    let local = t.local(i, f);
    // First move WITHIN the source line — a soft-wrapped line has several visual lines and
    // vertical motion must walk them before hopping to the neighboring source line.
    if !l.empty
        && let Some(layout) = l.layout()
    {
        let vl = layout.line_for(local, Affinity::Downstream);
        let inside = if up { vl > 0 } else { vl + 1 < layout.line_count() };
        if inside {
            let mf = if up {
                motion::line_up(&layout, local, local, false)
            } else {
                motion::line_down(&layout, local, local, false)
            }
            .1;
            return apply(a, t.global(i, mf.min(l.len)), extend);
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
                    let b = if target.empty { 0 } else { layout.hit(x, y).0 };
                    t.global(i - 1, b)
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
                let b = if target.empty { 0 } else { layout.hit(x, 0.1).0 };
                t.global(i + 1, b)
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
        Some(_) if l.empty => l.start,
        Some(layout) => t.global(i, layout.hit(x as f32, (y - l.y.get()) as f32).0),
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
    with_text(id, |l| motion::left(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_left(t, a, f, extend)))
}
/// Move one grapheme right.
pub fn right(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::right(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_right(t, a, f, extend)))
}
/// Move one word left.
pub fn word_left(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::word_left(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_word_left(t, a, f, extend)))
}
/// Move one word right.
pub fn word_right(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::word_right(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_word_right(t, a, f, extend)))
}
/// Move to the start of the visual line.
pub fn line_start(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::line_start(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_line_start(t, a, f, extend)))
}
/// Move to the end of the visual line.
pub fn line_end(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::line_end(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_line_end(t, a, f, extend)))
}
/// Move up one (visual) line.
pub fn line_up(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::line_up(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_vertical(t, a, f, extend, true)))
}
/// Move down one (visual) line.
pub fn line_down(id: u64, a: usize, f: usize, extend: bool) -> Option<(usize, usize)> {
    with_text(id, |l| motion::line_down(l, a, f, extend))
        .or_else(|| with_lines(id, |t| lines_vertical(t, a, f, extend, false)))
}

/// Map a point (in the text's local space) to a caret byte offset.
pub fn hit(id: u64, x: f64, y: f64) -> Option<usize> {
    with_text(id, |l| l.hit(x as f32, y as f32).0).or_else(|| with_lines(id, |t| lines_hit(t, x, y)))
}
/// The word (anchor, focus) under a point — for double-click select.
pub fn word_at(id: u64, x: f64, y: f64) -> Option<(usize, usize)> {
    with_text(id, |l| l.word_at(x as f32, y as f32)).or_else(|| {
        with_lines(id, |t| {
            if t.lines.is_empty() {
                return (0, 0);
            }
            let i = t.line_at_y(y);
            let l = &t.lines[i];
            match l.layout() {
                Some(_) if l.empty => (l.start, l.start),
                Some(layout) => {
                    let (s, e) = layout.word_at(x as f32, (y - l.y.get()) as f32);
                    (t.global(i, s), t.global(i, e))
                }
                None => (l.start, l.start),
            }
        })
    })
}
/// Extend the current selection's focus to a point — for drag-select.
pub fn extend_to(id: u64, a: usize, _f: usize, x: f64, y: f64) -> Option<(usize, usize)> {
    with_text(id, |l| (a, l.hit(x as f32, y as f32).0))
        .or_else(|| with_lines(id, |t| (a, lines_hit(t, x, y))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ime_caret_round_trip() {
        let r = Rect::new(10.0, 20.0, 11.5, 38.0);
        set_ime_caret(7, r);
        assert_eq!(ime_caret(), Some(r));
        assert_eq!(take_ime_caret_update(), Some(r));
        assert_eq!(take_ime_caret_update(), None, "unchanged: nothing to apply");
        set_ime_caret(7, r);
        assert_eq!(take_ime_caret_update(), None, "the same rect is not a change");
        clear_ime_caret(8);
        assert_eq!(ime_caret_census(), 1, "another id does not clear it");
        clear(7);
        assert_eq!((ime_caret(), ime_caret_census()), (None, 0));
    }
}
