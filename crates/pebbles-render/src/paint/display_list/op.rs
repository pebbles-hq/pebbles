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
    },
    #[non_exhaustive]
    Stroke {
        style: Stroke,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        shape: Geometry,
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
    /// `normalized_coords`; `font_weight` is a transitional field removed by C04.
    #[non_exhaustive]
    Glyphs {
        font: FontData,
        font_size: f32,
        font_weight: u16,
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
