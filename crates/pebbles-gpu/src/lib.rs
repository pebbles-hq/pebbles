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

mod context;
mod error;
mod execute;
mod frame_targets;
#[allow(unsafe_code)] // bytemuck derives expand to `unsafe impl Pod`; no hand-written unsafe
mod gpu_types;
mod init;
mod prepare;
mod raster;
mod renderer;
mod surface;
#[cfg(test)]
pub(crate) mod test_support;

pub use context::{DeviceId, GpuCaps, GpuContext, GpuDevice};
pub use error::{AcquireError, CaptureError, GpuError};
pub use init::{GpuInit, GpuPower, HostOs};
pub use renderer::{RenderParams, Renderer};
pub use surface::RenderSurface;
