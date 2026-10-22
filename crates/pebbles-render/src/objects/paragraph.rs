//! [`RenderParagraph`] — a leaf render object that lays out text with the cosmic-text
//! [`TextLayout`](crate::text::TextLayout) and paints its glyph runs as backend-neutral ops.
//!
//! Layout asks the window's [`TextEnv`] for a layout (shape and layout caches keyed by
//! content, never by color); the `Rc<TextLayout>` survives from layout to paint. Text is laid
//! out in **logical** pixels; the renderer applies the device scale to the whole scene.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use kurbo::Affine;
use pebbles_foundation::{Axis, Color, Offset, Rect, Size, TextAlign, TextDirection};
use peniko::{Brush, Fill};
use smallvec::SmallVec;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::text::layout::LayoutParams;
use crate::text::style::{SpanSpec, TextStyleSpec, to_cosmic_align};
use crate::text::{TextEnv, TextLayout, TextRequest};
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

type LinkBoxes = Rc<RefCell<Vec<(Rect, usize)>>>;

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
    /// TAB advance in space widths: a TAB moves to the next multiple of `tab_width × space
    /// advance`. Default 1 (one column in monospace).
    pub tab_width: u16,
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
            tab_width: crate::text::style::DEFAULT_TAB_WIDTH,
        }
    }
}

/// A ranged style override for rich text: a byte `range` of the paragraph's text
/// plus the properties that differ from the base [`ParagraphStyle`]. One paragraph
/// with N spans shapes as ONE layout with per-range styles — never one
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

/// A leaf render object that lays out and paints text.
pub struct RenderParagraph {
    pub text: String,
    pub style: ParagraphStyle,
    /// Ranged style overrides (rich text). Empty for plain paragraphs.
    pub spans: Vec<TextSpanStyle>,
    /// Where the paragraph publishes each link span's laid-out boxes
    /// `(rect in local space, link index)` — shared with the widget's tap handler.
    pub link_boxes: Option<LinkBoxes>,
    /// When set, the shaped layout is published to [`crate::text_edit`] under this
    /// id at layout, so the owning widget can hit-test a point → byte for text
    /// selection. Opt-in — plain/rich paragraphs leave it `None`.
    pub select_id: Option<u64>,
    /// The active selection `(anchor, focus)` byte offsets, fed by the widget.
    /// Painted behind the glyphs; `None` (or `anchor == focus`) paints nothing.
    pub selection: Option<(usize, usize)>,
    /// Selection highlight color.
    pub selection_color: Color,
    /// The link index (into the widget's URL list) to draw a keyboard-focus ring
    /// around — for `Tab`-through-links a11y. `None` draws no ring.
    pub focused_link: Option<usize>,
    /// Chip (inline-code) backgrounds computed from the shaped layout, local space.
    chips: Vec<(Rect, Color)>,
    /// The layout, produced in [`RenderObject::layout`] and consumed in paint. `Rc`: shared
    /// through the window's [`TextEnv`] caches, so a REBUILT paragraph (virtual-list windows
    /// sliding, unrelated toggles) reuses the shape and layout instead of redoing them.
    cached: Option<Rc<TextLayout>>,
    /// The layout-cache key of `cached`: an unchanged key skips even the cache lookup.
    layout_key: Option<u64>,
    /// Brush table (slot 0 = base color, slot i + 1 = span i), rebuilt every layout: colors
    /// never enter the shape key (03 §3.9.4).
    brushes: SmallVec<[Brush; 4]>,
}

impl RenderParagraph {
    pub fn new(text: impl Into<String>, style: ParagraphStyle) -> Self {
        RenderParagraph {
            text: text.into(),
            style,
            spans: Vec::new(),
            link_boxes: None,
            select_id: None,
            selection: None,
            selection_color: Color::from_rgba8(59, 130, 246, 70),
            focused_link: None,
            chips: Vec::new(),
            cached: None,
            layout_key: None,
            brushes: SmallVec::new(),
        }
    }

    /// A rich paragraph: one shaped layout with per-range style overrides.
    pub fn with_spans(text: impl Into<String>, style: ParagraphStyle, spans: Vec<TextSpanStyle>) -> Self {
        let mut p = Self::new(text, style);
        p.spans = spans;
        p
    }

    /// The layout request for this paragraph at `width` (03 §3.5.1: a width only when soft
    /// wrap is on and the box is bounded).
    fn params(&self, width: Option<f32>) -> LayoutParams {
        let rtl = crate::direction::text_direction() == TextDirection::Rtl;
        let width = if self.style.soft_wrap { width } else { None };
        LayoutParams {
            width,
            wrap: width.is_some(),
            align: Some(to_cosmic_align(self.style.align, rtl)),
            max_lines: self.style.max_lines,
            ellipsis: self.style.ellipsis,
        }
    }

