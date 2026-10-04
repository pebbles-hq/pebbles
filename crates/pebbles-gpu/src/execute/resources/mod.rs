//! GPU resources (atlas, image cache) — at this card just the glyph atlas.

pub(crate) mod glyph_atlas;

pub(crate) use glyph_atlas::{ATLAS_SIZE, Atlas, AtlasRegion};
