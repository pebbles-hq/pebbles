//! [`RenderParagraph`] — a leaf render object that shapes text with parley and
//! paints its glyph runs into the vello scene.
//!
//! Layout builds and caches a `parley::Layout` (paint has no font context, so the
//! shaped layout must survive from layout to paint). Text is laid out in **logical**
//! pixels at scale 1.0; the shell applies the device-scale transform to the whole
//! scene, and because vello rasterizes glyph outlines through that transform the
//! text stays crisp at any DPI.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use pebbles_foundation::{Axis, Color, Offset, Rect, Size, TextAlign, TextDirection};
use parley::{
    Alignment, AlignmentOptions, FontStyle, FontWeight, Layout, LineHeight, PositionedLayoutItem,
    StyleProperty,
};
use vello::Glyph;
use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::text::TextEnv;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// Styling for a paragraph of text.
#[derive(Clone, Debug)]
pub struct ParagraphStyle {
    pub font_size: f32,
    pub color: Color,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    /// Font weight (400 = normal, 600 = semibold, 700 = bold).
    pub weight: f32,
    /// Horizontal alignment within the paragraph's width.
    pub align: TextAlign,
    /// Extra spacing between letters (logical px; 0 = none).
    pub letter_spacing: f32,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// A font family name (system fallback if unset/unavailable).
    pub font_family: Option<String>,
    /// Clamp the paragraph to at most this many lines (excess lines are dropped).
    pub max_lines: Option<u32>,
    /// With `max_lines`, append "…" to the last kept line when the text overflows.
    pub ellipsis: bool,
    /// Break onto the next line when the text exceeds the available width
    /// (`true`, the default). `false` shapes a single unbounded line that clips to
    /// the box (the classic one-line label, combine with `ellipsis`).
    pub soft_wrap: bool,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        ParagraphStyle {
            font_size: 16.0,
            color: pebbles_foundation::palette::BLACK,
            line_height: 1.2,
            weight: 400.0,
            align: TextAlign::Start,
            letter_spacing: 0.0,
            italic: false,
            underline: false,
            strikethrough: false,
            font_family: None,
            max_lines: None,
            ellipsis: false,
            soft_wrap: true,
        }
    }
}

/// A ranged style override for rich text: a byte `range` of the paragraph's text
/// plus the properties that differ from the base [`ParagraphStyle`]. One paragraph
/// with N spans shapes as ONE parley layout with per-range styles — never one
/// widget per styled run.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpanStyle {
    pub range: Range<usize>,
    /// Font weight override (400 normal … 700 bold).
    pub weight: Option<f32>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Color override for the range.
    pub color: Option<Color>,
    /// Font family override (e.g. the monospace family for inline code).
    pub family: Option<String>,
    /// Absolute font-size override (px).
    pub size: Option<f32>,
    /// Paint a rounded "chip" background behind the range (inline code).
    pub chip: Option<Color>,
    /// This range is a link — the index into the owning widget's URL list. The
    /// paragraph publishes the range's laid-out boxes through
    /// [`RenderParagraph::link_boxes`] so the widget's tap handler can resolve a
    /// local point to a link without re-walking the layout.
    pub link: Option<usize>,
}

impl TextSpanStyle {
    /// A no-override span for `range` — set fields from here.
    pub fn new(range: Range<usize>) -> Self {
        TextSpanStyle {
            range,
            weight: None,
            italic: false,
            underline: false,
            strikethrough: false,
            color: None,
            family: None,
            size: None,
            chip: None,
            link: None,
        }
    }
}

/// Map to parley's alignment. `Start`/`End` resolve against the ambient text
/// direction (D2): under RTL, `Start` is the right edge and `End` the left.
fn to_parley_align(a: TextAlign, rtl: bool) -> Alignment {
    match a {
        TextAlign::Left => Alignment::Left,
        TextAlign::Right => Alignment::Right,
        TextAlign::Center => Alignment::Center,
        TextAlign::Justify => Alignment::Justify,
        TextAlign::Start => {
            if rtl {
                Alignment::Right
            } else {
                Alignment::Left
            }
        }
        TextAlign::End => {
            if rtl {
                Alignment::Left
            } else {
                Alignment::Right
            }
        }
    }
}

