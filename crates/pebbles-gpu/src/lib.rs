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

mod context;
mod error;
mod execute;
mod frame_targets;
#[allow(unsafe_code)] // bytemuck derives expand to `unsafe impl Pod`; no hand-written unsafe
mod gpu_types;
mod init;
#[cfg(not(target_family = "wasm"))]
mod offscreen;
mod prepare;
mod raster;
#[cfg(not(target_family = "wasm"))]
mod readback;
mod renderer;
mod stats;
mod surface;
#[cfg(test)]
pub(crate) mod test_support;

pub use context::{DeviceHealth, DeviceId, GpuCaps, GpuContext, GpuDevice};
pub use error::{AcquireError, CaptureError, GpuError};
pub use frame_targets::{FrameParams, FrameTargets, RenderTarget};
pub use init::{GpuInit, GpuPower, HostOs};
#[cfg(not(target_family = "wasm"))]
pub use offscreen::{OffscreenRenderer, RgbaImage};
pub use renderer::Renderer;
pub use stats::{FrameStats, Unsupported};
pub use surface::{Acquire, AcquireStatus, FrameAction, RenderSurface, SurfaceFrame, action_for};
