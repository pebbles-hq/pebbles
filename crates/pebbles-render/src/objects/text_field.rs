//! [`RenderTextField`] — a leaf render object that paints editable text with a
//! selection highlight and a caret. It is display-only: the widget layer owns the
//! string and the selection (anchor + focus byte offsets) and feeds them in; this
//! object lays the text out as a cosmic-text [`TextLayout`], paints the selection rects +
//! glyphs + caret (all via `TextLayout` geometry), and publishes the layout to
//! [`crate::text_edit`] so the widget can do cursor motion and hit-testing.

use std::rc::Rc;

use kurbo::{Affine, Rect};
use pebbles_foundation::{Color, Offset, Size};
use peniko::Fill;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::text::layout::LayoutParams;
use crate::text::style::TextStyleSpec;
use crate::text::{Affinity, TextLayout, TextRequest};
use crate::tree::{LayoutCx, PaintCx};

/// Line count above which a fresh table is built LAZILY (P5.2): heights start
/// as estimates and a line shapes on first visibility/need. At or below it,
/// every line shapes at build — exact heights, no corrective passes — so the
/// common small field (forms, chat boxes) keeps the simple exact path.
const LAZY_THRESHOLD: usize = 256;

/// Lines materialized around the caret at every layout (P5.2): caret paint,
/// scroll-to-caret and burst motion (key-repeat between frames) always resolve
/// against shaped lines without waiting for a paint.
const CARET_WINDOW: usize = 32;

/// Visual styling for an editable text field.
#[derive(Clone, Debug)]
pub struct TextFieldStyle {
    pub font_size: f32,
    pub weight: f32,
    pub line_height: f32,
    /// Color of entered text.
    pub color: Color,
    /// Color of the placeholder (shown when empty).
    pub placeholder_color: Color,
    /// Color of the caret.
    pub caret_color: Color,
    /// Fill behind the selected range.
    pub selection_color: Color,
}

impl Default for TextFieldStyle {
    fn default() -> Self {
        TextFieldStyle {
            font_size: 14.0,
            weight: 400.0,
            line_height: 1.3,
            color: pebbles_foundation::palette::BLACK,
            placeholder_color: pebbles_foundation::palette::zinc::S400,
            caret_color: pebbles_foundation::palette::BLACK,
            selection_color: Color::from_rgba8(59, 130, 246, 70),
        }
    }
}

/// A leaf render object that paints editable text with a selection + caret.
pub struct RenderTextField {
    /// The entered text (source of truth lives in the widget layer).
    pub text: String,
    /// Placeholder shown when `text` is empty.
    pub placeholder: String,
    /// Selection anchor (byte offset into `text`).
    pub anchor: usize,
    /// Selection focus / caret (byte offset into `text`).
    pub focus: usize,
    /// IME preedit (composition) text, shown underlined at the caret; empty when not
    /// composing. It is not part of `text` until the IME commits it.
    pub preedit: String,
    /// Whether the field is focused (controls the caret + hit-test publishing).
    pub focused: bool,
    /// Blink phase — the caret is drawn only while this is `true` (the widget
    /// layer toggles it ~2 Hz while focused; solid while composing).
    pub caret_visible: bool,
    /// If set, every character renders as this glyph (password fields).
    pub obscure: Option<char>,
    /// Whether newlines stack (textarea) vs. a single line.
    pub multiline: bool,
    /// Stable id for publishing the layout to [`crate::text_edit`].
    pub field_id: u64,
    pub style: TextFieldStyle,
    /// The display layout (Rc so it can be published + kept for paint) — the SINGLE-layout
    /// path (single-line fields, placeholder, unbounded width).
    cached: Option<Rc<TextLayout>>,
    /// The per-line table (P5) — the multi-line path: one shaped layout per
    /// source line, so a keystroke re-shapes one line, not one document.
    table: Option<Rc<crate::text_edit::LineTable>>,
    /// Key of the last-built table (text + style + wrap width): a caret blink or
    /// focus flip re-layouts with the SAME key and keeps the table untouched.
    table_key: Option<u64>,
    line_px: f64,
}