// Debug-only tally of parley re-shapes (E3): the tests assert a stable string doesn't
// re-shape on a repeat layout. Bumped once per layout that actually shapes.
#[cfg(debug_assertions)]
thread_local! {
    static SHAPES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Debug-only: total parley re-shapes since [`reset_shape_count`]. Test hook (E3).
#[cfg(debug_assertions)]
pub fn shape_count() -> u64 {
    SHAPES.with(std::cell::Cell::get)
}

/// Debug-only: reset the re-shape tally.
#[cfg(debug_assertions)]
pub fn reset_shape_count() {
    SHAPES.with(|c| c.set(0));
}

/// A leaf render object that lays out and paints text.
pub struct RenderParagraph {
    pub text: String,
    pub style: ParagraphStyle,
    /// Ranged style overrides (rich text). Empty for plain paragraphs.
    pub spans: Vec<TextSpanStyle>,
    /// Where the paragraph publishes each link span's laid-out boxes
    /// `(rect in local space, link index)` — shared with the widget's tap handler.
    pub link_boxes: Option<Rc<RefCell<Vec<(Rect, usize)>>>>,
    /// Chip (inline-code) backgrounds computed from the shaped layout, local space.
    chips: Vec<(Rect, Color)>,
    /// Shaped layout, produced in [`RenderObject::layout`] and consumed in paint.
    cached: Option<Layout<Brush>>,
    /// E3 shape cache: a hash of `(text, style, spans, max_advance)` from the last
    /// shape, and the unconstrained size it produced. A layout whose key matches
    /// reuses `cached` (no re-shape); the returned size is re-`constrain`ed against
    /// the fresh constraints, so min-bound changes are still honored.
    shape_key: Option<u64>,
    shape_size: Size,
}

impl RenderParagraph {
    pub fn new(text: impl Into<String>, style: ParagraphStyle) -> Self {
        RenderParagraph {
            text: text.into(),
            style,
            spans: Vec::new(),
            link_boxes: None,
            chips: Vec::new(),
            cached: None,
            shape_key: None,
            shape_size: Size::new(0.0, 0.0),
        }
    }

    /// A rich paragraph: one shaped layout with per-range style overrides.
    pub fn with_spans(
        text: impl Into<String>,
        style: ParagraphStyle,
        spans: Vec<TextSpanStyle>,
    ) -> Self {
        let mut p = Self::new(text, style);
        p.spans = spans;
        p
    }

    /// A cheap key over everything that affects the shaped layout — field-by-field
    /// bit hashing. (Never a `Debug` format: that heap-allocated a string per
    /// paragraph per layout pass, which at document scale was an allocation storm.)
    fn shape_hash(&self, max_advance: Option<f32>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.text.hash(&mut h);
        let s = &self.style;
        s.font_size.to_bits().hash(&mut h);
        for c in s.color.components {
            c.to_bits().hash(&mut h);
        }
        s.line_height.to_bits().hash(&mut h);
        s.weight.to_bits().hash(&mut h);
        (s.align as u8).hash(&mut h);
        s.letter_spacing.to_bits().hash(&mut h);
        (s.italic, s.underline, s.strikethrough, s.soft_wrap, s.ellipsis).hash(&mut h);
        s.max_lines.hash(&mut h);
        s.font_family.hash(&mut h);
        for span in &self.spans {
            span.range.hash(&mut h);
            span.weight.map(f32::to_bits).hash(&mut h);
            (span.italic, span.underline, span.strikethrough).hash(&mut h);
            if let Some(c) = span.color {
                for v in c.components {
                    v.to_bits().hash(&mut h);
                }
            }
            span.family.hash(&mut h);
            span.size.map(f32::to_bits).hash(&mut h);
            if let Some(c) = span.chip {
                for v in c.components {
                    v.to_bits().hash(&mut h);
                }
            }
            span.link.hash(&mut h);
        }
        max_advance.map(f32::to_bits).hash(&mut h);
        // D2: ambient direction affects Start/End alignment, so it's part of the key.
        (crate::direction::text_direction() == TextDirection::Rtl).hash(&mut h);
        h.finish()
    }

    /// Shape `s` into a broken, aligned layout with this paragraph's style.
    fn build(&self, text_env: &mut TextEnv, s: &str, max_advance: Option<f32>) -> Layout<Brush> {
        let mut builder = text_env.layout.ranged_builder(&mut text_env.fonts, s, 1.0, true);
        builder.push_default(StyleProperty::FontSize(self.style.font_size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            self.style.line_height,
        )));
        builder.push_default(StyleProperty::Brush(Brush::Solid(self.style.color)));
        if self.style.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(self.style.letter_spacing));
        }
        if self.style.italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        if self.style.underline {
            builder.push_default(StyleProperty::Underline(true));
        }
        if self.style.strikethrough {
            builder.push_default(StyleProperty::Strikethrough(true));
        }
        if let Some(family) = &self.style.font_family {
            builder.push_default(StyleProperty::FontFamily(family.as_str().into()));
        }
        // Ranged overrides (rich text): each span pushes only the properties it
        // changes, over its byte range — one shaped layout for the whole block.
        for span in &self.spans {
            let r = span.range.clone();
            if r.is_empty() || r.end > s.len() {
                continue;
            }
            if let Some(w) = span.weight {
                builder.push(StyleProperty::FontWeight(FontWeight::new(w)), r.clone());
            }
            if span.italic {
                builder.push(StyleProperty::FontStyle(FontStyle::Italic), r.clone());
            }
            if span.underline {
                builder.push(StyleProperty::Underline(true), r.clone());
            }
            if span.strikethrough {
                builder.push(StyleProperty::Strikethrough(true), r.clone());
            }
            if let Some(c) = span.color {
                builder.push(StyleProperty::Brush(Brush::Solid(c)), r.clone());
            }
            if let Some(f) = &span.family {
                builder.push(StyleProperty::FontFamily(f.as_str().into()), r.clone());
            }
            if let Some(px) = span.size {
                builder.push(StyleProperty::FontSize(px), r.clone());
            }
        }
        let mut layout: Layout<Brush> = builder.build(s);
        layout.break_all_lines(max_advance);
        let rtl = crate::direction::text_direction() == TextDirection::Rtl;
        layout.align(to_parley_align(self.style.align, rtl), AlignmentOptions::default());
        layout
    }

    /// The laid-out boxes (local space) of a byte `range` — one rect per line the
    /// range crosses, spanning the covered clusters.
    fn range_boxes(layout: &Layout<Brush>, range: &Range<usize>, out: &mut Vec<Rect>) {
        for line in layout.lines() {
            let lm = line.metrics();
            let (mut x0, mut x1): (Option<f64>, f64) = (None, 0.0);
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(gr) = item else { continue };
                let run = gr.run();
                let rr = run.text_range();
                if rr.end <= range.start || rr.start >= range.end {
                    continue;
                }
                let mut x = f64::from(gr.offset());
                for cluster in run.clusters() {
                    let cr = cluster.text_range();
                    let adv = f64::from(cluster.advance());
                    if cr.start < range.end && cr.end > range.start {
                        if x0.is_none() {
                            x0 = Some(x);
                        }
                        x1 = x1.max(x + adv);
                    }
                    x += adv;
                }
            }
            if let Some(x0) = x0 {
                out.push(Rect::new(
                    x0,
                    f64::from(lm.block_min_coord),
                    x1,
                    f64::from(lm.block_max_coord),
                ));
            }
        }
    }

    /// Refresh chip backgrounds + published link boxes from the current layout.
    /// Runs on every layout execution (not just re-shapes): a widget update swaps
    /// in a fresh `link_boxes` cell that must be re-filled even when the shape is
    /// cache-hit.
    fn refresh_span_geometry(&mut self) {
        let Some(layout) = &self.cached else { return };
        let needs_chips = self.spans.iter().any(|s| s.chip.is_some());
        let needs_links =
            self.link_boxes.is_some() && self.spans.iter().any(|s| s.link.is_some());
        if !needs_chips && !needs_links {
            return;
        }
        self.chips.clear();
        let mut links: Vec<(Rect, usize)> = Vec::new();
        let mut boxes: Vec<Rect> = Vec::new();
        for span in &self.spans {
            if span.chip.is_none() && span.link.is_none() {
                continue;
            }
            boxes.clear();
            Self::range_boxes(layout, &span.range, &mut boxes);
            for r in &boxes {
                if let Some(c) = span.chip {
                    self.chips.push((r.inflate(2.0, 0.5), c));
                }
                if let Some(ix) = span.link {
                    links.push((*r, ix));
                }
            }
        }
        if let Some(cell) = &self.link_boxes {
            *cell.borrow_mut() = links;
        }
    }
}

