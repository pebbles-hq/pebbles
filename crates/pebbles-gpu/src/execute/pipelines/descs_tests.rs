use std::collections::HashSet;

use naga::{AddressSpace, Binding, Interpolation, ScalarKind, ShaderStage, TypeInner};

use super::{PipelineDesc, pipeline_descs};
use crate::execute::pipelines::PipelineKind;

/// The full WGSL (prelude + file) of a table row, from the V02 shader table.
fn source(desc: &PipelineDesc) -> &'static str {
    let name = desc.wgsl.trim_end_matches(".wgsl");
    crate::shaders::ALL
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| panic!("{name} not in shaders::ALL"))
}

fn is_integer(module: &naga::Module, ty: naga::Handle<naga::Type>) -> bool {
    match module.types[ty].inner {
        TypeInner::Scalar(s) | TypeInner::Vector { scalar: s, .. } => {
            matches!(s.kind, ScalarKind::Uint | ScalarKind::Sint)
        }
        _ => false,
    }
}

/// `(location, is_integer, interpolation)` of every fragment input of `ep`.
fn fragment_inputs(module: &naga::Module, ep: &naga::EntryPoint) -> Vec<(u32, bool, Option<Interpolation>)> {
    let mut out = Vec::new();
    let mut visit = |binding: &Option<Binding>, ty| {
        if let Some(Binding::Location { location, interpolation, .. }) = binding {
            out.push((*location, is_integer(module, ty), *interpolation));
        }
    };
    for arg in &ep.function.arguments {
        if let TypeInner::Struct { members, .. } = &module.types[arg.ty].inner {
            for m in members {
                visit(&m.binding, m.ty);
            }
        } else {
            visit(&arg.binding, arg.ty);
        }
    }
    out
}

#[test]
fn descriptors_respect_downlevel_limits() {
    for d in pipeline_descs() {
        let k = d.kind;
        if let Some(l) = &d.layout {
            assert!(l.array_stride <= 255, "{k:?}: stride {}", l.array_stride);
            assert!(l.attributes.len() <= 16, "{k:?}: {} attributes", l.attributes.len());
            let locs: HashSet<u32> = l.attributes.iter().map(|a| a.shader_location).collect();
            assert_eq!(locs.len(), l.attributes.len(), "{k:?}: duplicate shader locations");
        }
        let module = naga::front::wgsl::parse_str(source(&d)).unwrap_or_else(|e| panic!("{k:?}: {e}"));
        let vs = module.entry_points.iter().find(|e| e.name == d.vs && e.stage == ShaderStage::Vertex);
        let fs = module.entry_points.iter().find(|e| e.name == d.fs && e.stage == ShaderStage::Fragment);
        assert!(vs.is_some(), "{k:?}: vertex entry {} missing", d.vs);
        let fs = fs.unwrap_or_else(|| panic!("{k:?}: fragment entry {} missing", d.fs));
        let inputs = fragment_inputs(&module, fs);
        assert!(inputs.len() <= 15, "{k:?}: {} inter-stage locations", inputs.len());
        for (loc, integer, interp) in inputs {
            if integer {
                assert_eq!(
                    interp,
                    Some(Interpolation::Flat),
                    "{k:?}: integer input @location({loc}) must be flat"
                );
            }
        }
        let storage =
            module.global_variables.iter().any(|(_, g)| matches!(g.space, AddressSpace::Storage { .. }));
        assert!(!storage, "{k:?}: storage globals are not downlevel-safe");
    }
}

#[test]
fn table_matches_doc_02_section_9() {
    use wgpu::PrimitiveTopology::{TriangleList, TriangleStrip};
    use wgpu::VertexStepMode::{Instance, Vertex};
    // Copied from 02 §9 (update both together).
    let doc = [
        (PipelineKind::Solid, TriangleList, Some((24, Vertex))),
        (PipelineKind::Sdf, TriangleStrip, Some((80, Instance))),
        (PipelineKind::Glyph, TriangleStrip, Some((64, Instance))),
        (PipelineKind::Image, TriangleStrip, Some((64, Instance))),
        (PipelineKind::Shadow, TriangleStrip, Some((48, Instance))),
        (PipelineKind::CompositeOver, TriangleStrip, None),
    ];
    let code: Vec<_> = pipeline_descs()
        .into_iter()
        .map(|d| (d.kind, d.topology, d.layout.map(|l| (l.array_stride, l.step_mode))))
        .collect();
    assert_eq!(code, doc);
    // Group 1 per §9: glyph atlas, image texture, composite layer.
    let group1: Vec<_> = pipeline_descs().into_iter().filter(|d| d.group1).map(|d| d.kind).collect();
    assert_eq!(group1, [PipelineKind::Glyph, PipelineKind::Image, PipelineKind::CompositeOver]);
}
