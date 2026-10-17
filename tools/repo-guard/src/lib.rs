//! Test-only helpers for `tests/*.rs`: read files of the pebbles workspace and parse the
//! few TOML shapes we check, line by line. Not a library for anyone else.
use std::path::{Path, PathBuf};

/// The pebbles workspace root (two directories above this crate).
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().expect("workspace root")
}

/// Read a file given relative to the workspace root.
pub fn read(rel: &str) -> String {
    std::fs::read_to_string(workspace_root().join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// `(key, value)` pairs of the `[features]` table. Multi-line arrays are joined;
/// `#` comments are dropped.
pub fn features(manifest: &str) -> Vec<(String, String)> {
    let mut in_features = false;
    let mut out = Vec::new();
    let mut lines = manifest.lines();
    while let Some(line) = lines.next() {
        let t = line.split('#').next().unwrap_or("").trim();
        // Table headers start with `[` (array continuation lines never do: they start
        // with `"` or `]`). Target tables contain `=` inside cfg(..), so do not test for it.
        if t.starts_with('[') {
            in_features = t == "[features]";
            continue;
        }
        if !in_features || t.is_empty() {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            let mut value = v.trim().to_string();
            while value.matches('[').count() > value.matches(']').count() {
                match lines.next() {
                    Some(l) => {
                        value.push(' ');
                        value.push_str(l.split('#').next().unwrap_or("").trim());
                    }
                    None => break,
                }
            }
            out.push((k.trim().to_string(), value));
        }
    }
    out
}

/// Feature keys only.
pub fn feature_keys(manifest: &str) -> Vec<String> {
    features(manifest).into_iter().map(|(k, _)| k).collect()
}

/// Dependency names declared in any `[...dependencies]` table (normal, dev, build,
/// target-specific, workspace). `foo.workspace = true` yields `foo`.
pub fn dependency_keys(manifest: &str) -> Vec<String> {
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in manifest.lines() {
        let t = line.split('#').next().unwrap_or("").trim();
        if t.starts_with('[') {
            in_deps = t.trim_end_matches(']').ends_with("dependencies");
            continue;
        }
        if !in_deps || t.is_empty() {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            let k = k.trim();
            let k = k.split('.').next().unwrap_or(k).trim_matches('"');
            if !k.is_empty() && !k.starts_with('"') {
                out.push(k.to_string());
            }
        }
    }
    out
}

/// Every `Cargo.toml` directly under `crates/*/`, `examples/*/`, `tools/*/`, as paths
/// relative to the workspace root, sorted.
pub fn member_manifests() -> Vec<String> {
    let root = workspace_root();
    let mut out = Vec::new();
    for dir in ["crates", "examples", "tools"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for e in entries.flatten() {
            if e.path().join("Cargo.toml").is_file() {
                out.push(format!("{dir}/{}/Cargo.toml", e.file_name().to_string_lossy()));
            }
        }
    }
    out.sort();
    out
}
