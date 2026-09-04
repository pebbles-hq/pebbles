//! [`Text`] — a leaf widget that shapes and paints a string. Backs
//! [`pebbles_render::RenderParagraph`].

use pebbles_foundation::{Color, TextAlign};
use pebbles_render::{ParagraphStyle, RenderObject, RenderParagraph};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// A run of styled text.
#[derive(Clone)]
pub struct Text {
    pub data: String,
    pub style: ParagraphStyle,
}

/// Create a [`Text`] widget. Chain `.size(..)` / `.color(..)` to style it.
pub fn text(data: impl Into<String>) -> Text {
    Text { data: data.into(), style: ParagraphStyle::default() }
}

/// E5 — a `Text` bound to a `Signal<String>`, isolated in its own leaf component: a
/// write re-renders ONLY this text node, not the owning component. That's the spike's
/// finding — per-component granularity, applied to a leaf, already gives fine-grained
/// text updates, so the heavier render-object-direct-write path stays unbuilt (its win
/// is unproven per the E5 charter). Style it via the closure, e.g.
/// `text_signal(count)` or wrap: `text(sig.get()).size(24.0)` inside `component(..)`.
pub fn text_signal(signal: pebbles_core::Signal<String>) -> impl pebbles_core::IntoWidget {
    pebbles_core::component_props(render_text_signal, TextSignalProps { signal })
}

#[derive(Clone)]
struct TextSignalProps {
    signal: pebbles_core::Signal<String>,
}

fn render_text_signal(p: &TextSignalProps) -> Text {
    text(p.signal.get())
}

impl Text {
    /// Set the font size (logical px).
    pub fn size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self
    }

    /// Set the text color.
    pub fn color(mut self, color: Color) -> Self {
        self.style.color = color;
        self
    }

    /// Set the line height as a multiple of the font size.
    pub fn line_height(mut self, factor: f32) -> Self {
        self.style.line_height = factor;
        self
    }

    /// Set an explicit font weight (400 normal … 700 bold).
    pub fn weight(mut self, weight: f32) -> Self {
        self.style.weight = weight;
        self
    }

    /// Semibold (600).
    pub fn semibold(mut self) -> Self {
        self.style.weight = 600.0;
        self
    }

    /// Bold (700).
    pub fn bold(mut self) -> Self {
        self.style.weight = 700.0;
        self
    }

    /// Horizontal alignment within the text's width.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.style.align = align;
        self
    }
    /// Extra spacing between letters (logical px).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.style.letter_spacing = px;
        self
    }
    /// Render italic.
    pub fn italic(mut self) -> Self {
        self.style.italic = true;
        self
    }
    /// Draw an underline.
    pub fn underline(mut self) -> Self {
        self.style.underline = true;
        self
    }
    /// Draw a strike-through line.
    pub fn strikethrough(mut self) -> Self {
        self.style.strikethrough = true;
        self
    }
    /// Select a font family by name (system fallback if unavailable).
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.style.font_family = Some(family.into());
        self
    }
    /// Clamp to at most `n` lines (excess dropped).
    pub fn max_lines(mut self, n: u32) -> Self {
        self.style.max_lines = Some(n);
        self
    }
    /// With `max_lines`, append "…" to the last line when the text overflows.
    pub fn ellipsis(mut self) -> Self {
        self.style.ellipsis = true;
        self
    }
    /// Disable line wrapping: the text shapes as a single unbounded line that clips
    /// to its box (combine with [`Self::ellipsis`] for a one-line "…" label).
    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.style.soft_wrap = wrap;
        self
    }

    /// Style this text from an explicit [`ParagraphStyle`].
    pub fn paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.style = style;
        self
    }

    /// Apply a general [`Style`](crate::Style): its text properties (color, font
    /// size/weight, line height) style the text, and its box properties (padding,
    /// background, …) wrap it.
    pub fn style(mut self, s: crate::style::Style) -> AnyWidget {
        if let Some(c) = s.color {
            self.style.color = c;
        }
        if let Some(fs) = s.font_size {
            self.style.font_size = fs;
        }
        if let Some(w) = s.font_weight {
            self.style.weight = w;
        }
        if let Some(lh) = s.line_height {
            self.style.line_height = lh;
        }
        if let Some(a) = s.text_align {
            self.style.align = a;
        }
        if let Some(ls) = s.letter_spacing {
            self.style.letter_spacing = ls;
        }
        if let Some(i) = s.italic {
            self.style.italic = i;
        }
        if let Some(u) = s.underline {
            self.style.underline = u;
        }
        if let Some(st) = s.strikethrough {
            self.style.strikethrough = st;
        }
        if let Some(f) = &s.font_family {
            self.style.font_family = Some(f.clone());
        }
        if let Some(m) = s.max_lines {
            self.style.max_lines = Some(m);
        }
        if let Some(e) = s.ellipsis {
            self.style.ellipsis = e;
        }
        if let Some(sw) = s.soft_wrap {
            self.style.soft_wrap = sw;
        }
        crate::style::styled(self, s)
    }
}

