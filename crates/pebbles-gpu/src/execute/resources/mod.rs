//! GPU resources: arenas, the Globals uniform, bind-group layouts, the glyph atlas and
//! the image cache.

pub(crate) mod atlas_alloc;
pub(crate) mod bind_groups;
pub(crate) mod buffers;
pub(crate) mod dynamic_images;
pub(crate) mod globals;
pub(crate) mod glyph_atlas;
pub(crate) mod image_cache;
pub(crate) mod image_mips;
pub(crate) mod layer_pool;
