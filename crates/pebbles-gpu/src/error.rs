//! Typed GPU errors (DR-12): hand-written `Display` + `std::error::Error`, no `thiserror`.

use std::fmt;

/// A GPU initialization, surface or device error. Owned by 08 §3.8 (variants and
/// `Display` text).
#[derive(Debug)]
#[non_exhaustive]
pub enum GpuError {
    /// No acceptable adapter on the enabled backends.
    NoAdapter {
        /// The backends that were tried.
        backends: wgpu::Backends,
        /// Whether only software adapters were requested.
        software_only: bool,
        /// Adapters enumerated before filtering (native); 0 on the web.
        enumerated: usize,
        /// The web `request_adapter` error, when there is one.
        source: Option<wgpu::RequestAdapterError>,
    },
    /// The adapter refused Pebbles' baseline limits/features.
    RequestDevice {
        /// The adapter name.
        adapter: String,
        /// The underlying `request_device` error.
        source: wgpu::RequestDeviceError,
    },
    /// `Instance::create_surface` failed (e.g. no WebGL2 context on the canvas).
    CreateSurface(wgpu::CreateSurfaceError),
    /// No device's adapter can present to this surface.
    SurfaceUnsupported {
        /// The adapter name.
        adapter: String,
    },
    /// The surface offers no 8-bit RGBA/BGRA format.
    NoSurfaceFormat {
        /// The formats the surface offered.
        offered: Vec<wgpu::TextureFormat>,
    },
    /// The device was lost (driver reset, TDR, GPU removed).
    DeviceLost,
    /// The render target's (view) format is not one the renderer can draw into.
    UnsupportedTargetFormat(wgpu::TextureFormat),
    /// Target size outside `1..=max` per side.
    InvalidTargetSize {
        /// The requested width.
        width: u32,
        /// The requested height.
        height: u32,
        /// The maximum allowed size per side.
        max: u32,
    },
}

impl fmt::Display for GpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter { backends, software_only, enumerated, source } => {
                write!(
                    f,
                    "no usable GPU adapter on {backends:?} ({enumerated} enumerated{})",
                    if *software_only { ", software only" } else { "" }
                )?;
                if let Some(e) = source {
                    write!(f, ": {e}")?;
                }
                Ok(())
            }
            Self::RequestDevice { adapter, source } => {
                write!(f, "GPU \"{adapter}\" refused the device request: {source}")
            }
            Self::CreateSurface(e) => write!(f, "could not create a GPU surface for the window: {e}"),
            Self::SurfaceUnsupported { adapter } => {
                write!(f, "GPU \"{adapter}\" cannot present to this window")
            }
            Self::NoSurfaceFormat { offered } => {
                write!(f, "the window surface offers no 8-bit RGBA/BGRA format (offered {offered:?})")
            }
            Self::DeviceLost => f.write_str("the GPU device was lost"),
            Self::UnsupportedTargetFormat(format) => write!(f, "cannot render into a {format:?} target"),
            Self::InvalidTargetSize { width, height, max } => {
                write!(f, "target size {width}x{height} must be within 1..={max} pixels per side")
            }
        }
    }
}

impl std::error::Error for GpuError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NoAdapter { source: Some(e), .. } => Some(e),
            Self::RequestDevice { source, .. } => Some(source),
            Self::CreateSurface(e) => Some(e),
            _ => None,
        }
    }
}

impl From<wgpu::CreateSurfaceError> for GpuError {
    fn from(e: wgpu::CreateSurfaceError) -> Self {
        Self::CreateSurface(e)
    }
}

/// A frame could not be acquired; the caller's reaction is specified in 08 §5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AcquireError {
    /// The acquire timed out.
    Timeout,
    /// The window is occluded.
    Occluded,
    /// The runner reconfigures and requests a redraw.
    Outdated,
    /// Recreate the surface via `GpuContext::create_surface`.
    Lost,
    /// Acquiring raised a validation error.
    Validation,
}

