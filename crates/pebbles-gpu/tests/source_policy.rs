//! Source-policy tests for pebbles-gpu (11 §3.5, §4.3; S42, S51): static shaders, no
//! hand-written `unsafe`, every WGSL file compiled in, every wgpu descriptor labeled.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, ext: &str, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, ext, out);
        } else if p.extension().is_some_and(|e| e == ext) {
            let src = std::fs::read_to_string(&p).unwrap_or_default();
            out.push((p, src));
        }
    }
}

fn src_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
}

/// Every `.rs` file under `src/`.
fn rust_sources() -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    walk(&src_dir(), "rs", &mut out);
    out
}

/// `(line_no, code before "//")` of `src`.
fn code_lines(src: &str) -> impl Iterator<Item = (usize, &str)> {
    src.lines().enumerate().map(|(i, l)| (i + 1, l.split("//").next().unwrap_or("")))
}

fn violations(pred: impl Fn(&str) -> bool) -> Vec<String> {
    let mut v = Vec::new();
    for (path, src) in rust_sources() {
        for (n, code) in code_lines(&src) {
            if pred(code) {
                v.push(format!("{}:{n}: {}", path.display(), code.trim()));
            }
        }
    }
    v
}

#[test]
fn no_runtime_wgsl() {
    let v = violations(|c| {
        c.contains("Wgsl(format!") || c.contains("Wgsl(String") || c.contains("Wgsl(Cow::Owned")
    });
    assert!(v.is_empty(), "runtime-built WGSL:\n{}", v.join("\n"));
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn has_word(code: &str, word: &str) -> bool {
    code.match_indices(word).any(|(i, _)| {
        let before = code[..i].chars().next_back().is_none_or(|c| !is_ident(c));
        let after = code[i + word.len()..].chars().next().is_none_or(|c| !is_ident(c));
        before && after
    })
}

/// The only sanctioned `unsafe`: M18's test-only counting allocator (`#[cfg(test)]` module, a
/// `GlobalAlloc` impl that forwards to `System`; DR-18 candidate, 06 §11).
const UNSAFE_ALLOWED: &[&str] = &["src/test_alloc.rs"];

#[test]
fn no_handwritten_unsafe() {
    // `unsafe_code` in lint attributes is an identifier, not the keyword.
    let v: Vec<String> = violations(|c| has_word(c, "unsafe"))
        .into_iter()
        .filter(|l| !UNSAFE_ALLOWED.iter().any(|f| l.replace('\\', "/").contains(f)))
        .collect();
    assert!(v.is_empty(), "hand-written unsafe:\n{}", v.join("\n"));
}

#[test]
fn sanctioned_unsafe_is_test_only() {
    // A Windows checkout may have CRLF line endings.
    let lib = std::fs::read_to_string(src_dir().join("lib.rs")).unwrap_or_default().replace("\r\n", "\n");
    assert!(lib.contains("#[cfg(test)]\nmod test_alloc;"), "test_alloc must stay #[cfg(test)]");
}

#[test]
fn every_wgsl_file_is_included() {
    let mut wgsl = Vec::new();
    walk(&src_dir(), "wgsl", &mut wgsl);
    assert!(!wgsl.is_empty());
    let rust = rust_sources();
    for (path, _) in wgsl {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let included = rust.iter().any(|(_, s)| {
            s.lines().any(|l| l.contains("include_str!(") && l.contains(name))
                || s.contains(&format!("shader!(\"{name}\")"))
                || s.contains(&format!("shader!(\"{name}\", brush)"))
        });
        assert!(included, "{name} is not compiled into any pipeline");
    }
}

#[test]
fn no_unlabeled_descriptors() {
    let v = violations(|c| c.contains("label: None"));
    assert!(v.is_empty(), "unlabeled descriptors:\n{}", v.join("\n"));
    let bad = violations(|c| {
        c.match_indices("label: Some(\"").any(|(i, m)| !c[i + m.len()..].starts_with("pebbles.gpu."))
    });
    assert!(bad.is_empty(), "labels must start with pebbles.gpu.:\n{}", bad.join("\n"));
}
