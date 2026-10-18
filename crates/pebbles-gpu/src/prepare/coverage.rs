//! The op-coverage matrix (02 §2.2) as code: one `Feature` per OC row, its policy, the rows
//! delivered so far, and the per-plan set of rows that fell back (R01). Logging stays C17's
//! (`WarnOnce`, keyed by `Unsupported`); `FeatureSet` only records which rows fell back.

/// One row of the op-coverage matrix, named after the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
#[allow(dead_code)] // rows whose ops do not exist yet are named for the matrix, never produced
pub(crate) enum Feature {
    FillSolid,
    FillEvenOdd,
    FillGradientLinear,
    FillGradientRadial,
    FillGradientSweep,
    GradientStopsExtend,
    GradientNonSrgbInterpolation,
    GradientBrushTransform,
    FillImageBrush,
    ImageBrushTransform,
    StrokeSolid,
    StrokeDashes,
    StrokeJoinsCaps,
    StrokeHairline,
    StrokeGradient,
    StrokeImageBrush,
    ClipRect,
    ClipRectQuarterTurn,
    ClipRectRotated,
    ClipRRectZeroRadius,
    ClipRRect,
    ClipCircle,
    ClipEllipse,
    ClipPath,
    ClipEvenOdd,
    ClipNested,
    LayerAlpha,
    LayerMix,
    LayerCompose,
    LayerMixAndCompose,
    LayerUnbalanced,
    ImageRgba8,
    ImageBgra8,
    ImagePremultiplied,
    ImageQuality,
    ImageExtend,
    ImageAlpha,
    ImageTransform,
    ImageOversize,
    ImageMalformed,
    ImageMipmaps,
    ShadowAxisAligned,
    ShadowRotated,
    ShadowDegenerate,
    GlyphsSolid,
    GlyphsVariable,
    GlyphsTransform,
    GlyphsRotated,
    GlyphsLarge,
    GlyphsColor,
    GlyphsColorRotated,
    GlyphsNonSolidBrush,
    GlyphsEvenOdd,
    GlyphsDegenerate,
    FragmentAppend,
    NonFinite,
    Offscreen,
    ImageNine,
    DynamicImage,
    BlurredFill,
    BlurredGlyphs,
    Vertices,
    SpriteAtlas,
    EffectBlur,
    BackdropFilter,
    EffectColorMatrix,
    EffectMorphology,
    ProjectedLayer,
    ShaderBrush,
    GradientDither,
    ClipGpuMask,
}

use Feature as F;

/// Every row, in OC order (OC-01 … OC-71).
#[cfg_attr(not(test), allow(dead_code))] // read by the R01 tests and the Phase 5 gate
pub(crate) const ALL: [Feature; 71] = [
    F::FillSolid,
    F::FillEvenOdd,
    F::FillGradientLinear,
    F::FillGradientRadial,
    F::FillGradientSweep,
    F::GradientStopsExtend,
    F::GradientNonSrgbInterpolation,
    F::GradientBrushTransform,
    F::FillImageBrush,
    F::ImageBrushTransform,
    F::StrokeSolid,
    F::StrokeDashes,
    F::StrokeJoinsCaps,
    F::StrokeHairline,
    F::StrokeGradient,
    F::StrokeImageBrush,
    F::ClipRect,
    F::ClipRectQuarterTurn,
    F::ClipRectRotated,
    F::ClipRRectZeroRadius,
    F::ClipRRect,
    F::ClipCircle,
    F::ClipEllipse,
    F::ClipPath,
    F::ClipEvenOdd,
    F::ClipNested,
    F::LayerAlpha,
    F::LayerMix,
    F::LayerCompose,
    F::LayerMixAndCompose,
    F::LayerUnbalanced,
    F::ImageRgba8,
    F::ImageBgra8,
    F::ImagePremultiplied,
    F::ImageQuality,
    F::ImageExtend,
    F::ImageAlpha,
    F::ImageTransform,
    F::ImageOversize,
    F::ImageMalformed,
    F::ImageMipmaps,
    F::ShadowAxisAligned,
    F::ShadowRotated,
    F::ShadowDegenerate,
    F::GlyphsSolid,
    F::GlyphsVariable,
    F::GlyphsTransform,
    F::GlyphsRotated,
    F::GlyphsLarge,
    F::GlyphsColor,
    F::GlyphsColorRotated,
    F::GlyphsNonSolidBrush,
    F::GlyphsEvenOdd,
    F::GlyphsDegenerate,
    F::FragmentAppend,
    F::NonFinite,
    F::Offscreen,
    F::ImageNine,
    F::DynamicImage,
    F::BlurredFill,
    F::BlurredGlyphs,
    F::Vertices,
    F::SpriteAtlas,
    F::EffectBlur,
    F::BackdropFilter,
    F::EffectColorMatrix,
    F::EffectMorphology,
    F::ProjectedLayer,
    F::ShaderBrush,
    F::GradientDither,
    F::ClipGpuMask,
];
const _: () = assert!(ALL.len() <= 128);

/// Whether a row must be delivered by the Phase 5 gate or may stay a documented fallback.
#[cfg_attr(not(test), allow(dead_code))] // read by the R01 tests and the Phase 5 gate
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Policy {
    Must,
    May,
}

/// `May` for exactly the §2.2 MAY rows (OC-07, OC-09, OC-10, OC-16, OC-30, OC-53).
#[cfg_attr(not(test), allow(dead_code))] // read by the R01 tests and the Phase 5 gate
pub(crate) const fn policy(f: Feature) -> Policy {
    match f {
        F::GradientNonSrgbInterpolation
        | F::FillImageBrush
        | F::ImageBrushTransform
        | F::StrokeImageBrush
        | F::LayerMixAndCompose
        | F::GlyphsEvenOdd => Policy::May,
        _ => Policy::Must,
    }
}

/// A set of rows (one bit per `Feature`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FeatureSet(u128);

impl FeatureSet {
    pub(crate) fn insert(&mut self, f: Feature) {
        self.0 |= 1u128 << f as u8;
    }

    #[cfg_attr(not(test), allow(dead_code))] // read by the R01 tests and the Phase 5 gate
    pub(crate) fn contains(&self, f: Feature) -> bool {
        self.0 & (1u128 << f as u8) != 0
    }

    #[allow(dead_code)] // read by tests and the Phase 5 report
    pub(crate) fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

/// Rows whose delivering card is `[x]`. Each card appends its rows in the PR that ticks it.
#[cfg_attr(not(test), allow(dead_code))] // read by the R01 tests and the Phase 5 gate
pub(crate) const DELIVERED: &[Feature] = &[
    F::FillSolid,
    F::FillEvenOdd, // R03
    F::StrokeSolid,
    F::StrokeDashes,    // R06
    F::StrokeJoinsCaps, // R06
    F::StrokeHairline,
    F::ClipRect,
    F::ClipNested,
    F::ClipRectQuarterTurn, // R07
    F::ClipRRectZeroRadius, // R07
    F::LayerUnbalanced,
    F::FragmentAppend,
    F::NonFinite, // R02
    F::Offscreen, // R04
    F::ImageRgba8,
    F::ImageBgra8,
    F::ImagePremultiplied,
    F::ImageOversize,
    F::GlyphsSolid,
    F::GlyphsVariable,
    F::GlyphsTransform, // R20
    F::GlyphsRotated,   // R20
    F::GlyphsLarge,     // R20
];

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
