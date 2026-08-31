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
//! Styles compose with [`Style::merge`] (later wins), or [`styles`] for the RN
//! `style={[a, b, c]}` idiom. Layers are cheap to `clone`.
//!
//! # The application contract
//!
//! A `Style` reaches pixels three ways — each applies a disjoint slice:
//!
//! | Route | What applies | Where it lands |
//! |-------|--------------|----------------|
//! | `.styled(s)` on any widget | box props only | wraps AROUND the widget |
//! | `Text::style(s)` (or typography) | text props only | the glyphs |
//! | component `.style(s)` (Card/Alert/Badge/TextField/…) | box props (+ text where meaningful) | merged ONTO the component's base, user wins (`base.merge(user)`) |
//!
//! Precedence, low → high: component defaults → semantic builders (`.variant`/`.color`)
//! → component `.style(s)` → an outer `.styled(s)` wrapper. Interactive state colors
//! (hover/press) are derived AFTER, by the component, from the resolved base — `Style`
//! sets the static base, never per-state deltas.
//!
//! `styled()` wraps in this fixed order (inner → outer): `Align → Padding →
//! DecoratedBox → ConstrainedBox(min/max) → SizedBox(w/h) → AspectRatio → Transform →
//! Opacity → cursor(GestureDetector) → Margin`.
//!
//! # Deliberately OUT (do not add — CSS/layout separation)
//!
//! `overflow` (scrolling is a widget — `SingleChildScrollView`/`.scrollable()`) ·
//! `position`/`inset` (`Stack`/`Positioned`) · `gap` (flex `.spacing()` owns it) ·
//! per-state styles like `hover()` (state visuals live in components, rule 4) ·
//! `z_index` (paint order = tree order) · `text_transform` (do it in app code) · text
//! cascade/inheritance (a `Style`'s text props affect only the `Text` they're on) ·
//! `blur`/`backdrop` (no cheap vello primitive today). Ambient values come from
//! [`theme()`](crate::theme), never a style cascade.

use pebbles_foundation::{Alignment, Color, EdgeInsets, TextAlign};
use pebbles_render::{
    Affine, BlendMode, Border, BorderRadius, BorderSide, BoxConstraints, BoxDecoration, BoxShadow,
    BoxShape, Cursor, Gradient, Image, ImageFit, image_from_rgba8,
};

use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{
    Align, ConstrainedBox, DecoratedBox, GestureDetector, Opacity, Padding, SizedBox, aspect_ratio,
    transform,
};