pebbles_core::render_widget!(Text);

impl RenderWidget for Text {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderParagraph::new(self.data.clone(), self.style.clone()))
    }

    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderParagraph>() {
            p.text = self.data.clone();
            p.style = self.style.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Rich text — one paragraph, many styled ranges (Flutter's TextSpan vocabulary)
// ---------------------------------------------------------------------------

/// One styled run of a rich paragraph. Build with [`span`], chain the style
/// setters, and hand a `Vec<TextSpan>` to [`text_rich`]. The whole paragraph
/// shapes as ONE layout with per-range styles — word-wrap, spacing, and BiDi are
/// the text engine's job, never a widget-per-word composition.
#[derive(Clone)]
pub struct TextSpan {
    pub text: String,
    pub weight: Option<f32>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Option<Color>,
    pub family: Option<String>,
    pub size: Option<f32>,
    /// Rounded background behind the run (inline code chips).
    pub chip: Option<Color>,
    /// Navigation target — taps resolve through [`RichText::on_link`].
    pub link: Option<String>,
}

/// A plain [`TextSpan`] over `text` — chain setters to style it.
pub fn span(text: impl Into<String>) -> TextSpan {
    TextSpan {
        text: text.into(),
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

impl TextSpan {
    pub fn weight(mut self, w: f32) -> Self {
        self.weight = Some(w);
        self
    }
    /// Semibold (600).
    pub fn semibold(mut self) -> Self {
        self.weight = Some(600.0);
        self
    }
    /// Bold (700).
    pub fn bold(mut self) -> Self {
        self.weight = Some(700.0);
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }
    /// Absolute font-size override (px).
    pub fn size(mut self, px: f32) -> Self {
        self.size = Some(px);
        self
    }
    /// Rounded background chip behind the run (inline code).
    pub fn chip(mut self, bg: Color) -> Self {
        self.chip = Some(bg);
        self
    }
    /// Mark the run as a link to `url` (style it explicitly — links don't
    /// auto-underline).
    pub fn link(mut self, url: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self
    }
}

/// A rich paragraph: many styled ranges, one shaped layout. Links resolve by
/// GEOMETRY — the render paragraph publishes each link range's laid-out boxes and
/// the tap handler hit-tests them — so a link that wraps across lines is exactly
/// as clickable as its glyphs, with no per-word widgets.
pub struct RichText {
    base: ParagraphStyle,
    spans: Vec<TextSpan>,
    on_link: Option<std::rc::Rc<dyn Fn(&str)>>,
}

/// Build a rich paragraph from spans. Base style via the setters.
pub fn text_rich(spans: Vec<TextSpan>) -> RichText {
    RichText { base: ParagraphStyle::default(), spans, on_link: None }
}

impl RichText {
    /// Base font size (spans may override per-range).
    pub fn size(mut self, px: f32) -> Self {
        self.base.font_size = px;
        self
    }
    /// Base text color.
    pub fn color(mut self, color: Color) -> Self {
        self.base.color = color;
        self
    }
    pub fn line_height(mut self, factor: f32) -> Self {
        self.base.line_height = factor;
        self
    }
    pub fn align(mut self, align: TextAlign) -> Self {
        self.base.align = align;
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.base.font_family = Some(family.into());
        self
    }
    /// Base weight (spans may override).
    pub fn weight(mut self, w: f32) -> Self {
        self.base.weight = w;
        self
    }
    /// Style the whole paragraph from an explicit [`ParagraphStyle`].
    pub fn paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.base = style;
        self
    }
    /// Disable soft wrapping: lines break only at `\n` (code blocks).
    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.base.soft_wrap = wrap;
        self
    }
    /// Called with the URL when any link span is tapped.
    pub fn on_link(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_link = Some(std::rc::Rc::new(f));
        self
    }
}

/// The leaf render widget behind [`RichText`] (post-resolution: byte ranges,
/// URL indices, and the shared link-box cell are already computed).
#[derive(Clone)]
struct RichTextLeaf {
    text: String,
    style: ParagraphStyle,
    spans: Vec<pebbles_render::TextSpanStyle>,
    boxes: Option<std::rc::Rc<std::cell::RefCell<Vec<(pebbles_foundation::Rect, usize)>>>>,
}

pebbles_core::render_widget!(RichTextLeaf);

impl RenderWidget for RichTextLeaf {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut p = RenderParagraph::with_spans(self.text.clone(), self.style.clone(), self.spans.clone());
        p.link_boxes = self.boxes.clone();
        Box::new(p)
    }

    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderParagraph>() {
            p.text = self.text.clone();
            p.style = self.style.clone();
            p.spans = self.spans.clone();
            p.link_boxes = self.boxes.clone();
        }
    }
}

