//! Render pipeline builders (one file per pipeline kind).

pub(crate) mod glyph;
pub(crate) mod image;
pub(crate) mod shadow;
pub(crate) mod solid;

pub(crate) use glyph::{GlyphPipeline, make_glyph_pipeline};
pub(crate) use image::{ImagePipeline, make_image_pipeline};
pub(crate) use shadow::{ShadowPipeline, make_shadow_pipeline};
pub(crate) use solid::make_pipeline;
