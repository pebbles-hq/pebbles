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
