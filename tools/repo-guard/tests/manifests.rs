//! Manifest invariants of documentations/wgpu-migration/04 §2 (Phase 7 end state): one renderer,
//! no backend features. Never loosen a test to make it pass.
use repo_guard::{dependency_keys, feature_keys, member_manifests, read};

/// Former render-backend feature names. None may come back (DR-11): there is one renderer.
/// Spelled with `concat!` so this guard's own source never contains the deleted name.
const BACKEND_NAMES: [&str; 3] = ["wgpu", concat!("vel", "lo-hybrid"), concat!("vel", "lo")];

#[test]
fn guard_parses_the_real_manifests() {
    assert!(feature_keys(&read("crates/pebbles/Cargo.toml")).iter().any(|k| k == "tokio"));
    assert!(dependency_keys(&read("crates/pebbles-shell/Cargo.toml")).iter().any(|k| k == "winit"));
    assert!(member_manifests().iter().any(|m| m == "examples/counter/Cargo.toml"));
}

#[test]
fn no_backend_feature_anywhere() {
    for m in member_manifests() {
        for k in feature_keys(&read(&m)) {
            assert!(!BACKEND_NAMES.contains(&k.as_str()), "{m} still declares `{k}` (DR-11)");
        }
    }
    let tmpl = read("crates/pebbles-cli/templates/app/Cargo.toml.tmpl");
    assert!(!tmpl.contains("[features]"), "app template declares no features");
}

#[test]
fn internal_workspace_deps_keep_default_features() {
    for line in read("Cargo.toml").lines().filter(|l| l.trim_start().starts_with("pebbles")) {
        assert!(!line.contains("default-features = false"), "{line}");
    }
}

/// X02: no `cfg` on a former backend feature in any source file of the framework or examples.
#[test]
fn no_backend_cfg_anywhere() {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() && p.file_name().is_some_and(|n| n != "target") {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    for d in ["crates", "examples"] {
        walk(&repo_guard::workspace_root().join(d), &mut files);
    }
    assert!(files.len() > 100, "walked the sources");
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for n in BACKEND_NAMES {
            assert!(!text.contains(&format!("feature = \"{n}\"")), "{}: cfg on feature `{n}`", f.display());
        }
    }
}

#[test]
fn no_manifest_names_fontdb_directly() {
    assert!(!dependency_keys(&read("Cargo.toml")).iter().any(|k| k == "fontdb"), "workspace");
    for m in member_manifests() {
        assert!(!dependency_keys(&read(&m)).iter().any(|k| k == "fontdb"), "{m}: use cosmic_text::fontdb");
    }
}

#[test]
fn renderer_only_deps_live_in_pebbles_gpu() {
    for dep in ["lyon", "lyon_tessellation", "swash", "zeno", "skrifa", "guillotiere", "bytemuck"] {
        for m in member_manifests() {
            if m == "crates/pebbles-gpu/Cargo.toml" || m.starts_with("tools/") {
                continue;
            }
            assert!(!dependency_keys(&read(&m)).iter().any(|k| k == dep), "{dep} must not be in {m}");
        }
    }
}

#[test]
fn cosmic_text_lives_only_in_pebbles_render() {
    for m in member_manifests() {
        if m == "crates/pebbles-render/Cargo.toml" {
            continue;
        }
        assert!(!dependency_keys(&read(&m)).iter().any(|k| k == "cosmic-text"), "{m}");
    }
}
