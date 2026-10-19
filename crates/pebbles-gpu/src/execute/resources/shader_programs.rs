//! Shader-brush programs (09 §6.1, L43): app WGSL assembled with the brush template, validated
//! with naga before the device sees it (same verdict on every backend, WebGL2 included), then
//! compiled once and cached with its pipelines per `(variant, format, samples)`. A failing
//! program is logged once and draws nothing; an unused program is dropped after 600 frames.

use std::collections::HashMap;

use pebbles_render::paint::ShaderProgram;

/// Why a shader program was rejected; `line` is 1-based in the app source when the error is
/// inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaderError {
    /// The naga diagnostic (or the broken rule).
    pub message: String,
    /// 1-based line in the app source, when known and inside it.
    pub line: Option<usize>,
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => write!(f, "shader brush, line {l}: {}", self.message),
            None => write!(f, "shader brush: {}", self.message),
        }
    }
}

impl std::error::Error for ShaderError {}

/// Frames an unused program stays cached.
const PROGRAM_IDLE_FRAMES: u64 = 600;

/// The bound globals the template declares (the app may not add any).
const TEMPLATE_BINDINGS: &[&str] = &["t0", "t1", "s0", "u"];

fn app_line(module_line: u32) -> Option<usize> {
    let head = crate::shaders::shader_brush_head_lines();
    let l = usize::try_from(module_line).ok()?;
    (l > head).then(|| l - head)
}

fn rule(message: &str) -> ShaderError {
    ShaderError { message: message.to_owned(), line: None }
}

/// Whether `block` (recursively) contains `discard`.
fn has_kill(block: &naga::Block) -> bool {
    block.iter().any(|s| match s {
        naga::Statement::Kill => true,
        naga::Statement::Block(b) => has_kill(b),
        naga::Statement::If { accept, reject, .. } => has_kill(accept) || has_kill(reject),
        naga::Statement::Switch { cases, .. } => cases.iter().any(|c| has_kill(&c.body)),
        naga::Statement::Loop { body, continuing, .. } => has_kill(body) || has_kill(continuing),
        _ => false,
    })
}

/// Validate `p` without a device (09 §6.1): parse + validate the assembled module, enforce the
/// WebGL2 / determinism rules (no storage, compute, extra bindings or `discard`) and translate
/// every entry point to GLSL ES 3.00. Apps call it in their tests.
pub fn validate_shader_program(p: &ShaderProgram) -> Result<(), ShaderError> {
    let src = crate::shaders::shader_brush_source(p.source());
    let module = naga::front::wgsl::parse_str(&src).map_err(|e| ShaderError {
        message: e.message().to_owned(),
        line: e.location(&src).and_then(|l| app_line(l.line_number)),
    })?;
    let info =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .map_err(|e| ShaderError {
                message: e.as_inner().to_string(),
                line: e.location(&src).and_then(|l| app_line(l.line_number)),
            })?;
    if !module.functions.iter().any(|(_, f)| f.name.as_deref() == Some("pebbles_paint")) {
        return Err(rule("missing `fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32>`"));
    }
    for (_, g) in module.global_variables.iter() {
        if matches!(g.space, naga::AddressSpace::Storage { .. }) {
            return Err(rule("storage buffers are not allowed (WebGL2)"));
        }
        if g.binding.is_some()
            && g.binding.as_ref().is_some_and(|b| b.group == 1)
            && !g.name.as_deref().is_some_and(|n| TEMPLATE_BINDINGS.contains(&n))
        {
            return Err(rule("only t0, t1, s0 and u may be bound"));
        }
    }
    for (_, ty) in module.types.iter() {
        if matches!(ty.inner, naga::TypeInner::Image { class: naga::ImageClass::Storage { .. }, .. }) {
            return Err(rule("storage textures are not allowed (WebGL2)"));
        }
    }
    if module.entry_points.iter().any(|ep| ep.stage == naga::ShaderStage::Compute) {
        return Err(rule("compute entry points are not allowed"));
    }
    if module.functions.iter().any(|(_, f)| has_kill(&f.body)) {
        return Err(rule("`discard` is not allowed (determinism, 09 §6.1)"));
    }
    for ep in &module.entry_points {
        let opts = naga::back::glsl::Options {
            version: naga::back::glsl::Version::Embedded { version: 300, is_webgl: true },
            ..Default::default()
        };
        let popts = naga::back::glsl::PipelineOptions {
            shader_stage: ep.stage,
            entry_point: ep.name.clone(),
            multiview: None,
        };
        let mut out = String::new();
        naga::back::glsl::Writer::new(
            &mut out,
            &module,
            &info,
            &opts,
            &popts,
            naga::proc::BoundsCheckPolicies::default(),
        )
        .and_then(|mut w| w.write())
        .map_err(|e| rule(&format!("{}: not expressible in GLSL ES 3.00 (WebGL2): {e}", ep.name)))?;
    }
    Ok(())
}

