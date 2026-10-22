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
use peniko::Fill;
use smallvec::SmallVec;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::text::layout::LayoutParams;
use crate::text::style::{SpanSpec, TextStyleSpec, to_cosmic_align};
use crate::text::{TextEnv, TextLayout, TextRequest};
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

type LinkBoxes = Rc<RefCell<Vec<(Rect, usize)>>>;

/// How a text decoration line is drawn (Flutter `TextDecorationStyle`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextDecorationStyle {
    /// One continuous line.
    #[default]
    Solid,
    /// Two parallel lines.
    Double,
    /// Round dots.
    Dotted,
    /// Short dashes.
    Dashed,
    /// A sine-like wave.
    Wavy,
}

/// Where an inline placeholder sits on its line (Flutter `PlaceholderAlignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PlaceholderAlignment {
    /// The box's `baseline_offset` sits on the text baseline.
    #[default]
    Baseline,
    /// The box's bottom sits on the baseline.
    AboveBaseline,
    /// The box's top sits on the baseline.
    BelowBaseline,
    /// The box's top meets the line's top.
    Top,
    /// The box's bottom meets the line's bottom.
    Bottom,
    /// The box is centered in the line.
    Middle,
}

/// An inline box laid out with the text (Flutter `PlaceholderSpan`): one U+FFFC in the
/// paragraph text per placeholder, in order. Nothing is painted for it (the widget paints).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placeholder {
    pub width: f32,
    pub height: f32,
    pub alignment: PlaceholderAlignment,
    /// Distance from the box's top to its baseline (`Baseline` alignment).
    pub baseline_offset: f32,
}

/// An OpenType feature setting (Flutter `FontFeature`), e.g. `tnum = 1`, `liga = 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontFeature {
    pub tag: [u8; 4],
    pub value: u32,
}

/// A variable-font axis setting (Flutter `FontVariation`), e.g. `opsz = 32`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontVariation {
    pub axis: [u8; 4],
    pub value: f32,
}

/// A glyph shadow (Flutter `Shadow` in `TextStyle.shadows`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextShadow {
    /// Offset of the shadow from the glyphs (logical px).
    pub offset: Offset,
    /// Gaussian blur sigma (0 = a sharp copy).
    pub blur_sigma: f32,
    pub color: Color,
}

/// A minimum (or forced) line box for every line of a paragraph (Flutter `StrutStyle`).
#[derive(Clone, Debug, PartialEq)]
pub struct StrutStyle {
    /// The strut's font family (`None` = the default family).
    pub font_family: Option<String>,
    /// The strut font size (px).
    pub font_size: f32,
    /// Line height as a multiple of `font_size` (`None` = the font's ascent + descent).
    pub height: Option<f32>,
    /// Extra leading as a multiple of `font_size`, split half above and half below.
    pub leading: Option<f32>,
    /// Every line is exactly the strut's height, whatever its glyphs.
    pub force_strut_height: bool,
}

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
    /// Extra advance added to each word separator (space, NBSP, …), logical px; may be negative.
    pub word_spacing: f32,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// A line over the text, at the font's ascent.
    pub overline: bool,
    /// How underline / overline / strikethrough lines are drawn.
    pub decoration_style: TextDecorationStyle,
    /// Decoration color (`None` = the text color).
    pub decoration_color: Option<Color>,
    /// Decoration thickness as a multiple of the font's own (1.0 = the font's).
    pub decoration_thickness: f32,
    /// OpenType feature settings for the whole paragraph.
    pub font_features: Vec<FontFeature>,
    /// Variable-font axis settings (`wght` overrides `weight` when set).
    pub font_variations: Vec<FontVariation>,
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
    /// A minimum (or forced) line box for every line (`None` = the glyphs' own).
    pub strut: Option<StrutStyle>,
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
            word_spacing: 0.0,
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
            decoration_style: TextDecorationStyle::Solid,
            decoration_color: None,
            decoration_thickness: 1.0,
            font_features: Vec::new(),
            font_variations: Vec::new(),
            font_family: None,
            max_lines: None,
            ellipsis: false,
            soft_wrap: true,
            strut: None,
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
    /// Overline the range.
    pub overline: bool,
    /// Decoration style override for the range.
    pub decoration_style: Option<TextDecorationStyle>,
    /// Decoration color override for the range.
    pub decoration_color: Option<Color>,
    /// Decoration thickness multiplier override for the range.
    pub decoration_thickness: Option<f32>,
    /// Color override for the range.
    pub color: Option<Color>,
    /// Glyph shadows of the range, painted under its glyphs in list order.
    pub shadows: Vec<TextShadow>,
    /// Fill behind the range, spanning the full line box (no horizontal padding).
    pub background: Option<Color>,
    /// Font family override (e.g. the monospace family for inline code).
    pub family: Option<String>,
    /// Absolute font-size override (px).
    pub size: Option<f32>,
    /// Word-spacing override for the range (px).
    pub word_spacing: Option<f32>,
    /// OpenType features for the range (empty = the paragraph's).
    pub font_features: Vec<FontFeature>,
    /// Variable-font axes for the range (empty = the paragraph's).
    pub font_variations: Vec<FontVariation>,
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
            overline: false,
            decoration_style: None,
            decoration_color: None,
            decoration_thickness: None,
            color: None,
            shadows: Vec::new(),
            background: None,
            family: None,
            size: None,
            word_spacing: None,
            font_features: Vec::new(),
            font_variations: Vec::new(),
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
    /// Inline boxes, one per U+FFFC in `text`, in order (T48). Set directly for bare
    /// placeholders; recomputed from the children every layout when `inline` is non-empty.
    pub placeholders: Vec<Placeholder>,
    /// Inline render children (WidgetSpan, T50): per child its alignment and optional baseline
    /// offset; the children themselves are this object's render children, in order.
    pub inline: Vec<(PlaceholderAlignment, Option<f32>)>,
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
    brushes: SmallVec<[crate::text::paint::TextPaint; 4]>,
}

