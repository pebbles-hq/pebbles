//! The full-Vello GPU host — the working default. Everything is a direct re-export of
//! `vello`'s own types; only `new_renderer` (moved here from the runner) carries the
//! Pebbles renderer options.

pub(crate) use vello::util::{RenderContext, RenderSurface};
pub(crate) use vello::{AaConfig, RenderParams, Renderer};

/// Build the Vello renderer with Pebbles' options. `PEBBLES_CPU_RENDER=1` forces the CPU
/// pipeline (slower, but dodges driver glyph-atlas bugs seen on some Intel/Vulkan setups
/// where the GPU renderer emits `Texture … is invalid` and text fails to draw).
pub(crate) fn new_renderer(device: &wgpu::Device, _queue: &wgpu::Queue) -> Renderer {
    use vello::{AaSupport, RendererOptions};
    let use_cpu = std::env::var("PEBBLES_CPU_RENDER").is_ok_and(|v| v == "1" || v == "true");
    if use_cpu {
        pebbles_core::log::warn(
            pebbles_core::log::Cat::Gpu,
            "PEBBLES_CPU_RENDER — using vello's CPU pipeline (slower, but avoids GPU driver bugs)"
                .to_string(),
        );
    }
    Renderer::new(
        device,
        RendererOptions {
            use_cpu,
            antialiasing_support: AaSupport::all(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .expect("create vello renderer")
}
