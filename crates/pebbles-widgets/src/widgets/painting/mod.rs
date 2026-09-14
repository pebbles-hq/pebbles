//! Painting, clipping, and visual-effect widgets.

mod canvas;
mod decorated;
mod effects;
mod effects_extra;

pub use canvas::{CanvasWidget, canvas};
pub use decorated::DecoratedBox;
pub use effects::{ClipRRect, Opacity, RepaintBoundary, clip_rrect, opacity, repaint_boundary};
pub use effects_extra::{
    ClipOval, ClipPath, ColorFiltered, ShaderMask, clip_oval, clip_path, clip_rect, color_filtered,
    shader_mask,
};