impl RenderTextField {
    pub fn new(text: impl Into<String>, style: TextFieldStyle) -> Self {
        RenderTextField {
            text: text.into(),
            placeholder: String::new(),
            anchor: 0,
            focus: 0,
            preedit: String::new(),
            focused: false,
            caret_visible: true,
            obscure: None,
            multiline: false,
            field_id: 0,
            style,
            cached: None,
            table: None,
            table_key: None,
            line_px: 0.0,
        }
    }

    /// Hash of every style input that affects shaping (shared by both paths).
    fn style_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.style.font_size.to_bits().hash(&mut h);
        self.style.weight.to_bits().hash(&mut h);
        self.style.line_height.to_bits().hash(&mut h);
        for c in self.style.color.components {
            c.to_bits().hash(&mut h);
        }
        h.finish()
    }

    /// Paint-time metadata WITHOUT building the (possibly multi-megabyte) display
    /// string: the caret's display-byte position, the preedit's display range,
    /// and whether a composition is active. Valid for the non-obscured paths.
    fn composed_meta(&self) -> (usize, Option<(usize, usize)>, bool) {
        if self.text.is_empty() && self.preedit.is_empty() {
            return (0, None, false);
        }
        let f = self.focus.min(self.text.len());
        if self.preedit.is_empty() {
            return (f, None, false);
        }
        (f + self.preedit.len(), Some((f, f + self.preedit.len())), true)
    }

    /// Map a real-text byte offset onto the display string (identity unless obscured).
    fn to_display(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        match self.obscure {
            Some(ch) => self.text[..byte].chars().count() * ch.len_utf8(),
            None => byte,
        }
    }

    /// The final string to lay out (with any IME preedit spliced in at the caret), its
    /// color, the preedit's byte range within the display string (for the composition
    /// underline), and the caret's display-byte position.
    fn composed(&self) -> Composed {
        // Empty and not composing → placeholder.
        if self.text.is_empty() && self.preedit.is_empty() {
            return Composed {
                text: self.placeholder.clone(),
                color: self.style.placeholder_color,
                preedit: None,
                caret: 0,
            };
        }
        // Password: never visualize composition; render obscured text.
        if let Some(ch) = self.obscure {
            let s = std::iter::repeat_n(ch, self.text.chars().count()).collect();
            return Composed {
                text: s,
                color: self.style.color,
                preedit: None,
                caret: self.to_display(self.focus),
            };
        }
        let f = self.focus.min(self.text.len());
        if self.preedit.is_empty() {
            return Composed { text: self.text.clone(), color: self.style.color, preedit: None, caret: f };
        }
        // Composing: splice the preedit in at the caret; caret sits at its end.
        let mut s = String::with_capacity(self.text.len() + self.preedit.len());
        s.push_str(&self.text[..f]);
        s.push_str(&self.preedit);
        s.push_str(&self.text[f..]);
        let end = f + self.preedit.len();
        Composed { text: s, color: self.style.color, preedit: Some((f, end)), caret: end }
    }
}

impl RenderTextField {
    /// Publish the caret in window coordinates for the OS IME candidate window (03b E17), or
    /// drop it when unfocused. `offset` is this field's absolute paint offset.
    fn publish_ime_caret(&self, offset: Offset) {
        if !self.focused {
            crate::text_edit::clear_ime_caret(self.field_id);
            return;
        }
        let (caret, _, _) = self.composed_meta();
        let local = match (&self.table, &self.cached) {
            (Some(table), _) => table.caret_rect(caret, 1.5),
            (None, Some(layout)) => layout.caret_rect(caret, Affinity::Downstream, 1.5),
            (None, None) => return,
        };
        crate::text_edit::set_ime_caret(self.field_id, local + offset);
    }
}

