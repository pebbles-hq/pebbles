//! B12 / D-12: the renderer never scans system fonts (no cosmic-text, DR-03, DR-07).

fn rust_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn renderer_never_loads_system_fonts() {
    let mut files = Vec::new();
    rust_files(std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")), &mut files);
    assert!(!files.is_empty());
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read source");
        for needle in ["load_system_fonts", "FontSystem"] {
            assert!(!text.contains(needle), "{} contains `{needle}`", file.display());
        }
    }
}
