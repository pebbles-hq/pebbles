//! The GPU host the runner drives: a thin adapter over `pebbles-gpu` (`RenderContext`,
//! `RenderSurface`, `Renderer`, `present_frame`, `PresentOutcome`). The scene type is
//! `pebbles_render::Scene`.

mod wgpu_host;
#[cfg_attr(target_family = "wasm", allow(unused_imports))] // startup_failure_text: native only
pub(crate) use wgpu_host::{
    RenderContext, RenderSurface, Renderer, census_values, error_text, frame_stats_line, present_frame,
    repeated_reset_guidance, startup_failure_text,
};

pub(crate) use pebbles_render::Scene;

/// The GPU platform policy the runner builds contexts from.
#[derive(Clone)]
pub(crate) struct HostInit {
    pub gpu: pebbles_gpu::GpuInit,
}

impl HostInit {
    /// Desktop/web: carries the event loop's display handle. Built once, on the first
    /// `resumed`, from the active event loop (no `app.rs` change, 00 N16).
    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    pub(crate) fn new(event_loop: &winit::event_loop::ActiveEventLoop) -> Self {
        Self {
            gpu: {
                #[cfg_attr(not(target_family = "wasm"), allow(unused_mut))]
                let mut init = pebbles_gpu::GpuInit::from_env();
                // 08 P03: `?pebbles-force-webgl=1` drops WebGPU so the WebGL2 path is testable in
                // a browser that has WebGPU.
                #[cfg(target_family = "wasm")]
                if crate::web_gpu::query_param("pebbles-force-webgl").as_deref() == Some("1") {
                    use pebbles_foundation::log::{Cat, Level, logf};
                    let forced = init.backends - wgpu::Backends::BROWSER_WEBGPU;
                    // Echoed to the browser console too, like the GPU lines (08 §15): testers read it there.
                    if forced.contains(wgpu::Backends::GL) {
                        init.backends = forced;
                        let line = "WebGL2 forced by ?pebbles-force-webgl=1";
                        logf(Level::Info, Cat::Gpu, format_args!("{line}"));
                        web_sys::console::info_1(&format!("pebbles: {line}").into());
                    } else {
                        let line = "?pebbles-force-webgl=1 ignored: built without the `webgl` feature";
                        logf(Level::Warn, Cat::Gpu, format_args!("{line}"));
                        web_sys::console::warn_1(&format!("pebbles: {line}").into());
                    }
                }
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
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub(crate) fn headless() -> Self {
        Self { gpu: pebbles_gpu::GpuInit::headless() }
    }
}

/// A GPU context built from `init`.
pub(crate) async fn new_context(init: &HostInit) -> RenderContext {
    RenderContext::new(&init.gpu).await
}

/// The outcome of `present_frame` (06 §3.5); the runner performs the follow-up (08 §5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PresentOutcome {
    /// The frame was presented.
    Presented,
    /// Nothing was presented and a redraw is already scheduled.
    RedrawRequested,
    /// The window is not visible: stop rendering until a visibility event (08 §5.4).
    Occluded,
    /// The surface is lost: recreate it, then redraw.
    RecreateSurface,
    /// The device is poisoned: rebuild the GPU stack (08 §6.5).
    ResetGpu,
}