enum Compiled {
    Ok(wgpu::ShaderModule),
    Failed,
}

struct Program {
    compiled: Compiled,
    pipelines: HashMap<(u8, wgpu::TextureFormat, u32), wgpu::RenderPipeline>,
    last_used: u64,
}

/// The compiled programs of one device.
#[derive(Default)]
pub(crate) struct ShaderPrograms {
    programs: HashMap<u64, Program>,
    layout: Option<wgpu::PipelineLayout>,
    frame: u64,
}

impl ShaderPrograms {
    /// Compile `p` on first use; false when it failed (logged once, at its first failure).
    pub(crate) fn ensure(&mut self, device: &wgpu::Device, p: &ShaderProgram) -> bool {
        let frame = self.frame;
        let prog = self.programs.entry(p.id()).or_insert_with(|| {
            let compiled = match validate_shader_program(p) {
                Ok(()) => Compiled::Ok(device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("pebbles.gpu.shader.shader_brush"),
                    source: wgpu::ShaderSource::Wgsl(crate::shaders::shader_brush_source(p.source()).into()),
                })),
                Err(e) => {
                    pebbles_foundation::log::logf(
                        pebbles_foundation::log::Level::Warn,
                        pebbles_foundation::log::Cat::Gpu,
                        format_args!("{e}; the program draws nothing (09 §6.1)"),
                    );
                    Compiled::Failed
                }
            };
            Program { compiled, pipelines: HashMap::new(), last_used: frame }
        });
        prog.last_used = frame;
        matches!(prog.compiled, Compiled::Ok(_))
    }

    /// Build the coverage-variant pipeline `(variant, format, samples)` of a compiled program
    /// (before any pass opens).
    pub(crate) fn ensure_pipeline(
        &mut self,
        device: &wgpu::Device,
        layouts: &super::bind_groups::BindGroupLayouts,
        id: u64,
        target: (u8, wgpu::TextureFormat, u32),
        stats: &mut crate::stats::FrameStats,
    ) {
        let (variant, format, samples) = target;
        let layout = self
            .layout
            .get_or_insert_with(|| crate::execute::pipelines::shader_brush::pipeline_layout(device, layouts))
            .clone();
        let Some(prog) = self.programs.get_mut(&id) else { return };
        let Compiled::Ok(module) = &prog.compiled else { return };
        if prog.pipelines.contains_key(&(variant, format, samples)) {
            return;
        }
        if let Some(p) =
            crate::execute::pipelines::shader_brush::build(device, module, &layout, variant, format, samples)
        {
            prog.pipelines.insert((variant, format, samples), p);
            stats.pipelines_created += 1;
        }
    }

    pub(crate) fn pipeline(
        &self,
        id: u64,
        variant: u8,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Option<&wgpu::RenderPipeline> {
        self.programs.get(&id)?.pipelines.get(&(variant, format, samples))
    }

    /// End of frame: drop programs unused for `PROGRAM_IDLE_FRAMES`.
    pub(crate) fn end_frame(&mut self) {
        let frame = self.frame;
        self.programs.retain(|_, p| frame.saturating_sub(p.last_used) < PROGRAM_IDLE_FRAMES);
        self.frame += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UV: &str = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(local.x / 64.0, 0.0, 0.0, 1.0);
}";

    #[test]
    fn validate_accepts_minimal_program() {
        assert_eq!(validate_shader_program(&ShaderProgram::new(UV)), Ok(()));
    }

    #[test]
    fn validate_accepts_uniforms_and_textures() {
        let src = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    let c = textureSampleLevel(t0, s0, local / u.size, 0.0);
    return c * u.values[0].x + textureSampleLevel(t1, s0, local / u.size, 0.0) * u.values[0].y;
}";
        assert_eq!(validate_shader_program(&ShaderProgram::new(src)), Ok(()));
    }

    #[test]
    fn validate_rejects_bad_wgsl() {
        let src = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {\n    return vec4<f32>(1.0, 0.0, 0.0 1.0);\n}";
        let e = validate_shader_program(&ShaderProgram::new(src)).expect_err("syntax error");
        assert_eq!(e.line, Some(2), "{e}");
    }

    #[test]
    fn validate_rejects_missing_entry_and_discard_and_storage() {
        let missing = "fn other() -> f32 { return 1.0; }";
        assert!(validate_shader_program(&ShaderProgram::new(missing)).is_err());
        let discard = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    if local.x > 1.0 { discard; }
    return vec4<f32>(0.0);
}";
        assert!(validate_shader_program(&ShaderProgram::new(discard)).is_err());
        let storage = "@group(1) @binding(7) var<storage, read> x: array<f32>;
fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> { return vec4<f32>(x[0]); }";
        assert!(validate_shader_program(&ShaderProgram::new(storage)).is_err());
    }
}
