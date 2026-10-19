//! The display list's vocabulary: shape primitives, glyphs and the [`Op`] enum.
//!
//! **Renderer contract, not app API.** [`Op`] is produced only by
//! [`Painter`](super::Painter) and consumed by `pebbles-gpu` and (until Phase 7) the
//! hybrid flush. New variants and new fields are added in minor releases — always
//! match with `..` and a wildcard arm.

use std::borrow::Cow;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Circle, Rect, RoundedRect, Shape, Stroke};
use peniko::{BlendMode, Brush, Color, Fill, FontData, ImageBrush};

use super::TOL;

/// A shaped glyph: glyph id + pen position (baseline origin) in the run's local space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// A font variation coordinate in F2Dot14 (same as `swash::NormalizedCoord` and skrifa's).
pub type NormalizedCoord = i16;

/// The geometry of a fill, stroke or clip. Shape primitives are PRESERVED so renderers
/// can take analytic fast paths (kurbo emits a `CurveTo` per corner even for radius 0, so
/// a flattened path can no longer be recognised as a rect — DR-02).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Geometry {
    Rect(Rect),
    RoundedRect(RoundedRect),
    Circle(Circle),
    /// Any other shape, flattened once at record time with tolerance `TOL`.
    Path(BezPath),
}

impl Geometry {
    /// `as_rect` → `as_rounded_rect` → `as_circle` → `Path(shape.to_path(TOL))` (13 §10).
    pub fn from_shape(shape: &impl Shape) -> Self {
        if let Some(rect) = shape.as_rect() {
            Self::Rect(rect)
        } else if let Some(rounded) = shape.as_rounded_rect() {
            Self::RoundedRect(rounded)
        } else if let Some(circle) = shape.as_circle() {
            Self::Circle(circle)
        } else {
            Self::Path(shape.to_path(TOL))
        }
    }

    /// The geometry as a path (borrowed for `Path`, built for primitives).
    pub fn to_path(&self, tolerance: f64) -> Cow<'_, BezPath> {
        match self {
            Self::Rect(rect) => Cow::Owned(rect.to_path(tolerance)),
            Self::RoundedRect(rounded) => Cow::Owned(rounded.to_path(tolerance)),
            Self::Circle(circle) => Cow::Owned(circle.to_path(tolerance)),
            Self::Path(path) => Cow::Borrowed(path),
        }
    }

    /// Local-space bounds.
    pub fn bounding_box(&self) -> Rect {
        match self {
            Self::Rect(rect) => *rect,
            Self::RoundedRect(rounded) => rounded.rect(),
            Self::Circle(circle) => circle.bounding_box(),
            Self::Path(path) => path.bounding_box(),
        }
    }
}

/// One recorded paint verb.
///
/// **Renderer contract, not app API.** Produced only by [`Painter`](super::Painter);
/// consumed by `pebbles-gpu` and (until Phase 7) the hybrid flush. New variants and new
/// fields are added in minor releases — always match with `..` and a wildcard arm.
/// How the renderer may rasterize a path fill/stroke (02 §5.2, R16).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PathHint {
    /// The renderer picks the tier: small paths become cached coverage masks (Tier B).
    #[default]
    Auto,
    /// The path changes every frame (animated canvases): skip the CPU mask rasterizer.
    Volatile,
}

/// Whether an op's edges snap to the device pixel grid (02 §5.3, R18).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelSnap {
    /// Snap axis-aligned rect / rounded-rect edges, border widths and unit-scale image origins.
    #[default]
    Auto,
    /// Exact sub-pixel placement (animated content).
    Off,
}

/// How a blurred mask combines with the original coverage (Flutter's `BlurStyle`, 02 R42).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlurStyle {
    /// The blur.
    #[default]
    Normal,
    /// The shape solid, the blur outside it.
    Solid,
    /// Only the blur outside the shape.
    Outer,
    /// Only the blur inside the shape.
    Inner,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Op {
    #[non_exhaustive]
    Fill {
        style: Fill,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        shape: Geometry,
        hint: PathHint,
        snap: PixelSnap,
    },
    #[non_exhaustive]
    Stroke {
        style: Stroke,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        shape: Geometry,
        hint: PathHint,
        snap: PixelSnap,
    },
    /// Clip to `clip` (filled with `clip_fill`) and composite the layer with `blend` at
    /// `alpha`.
    #[non_exhaustive]
    PushLayer {
        clip_fill: Fill,
        blend: BlendMode,
        alpha: f32,
        transform: Affine,
        clip: Geometry,
    },
    PopLayer,
    #[non_exhaustive]
    Image {
        image: ImageBrush,
        transform: Affine,
        snap: PixelSnap,
    },
    /// A blurred coverage mask of any shape (02 R42, Flutter `MaskFilter.blur`): the shape's
    /// coverage blurred by `sigma` (local units) and combined per `blur_style`, painted `color`.
    #[non_exhaustive]
    BlurredFill {
        style: Fill,
        transform: Affine,
        color: Color,
        shape: Geometry,
        sigma: f32,
        blur_style: BlurStyle,
    },
    /// A glyph run's union blurred as one mask (02 R42; text shadows).
    #[non_exhaustive]
    BlurredGlyphs {
        font: FontData,
        font_size: f32,
        normalized_coords: Vec<NormalizedCoord>,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: Vec<Glyph>,
        color: Color,
        sigma: f32,
    },
    /// A dynamic image (02 R37): drawn like `Image`, uploaded in place by generation.
    #[non_exhaustive]
    DynamicImage {
        frame: crate::paint::DynamicImageFrame,
        quality: peniko::ImageQuality,
        alpha: f32,
        transform: Affine,
    },
    /// A nine-slice image (Flutter `drawImageNine`, 02 R36): `center` (image px) stretches to fill
    /// `dst`; the four corners keep their size (scaled down only when they do not fit).
    #[non_exhaustive]
    ImageNine {
        image: ImageBrush,
        center: Rect,
        dst: Rect,
        transform: Affine,
    },
    /// Analytic blurred rounded rect (shadow).
    #[non_exhaustive]
    BlurredRect {
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    },
    /// A shaped glyph run. `font` carries the bytes (`Blob` + face index); renderers
    /// rasterize from it directly (DR-07). Variable-font axes travel as
    /// `normalized_coords` (weight included).
    #[non_exhaustive]
    Glyphs {
        font: FontData,
        font_size: f32,
        normalized_coords: Vec<NormalizedCoord>,
        brush: Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        style: Fill,
        glyphs: Vec<Glyph>,
    },
    /// A retained fragment appended under `transform`; `ops` is SHARED with the source
    /// `Scene`.
    #[non_exhaustive]
    Fragment {
        transform: Option<Affine>,
        ops: Arc<Vec<Op>>,
    },
}
