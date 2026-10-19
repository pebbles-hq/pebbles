//! V02: every shader validates with naga, translates to GLSL ES 300 for WebGL2, and stays within
//! the WebGL2 profile (no storage, no compute, ≤ 15 inter-stage varyings, ≤ 4 bind groups).

use super::ALL;

fn parse_and_validate(name: &str, src: &str) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(src)
        .unwrap_or_else(|e| panic!("{name}: parse error\n{}", e.emit_to_string(src)));
    let info =
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}: validation error\n{}", e.emit_to_string(src)));
    (module, info)
}

#[test]
fn every_shader_validates_with_naga() {
    for (name, src) in ALL {
        parse_and_validate(name, src);
    }
}

#[test]
fn every_entry_point_translates_to_glsl_es_300_webgl() {
    for (name, src) in ALL {
        let (module, info) = parse_and_validate(name, src);
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
            .unwrap_or_else(|e| panic!("{name}::{}: {e:?}", ep.name));
        }
    }
}

#[test]
fn no_shader_uses_storage_or_compute() {
    for (name, src) in ALL {
        let (module, _) = parse_and_validate(name, src);
        for (_, g) in module.global_variables.iter() {
            assert!(!matches!(g.space, naga::AddressSpace::Storage { .. }), "{name}: storage buffer");
        }
        for (_, ty) in module.types.iter() {
            assert!(
                !matches!(ty.inner, naga::TypeInner::Image { class: naga::ImageClass::Storage { .. }, .. }),
                "{name}: storage texture"
            );
        }
        assert!(
            module.entry_points.iter().all(|ep| ep.stage != naga::ShaderStage::Compute),
            "{name}: compute"
        );
    }
}

#[test]
fn inter_stage_varyings_within_webgl2_limit() {
    let max = wgpu::Limits::downlevel_webgl2_defaults().max_inter_stage_shader_variables;
    for (name, src) in ALL {
        let (module, _) = parse_and_validate(name, src);
        for ep in module.entry_points.iter().filter(|ep| ep.stage == naga::ShaderStage::Vertex) {
            let Some(result) = &ep.function.result else { continue };
            let count = match &module.types[result.ty].inner {
                naga::TypeInner::Struct { members, .. } => members
                    .iter()
                    .filter(|m| matches!(m.binding, Some(naga::Binding::Location { .. })))
                    .count(),
                _ => usize::from(matches!(result.binding, Some(naga::Binding::Location { .. }))),
            };
            assert!(
                u32::try_from(count).is_ok_and(|c| c <= max),
                "{name}::{}: {count} varyings > {max}",
                ep.name
            );
        }
    }
}

#[test]
fn bind_groups_within_webgl2_limit() {
    let max = wgpu::Limits::downlevel_webgl2_defaults().max_bind_groups;
    for (name, src) in ALL {
        let (module, _) = parse_and_validate(name, src);
        for (_, g) in module.global_variables.iter() {
            if let Some(b) = &g.binding {
                assert!(b.group < max, "{name}: group {} >= {max}", b.group);
            }
        }
    }
}

#[test]
fn shader_table_is_complete() {
    // Included into other shaders (the prelude; the brush coverage variants, 09 §6).
    const PRELUDES: &[&str] = &["prelude", "brush_cover"];
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/shaders");
    for e in std::fs::read_dir(dir).expect("shaders dir").flatten() {
        let path = e.path();
        if path.extension().is_some_and(|x| x == "wgsl") {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_owned();
            assert!(
                PRELUDES.contains(&stem.as_str()) || ALL.iter().any(|(n, _)| *n == stem),
                "{stem}.wgsl not in ALL"
            );
        }
    }
}

/// R13: every fragment entry point returns through `finish_color(..)` — except the composites
/// and the blit, whose texels already passed through it when they were drawn.
#[test]
fn every_fragment_calls_finish_color() {
    for (name, src) in ALL {
        let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{name}: {e}"));
        for ep in module.entry_points.iter().filter(|e| e.stage == naga::ShaderStage::Fragment) {
            // Composites, the blit and effect passes move already-finished texels.
            if ["fs_over", "fs_compose", "fs_backdrop", "fs_blit"].contains(&ep.name.as_str())
                || *name == "effects"
            {
                continue;
            }
            let start =
                src.find(&format!("fn {}(", ep.name)).unwrap_or_else(|| panic!("{name}: {}", ep.name));
            let body = &src[start..];
            let end = body.find("\n}").unwrap_or(body.len());
            assert!(body[..end].contains("finish_color("), "{name}::{} does not call finish_color", ep.name);
        }
    }
}