impl pebbles_core::IntoWidget for RichText {
    fn into_widget(self) -> AnyWidget {
        // Resolve spans → concatenated text + byte-ranged style overrides.
        let mut text = String::new();
        let mut rspans: Vec<pebbles_render::TextSpanStyle> = Vec::with_capacity(self.spans.len());
        let mut urls: Vec<String> = Vec::new();
        for s in &self.spans {
            let start = text.len();
            text.push_str(&s.text);
            let mut rs = pebbles_render::TextSpanStyle::new(start..text.len());
            rs.weight = s.weight;
            rs.italic = s.italic;
            rs.underline = s.underline;
            rs.strikethrough = s.strikethrough;
            rs.color = s.color;
            rs.family = s.family.clone();
            rs.size = s.size;
            rs.chip = s.chip;
            if let Some(url) = &s.link {
                rs.link = Some(urls.len());
                urls.push(url.clone());
            }
            rspans.push(rs);
        }
        let boxes = (self.on_link.is_some() && !urls.is_empty())
            .then(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
        let leaf = RichTextLeaf { text, style: self.base, spans: rspans, boxes: boxes.clone() };
        match (self.on_link, boxes) {
            (Some(f), Some(boxes)) => {
                // The tap handler resolves the LOCAL hit point against the link
                // boxes the paragraph published at layout. Cursor stays Default:
                // only the link glyphs are interactive, not the whole paragraph.
                crate::widgets::GestureDetector::new(leaf)
                    .on_tap(pebbles_core::action_event(move |e| {
                        let hit = boxes
                            .borrow()
                            .iter()
                            .find(|(r, _)| r.contains(e.position.to_point()))
                            .map(|&(_, ix)| ix);
                        if let Some(ix) = hit
                            && let Some(url) = urls.get(ix)
                        {
                            f(url);
                        }
                    }))
                    .cursor(pebbles_render::Cursor::Default)
                    .into_widget()
            }
            _ => leaf.into_widget(),
        }
    }
}
