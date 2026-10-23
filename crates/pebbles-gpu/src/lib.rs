//! The first-party wgpu renderer for Pebbles: CPU **preparation** of the shared display
//! list and GPU **execution** on wgpu.
//!
//! # Layer contract
//!
//! * `prepare` walks the `pebbles_render` display list into GPU-ready geometry on the
//!   CPU — it never touches a device.
//! * `renderer`/`execute`/`raster` own every wgpu resource and run the render.
//! * `context`/`surface` own the instance, adapter, device pool and surfaces.
//!
//! This is an **implementation crate** with no semver guarantees before 1.0 beyond what
//! `pebbles::shell` re-exports. Apps use `pebbles::shell`, not this crate directly.

#![deny(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::print_stdout, clippy::print_stderr)]
#![warn(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_possible_wrap)]
#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable))]

#[doc(hidden)]
pub mod __fuzz;
mod budgets;
mod census;
mod context;
mod diag;
#[cfg(test)]
mod downlevel;
mod error;
mod execute;
mod frame_targets;
#[allow(unsafe_code)] // bytemuck derives expand to `unsafe impl Pod`; no hand-written unsafe
mod gpu_types;
mod health;
mod init;
mod limits;
#[cfg(not(target_family = "wasm"))]
mod offscreen;
mod prepare;
mod raster;
#[cfg(not(target_family = "wasm"))]
mod readback;
mod renderer;
mod shaders;
mod stats;
mod surface;
#[cfg(test)]
mod test_alloc;
#[cfg(test)]
pub(crate) mod test_support;
mod timing;

pub use budgets::{BudgetPlatform, GpuBudgets};
pub use census::GpuCensus;
pub use context::{DeviceHealth, DeviceId, GpuCaps, GpuContext, GpuDevice};
pub use diag::{backend_error_count, install_log_bridge};
pub use error::{AcquireError, CaptureError, GpuError};
pub use execute::custom::{CustomPrepare, CustomRender, CustomRenderer};
pub use execute::resources::shader_programs::{ShaderError, validate_shader_program};
pub use frame_targets::{FrameParams, FrameTargets, RenderTarget};
#[cfg(not(target_family = "wasm"))]
pub use init::backend_rank;
pub use init::{GpuInit, GpuPower, HostOs};
pub use limits::RendererLimits;
#[cfg(not(target_family = "wasm"))]
pub use offscreen::{OffscreenRenderer, RgbaImage};
pub use pebbles_render::paint::display_list::{CustomPaint, TextureId};
pub use renderer::Renderer;

#[doc(hidden)]
pub mod testing;
pub use stats::{FrameStats, MemoryReport, Unsupported};
/// R14's seeded oracle check in render-check: the Tier B mask of `path` over a `w × h` target.
#[doc(hidden)]
pub fn __tier_b_mask_for_check(
    path: &kurbo::BezPath,
    device: kurbo::Affine,
    rule: peniko::Fill,
    w: u32,
    h: u32,
) -> Vec<u8> {
    raster::coverage::full_mask(path, device, rule, w, h)
}
pub use surface::{
    Acquire, AcquireStatus, AcquireStreak, ConfigureDecision, FrameAction, RenderSurface, SurfaceFrame,
    action_for, configure_decision, escalate,
};