impl RenderObject for RenderParagraph {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let max_advance = if self.style.soft_wrap && constraints.has_bounded_width() {
            Some(constraints.max_width as f32)
        } else {
            None
        };

        // E3: skip the parley re-shape when text/style/wrap-width are unchanged. The
        // cached `Layout` survives for paint; only re-`constrain` the stored size.
        let key = self.shape_hash(max_advance);
        if self.shape_key == Some(key) && self.cached.is_some() {
            self.refresh_span_geometry();
            return constraints.constrain(self.shape_size);
        }
        #[cfg(debug_assertions)]
        SHAPES.with(|c| c.set(c.get() + 1));

        let text_env = &mut *cx.text;
        let mut layout = self.build(text_env, &self.text, max_advance);

        // `max_lines` clamp. With `ellipsis`, when the text overflows, re-shape the
        // longest character prefix that still fits in `max_lines` lines once "…" is
        // appended (binary search on the char boundary).
        let full_h = layout.height() as f64;
        let mut height = full_h;
        if let Some(max) = self.style.max_lines {
            let max = max as usize;
            let over = layout.lines().count() > max;
            if over && self.style.ellipsis && !self.text.is_empty() {
                let chars: Vec<usize> = self.text.char_indices().map(|(i, _)| i).collect();
                let (mut lo, mut hi, mut best) = (0usize, chars.len(), String::from("…"));
                while lo < hi {
                    let mid = (lo + hi).div_ceil(2);
                    let cut = chars.get(mid).copied().unwrap_or(self.text.len());
                    let candidate = format!("{}…", &self.text[..cut]);
                    let trial = self.build(text_env, &candidate, max_advance);
                    if trial.lines().count() <= max {
                        best = candidate;
                        lo = mid;
                    } else {
                        hi = mid - 1;
                    }
                }
                layout = self.build(text_env, &best, max_advance);
            }
            // Report a height covering only the kept lines; paint skips the rest.
            let h: f64 = layout.lines().take(max).map(|l| l.metrics().line_height as f64).sum();
            if h > 0.0 {
                height = h.min(layout.height() as f64);
            }
        }
        let size = Size::new(layout.width() as f64, height);
        self.cached = Some(layout);
        self.shape_size = size;
        self.shape_key = Some(key);
        self.refresh_span_geometry();
        constraints.constrain(size)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(layout) = &self.cached else { return };
        let transform = Affine::translate((offset.x, offset.y));

