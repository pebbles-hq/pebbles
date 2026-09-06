//! The **GPU host seam** — the wgpu context/surface/renderer the runner drives, in
//! whichever backend `pebbles-render` was built with. Both impls expose the SAME API
//! (`RenderContext`, `RenderSurface`, `Renderer`, `RenderParams`, `AaConfig`,
//! `new_renderer`, `Scene`), so the runner's frame loop is backend-agnostic.
//!
//! - `vello`  — thin re-export of `vello::util` + `vello::Renderer` (GPU compute).
//! - `vello-hybrid` — reuses `vello::util` for the backend-agnostic surface/blit
//!   plumbing, swaps the intermediate target to a `RENDER_ATTACHMENT` texture, and
//!   drives a `vello_hybrid::Renderer` that flushes the recorded op-list and rasterizes.
//!
//! The scene type is always `pebbles_render::Scene` (backend-correct by construction).

#[cfg(feature = "vello")]
mod vello_host;
#[cfg(feature = "vello")]
pub(crate) use vello_host::{new_renderer, AaConfig, RenderContext, RenderParams, Renderer, RenderSurface};

#[cfg(feature = "vello-hybrid")]
mod hybrid_host;
#[cfg(feature = "vello-hybrid")]
pub(crate) use hybrid_host::{new_renderer, AaConfig, RenderContext, RenderParams, Renderer, RenderSurface};

/// The retained scene the runner paints into and presents — always the backend-correct
/// `pebbles-render` type (a `vello::Scene` under `vello`, a recorded op-list under hybrid).
pub(crate) use pebbles_render::Scene;
