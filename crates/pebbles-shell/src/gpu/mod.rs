//! The **GPU host seam** — the wgpu context/surface/renderer the runner drives, in
//! whichever backend `pebbles-render` was built with. Both impls expose the SAME API
//! (`RenderContext`, `RenderSurface`, `Renderer`, `RenderParams`, `AaConfig`,
//! `new_renderer`, `Scene`), so the runner's frame loop is backend-agnostic.
//!
//! - `vello`  — thin re-export of `vello::util` + `vello::Renderer` (GPU compute).
//! - `vello-hybrid` — reuses `vello::util` for the backend-agnostic surface/blit
//!   plumbing, swaps the intermediate target to a `RENDER_ATTACHMENT` texture, and
//!   drives a `vello_hybrid::Renderer` that flushes the recorded op-list and rasterizes.
//! - `wgpu`  — the first-party host: owns its own wgpu instance/adapter/device pool and
//!   surface (no Vello types), and drives a wgpu `Renderer` (in `wgpu_host.rs`).
//!
//! The scene type is always `pebbles_render::Scene` (backend-correct by construction).

// Mirror pebbles-render's transition precedence: `wgpu` wins while it is opt-in.
#[cfg(feature = "wgpu")]
mod wgpu_host;
#[cfg(feature = "wgpu")]
pub(crate) use wgpu_host::{AaConfig, RenderContext, RenderParams, RenderSurface, Renderer, new_renderer};

#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
mod vello_host;
#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
pub(crate) use vello_host::{AaConfig, RenderContext, RenderParams, RenderSurface, Renderer, new_renderer};

#[cfg(all(feature = "vello-hybrid", not(feature = "wgpu")))]
mod hybrid_host;
#[cfg(all(feature = "vello-hybrid", not(feature = "wgpu")))]
pub(crate) use hybrid_host::{AaConfig, RenderContext, RenderParams, RenderSurface, Renderer, new_renderer};

/// The retained scene the runner paints into and presents — always the backend-correct
/// `pebbles-render` type (a `vello::Scene` under `vello`, a recorded op-list under hybrid).
pub(crate) use pebbles_render::Scene;
