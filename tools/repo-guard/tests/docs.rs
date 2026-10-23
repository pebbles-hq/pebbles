//! Documentation invariants of the wgpu migration (04, 12).
use repo_guard::{read, workspace_root};

/// X06 zero trace: no file in the repository names the removed renderer or text stack.
/// Exempt: `CHANGELOG.md` (it records what past releases shipped) and the license attributions
/// of code ported from that project, which Apache-2.0 requires us to keep (`ATTRIBUTIONS`).
/// Names are spelled with `concat!` so this guard's own source never matches.
#[test]
fn no_trace_of_the_removed_renderer_or_text_stack() {
    const ATTRIBUTIONS: [&str; 4] = [
        "NOTICE",
        "crates/pebbles-gpu/src/prepare/gradient.rs",
        "crates/pebbles-gpu/src/prepare/shadows.rs",
        "crates/pebbles-gpu/src/shaders/gradient.wgsl",
    ];
    const BANNED: [&str; 4] =
        [concat!("vel", "lo"), concat!("par", "ley"), concat!("gli", "fo"), concat!("font", "ique")];
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                if !matches!(name.as_ref(), "target" | ".git" | "corpus" | "artifacts" | "dist") {
                    walk(&p, out);
                }
            } else if name != "CHANGELOG.md" {
                out.push(p);
            }
        }
    }
    let root = workspace_root();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let mut hits = Vec::new();
    for f in files {
        let rel = f.strip_prefix(&root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
        if ATTRIBUTIONS.contains(&rel.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else { continue }; // binary files
        let lower = text.to_lowercase();
        for b in BANNED {
            if let Some(i) = lower.find(b) {
                let line = lower[..i].matches('\n').count() + 1;
                hits.push(format!("{rel}:{line}: `{b}`"));
            }
        }
    }
    assert!(hits.is_empty(), "traces of the removed stack:\n{}", hits.join("\n"));
}

#[test]
fn crate_readmes_state_eleven_crates() {
    for e in std::fs::read_dir(workspace_root().join("crates")).expect("crates").flatten() {
        let p = e.path().join("README.md");
        if let Ok(text) = std::fs::read_to_string(&p) {
            assert!(!text.contains("10-crate"), "{} still says 10-crate", p.display());
        }
    }
    assert!(read("README.md").contains("crates/pebbles-gpu"), "README crate table lists pebbles-gpu");
}