/// The result of [`RenderTextField::composed`]: the display string plus the geometry
/// hints paint needs (caret position, preedit underline range).
struct Composed {
    text: String,
    color: Color,
    /// Byte range of the preedit within `text`, if composing.
    preedit: Option<(usize, usize)>,
    /// Caret position as a byte offset into `text`.
    caret: usize,
}

impl RenderObject for RenderTextField {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        if !self.focused {
            crate::text_edit::clear_ime_caret(self.field_id);
        }
        // Multi-line content takes the LINE-TABLE path (P5): per-line shaped
        // layouts through the window cache — a keystroke re-shapes ONE line.
        // Single-line fields, passwords, the placeholder, and unbounded widths
        // keep the single-layout path.
        let composed = self.composed();
        if self.multiline
            && self.obscure.is_none()
            && !(self.text.is_empty() && self.preedit.is_empty())
            && constraints.has_bounded_width()
        {
            return self.layout_lines(cx, constraints, &composed);
        }
        self.table = None;
        self.table_key = None;
        let max_advance = if self.multiline && constraints.has_bounded_width() {
            Some(constraints.max_width as f32)
        } else {
            None
        };
        let display = &composed.text;

        // Lay out through the window caches: caret blinks, focus flips, selection moves and
        // unrelated rebuilds re-layout this field WITHOUT re-shaping the document — only a
        // text / style / wrap-width change shapes. One shape per keystroke, zero otherwise
        // (the P5 editor contract). Colors are painted, never part of the key.
        let spec = TextStyleSpec::for_field(&self.style);
        let params = LayoutParams {
            width: max_advance,
            wrap: max_advance.is_some(),
            align: None,
            max_lines: None,
            ellipsis: false,
            strut: None,
        };
        let req = TextRequest { text: display, style: &spec, spans: &[], params };
        let key = cx.text.text_key(&req);
        let layout = cx.text.layout_text(&req, key);

        self.line_px = (self.style.font_size as f64) * (self.style.line_height as f64);
        let height = (layout.height() as f64).max(self.line_px);
        let width =
            if constraints.has_bounded_width() { constraints.max_width } else { f64::from(layout.width()) };
        // Publish for hit-testing / motion — but only real (unobscured) text, and not
        // while composing (the published layout would include the transient preedit).
        if self.obscure.is_none() && self.preedit.is_empty() && !self.text.is_empty() {
            crate::text_edit::store(self.field_id, layout.clone());
        } else {
            crate::text_edit::clear(self.field_id);
        }
        self.cached = Some(layout);
        constraints.constrain(Size::new(width, height))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        self.publish_ime_caret(cx.window_offset(offset));
        if let Some(table) = &self.table {
            let table = table.clone();
            self.paint_lines(cx, offset, &table);
            return;
        }
        let Some(layout) = &self.cached else { return };
        let composed = self.composed();
        let composing = composed.preedit.is_some();
        let visible = cx.visible();
        let mut rects: Vec<Rect> = Vec::new();

