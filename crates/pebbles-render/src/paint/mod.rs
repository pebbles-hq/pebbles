//! The **render-backend seam**. RenderObjects paint into a [`Painter`], never a
//! concrete GPU scene — so the rasterizer is swappable without touching a single
//! widget. Each backend lives in its own file (mirroring the shell's `gpu/` split):
//!
//! - `hybrid_backend` (`vello-hybrid` feature, **default**) — `Scene` is a retained,
//!   backend-neutral op-list; the `Painter` verbs record ops (owning their `kurbo`/`peniko`
//!   inputs), and the shell flushes the list into a `vello_hybrid::Scene`. Low-power hybrid
//!   CPU+GPU raster.
//! - `wgpu_backend` (`wgpu` feature, opt-in) — the first-party renderer (wgpu migration).
//!
//! `vello-hybrid` wins whenever it is enabled (DR-10); select wgpu with
//! `--no-default-features --features wgpu` (the CLI's `--renderer wgpu` does exactly this).
//! Both expose the SAME `Scene` / `scene()` / `Painter` / `Glyph` surface, so the widget
//! layer is identical across backends. The geometry/brush vocabulary ([`Affine`], [`Fill`],
//! [`Stroke`], [`Brush`], …) is `kurbo`/`peniko` — renderer-neutral, re-exported here and
//! imported by RenderObjects from `crate::paint` rather than from any backend crate.

// Pick a backend. `vello-hybrid` wins whenever it is enabled (DR-10), so
// `--all-features` keeps testing the shipping default; select wgpu with
// `--no-default-features --features wgpu`.
#[cfg(not(any(feature = "wgpu", feature = "vello-hybrid")))]
compile_error!("pebbles-render: enable a render backend feature — `vello-hybrid` (default) or `wgpu`");

// ---- backend-neutral vocabulary (identical types for both backends) --------
pub use kurbo::{self, Affine, BezPath, Circle, Point, Rect, Shape, Stroke};
pub use peniko::{self, Brush, BrushRef, Color, Fill, FontData};

// ---- the active backend supplies Scene / scene() / Painter / Glyph ----------
// `vello-hybrid` wins whenever it is enabled (DR-10), so exactly one backend
// module is ever built.
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
mod wgpu_backend;
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
pub use wgpu_backend::{
    Batch, Glyph, GlyphRun, ImageQuad, ImageUpload, ImageVertex, Mesh, NormalizedCoord, Painter, Scene,
    Scissor, ShadowVertex, SolidMesh, Vertex, scene,
};

#[cfg(feature = "vello-hybrid")]
mod hybrid_backend;
#[cfg(feature = "vello-hybrid")]
pub use hybrid_backend::{Glyph, NormalizedCoord, Painter, Scene, scene};
