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
//! `vello-hybrid` wins whenever it is enabled (DR-10); select wgpu with
//! `--no-default-features --features wgpu` (the CLI's `--renderer wgpu` does exactly this).
//! The scene type is always `pebbles_render::Scene` (backend-correct by construction).

// Mirror pebbles-render's transition precedence: `vello-hybrid` wins whenever it is
// enabled (DR-10), so `--all-features` keeps testing the shipping default.
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
mod wgpu_host;
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
pub(crate) use wgpu_host::{AaConfig, RenderContext, RenderParams, RenderSurface, Renderer, new_renderer};

#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
mod vello_host;
#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
pub(crate) use vello_host::{AaConfig, RenderContext, RenderParams, RenderSurface, Renderer, new_renderer};

#[cfg(feature = "vello-hybrid")]
mod hybrid_host;
#[cfg(feature = "vello-hybrid")]
pub(crate) use hybrid_host::{AaConfig, RenderContext, RenderParams, RenderSurface, Renderer, new_renderer};

// The selected host, as a crate-private marker read by the tests below (and by P09's
// adapter-info line).
#[cfg(feature = "vello-hybrid")]
#[allow(dead_code)] // read by tests and by P09's adapter-info line
pub(crate) const HOST: &str = "vello-hybrid";
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
#[allow(dead_code)]
pub(crate) const HOST: &str = "wgpu";
#[cfg(all(feature = "vello", not(any(feature = "vello-hybrid", feature = "wgpu"))))]
#[allow(dead_code)]
pub(crate) const HOST: &str = "vello";

/// The retained scene the runner paints into and presents — always the backend-correct
/// `pebbles-render` type (a `vello::Scene` under `vello`, a recorded op-list under hybrid).
pub(crate) use pebbles_render::Scene;

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(all(feature = "wgpu", feature = "vello-hybrid"))]
    fn hybrid_wins_when_both_backends_are_enabled() {
        assert_eq!(super::HOST, "vello-hybrid");
    }
    #[test]
    #[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
    fn wgpu_alone_selects_the_wgpu_host() {
        assert_eq!(super::HOST, "wgpu");
    }
}
