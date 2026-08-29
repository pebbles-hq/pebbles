//! [`Style`] — a general, apply-anywhere style value, in the spirit of React
//! Native's `StyleSheet` / CSS rather than Flutter's widget-specific `ButtonStyle`.
//!
//! A `Style` is a cheap `Copy` bag of **optional** properties. Apply it to *any*
//! widget with [`StyleExt::styled`]; only the applicable properties take effect
//! (e.g. `font_size` on a plain box simply does nothing). Define styles wherever
//! you like — a module of functions is your "stylesheet":
//!
//! ```ignore
//! // styles.rs
//! pub fn card() -> Style {
//!     style().background(theme().colors.card).padding_all(16.0).radius_all(12.0)
//!         .border(Border::new(theme().colors.border, 1.0))
//! }
//! pub fn heading() -> Style { style().color(theme().colors.foreground).font_size(30.0).bold() }
//!
//! // anywhere.rs
//! column(children![...]).styled(styles::card())
//! text("Title").style(styles::heading())
//! ```
//!
//! Styles compose with [`Style::merge`] (later wins), like layering CSS classes.

use pebbles_foundation::{Alignment, Color, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow};

use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{Align, DecoratedBox, Opacity, Padding, SizedBox};

/// A general style value. Every field is optional; unset fields are inherited from
/// a merged base or simply not applied.
#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    // ---- box (apply to any widget) ----
    pub background: Option<Color>,
    pub padding: Option<EdgeInsets>,
    pub margin: Option<EdgeInsets>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub border: Option<Border>,
    pub radius: Option<BorderRadius>,
    pub shadow: Option<BoxShadow>,
    pub opacity: Option<f32>,
    pub align: Option<Alignment>,
    // ---- text (apply only to `Text`) ----
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub font_weight: Option<f32>,
    pub line_height: Option<f32>,
}

/// Start a new [`Style`].
pub fn style() -> Style {
    Style::default()
}

impl Style {
    pub fn new() -> Self {
        Self::default()
    }

    // ---- box setters ----
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn padding(mut self, insets: EdgeInsets) -> Self {
        self.padding = Some(insets);
        self
    }
    pub fn padding_all(self, value: f64) -> Self {
        self.padding(EdgeInsets::all(value))
    }
    pub fn padding_xy(self, horizontal: f64, vertical: f64) -> Self {
        self.padding(EdgeInsets::symmetric(horizontal, vertical))
    }
    pub fn margin(mut self, insets: EdgeInsets) -> Self {
        self.margin = Some(insets);
        self
    }
    pub fn margin_all(self, value: f64) -> Self {
        self.margin(EdgeInsets::all(value))
    }
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
    pub fn size(self, width: f64, height: f64) -> Self {
        self.width(width).height(height)
    }
    pub fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = Some(radius);
        self
    }
    pub fn radius_all(self, value: f64) -> Self {
        self.radius(BorderRadius::all(value))
    }
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = Some(opacity);
        self
    }
    pub fn align(mut self, alignment: Alignment) -> Self {
        self.align = Some(alignment);
        self
    }

    // ---- text setters ----
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn font_size(mut self, size: f32) -> Self {
        self.font_size = Some(size);
        self
    }
    pub fn font_weight(mut self, weight: f32) -> Self {
        self.font_weight = Some(weight);
        self
    }
    pub fn semibold(self) -> Self {
        self.font_weight(600.0)
    }
    pub fn bold(self) -> Self {
        self.font_weight(700.0)
    }
    pub fn line_height(mut self, factor: f32) -> Self {
        self.line_height = Some(factor);
        self
    }

    /// Layer `other` on top of `self` — `other`'s set fields win (like stacking CSS
    /// classes or `style={[base, override]}` in React Native).
    pub fn merge(self, other: Style) -> Style {
        Style {
            background: other.background.or(self.background),
            padding: other.padding.or(self.padding),
            margin: other.margin.or(self.margin),
            width: other.width.or(self.width),
            height: other.height.or(self.height),
            border: other.border.or(self.border),
            radius: other.radius.or(self.radius),
            shadow: other.shadow.or(self.shadow),
            opacity: other.opacity.or(self.opacity),
            align: other.align.or(self.align),
            color: other.color.or(self.color),
            font_size: other.font_size.or(self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            line_height: other.line_height.or(self.line_height),
        }
    }

    /// The [`BoxDecoration`] implied by this style's background/border/radius/shadow,
    /// or `None` if none are set.
    pub(crate) fn decoration(&self) -> Option<BoxDecoration> {
        if self.background.is_none()
            && self.border.is_none()
            && self.radius.is_none()
            && self.shadow.is_none()
        {
            return None;
        }
        let mut d = BoxDecoration::new();
        if let Some(c) = self.background {
            d = d.color(c);
        }
        if let Some(b) = self.border {
            d = d.border(b);
        }
        if let Some(r) = self.radius {
            d = d.radius(r);
        }
        if let Some(s) = self.shadow {
            d = d.shadow(s);
        }
        Some(d)
    }
}

/// Apply a [`Style`]'s **box** properties around any widget (text properties are
/// ignored — they only apply via `Text::style`).
pub fn styled(child: impl IntoWidget, s: Style) -> AnyWidget {
    let mut w = child.into_widget();
    if let Some(a) = s.align {
        w = Align::new(a, w).into_widget();
    }
    if let Some(p) = s.padding {
        w = Padding::new(p, w).into_widget();
    }
    if let Some(d) = s.decoration() {
        w = DecoratedBox::new(d, w).into_widget();
    }
    if s.width.is_some() || s.height.is_some() {
        w = SizedBox::new(s.width, s.height, Some(w)).into_widget();
    }
    if let Some(o) = s.opacity {
        w = Opacity::new(o, w).into_widget();
    }
    if let Some(m) = s.margin {
        w = Padding::new(m, w).into_widget();
    }
    w
}

/// `.styled(style)` on every widget.
pub trait StyleExt: IntoWidget + Sized {
    /// Apply a [`Style`]'s box properties around this widget.
    fn styled(self, style: Style) -> AnyWidget {
        styled(self, style)
    }
}
impl<W: IntoWidget> StyleExt for W {}
