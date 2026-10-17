//! Manifest invariants of documentations/wgpu-migration/04 §2. Phase cards flip the
//! constants below; never loosen a test to make it pass.
use repo_guard::{dependency_keys, feature_keys, features, member_manifests, read};

/// The six backend-selectable framework crates (DR-10/DR-11).
const BACKEND_CRATES: [&str; 6] = [
    "crates/pebbles-render/Cargo.toml",
    "crates/pebbles-core/Cargo.toml",
    "crates/pebbles-widgets/Cargo.toml",
    "crates/pebbles-testing/Cargo.toml",
    "crates/pebbles-shell/Cargo.toml",
    "crates/pebbles/Cargo.toml",
];
/// Backend names that can appear as feature keys during the migration.
const BACKEND_NAMES: [&str; 3] = ["wgpu", "vello-hybrid", "vello"];

/// Phase C..5: Some("vello-hybrid"). K20 (Phase 6): Some("wgpu"). K30 (Phase 7): None.
const EXPECTED_DEFAULT_BACKEND: Option<&str> = Some("vello-hybrid");
/// Backends every BACKEND_CRATES manifest must declare (I1). K30 sets &[].
const TRANSITION_BACKENDS: &[&str] = &["vello-hybrid", "wgpu"];
/// Backend feature keys every examples/* manifest must declare, exactly.
/// K23 (Phase 6): &["wgpu"]. K30: &[].
const EXAMPLE_BACKENDS: &[&str] = &["vello-hybrid", "wgpu"];

#[test]
fn guard_parses_the_real_manifests() {
    assert!(feature_keys(&read("crates/pebbles/Cargo.toml")).iter().any(|k| k == "tokio"));
    assert!(dependency_keys(&read("crates/pebbles-shell/Cargo.toml")).iter().any(|k| k == "winit"));
    assert!(member_manifests().iter().any(|m| m == "examples/counter/Cargo.toml"));
}

#[test]
fn backend_crates_default_to_the_expected_backend() {
    for m in BACKEND_CRATES {
        let feats = features(&read(m));
        let default = feats.iter().find(|(k, _)| k == "default").map(|(_, v)| v.clone());
        match EXPECTED_DEFAULT_BACKEND {
            Some(b) => {
                let d = default.unwrap_or_else(|| panic!("{m}: no default"));
                assert!(d.contains(&format!("\"{b}\"")), "{m}: default = {d}, want {b}");
                for other in BACKEND_NAMES.iter().filter(|n| **n != b) {
                    assert!(!d.contains(&format!("\"{other}\"")), "{m}: default also selects {other}");
                }
            }
            None => {
                if let Some(d) = default {
                    for n in BACKEND_NAMES {
                        assert!(!d.contains(&format!("\"{n}\"")), "{m}: default selects {n}");
                    }
                }
            }
        }
    }
}

#[test]
fn backend_crates_declare_every_transition_backend() {
    for m in BACKEND_CRATES {
        let keys = feature_keys(&read(m));
        for b in TRANSITION_BACKENDS {
            assert!(keys.iter().any(|k| k == b), "{m} lacks feature `{b}` (invariant I1)");
        }
    }
}

#[test]
fn examples_declare_exactly_the_expected_backends() {
    for m in member_manifests().into_iter().filter(|m| m.starts_with("examples/")) {
        let mut got: Vec<String> =
            feature_keys(&read(&m)).into_iter().filter(|k| BACKEND_NAMES.contains(&k.as_str())).collect();
        got.sort();
        let mut want: Vec<String> = EXAMPLE_BACKENDS.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(got, want, "{m}");
    }
}

#[test]
fn no_manifest_declares_the_classic_vello_feature() {
    for m in member_manifests() {
        assert!(!feature_keys(&read(&m)).iter().any(|k| k == "vello"), "{m} still has `vello` (C02)");
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
