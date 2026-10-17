//! GPU resources: arenas, the Globals uniform, bind-group layouts, the glyph atlas and
//! the image cache.

pub(crate) mod bind_groups;
pub(crate) mod buffers;
pub(crate) mod globals;
pub(crate) mod glyph_atlas;

pub(crate) use glyph_atlas::{ATLAS_SIZE, Atlas, AtlasRegion};
