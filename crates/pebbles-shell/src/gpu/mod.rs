//! The **GPU host seam** — the context/surface/renderer the runner drives, in whichever
//! backend `pebbles-render` was built with. Both hosts expose the SAME seam
//! (`RenderContext`, `RenderSurface`, `Renderer`, `present_frame`, `PresentOutcome`), so
//! the runner's frame loop is backend-agnostic.
//!
//! - `vello-hybrid` — reuses `vello::util` for the backend-agnostic surface plumbing and
//!   drives a `vello_hybrid::Renderer` that flushes the recorded op-list and rasterizes.
//! - `wgpu` — the first-party host: a thin adapter over `pebbles-gpu`.
//!
//! `vello-hybrid` wins whenever it is enabled (DR-10); select wgpu with
//! `--no-default-features --features wgpu` (the CLI's `--renderer wgpu` does exactly this).
//! The scene type is always `pebbles_render::Scene`.

#[cfg(not(any(feature = "vello-hybrid", feature = "wgpu")))]
compile_error!("pebbles-shell: enable a GPU host — `vello-hybrid` (default) or `wgpu`");

#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
mod wgpu_host;
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
pub(crate) use wgpu_host::{RenderContext, RenderSurface, Renderer, present_frame};

#[cfg(feature = "vello-hybrid")]
mod hybrid_host;
#[cfg(feature = "vello-hybrid")]
pub(crate) use hybrid_host::{RenderContext, RenderSurface, Renderer, new_renderer, present_frame};

pub(crate) use pebbles_render::Scene;

/// Transitional host-init wrapper: one field (`gpu: GpuInit`) per host, kept `cfg` in one
/// place. Collapses to `GpuInit` at Phase 7.
#[derive(Clone)]
pub(crate) struct HostInit {
    /// The wgpu platform policy. Dead under `--all-features` (hybrid wins, DR-10).
    #[cfg(feature = "wgpu")]
    #[cfg_attr(feature = "vello-hybrid", allow(dead_code))]
    pub gpu: pebbles_gpu::GpuInit,
}

impl HostInit {
    /// Desktop/web: carries the event loop's display handle. Built once, on the first
    /// `resumed`, from the active event loop (no `app.rs` change, 00 N16).
    #[cfg_attr(any(not(feature = "wgpu"), target_family = "wasm"), allow(unused_variables))]
    pub(crate) fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
        Self {
            #[cfg(feature = "wgpu")]
            gpu: {
                let init = pebbles_gpu::GpuInit::from_env();
                // GL presentation on Wayland/X11 needs the display handle; the other
                // backends ignore it. None on web (no display handle).
                #[cfg(not(target_family = "wasm"))]
                let init = init.with_display({
                    use std::sync::Arc;
                    Arc::new(event_loop.owned_display_handle()) as Arc<dyn wgpu::wgt::WgpuHasDisplayHandle>
                });
                init
            },
        }
    }

    /// Capture / tests: no display.
    #[cfg_attr(
        any(not(feature = "wgpu"), feature = "vello-hybrid", target_family = "wasm"),
        allow(dead_code)
    )]
    pub(crate) fn headless() -> Self {
        Self {
            #[cfg(feature = "wgpu")]
            gpu: pebbles_gpu::GpuInit::headless(),
        }
    }
}

/// One signature for every host. `vello-hybrid` ignores `init` (`RenderContext::new()`).
#[cfg(feature = "vello-hybrid")]
pub(crate) async fn new_context(init: &HostInit) -> RenderContext {
    let _ = init;
    RenderContext::new()
}

/// One signature for every host. The wgpu host builds a `GpuContext` from `init`.
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
pub(crate) async fn new_context(init: &HostInit) -> RenderContext {
    RenderContext::new(&init.gpu).await
}

/// The selected host, as a crate-private marker read by the tests below (and by P09's
/// adapter-info line).
#[cfg(feature = "vello-hybrid")]
#[allow(dead_code)] // read by tests and by P09's adapter-info line
pub(crate) const HOST: &str = "vello-hybrid";
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
#[allow(dead_code)]
pub(crate) const HOST: &str = "wgpu";

/// The outcome of `present_frame`. Only `Presented`/`Skipped` until C19 adds the rest.
#[cfg_attr(feature = "vello-hybrid", allow(dead_code))]
pub(crate) enum PresentOutcome {
    Presented,
    Skipped,
}

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
