//! [`BoxDecoration`] — the visual styling a box can paint: background color,
//! border, corner radius and drop shadows. This is what makes cards, buttons,
//! inputs and surfaces look like a design system rather than flat rectangles.

use pebbles_foundation::{Alignment, Color, Offset};
/// A ready-to-paint background image (peniko's image brush).
pub use vello::peniko::ImageBrush as Image;
pub use vello::peniko::Mix as BlendMode;

/// Build a paintable [`Image`] from raw, straight-alpha RGBA8 pixels
/// (`width * height * 4` bytes, row-major). Decoders (PNG/JPEG) go through this.
pub fn image_from_rgba8(width: u32, height: u32, rgba: Vec<u8>) -> Image {
    use vello::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
    let data = ImageData {
        data: Blob::from(rgba),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    };
    ImageBrush::new(data)
}

/// How a background [`Image`] is scaled to fill its box (like CSS `object-fit`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    /// Scale to cover the whole box, cropping overflow (the default).
    #[default]
    Cover,
    /// Scale to fit entirely inside the box (may letterbox).
    Contain,
    /// Stretch to exactly fill the box (may distort).
    Fill,
    /// No scaling — natural size, centered.
    None,
}

/// Per-corner radii, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: f64,
    pub top_right: f64,
    pub bottom_right: f64,
    pub bottom_left: f64,
}

impl BorderRadius {
    pub const ZERO: BorderRadius = BorderRadius::all(0.0);

    /// The same radius on all four corners.
    pub const fn all(r: f64) -> Self {
        BorderRadius { top_left: r, top_right: r, bottom_right: r, bottom_left: r }
    }

    /// The largest single radius (used where a uniform radius is required).
    pub fn max(&self) -> f64 {
        self.top_left.max(self.top_right).max(self.bottom_right).max(self.bottom_left)
    }

    pub(crate) fn to_radii(self) -> kurbo::RoundedRectRadii {
        kurbo::RoundedRectRadii::new(self.top_left, self.top_right, self.bottom_right, self.bottom_left)
    }
}

/// One edge of a [`Border`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSide {
    pub color: Color,
    pub width: f64,
}

impl BorderSide {
    /// An invisible side.
    pub const NONE: BorderSide = BorderSide { color: Color::from_rgba8(0, 0, 0, 0), width: 0.0 };
    pub const fn new(color: Color, width: f64) -> Self {
        BorderSide { color, width }
    }
}

/// A border around a box — four independently-styled [`BorderSide`]s. A uniform
/// border ([`Border::new`]) is stroked crisply along the (rounded) outline; a
/// non-uniform border strokes each edge as a straight inset line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub top: BorderSide,
    pub right: BorderSide,
    pub bottom: BorderSide,
    pub left: BorderSide,
}

impl Border {
    /// A uniform border on all four sides.
    pub fn new(color: Color, width: f64) -> Self {
        Border::all(BorderSide::new(color, width))
    }
    /// A uniform border from a single side spec.
    pub fn all(side: BorderSide) -> Self {
        Border { top: side, right: side, bottom: side, left: side }
    }
    /// Vertical (top/bottom) and horizontal (left/right) sides.
    pub fn symmetric(vertical: BorderSide, horizontal: BorderSide) -> Self {
        Border { top: vertical, bottom: vertical, left: horizontal, right: horizontal }
    }
    /// Specific sides only (the rest are [`BorderSide::NONE`]).
    pub fn only(top: BorderSide, right: BorderSide, bottom: BorderSide, left: BorderSide) -> Self {
        Border { top, right, bottom, left }
    }
    /// Whether all four sides are identical.
    pub fn is_uniform(&self) -> bool {
        self.top == self.right && self.right == self.bottom && self.bottom == self.left
    }
}

/// A soft drop shadow.
#[derive(Clone, Copy, Debug)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Offset,
    /// Gaussian blur radius.
    pub blur: f64,
    /// How far to grow (or shrink, if negative) the shadow rect before blurring.
    pub spread: f64,
}

impl BoxShadow {
    pub fn new(color: Color, offset: Offset, blur: f64, spread: f64) -> Self {
        BoxShadow { color, offset, blur, spread }
    }
}

