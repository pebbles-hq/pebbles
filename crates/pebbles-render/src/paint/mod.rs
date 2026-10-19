//! The **render-backend seam**. RenderObjects paint into a [`Painter`], never a
//! concrete GPU scene — so the rasterizer is swappable without touching a single
//! widget. The display list ([`display_list`]) is backend-neutral and shared by every
//! consumer (DR-02): the vello-hybrid host flushes it (`hybrid_flush`, until Phase 7),
//! and the wgpu backend reads it via `wgpu_backend` (until C05 moves it into pebbles-gpu).
//!
//! `vello-hybrid` wins whenever it is enabled (DR-10); select wgpu with
//! `--no-default-features --features wgpu` (the CLI's `--renderer wgpu` does exactly this).
//! Both expose the SAME `Scene` / `scene()` / `Painter` / `Glyph` surface, so the widget
//! layer is identical across backends. The geometry/brush vocabulary ([`Affine`], [`Fill`],
//! [`Stroke`], [`Brush`], …) is `kurbo`/`peniko` — renderer-neutral, re-exported here and
//! imported by RenderObjects from `crate::paint` rather than from any backend crate.

// ---- backend-neutral vocabulary (identical types for all consumers) --------
pub use kurbo::{self, Affine, BezPath, Circle, Point, Rect, Shape, Stroke};
pub use peniko::{self, Brush, BrushRef, Color, Fill, FontData};

// ---- the shared display list (DR-02) ----------------------------------------
pub mod display_list;
pub mod dynamic_image;
pub use display_list::{Glyph, NormalizedCoord, Painter, Scene, scene};
pub use dynamic_image::{DynamicImage, DynamicImageFrame, IRect};

// Transitional consumer: replays the display list into a live `vello_hybrid::Scene`
// (feature `vello-hybrid`; deleted whole at Phase 7, X01).
#[cfg(feature = "vello-hybrid")]
mod hybrid_flush;