impl fmt::Display for AcquireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Timeout => "timed out acquiring the next surface frame",
            Self::Occluded => "the window is occluded; frame skipped",
            Self::Outdated => "the surface is outdated and must be reconfigured",
            Self::Lost => "the surface was lost and must be recreated",
            Self::Validation => "acquiring the surface frame raised a validation error",
        })
    }
}

impl std::error::Error for AcquireError {}

/// A headless capture failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum CaptureError {
    /// The requested size is outside `1..=max` per side.
    InvalidSize {
        /// The requested width.
        width: u32,
        /// The requested height.
        height: u32,
        /// The maximum allowed size per side.
        max: u32,
    },
    /// A GPU error surfaced during capture.
    Gpu(GpuError),
    /// A Validation / OutOfMemory / Internal error caught by the capture's error scopes.
    Wgpu(wgpu::Error),
    /// The readback buffer could not be mapped.
    Map(wgpu::BufferAsyncError),
    /// Waiting for the GPU failed.
    Poll(wgpu::PollError),
    /// The map callback never reported (channel closed) — treated like a lost device.
    ReadbackAborted,
    /// The transitional `vello-hybrid` capture path failed (pebbles-shell; deleted at Phase 7).
    Backend(String),
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize { width, height, max } => {
                write!(f, "capture size {width}x{height} must be within 1..={max} pixels per side")
            }
            Self::Gpu(e) => write!(f, "{e}"),
            Self::Wgpu(e) => write!(f, "the GPU reported an error during capture: {e}"),
            Self::Map(e) => write!(f, "could not map the capture readback buffer: {e}"),
            Self::Poll(e) => write!(f, "waiting for the GPU failed: {e}"),
            Self::ReadbackAborted => f.write_str("the capture readback was aborted (device lost?)"),
            Self::Backend(e) => write!(f, "capture failed: {e}"),
        }
    }
}

impl std::error::Error for CaptureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Gpu(e) => Some(e),
            Self::Wgpu(e) => Some(e),
            Self::Map(e) => Some(e),
            Self::Poll(e) => Some(e),
            _ => None,
        }
    }
}

impl From<GpuError> for CaptureError {
    fn from(e: GpuError) -> Self {
        Self::Gpu(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_displays_one_sentence() {
        let acquire = [
            AcquireError::Timeout.to_string(),
            AcquireError::Occluded.to_string(),
            AcquireError::Outdated.to_string(),
            AcquireError::Lost.to_string(),
            AcquireError::Validation.to_string(),
        ];
        for s in &acquire {
            assert!(!s.is_empty());
            assert!(!s.ends_with('.'));
            assert!(!s.contains('\n'));
        }

        let capture = [
            CaptureError::InvalidSize { width: 1, height: 2, max: 3 }.to_string(),
            CaptureError::Gpu(GpuError::DeviceLost).to_string(),
            CaptureError::ReadbackAborted.to_string(),
        ];
        for s in &capture {
            assert!(!s.is_empty());
            assert!(!s.ends_with('.'));
            assert!(!s.contains('\n'));
        }

        let gpu = [
            GpuError::NoAdapter {
                backends: wgpu::Backends::VULKAN,
                software_only: false,
                enumerated: 0,
                source: None,
            }
            .to_string(),
            GpuError::SurfaceUnsupported { adapter: "x".into() }.to_string(),
            GpuError::NoSurfaceFormat { offered: vec![wgpu::TextureFormat::Rgba8Unorm] }.to_string(),
            GpuError::DeviceLost.to_string(),
            GpuError::UnsupportedTargetFormat(wgpu::TextureFormat::Rgba8Unorm).to_string(),
            GpuError::InvalidTargetSize { width: 1, height: 2, max: 3 }.to_string(),
        ];
        for s in &gpu {
            assert!(!s.is_empty());
            assert!(!s.ends_with('.'));
            assert!(!s.contains('\n'));
        }
    }
}