        // Chip (inline-code) backgrounds first, behind the glyphs — culled per chip.
        let visible = cx.visible();
        for (r, color) in &self.chips {
            let world = Rect::new(r.x0 + offset.x, r.y0 + offset.y, r.x1 + offset.x, r.y1 + offset.y);
            if world.y1 < visible.y0 || world.y0 > visible.y1 {
                continue;
            }
            cx.scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                *color,
                None,
                &world.to_rounded_rect(4.0),
            );
        }

        // Line-level culling: a single huge paragraph must not encode glyphs the
        // viewport can't show. Lines are in top-to-bottom order, so once one
        // starts below the visible window the rest can't be visible either.
        let max_lines = self.style.max_lines.map(|m| m as usize).unwrap_or(usize::MAX);
        for line in layout.lines().take(max_lines) {
            let m = line.metrics();
            if offset.y + f64::from(m.block_max_coord) < visible.y0 {
                continue;
            }
            if offset.y + f64::from(m.block_min_coord) > visible.y1 {
                break;
            }
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                // Horizontal culling within the line: an unwrapped multi-thousand-
                // glyph code line must not encode runs beyond the window's x-range.
                let run_x0 = offset.x + f64::from(glyph_run.offset());
                let run_x1 = run_x0 + f64::from(glyph_run.advance());
                if run_x1 < visible.x0 || run_x0 > visible.x1 {
                    continue;
                }
                crate::stats::bump_glyph_run();
                let run = glyph_run.run();
                let synthesis = run.synthesis();
                let glyph_transform = synthesis
                    .skew()
                    .map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0));

                cx.scene
                    .draw_glyphs(run.font())
                    .brush(&glyph_run.style().brush)
                    .transform(transform)
                    .glyph_transform(glyph_transform)
                    .font_size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .draw(
                        Fill::NonZero,
                        glyph_run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x, y: g.y }),
                    );

                // Underline / strikethrough decorations (base style or spans).
                let style_ref = glyph_run.style();
                if style_ref.underline.is_some() || style_ref.strikethrough.is_some() {
                    let rm = run.metrics();
                    let x0 = f64::from(glyph_run.offset()) + offset.x;
                    let x1 = x0 + f64::from(glyph_run.advance());
                    let baseline = f64::from(glyph_run.baseline()) + offset.y;
                    if let Some(dec) = &style_ref.underline {
                        let top = baseline - f64::from(dec.offset.unwrap_or(rm.underline_offset));
                        let size_v = f64::from(dec.size.unwrap_or(rm.underline_size));
                        cx.scene.fill(
                            Fill::NonZero,
                            Affine::IDENTITY,
                            &dec.brush,
                            None,
                            &Rect::new(x0, top, x1, top + size_v),
                        );
                    }
                    if let Some(dec) = &style_ref.strikethrough {
                        let top =
                            baseline - f64::from(dec.offset.unwrap_or(rm.strikethrough_offset));
                        let size_v = f64::from(dec.size.unwrap_or(rm.strikethrough_size));
                        cx.scene.fill(
                            Fill::NonZero,
                            Affine::IDENTITY,
                            &dec.brush,
                            None,
                            &Rect::new(x0, top, x1, top + size_v),
                        );
                    }
                }
            }
        }
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        match axis {
            // The widest unbreakable run — approximated as the widest
            // whitespace-separated token (Flutter uses the same word-boundary
            // notion for `getMinIntrinsicWidth`). Capped for pathological input.
            Axis::Horizontal => {
                let text_env = &mut *cx.text;
                let natural =
                    self.build(text_env, &self.text, None).width() as f64;
                let mut max_word: f64 = 0.0;
                for token in self.text.split_whitespace().take(256) {
                    let w = self.build(text_env, token, None).width() as f64;
                    max_word = max_word.max(w);
                }
                Some(if max_word > 0.0 { max_word } else { natural })
            }
            // The height the paragraph takes when wrapped at `cross_extent`.
            Axis::Vertical => {
                let max_advance = if cross_extent.is_finite() {
                    Some(cross_extent.max(0.0) as f32)
                } else {
                    None
                };
                Some(self.build(&mut *cx.text, &self.text, max_advance).height() as f64)
            }
        }
    }

    fn baseline(&self, _cx: &mut LayoutCx<'_>) -> Option<f64> {
        // The first line's baseline: the line's top offset plus its metrics'
        // baseline (the distance from the line top to the alphabetic baseline).
        self.cached.as_ref().and_then(|layout| {
            layout.lines().next().map(|line| {
                let m = line.metrics();
                (m.block_min_coord + m.baseline) as f64
            })
        })
    }

    fn debug_name(&self) -> &'static str {
        "RenderParagraph"
    }
}
