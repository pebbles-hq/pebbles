//! Downlevel audit (08 §11 DL-1…DL-14, P15): every vertex layout, uniform block, index buffer,
//! MSAA count and shader stays inside the WebGL2 profile. CPU-only; naga validation is V02's.

use crate::gpu_types::{
    ClipParams, CompositeParams, EffectParams, GradientParams, ImageBrushParams, MaskClipParams,
    ProjectedParams, ShaderBrushParams,
};

/// Every uniform block bound by a pipeline, with its size (DL-5).
const UNIFORM_BLOCKS: &[(&str, usize)] = &[
    ("Globals", size_of::<crate::execute::resources::globals::Globals>()),
    ("ClipParams", size_of::<ClipParams>()),
    ("MaskClipParams", size_of::<MaskClipParams>()),
    ("CompositeParams", size_of::<CompositeParams>()),
    ("GradientParams", size_of::<GradientParams>()),
    ("ImageBrushParams", size_of::<ImageBrushParams>()),
    ("ShaderBrushParams", size_of::<ShaderBrushParams>()),
    ("ProjectedParams", size_of::<ProjectedParams>()),
    ("EffectParams", size_of::<EffectParams>()),
];

fn webgl2() -> wgpu::Limits {
    wgpu::Limits::downlevel_webgl2_defaults()
}

#[test]
fn vertex_strides_fit_webgl2() {
    let l = webgl2();
    for d in crate::execute::pipelines::descs::pipeline_descs() {
        let Some(layout) = &d.layout else { continue };
        assert!(
            layout.array_stride <= u64::from(l.max_vertex_buffer_array_stride),
            "{:?}: stride {}",
            d.kind,
            layout.array_stride
        );
        assert_eq!(layout.array_stride % wgpu::VERTEX_ALIGNMENT, 0, "{:?}", d.kind);
        assert!(
            u32::try_from(layout.attributes.len()).unwrap_or(u32::MAX) <= l.max_vertex_attributes,
            "{:?}",
            d.kind
        );
    }
}

#[test]
fn uniform_blocks_fit_webgl2() {
    let l = webgl2();
    for (name, size) in UNIFORM_BLOCKS {
        assert_eq!(size % 16, 0, "{name}: {size} B is not a multiple of 16");
        assert!(*size as u64 <= l.max_uniform_buffer_binding_size, "{name}: {size} B");
    }
}

#[test]
fn index_buffers_are_index_copy_dst_only() {
    use crate::execute::resources::buffers::INDEX_BUFFER_USAGE;
    assert_eq!(INDEX_BUFFER_USAGE, wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST);
}

#[test]
fn msaa_is_four_or_one() {
    assert_eq!(crate::context::pick_msaa(true), 4);
    assert_eq!(crate::context::pick_msaa(false), 1);
}

#[test]
fn wgsl_uses_no_downlevel_forbidden_features() {
    for (name, src) in crate::shaders::ALL {
        for token in ["var<storage", "texture_storage_", "@compute", "sample_index", ", sample)"] {
            assert!(!src.contains(token), "{name}: uses `{token}` (not in the WebGL2 profile)");
        }
    }
}

#[test]
fn index_split_below_2_pow_24() {
    const { assert!(crate::prepare::segments::MAX_PORTABLE_VERTICES_PER_SEGMENT < 1 << 24) };
}
