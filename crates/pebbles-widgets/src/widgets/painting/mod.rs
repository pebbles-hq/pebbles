//! Painting, clipping, and visual-effect widgets.

mod canvas;
mod decorated;
mod effects;
mod effects_extra;

pub use canvas::{CanvasWidget, canvas};
pub use decorated::DecoratedBox;
pub use effects::{ClipRRect, Opacity, RepaintBoundary, clip_rrect, opacity, repaint_boundary};
pub use effects_extra::{
    BackdropFilter, ClipOval, ClipPath, ColorFiltered, ImageFiltered, ShaderMask, backdrop_filter, clip_oval,
    clip_path, clip_rect, color_filtered, image_filtered, shader_mask,
};
