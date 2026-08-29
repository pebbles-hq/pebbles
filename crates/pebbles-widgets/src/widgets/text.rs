//! [`Text`] — a leaf widget that shapes and paints a string. Backs
//! [`pebbles_render::RenderParagraph`].

use pebbles_foundation::Color;
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