        // 1. Selection highlight (skip for password / placeholder / while composing —
        // during composition the caret is collapsed at the preedit).
        let has_text = !self.text.is_empty();
        if has_text && !composing && self.obscure.is_none() && self.anchor != self.focus {
            let a = self.to_display(self.anchor);
            let f = self.to_display(self.focus);
            layout.selection_rects(a, f, &mut rects);
            for r in &rects {
                let rect = *r + offset;
                if rect.y1 < visible.y0 || rect.y0 > visible.y1 {
                    continue;
                }
                cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.selection_color, None, &rect);
            }
        }

        // 2. Glyphs — line- and run-culled to the visible rect like paragraphs.
        crate::text::paint::paint_text_layout(
            cx,
            layout,
            offset,
            &[crate::text::paint::TextPaint::solid(composed.color)],
        );

        // 3. Composition underline: a 1px rule beneath the preedit range.
        if let Some((p0, p1)) = composed.preedit {
            rects.clear();
            layout.selection_rects(p0, p1, &mut rects);
            for r in &rects {
                let rect =
                    Rect::new(offset.x + r.x0, offset.y + r.y1 - 1.5, offset.x + r.x1, offset.y + r.y1 - 0.5);
                cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.caret_color, None, &rect);
            }
        }

        // 4. Caret at the focus (when focused). While composing it sits at the end of
        // the preedit; the placeholder shows it at the very start.
        if self.focused && self.caret_visible {
            let f = if has_text || composing { composed.caret } else { 0 };
            let bb = layout.caret_rect(f, Affinity::Downstream, 1.5);
            let rect = Rect::new(
                offset.x + bb.x0,
                offset.y + bb.y0 + 1.0,
                offset.x + bb.x0 + 1.5,
                offset.y + bb.y1 - 1.0,
            );
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.caret_color, None, &rect);
        }
    }

    /// A text field repaints alone (caret blink, typing) without re-recording its
    /// surroundings.
    fn is_repaint_boundary(&self) -> bool {
        true
    }

    /// A focused field publishes its caret in WINDOW coordinates while painting (the OS
    /// IME candidate window): it must paint every frame so a move of any ancestor (a
    /// scroll) re-publishes it, so no enclosing layer may be replayed.
    fn paint_is_volatile(&self) -> bool {
        self.focused
    }

    fn debug_name(&self) -> &'static str {
        "RenderTextField"
    }
}

// ---------------------------------------------------------------------------
// The line-table path (P5): per-line shaping + windowed painting
// ---------------------------------------------------------------------------