/// A general style value (CSS-like). Every field is optional; unset fields are
/// inherited from a merged base or simply not applied. Not `Copy` — it can own a
/// gradient and a list of shadows — but cheap to `clone`.
#[derive(Clone, Debug, Default)]
pub struct Style {
    // ---- box (apply to any widget) ----
    pub background: Option<Color>,
    pub gradient: Option<Gradient>,
    pub padding: Option<EdgeInsets>,
    pub margin: Option<EdgeInsets>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub border: Option<Border>,
    pub radius: Option<BorderRadius>,
    pub shape: Option<BoxShape>,
    pub shadows: Vec<BoxShadow>,
    pub blend: Option<BlendMode>,
    pub image: Option<Image>,
    pub image_fit: Option<ImageFit>,
    pub opacity: Option<f32>,
    pub align: Option<Alignment>,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_width: Option<f64>,
    pub max_height: Option<f64>,
    pub aspect_ratio: Option<f64>,
    /// Paint + hit-test transform (rotate/scale/translate); layout is untouched.
    pub transform: Option<Affine>,
    /// The mouse cursor shown over this box.
    pub cursor: Option<Cursor>,
    // ---- text (apply only to `Text`) ----
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub font_weight: Option<f32>,
    pub line_height: Option<f32>,
    pub text_align: Option<TextAlign>,
    pub letter_spacing: Option<f32>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub font_family: Option<String>,
    pub max_lines: Option<u32>,
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
    /// A gradient background (overrides `background` when painted).
    pub fn gradient(mut self, gradient: Gradient) -> Self {
        self.gradient = Some(gradient);
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
    /// The box outline shape (rectangle or circle).
    pub fn shape(mut self, shape: BoxShape) -> Self {
        self.shape = Some(shape);
        self
    }
    /// Shorthand for a circular box.
    pub fn circle(self) -> Self {
        self.shape(BoxShape::Circle)
    }
    /// Append a drop shadow. Call repeatedly to stack several (CSS `box-shadow`).
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }
    /// How the background blends with what's painted behind it (CSS `mix-blend-mode`).
    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = Some(blend);
        self
    }
    /// A background image (decode with [`image_from_path`] / [`image_from_bytes`]).
    pub fn image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }
    /// How the background image scales to the box (default: `Cover`).
    pub fn image_fit(mut self, fit: ImageFit) -> Self {
        self.image_fit = Some(fit);
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

    // ---- size constraints ----
    pub fn min_width(mut self, v: f64) -> Self {
        self.min_width = Some(v);
        self
    }
    pub fn min_height(mut self, v: f64) -> Self {
        self.min_height = Some(v);
        self
    }
    pub fn max_width(mut self, v: f64) -> Self {
        self.max_width = Some(v);
        self
    }
    pub fn max_height(mut self, v: f64) -> Self {
        self.max_height = Some(v);
        self
    }
    /// Constrain the box to a width:height ratio (wraps in `AspectRatio`).
    pub fn aspect_ratio(mut self, ratio: f64) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }

    // ---- per-side borders (compose onto `border`) ----
    fn border_side(mut self, set: impl FnOnce(&mut Border)) -> Self {
        let mut b = self.border.unwrap_or(Border::all(BorderSide::NONE));
        set(&mut b);
        self.border = Some(b);
        self
    }
    pub fn border_top(self, side: BorderSide) -> Self {
        self.border_side(|b| b.top = side)
    }
    pub fn border_right(self, side: BorderSide) -> Self {
        self.border_side(|b| b.right = side)
    }
    pub fn border_bottom(self, side: BorderSide) -> Self {
        self.border_side(|b| b.bottom = side)
    }
    pub fn border_left(self, side: BorderSide) -> Self {
        self.border_side(|b| b.left = side)
    }
    /// Left + right sides.
    pub fn border_x(self, side: BorderSide) -> Self {
        self.border_side(|b| {
            b.left = side;
            b.right = side;
        })
    }
    /// Top + bottom sides.
    pub fn border_y(self, side: BorderSide) -> Self {
        self.border_side(|b| {
            b.top = side;
            b.bottom = side;
        })
    }

    // ---- transform (composes onto any existing transform) ----
    fn compose(mut self, m: Affine) -> Self {
        self.transform = Some(self.transform.map_or(m, |cur| cur * m));
        self
    }
    /// Rotate around the box origin (radians).
    pub fn rotate(self, radians: f64) -> Self {
        self.compose(Affine::rotate(radians))
    }
    /// Uniform scale.
    pub fn scale(self, factor: f64) -> Self {
        self.compose(Affine::scale(factor))
    }
    /// Non-uniform scale.
    pub fn scale_xy(self, sx: f64, sy: f64) -> Self {
        self.compose(Affine::scale_non_uniform(sx, sy))
    }
    /// Translate by `(x, y)` logical px.
    pub fn translate(self, x: f64, y: f64) -> Self {
        self.compose(Affine::translate((x, y)))
    }
    /// Apply an explicit affine transform.
    pub fn transform(self, matrix: Affine) -> Self {
        self.compose(matrix)
    }

    /// The mouse cursor shown while hovering this box.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
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
    /// Horizontal text alignment (applies to `Text`).
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.text_align = Some(align);
        self
    }
    /// Extra spacing between letters (logical px).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.letter_spacing = Some(px);
        self
    }
    pub fn italic(mut self, italic: bool) -> Self {
        self.italic = Some(italic);
        self
    }
    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = Some(underline);
        self
    }
    pub fn strikethrough(mut self, strikethrough: bool) -> Self {
        self.strikethrough = Some(strikethrough);
        self
    }
    /// A font family name (system fallback if unavailable).
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }
    /// Clamp `Text` to at most `n` lines (excess dropped).
    pub fn max_lines(mut self, n: u32) -> Self {
        self.max_lines = Some(n);
        self
    }

    /// Layer `other` on top of `self` — `other`'s set fields win (like stacking CSS
    /// classes or `style={[base, override]}` in React Native).
    pub fn merge(self, other: Style) -> Style {
        Style {
            background: other.background.or(self.background),
            gradient: other.gradient.or(self.gradient),
            padding: other.padding.or(self.padding),
            margin: other.margin.or(self.margin),
            width: other.width.or(self.width),
            height: other.height.or(self.height),
            border: other.border.or(self.border),
            radius: other.radius.or(self.radius),
            shape: other.shape.or(self.shape),
            // A non-empty shadow list wins wholesale (like a CSS `box-shadow` override).
            shadows: if other.shadows.is_empty() { self.shadows } else { other.shadows },
            blend: other.blend.or(self.blend),
            image: other.image.or(self.image),
            image_fit: other.image_fit.or(self.image_fit),
            opacity: other.opacity.or(self.opacity),
            align: other.align.or(self.align),
            min_width: other.min_width.or(self.min_width),
            min_height: other.min_height.or(self.min_height),
            max_width: other.max_width.or(self.max_width),
            max_height: other.max_height.or(self.max_height),
            aspect_ratio: other.aspect_ratio.or(self.aspect_ratio),
            transform: other.transform.or(self.transform),
            cursor: other.cursor.or(self.cursor),
            color: other.color.or(self.color),
            font_size: other.font_size.or(self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            line_height: other.line_height.or(self.line_height),
            text_align: other.text_align.or(self.text_align),
            letter_spacing: other.letter_spacing.or(self.letter_spacing),
            italic: other.italic.or(self.italic),
            underline: other.underline.or(self.underline),
            strikethrough: other.strikethrough.or(self.strikethrough),
            font_family: other.font_family.or(self.font_family),
            max_lines: other.max_lines.or(self.max_lines),
        }
    }

    /// The [`BoxDecoration`] implied by this style's background/border/radius/shadow,
    /// or `None` if none are set.
    pub(crate) fn decoration(&self) -> Option<BoxDecoration> {
        if self.background.is_none()
            && self.gradient.is_none()
            && self.border.is_none()
            && self.radius.is_none()
            && self.shape.is_none()
            && self.shadows.is_empty()
            && self.blend.is_none()
            && self.image.is_none()
        {
            return None;
        }
        let mut d = BoxDecoration::new();
        if let Some(c) = self.background {
            d = d.color(c);
        }
        if let Some(g) = &self.gradient {
            d = d.gradient(g.clone());
        }
        if let Some(b) = self.border {
            d = d.border(b);
        }
        if let Some(r) = self.radius {
            d = d.radius(r);
        }
        if let Some(s) = self.shape {
            d = d.shape(s);
        }
        for shadow in &self.shadows {
            d = d.shadow(*shadow);
        }
        if let Some(b) = self.blend {
            d = d.blend(b);
        }
        if let Some(img) = &self.image {
            d = d.image(img.clone());
            if let Some(fit) = self.image_fit {
                d = d.image_fit(fit);
            }
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
    // Min/max constraints slot between decoration and the fixed size.
    if s.min_width.is_some() || s.min_height.is_some() || s.max_width.is_some() || s.max_height.is_some()
    {
        let c = BoxConstraints {
            min_width: s.min_width.unwrap_or(0.0),
            min_height: s.min_height.unwrap_or(0.0),
            max_width: s.max_width.unwrap_or(f64::INFINITY),
            max_height: s.max_height.unwrap_or(f64::INFINITY),
        };
        w = ConstrainedBox::new(c, w).into_widget();
    }
    if s.width.is_some() || s.height.is_some() {
        w = SizedBox::new(s.width, s.height, Some(w)).into_widget();
    }
    if let Some(r) = s.aspect_ratio {
        w = aspect_ratio(r, w).into_widget();
    }
    if let Some(t) = s.transform {
        w = transform(t, w).into_widget();
    }
    if let Some(o) = s.opacity {
        w = Opacity::new(o, w).into_widget();
    }
    if let Some(cur) = s.cursor {
        w = GestureDetector::new(w).cursor(cur).into_widget();
    }
    if let Some(m) = s.margin {
        w = Padding::new(m, w).into_widget();
    }
    w
}

/// Compose several styles left-to-right (later layers win) — the RN `style={[a, b, c]}`
/// idiom. Equivalent to `a.merge(b).merge(c)`.
pub fn styles<I>(layers: I) -> Style
where
    I: IntoIterator<Item = Style>,
{
    layers.into_iter().fold(Style::default(), Style::merge)
}

/// `.styled(style)` on every widget.
pub trait StyleExt: IntoWidget + Sized {
    /// Apply a [`Style`]'s box properties around this widget.
    fn styled(self, style: Style) -> AnyWidget {
        styled(self, style)
    }
}
impl<W: IntoWidget> StyleExt for W {}

// ---------------------------------------------------------------------------
// Image decoding — PNG / JPEG bytes → a paintable `Image`.
// ---------------------------------------------------------------------------

/// Decode PNG/JPEG bytes into a paintable [`Image`] for `style().image(..)`.
/// Returns `None` if the data can't be decoded.
pub fn image_from_bytes(bytes: &[u8]) -> Option<Image> {
    let rgba = ::image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(image_from_rgba8(w, h, rgba.into_raw()))
}

/// Read and decode an image file into a paintable [`Image`].
pub fn image_from_path(path: impl AsRef<std::path::Path>) -> Option<Image> {
    image_from_bytes(&std::fs::read(path).ok()?)
}
