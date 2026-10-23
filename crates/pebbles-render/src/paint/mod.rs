//! The paint layer. RenderObjects paint into a [`Painter`], never a concrete GPU scene: the
//! display list ([`display_list`]) is consumed by `pebbles-gpu`. The geometry/brush vocabulary
//! ([`Affine`], [`Fill`], [`Stroke`], [`Brush`], …) is `kurbo`/`peniko`, re-exported here and
//! imported by RenderObjects from `crate::paint`.

// ---- backend-neutral vocabulary (identical types for all consumers) --------
pub use kurbo::{self, Affine, BezPath, Circle, Point, Rect, Shape, Stroke};
pub use peniko::{self, Brush, BrushRef, Color, Fill, FontData};

// ---- the shared display list (DR-02) ----------------------------------------
pub mod color_matrix;
pub mod display_list;
pub mod dynamic_image;
pub mod mesh;
pub mod projective;
pub mod shader_program;
pub use color_matrix::ColorMatrix;
pub use display_list::{Glyph, NormalizedCoord, Painter, Scene, scene};
pub use dynamic_image::{DynamicImage, DynamicImageFrame, IRect};
pub use mesh::{Mesh, MeshError, Sprite, VertexColorMode, VertexMode};
pub use projective::Projective;
pub use shader_program::{
    MAX_SHADER_IMAGES, MAX_SHADER_UNIFORMS, ShaderBrush, ShaderBrushError, ShaderProgram,
};
