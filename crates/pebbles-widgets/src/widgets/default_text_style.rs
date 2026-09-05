//! [`default_text_style`] — Flutter's `DefaultTextStyle` (and its animated sibling):
//! set an ambient text style for a subtree. Descendant [`Text`](crate::Text) widgets
//! inherit each property they didn't set explicitly.
//!
//! It's a component that provides an `InheritedTextStyle` context; because the
//! provider's guard spans its subtree's reconciliation, a descendant `Text` reads it
//! in `create_render_object`/`update_render_object` and merges (its own set properties
//! win). Nested `default_text_style`s compose — each starts from the ancestor's style
//! and overlays only what it sets.

use pebbles_foundation::{Color, TextAlign};
use pebbles_render::ParagraphStyle;

use crate::widgets::text::{InheritedTextStyle, TextFields, overlay_fields};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, component_props, consume_context, provide_context};

/// An ambient text style for a subtree. Build with [`default_text_style`] or
/// [`animated_default_text_style`].
#[derive(Clone)]
pub struct DefaultTextStyle {
    child: AnyWidget,
    style: ParagraphStyle,
    set: TextFields,
    /// `Some(secs)` = animate numeric properties + color toward the target.
    animate: Option<f64>,
}

/// Provide an ambient text style to `child`'s subtree (Flutter's `DefaultTextStyle`).
pub fn default_text_style(child: impl IntoWidget) -> DefaultTextStyle {
    DefaultTextStyle {
        child: child.into_widget(),
        style: ParagraphStyle::default(),
        set: TextFields::default(),
        animate: None,
    }
}

/// Like [`default_text_style`], but transitions to a new style over `.duration(..)`
/// (Flutter's `AnimatedDefaultTextStyle`). Font size, line height, weight, letter
/// spacing and color are eased; discrete properties (align, italic, family, …) snap.
pub fn animated_default_text_style(child: impl IntoWidget) -> DefaultTextStyle {
    DefaultTextStyle { animate: Some(0.2), ..default_text_style(child) }
}

impl DefaultTextStyle {
    /// The transition duration in seconds (only meaningful for
    /// [`animated_default_text_style`]).
    pub fn duration(mut self, secs: f64) -> Self {
        self.animate = Some(secs.max(0.0));
        self
    }
    /// Animate the style transition over 200ms (turns a plain
    /// [`default_text_style`] into an animated one).
    pub fn animate(mut self) -> Self {
        if self.animate.is_none() {
            self.animate = Some(0.2);
        }
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self.set.mark(TextFields::FONT_SIZE);
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.style.color = color;
        self.set.mark(TextFields::COLOR);
        self
    }
    pub fn line_height(mut self, factor: f32) -> Self {
        self.style.line_height = factor;
        self.set.mark(TextFields::LINE_HEIGHT);
        self
    }
    pub fn weight(mut self, weight: f32) -> Self {
        self.style.weight = weight;
        self.set.mark(TextFields::WEIGHT);
        self
    }
    pub fn semibold(self) -> Self {
        self.weight(600.0)
    }
    pub fn bold(self) -> Self {
        self.weight(700.0)
    }
    pub fn align(mut self, align: TextAlign) -> Self {
        self.style.align = align;
        self.set.mark(TextFields::ALIGN);
        self
    }
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.style.letter_spacing = px;
        self.set.mark(TextFields::LETTER_SPACING);
        self
    }
    pub fn italic(mut self) -> Self {
        self.style.italic = true;
        self.set.mark(TextFields::ITALIC);
        self
    }
    pub fn underline(mut self) -> Self {
        self.style.underline = true;
        self.set.mark(TextFields::UNDERLINE);
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.style.strikethrough = true;
        self.set.mark(TextFields::STRIKETHROUGH);
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.style.font_family = Some(family.into());
        self.set.mark(TextFields::FONT_FAMILY);
        self
    }
    pub fn max_lines(mut self, n: u32) -> Self {
        self.style.max_lines = Some(n);
        self.set.mark(TextFields::MAX_LINES);
        self
    }
    pub fn ellipsis(mut self) -> Self {
        self.style.ellipsis = true;
        self.set.mark(TextFields::ELLIPSIS);
        self
    }
    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.style.soft_wrap = wrap;
        self.set.mark(TextFields::SOFT_WRAP);
        self
    }
    /// Set the ambient style from an explicit [`ParagraphStyle`] (all properties).
    pub fn paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.style = style;
        self.set.mark(TextFields::ALL);
        self
    }
}

impl IntoWidget for DefaultTextStyle {
    fn into_widget(self) -> AnyWidget {
        component_props(render, self).into_widget()
    }
}

/// Ease the eased-numeric properties + color of `target` toward it over `secs`
/// (discrete properties snap). Uses position-stable `animated` calls.
fn ease_toward(target: &ParagraphStyle, secs: f64) -> ParagraphStyle {
    let mut s = target.clone();
    s.font_size = animated(target.font_size as f64, secs) as f32;
    s.line_height = animated(target.line_height as f64, secs) as f32;
    s.weight = animated(target.weight as f64, secs) as f32;
    s.letter_spacing = animated(target.letter_spacing as f64, secs) as f32;
    let [r, g, b, a] = target.color.components;
    s.color = Color::new([
        animated(r as f64, secs) as f32,
        animated(g as f64, secs) as f32,
        animated(b as f64, secs) as f32,
        animated(a as f64, secs) as f32,
    ]);
    s
}

fn render(p: &DefaultTextStyle) -> AnyWidget {
    // Start from the ancestor's ambient style (nesting composes), overlay this
    // provider's set properties → the target.
    let mut target = consume_context::<InheritedTextStyle>().map(|s| s.0).unwrap_or_default();
    overlay_fields(&mut target, &p.style, p.set);

    let provided = match p.animate {
        None => target,
        Some(secs) => ease_toward(&target, secs),
    };
    provide_context(InheritedTextStyle(provided));
    p.child.clone()
}