/// The outline shape of a box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoxShape {
    /// A rectangle, honoring the corner radius.
    #[default]
    Rectangle,
    /// A circle inscribed in the box (radius is ignored).
    Circle,
}

/// A multi-stop color gradient fill. Endpoints are given as [`Alignment`]s
/// (`-1..1` within the box), like CSS/Flutter, so a gradient scales with any size.
#[derive(Clone, Debug)]
pub enum Gradient {
    /// A linear gradient from `begin` to `end` through evenly-spaced `colors`.
    Linear { begin: Alignment, end: Alignment, colors: Vec<Color> },
    /// A radial gradient from `center` out to `radius` (fraction of the box's
    /// shorter side) through evenly-spaced `colors`.
    Radial { center: Alignment, radius: f64, colors: Vec<Color> },
    /// A conic ("sweep") gradient rotating clockwise around `center` from
    /// `start_angle` to `end_angle` (radians, 0 = positive X axis) through
    /// evenly-spaced `colors`. A full `0..2π` sweep has no seam.
    Sweep { center: Alignment, start_angle: f64, end_angle: f64, colors: Vec<Color> },
}

impl Gradient {
    /// A linear gradient. `linear-gradient(begin → end, colors…)`.
    pub fn linear(begin: Alignment, end: Alignment, colors: impl IntoIterator<Item = Color>) -> Self {
        Gradient::Linear { begin, end, colors: colors.into_iter().collect() }
    }
    /// A top-to-bottom linear gradient.
    pub fn vertical(colors: impl IntoIterator<Item = Color>) -> Self {
        Gradient::linear(Alignment::TOP_CENTER, Alignment::BOTTOM_CENTER, colors)
    }
    /// A left-to-right linear gradient.
    pub fn horizontal(colors: impl IntoIterator<Item = Color>) -> Self {
        Gradient::linear(Alignment::CENTER_LEFT, Alignment::CENTER_RIGHT, colors)
    }
    /// A radial gradient centered in the box (`radius` = fraction of the shorter side).
    pub fn radial(radius: f64, colors: impl IntoIterator<Item = Color>) -> Self {
        Gradient::Radial { center: Alignment::CENTER, radius, colors: colors.into_iter().collect() }
    }
    /// A full-circle conic gradient centered in the box, starting at the positive
    /// X axis (12 o'clock is `start_angle` + `-π/2`; pass an offset to rotate).
    pub fn sweep(colors: impl IntoIterator<Item = Color>) -> Self {
        Gradient::Sweep {
            center: Alignment::CENTER,
            start_angle: 0.0,
            end_angle: std::f64::consts::TAU,
            colors: colors.into_iter().collect(),
        }
    }
    /// A conic gradient with an explicit arc. `start_angle`/`end_angle` are radians
    /// clockwise from the positive X axis; a full-circle sweep must close the loop
    /// with matching first/last colors to avoid a hard seam.
    pub fn sweep_arc(start_angle: f64, end_angle: f64, colors: impl IntoIterator<Item = Color>) -> Self {
        Gradient::Sweep {
            center: Alignment::CENTER,
            start_angle,
            end_angle,
            colors: colors.into_iter().collect(),
        }
    }
}

/// The complete visual description of a box's surface.
#[derive(Clone, Debug, Default)]
pub struct BoxDecoration {
    pub color: Option<Color>,
    pub gradient: Option<Gradient>,
    pub border: Option<Border>,
    pub radius: BorderRadius,
    pub shape: BoxShape,
    pub shadows: Vec<BoxShadow>,
    /// A background image, painted over the fill and clipped to the box.
    pub image: Option<Image>,
    pub image_fit: ImageFit,
    /// How the background blends with what's painted behind it.
    pub blend: Option<BlendMode>,
}

impl BoxDecoration {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn gradient(mut self, gradient: Gradient) -> Self {
        self.gradient = Some(gradient);
        self
    }
    pub fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }
    pub fn shape(mut self, shape: BoxShape) -> Self {
        self.shape = shape;
        self
    }
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }
    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = Some(blend);
        self
    }
    pub fn image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }
    pub fn image_fit(mut self, fit: ImageFit) -> Self {
        self.image_fit = fit;
        self
    }
}