impl RenderParagraph {
    pub fn new(text: impl Into<String>, style: ParagraphStyle) -> Self {
        RenderParagraph {
            text: text.into(),
            style,
            spans: Vec::new(),
            link_boxes: None,
            placeholders: Vec::new(),
            inline: Vec::new(),
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
    fn params(&self, text: &mut TextEnv, width: Option<f32>) -> LayoutParams {
        let rtl = crate::direction::text_direction() == TextDirection::Rtl;
        let width = if self.style.soft_wrap { width } else { None };
        LayoutParams {
            width,
            wrap: width.is_some(),
            align: Some(to_cosmic_align(self.style.align, rtl)),
            max_lines: self.style.max_lines,
            ellipsis: self.style.ellipsis,
            strut: self.style.strut.as_ref().map(|s| text.strut_metrics(s)),
        }
    }

    /// The layout of this paragraph's text with `params`, through the window's caches.
    fn layout_with(&self, text: &mut TextEnv, params: LayoutParams) -> (Rc<TextLayout>, u64) {
        let mut spec = TextStyleSpec::from(&self.style);
        spec.placeholders.clone_from(&self.placeholders);
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
        // Inline children first: each becomes a placeholder of its laid-out size (T50).
        let children = cx.children();
        if !children.is_empty() {
            let loose = BoxConstraints::loose(Size::new(constraints.max_width, f64::INFINITY));
            self.placeholders = children
                .iter()
                .enumerate()
                .map(|(i, &child)| {
                    let size = cx.layout_child(child, loose);
                    let (alignment, offset) = self.inline.get(i).copied().unwrap_or_default();
                    let baseline =
                        offset.map(f64::from).or_else(|| cx.child_baseline(child)).unwrap_or(size.height);
                    Placeholder {
                        width: size.width as f32,
                        height: size.height as f32,
                        alignment,
                        baseline_offset: baseline as f32,
                    }
                })
                .collect();
        }
        let params = self.params(cx.text, width);
        let (layout, key) = self.layout_with(cx.text, params);
        let size = Size::new(f64::from(layout.width()), f64::from(layout.height()));
        for (i, &child) in children.iter().enumerate() {
            if let Some(r) = layout.placeholder_boxes().get(i) {
                cx.set_child_offset(child, Offset::new(r.x0, r.y0));
            }
        }
        self.cached = Some(layout);
        self.layout_key = Some(key);
        self.brushes.clear();
        let base = crate::text::paint::TextPaint::base(&self.style);
        self.brushes.push(base.clone());
        self.brushes.extend(self.spans.iter().map(|s| base.for_span(s)));
        self.refresh_span_geometry();
        constraints.constrain(size)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(layout) = &self.cached else { return };
        let visible = cx.visible();
        let mut rects: Vec<Rect> = Vec::new();

        // Span backgrounds, behind everything (Flutter `TextStyle.background`).
        crate::text::paint::paint_backgrounds(&mut cx.scene, visible, layout, offset, &self.spans);

        // Selection highlight, over the backgrounds (only when a non-empty range is set).
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

        // Glyph runs + decorations, culled per line and per run; inline children paint over the
        // text at their placeholder boxes (T50).
        crate::text::paint::paint_text_layout(cx, layout, offset, &self.brushes);
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        match axis {
            // The widest unbreakable run (Flutter's `getMinIntrinsicWidth` notion) from the ONE
            // unbounded layout's advances — memoized on the shape, never a reshape.
            Axis::Horizontal => {
                let params = self.params(cx.text, None);
                let (layout, _) = self.layout_with(cx.text, params);
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
                    strut: self.style.strut.as_ref().map(|s| cx.text.strut_metrics(s)),
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
