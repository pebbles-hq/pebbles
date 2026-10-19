//! App-provided WGSL paint (09 §6.1, L43; Flutter `FragmentShader`). pebbles-render stays
//! GPU-free: a [`ShaderProgram`] is only source + a process-unique id; the renderer compiles and
//! validates it (`pebbles_gpu::validate_shader_program` checks it without a device).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::Affine;

/// The most floats a [`ShaderBrush`] passes (`u.values: array<vec4<f32>, 16>`).
pub const MAX_SHADER_UNIFORMS: usize = 64;
/// The most images a [`ShaderBrush`] binds (`t0`, `t1`; WebGL2).
pub const MAX_SHADER_IMAGES: usize = 2;

/// WGSL defining `fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32>` (premultiplied
/// RGBA in 0..1) — the contract is 09 §6.1.
#[derive(Clone, Debug)]
pub struct ShaderProgram {
    source: Arc<str>,
    id: u64,
}

impl ShaderProgram {
    /// Wrap `wgsl`; no validation here (the renderer validates on first use).
    pub fn new(wgsl: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self { source: Arc::from(wgsl), id: NEXT.fetch_add(1, Ordering::Relaxed) }
    }

    /// The app WGSL.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Process-unique id (pipeline cache key).
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl PartialEq for ShaderProgram {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Why a [`ShaderBrush`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShaderBrushError {
    /// More than [`MAX_SHADER_UNIFORMS`] floats.
    TooManyUniforms(usize),
    /// More than [`MAX_SHADER_IMAGES`] images.
    TooManyImages(usize),
}

impl std::fmt::Display for ShaderBrushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyUniforms(n) => write!(f, "{n} uniforms (at most {MAX_SHADER_UNIFORMS})"),
            Self::TooManyImages(n) => write!(f, "{n} images (at most {MAX_SHADER_IMAGES})"),
        }
    }
}

impl std::error::Error for ShaderBrushError {}

/// A program plus its per-draw inputs. `transform` maps the brush's local space into the op's
/// local space (like a gradient's brush transform).
#[derive(Clone, Debug)]
pub struct ShaderBrush {
    pub program: ShaderProgram,
    /// ≤ 64 floats, read as `u.values` (zero-filled to 64).
    pub uniforms: Arc<[f32]>,
    /// ≤ 2 images, bound as `t0`, `t1` (missing ones are 1×1 transparent).
    pub images: Arc<[peniko::ImageBrush]>,
    pub transform: Affine,
}

impl ShaderBrush {
    /// A brush with the limits checked.
    pub fn new(
        program: ShaderProgram,
        uniforms: impl Into<Arc<[f32]>>,
        images: impl Into<Arc<[peniko::ImageBrush]>>,
        transform: Affine,
    ) -> Result<Self, ShaderBrushError> {
        let (uniforms, images) = (uniforms.into(), images.into());
        if uniforms.len() > MAX_SHADER_UNIFORMS {
            return Err(ShaderBrushError::TooManyUniforms(uniforms.len()));
        }
        if images.len() > MAX_SHADER_IMAGES {
            return Err(ShaderBrushError::TooManyImages(images.len()));
        }
        Ok(Self { program, uniforms, images, transform })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_count_is_bounded() {
        let p = ShaderProgram::new(
            "fn pebbles_paint(l: vec2<f32>, d: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0); }",
        );
        let none: [peniko::ImageBrush; 0] = [];
        assert!(ShaderBrush::new(p.clone(), vec![0.0; 64], none.clone(), Affine::IDENTITY).is_ok());
        assert_eq!(
            ShaderBrush::new(p, vec![0.0; 65], none, Affine::IDENTITY).err(),
            Some(ShaderBrushError::TooManyUniforms(65))
        );
    }

    #[test]
    fn program_ids_are_unique() {
        let (a, b) = (ShaderProgram::new(""), ShaderProgram::new(""));
        assert_ne!(a.id(), b.id());
        assert_eq!(a.clone(), a);
    }
}
