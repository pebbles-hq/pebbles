//! The **render-backend seam**. RenderObjects paint into a [`Painter`], never a
//! concrete GPU scene — so the rasterizer is swappable without touching a single
//! widget. Each backend lives in its own file (mirroring the shell's `gpu/` split):
//!
//! - `hybrid_backend` (`vello-hybrid` feature, **default**) — `Scene` is a retained,
//!   backend-neutral op-list; the `Painter` verbs record ops (owning their `kurbo`/`peniko`
//!   inputs), and the shell flushes the list into a `vello_hybrid::Scene`. Low-power hybrid
//!   CPU+GPU raster.
//! - `vello_backend` (`vello` feature, opt-in) — `Scene` IS a `vello::Scene`; the `Painter`
//!   verbs forward straight to it. GPU-compute raster; the opt-in for heavy vector.
//!
//! Both expose the SAME `Scene` / `scene()` / `Painter` / `Glyph` surface, so the widget
//! layer is identical across backends. The geometry/brush vocabulary ([`Affine`], [`Fill`],
//! [`Stroke`], [`Brush`], …) is `kurbo`/`peniko` — renderer-neutral, re-exported here and
//! imported by RenderObjects from `crate::paint` rather than from any backend crate.

// Pick a backend. During the transition `wgpu` wins if several are selected, which
// keeps `--all-features` deterministic while leaving the existing default untouched.
#[cfg(not(any(feature = "wgpu", feature = "vello", feature = "vello-hybrid")))]
compile_error!(
    "pebbles-render: enable a render backend feature — `vello-hybrid` (default), `vello`, or `wgpu`"
);

// ---- backend-neutral vocabulary (identical types for both backends) --------
pub use kurbo::{self, Affine, BezPath, Circle, Point, Rect, Shape, Stroke};
pub use peniko::{self, Brush, BrushRef, Color, Fill, FontData};

// ---- the active backend supplies Scene / scene() / Painter / Glyph ----------
// `wgpu` has transition precedence. Otherwise `vello` compiles only when hybrid is
// absent, so exactly one backend module is ever built.
#[cfg(feature = "wgpu")]
mod wgpu_backend;
#[cfg(feature = "wgpu")]
pub use wgpu_backend::{
    Batch, Glyph, GlyphRun, ImageQuad, ImageUpload, ImageVertex, Mesh, NormalizedCoord, Painter, Scene,
    Scissor, ShadowVertex, SolidMesh, Vertex, scene,
};

#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
mod vello_backend;
#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
pub use vello_backend::{Glyph, NormalizedCoord, Painter, Scene, scene};

#[cfg(all(feature = "vello-hybrid", not(feature = "wgpu")))]
mod hybrid_backend;
#[cfg(all(feature = "vello-hybrid", not(feature = "wgpu")))]
pub use hybrid_backend::{Glyph, NormalizedCoord, Painter, Scene, scene};
