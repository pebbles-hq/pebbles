//! The **render-backend seam**. RenderObjects paint into a [`Painter`], never a
//! concrete GPU scene — so the rasterizer is swappable without touching a single
//! widget. Each backend lives in its own file (mirroring the shell's `gpu/` split):
//!
//! - [`vello_backend`] (`vello` feature, default) — `Scene` IS a `vello::Scene`; the
//!   `Painter` verbs forward straight to it. GPU-compute raster; the opt-in for heavy vector.
//! - [`hybrid_backend`] (`vello-hybrid` feature) — `Scene` is a retained, backend-neutral
//!   op-list; the `Painter` verbs record ops (owning their `kurbo`/`peniko` inputs), and the
//!   shell flushes the list into a `vello_hybrid::Scene`. Low-power hybrid CPU+GPU raster.
//!
//! Both expose the SAME `Scene` / `scene()` / `Painter` / `Glyph` surface, so the widget
//! layer is identical across backends. The geometry/brush vocabulary ([`Affine`], [`Fill`],
//! [`Stroke`], [`Brush`], …) is `kurbo`/`peniko` — renderer-neutral, re-exported here and
//! imported by RenderObjects from `crate::paint` rather than from any backend crate.
//!
//! See `documentations/renderer-backend-plan.md` for the full phased plan.

// Exactly one backend must be selected (see this crate's `[features]`).
#[cfg(all(feature = "vello", feature = "vello-hybrid"))]
compile_error!(
    "pebbles-render: enable exactly ONE render backend feature — `vello` OR `vello-hybrid`, not both"
);
#[cfg(not(any(feature = "vello", feature = "vello-hybrid")))]
compile_error!(
    "pebbles-render: enable a render backend feature — `vello` (default) or `vello-hybrid`"
);

// ---- backend-neutral vocabulary (identical types for both backends) --------
pub use kurbo::{self, Affine, BezPath, Circle, Point, Rect, Shape, Stroke};
pub use peniko::{self, Brush, BrushRef, Color, Fill, FontData};

// ---- the active backend supplies Scene / scene() / Painter / Glyph ----------
#[cfg(feature = "vello")]
mod vello_backend;
#[cfg(feature = "vello")]
pub use vello_backend::{scene, Glyph, NormalizedCoord, Painter, Scene};

#[cfg(feature = "vello-hybrid")]
mod hybrid_backend;
#[cfg(feature = "vello-hybrid")]
pub use hybrid_backend::{scene, Glyph, NormalizedCoord, Painter, Scene};