impl RenderTextField {
    /// Multi-line layout: split the display text into source lines and build the
    /// per-line table. Small documents shape every line immediately (exact
    /// heights, today's behavior); past [`LAZY_THRESHOLD`] lines the table is
    /// built **lazily** (P5.2) — line heights are estimates until a line first
    /// shapes (on visibility, in paint, or in the caret window below), so a cold
    /// mount of a megabyte source costs O(window), not O(document). Unchanged
    /// lines carry their materialized layouts and measured heights across
    /// rebuilds — a keystroke still shapes exactly the line(s) it touched; a
    /// caret blink (same table key) skips the whole pass.
    fn layout_lines(
        &mut self,
        cx: &mut LayoutCx<'_>,
        constraints: BoxConstraints,
        composed: &Composed,
    ) -> Size {
        use std::hash::{Hash, Hasher};
        let width = constraints.max_width;
        let display = &composed.text;
        self.line_px = (self.style.font_size as f64) * (self.style.line_height as f64);
        let style_h = self.style_hash();

        let table_key = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            0x000F_1E1D_11E5_u64.hash(&mut h);
            display.hash(&mut h);
            style_h.hash(&mut h);
            width.to_bits().hash(&mut h);
            h.finish()
        };
        if self.table_key != Some(table_key) || self.table.is_none() {
            // Carry from the PREVIOUS table by line key: unchanged lines keep
            // their materialized layouts AND measured heights on a rebuild,
            // regardless of global cache pressure (a huge document's thousands
            // of line entries can age out of the cache during blink fast-paths —
            // the old table still owns them).
            struct Carry {
                layout: Option<Rc<TextLayout>>,
                height: f64,
                measured: bool,
            }
            let reuse: std::collections::HashMap<u64, Carry> = self
                .table
                .as_ref()
                .map(|t| {
                    t.lines
                        .iter()
                        .map(|l| {
                            (
                                l.key,
                                Carry {
                                    layout: l.layout.borrow().clone(),
                                    height: l.height.get(),
                                    measured: l.measured.get(),
                                },
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            // Height estimate for a never-shaped line: nominal line boxes by an
            // average-advance guess. An unwrapped line is EXACTLY one line box
            // (`LineHeight::FontSizeRelative` is content-independent), so only
            // wrapped lines can drift — and they correct on first visibility.
            let avg_char_px = (self.style.font_size as f64) * 0.55;
            let mut lines = Vec::with_capacity(display.split('\n').count());
            let (mut y, mut start) = (0.0_f64, 0_usize);
            for seg in display.split('\n') {
                let empty = seg.is_empty();
                // Empty lines shape a single space so caret/selection geometry
                // exists; local offsets clamp to len=0, the glyph is unreachable.
                let shaped: &str = if empty { " " } else { seg };
                let line_key = {
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    0x000F_1E1D_11E5_u64.hash(&mut h);
                    shaped.hash(&mut h);
                    style_h.hash(&mut h);
                    width.to_bits().hash(&mut h);
                    h.finish()
                };
                let (layout, height, measured) = match reuse.get(&line_key) {
                    Some(c) if c.measured => (c.layout.clone(), c.height, true),
                    _ => {
                        let est_rows = if empty || (seg.len() as f64) * avg_char_px <= width {
                            1.0
                        } else {
                            ((seg.len() as f64) * avg_char_px / width.max(1.0)).ceil()
                        };
                        // An empty line's fake space always lays out to exactly
                        // one line box — its "estimate" is already exact.
                        (None, self.line_px * est_rows, empty)
                    }
                };
                lines.push(crate::text_edit::LineSlot {
                    key: line_key,
                    start,
                    len: seg.len(),
                    y: std::cell::Cell::new(y),
                    height: std::cell::Cell::new(height),
                    measured: std::cell::Cell::new(measured),
                    layout: std::cell::RefCell::new(layout),
                    empty,
                });
                y += height;
                start += seg.len() + 1;
            }
            let table = crate::text_edit::LineTable {
                lines,
                text_len: display.len(),
                display: Rc::from(display.as_str()),
                shape: crate::text_edit::ShapeSpec { style: TextStyleSpec::for_field(&self.style), width },
                line_px: self.line_px,
                dirty_geometry: std::cell::Cell::new(false),
            };
            // Small documents: shape everything now — exact heights, zero
            // corrective passes, identical to the pre-P5.2 behavior.
            if table.line_count() <= LAZY_THRESHOLD && !table.lines.is_empty() {
                table.materialize_span(0, table.line_count() - 1, cx.text);
                table.reflow();
            }
            self.table = Some(Rc::new(table));
            self.table_key = Some(table_key);
        }
        let table = self.table.clone().expect("table just built");

        // Ensure the caret's neighborhood is shaped on EVERY layout (blink
        // included — a no-op once materialized): caret paint, scroll-to-caret
        // and the next few motion steps then always resolve against real
        // layouts, however far the caret jumped.
        if !table.lines.is_empty() {
            let ci = table.line_of(composed.caret.min(display.len()));
            table.materialize_span(ci.saturating_sub(CARET_WINDOW), ci + CARET_WINDOW, cx.text);
            table.reflow();
        }

        // Publish for hit-testing / motion — same conditions as the single path:
        // real text only, never while composing (offsets shift under the preedit).
        if self.preedit.is_empty() && !self.text.is_empty() {
            crate::text_edit::store_lines(self.field_id, table.clone());
        } else {
            crate::text_edit::clear(self.field_id);
        }
        self.cached = None;

        let height = table.total_height().max(self.line_px);
        constraints.constrain(Size::new(width, height))
    }

    /// Multi-line paint: selection, glyphs, preedit underline and caret through
    /// the line table — every stage windowed to the visible rect. Lazily built
    /// lines (P5.2) **materialize here**, the moment they scroll into view; when
    /// a measurement moves the estimated geometry, the pass reflows the table
    /// and requests a corrective relayout (estimate-then-measure, like the
    /// ListView's extents).
    fn paint_lines(&self, cx: &mut PaintCx<'_>, offset: Offset, table: &crate::text_edit::LineTable) {
        let visible = cx.visible();
        let (caret_byte, preedit, composing) = self.composed_meta();
        let has_text = !self.text.is_empty();
        if table.lines.is_empty() {
            return;
        }

        // 0. Materialize the visible window (±1 line so single-step vertical
        // motion always lands on a shaped line), then settle geometry. The span
        // is found by binary search on the y prefix — never a full-table scan.
        let lo = table.line_at_y(visible.y0 - offset.y).saturating_sub(1);
        let hi = (table.line_at_y(visible.y1 - offset.y) + 1).min(table.line_count() - 1);
        table.materialize_span(lo, hi, cx.text);
        if table.reflow() {
            // Heights moved: the size layout reported is stale — schedule the
            // corrective pass. Painting continues with the SETTLED y's below.
            cx.request_relayout();
        }
        let window = &table.lines[lo..=hi];

        // 1. Selection highlight (display offsets == text offsets: not obscured,
        // and skipped while composing, exactly like the single path).
        if has_text && !composing && self.anchor != self.focus {
            let (s0, s1) = (self.anchor.min(self.focus), self.anchor.max(self.focus));
            for l in window {
                let top = offset.y + l.y.get();
                if top + l.height.get() < visible.y0 {
                    continue;
                }
                if top > visible.y1 {
                    break;
                }
                let line_end = l.start + l.len;
                if line_end < s0 || l.start > s1 {
                    continue;
                }
                if l.empty {
                    // A fully-selected empty line shows a thin stub.
                    let r = Rect::new(offset.x, top, offset.x + 6.0, top + l.height.get());
                    cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.selection_color, None, &r);
                    continue;
                }
                let la = s0.saturating_sub(l.start).min(l.len);
                let lf = (s1 - l.start).min(l.len);
                if la >= lf {
                    continue;
                }
                let Some(layout) = l.layout() else { continue };
                let mut rects = Vec::new();
                layout.selection_rects(la, lf, &mut rects);
                for bb in rects {
                    let rect = Rect::new(offset.x + bb.x0, top + bb.y0, offset.x + bb.x1, top + bb.y1);
                    cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.selection_color, None, &rect);
                }
            }
        }

        // 2. Glyphs — only the visible window of lines and runs.
        let brushes = [crate::text::paint::TextPaint::solid(self.style.color)];
        for l in window {
            let top = offset.y + l.y.get();
            if top + l.height.get() < visible.y0 {
                continue;
            }
            if top > visible.y1 {
                break;
            }
            if l.empty {
                continue; // nothing visible on an empty line (the space is fake)
            }
            let Some(layout) = l.layout() else { continue };
            crate::text::paint::paint_text_layout(cx, &layout, Offset::new(offset.x, top), &brushes);
        }

        // 3. Composition underline beneath the preedit range — the affected
        // lines are found by byte range (the preedit hugs the caret, whose
        // window layout materialized).
        if let Some((p0, p1)) = preedit {
            let (i0, i1) = (table.line_of(p0), table.line_of(p1).min(table.line_count() - 1));
            for l in &table.lines[i0..=i1] {
                let top = offset.y + l.y.get();
                let line_end = l.start + l.len;
                if line_end < p0 || l.start > p1 || l.empty {
                    continue;
                }
                let la = p0.saturating_sub(l.start).min(l.len);
                let lf = (p1 - l.start).min(l.len);
                if la >= lf {
                    continue;
                }
                let Some(layout) = l.layout() else { continue };
                let mut rects = Vec::new();
                layout.selection_rects(la, lf, &mut rects);
                for bb in rects {
                    let rect =
                        Rect::new(offset.x + bb.x0, top + bb.y1 - 1.5, offset.x + bb.x1, top + bb.y1 - 0.5);
                    cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.caret_color, None, &rect);
                }
            }
        }

        // 4. Caret at the focus (display position; end of the preedit while
        // composing) — same 1px inset as the single path.
        if self.focused && self.caret_visible {
            let bb = table.caret_rect(caret_byte, 1.5);
            let rect = Rect::new(
                offset.x + bb.x0,
                offset.y + bb.y0 + 1.0,
                offset.x + bb.x0 + 1.5,
                offset.y + bb.y1 - 1.0,
            );
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.caret_color, None, &rect);
        }
    }
}