    /// The layout of this paragraph's text with `params`, through the window's caches.
    fn layout_with(&self, text: &mut TextEnv, params: LayoutParams) -> (Rc<TextLayout>, u64) {
        let spec = TextStyleSpec::from(&self.style);
        let spans: Vec<SpanSpec> = self.spans.iter().map(SpanSpec::from).collect();
        let req = TextRequest { text: &self.text, style: &spec, spans: &spans, params };
        let key = text.text_key(&req);
        if self.layout_key == Some(key.layout)
            && let Some(cached) = &self.cached
        {
            return (cached.clone(), key.layout);
        }
        (text.layout_text(&req, key), key.layout)
    }

    /// Refresh chip backgrounds + published link boxes from the current layout.
    /// Runs on every layout execution (not just re-shapes): a widget update swaps
    /// in a fresh `link_boxes` cell that must be re-filled even when the shape is
    /// cache-hit.
    fn refresh_span_geometry(&mut self) {
        let Some(layout) = &self.cached else { return };
        // Publish the shaped layout for point→byte hit-testing when selection is on.
        if let Some(id) = self.select_id {
            crate::text_edit::store(id, layout.clone());
        }
        let needs_chips = self.spans.iter().any(|s| s.chip.is_some());
        let needs_links = self.link_boxes.is_some() && self.spans.iter().any(|s| s.link.is_some());
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
            layout.range_boxes(span.range.clone(), &mut boxes);
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
        let width = constraints.has_bounded_width().then_some(constraints.max_width as f32);
        let (layout, key) = self.layout_with(cx.text, self.params(width));
        let size = Size::new(f64::from(layout.width()), f64::from(layout.height()));
        self.cached = Some(layout);
        self.layout_key = Some(key);
        self.brushes.clear();
        self.brushes.push(Brush::Solid(self.style.color));
        self.brushes.extend(self.spans.iter().map(|s| Brush::Solid(s.color.unwrap_or(self.style.color))));
        self.refresh_span_geometry();
        constraints.constrain(size)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(layout) = &self.cached else { return };
        let visible = cx.visible();
        let mut rects: Vec<Rect> = Vec::new();

        // Selection highlight, behind everything (only when a non-empty range is set).
        if let Some((a, f)) = self.selection
            && a != f
        {
            let len = self.text.len();
            layout.selection_rects(a.min(len), f.min(len), &mut rects);
            for r in &rects {
                let rect = *r + offset;
                if rect.y1 < visible.y0 || rect.y0 > visible.y1 {
                    continue;
                }
                cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.selection_color, None, &rect);
            }
        }

        // Keyboard-focus highlight behind the focused link's glyphs (a11y Tab-nav).
        if let Some(fl) = self.focused_link {
            rects.clear();
            for span in &self.spans {
                if span.link == Some(fl) {
                    layout.range_boxes(span.range.clone(), &mut rects);
                }
            }
            let ring = Color::from_rgba8(59, 130, 246, 64);
            for r in &rects {
                let rect = (*r + offset).inflate(1.0, 1.0);
                if rect.y1 < visible.y0 || rect.y0 > visible.y1 {
                    continue;
                }
                cx.scene.fill(Fill::NonZero, Affine::IDENTITY, ring, None, &rect.to_rounded_rect(3.0));
            }
        }

        // Chip (inline-code) backgrounds first, behind the glyphs — culled per chip.
        for (r, color) in &self.chips {
            let world = *r + offset;
            if world.y1 < visible.y0 || world.y0 > visible.y1 {
                continue;
            }
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, *color, None, &world.to_rounded_rect(4.0));
        }

        // Glyph runs + decorations, culled per line and per run.
        crate::text::paint::paint_text_layout(cx, layout, offset, &self.brushes);
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        match axis {
            // The widest unbreakable run (Flutter's `getMinIntrinsicWidth` notion) from the ONE
            // unbounded layout's advances — memoized on the shape, never a reshape.
            Axis::Horizontal => {
                let (layout, _) = self.layout_with(cx.text, self.params(None));
                Some(f64::from(cx.text.intrinsic_widths(&layout).0))
            }
            // The height when wrapped at `cross_extent` (max_lines ignored: quirk Q-3).
            Axis::Vertical => {
                let rtl = crate::direction::text_direction() == TextDirection::Rtl;
                let width = cross_extent.is_finite().then_some(cross_extent.max(0.0) as f32);
                let params = LayoutParams {
                    width,
                    wrap: width.is_some(),
                    align: Some(to_cosmic_align(self.style.align, rtl)),
                    max_lines: None,
                    ellipsis: false,
                };
                let (layout, _) = self.layout_with(cx.text, params);
                Some(f64::from(layout.height()))
            }
        }
    }

    fn baseline(&self, _cx: &mut LayoutCx<'_>) -> Option<f64> {
        self.cached.as_ref().and_then(|l| l.first_baseline()).map(f64::from)
    }

    fn debug_name(&self) -> &'static str {
        "RenderParagraph"
    }
}
