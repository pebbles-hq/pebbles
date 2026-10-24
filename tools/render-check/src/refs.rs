//! Committed self-goldens (§8.3–8.4): loading for `--reference dir` and the size budget that
//! keeps `refs/` plain-git sized (§9.3; no LFS).

use std::path::Path;

use crate::CheckError;
use crate::case::Case;
use crate::metric::Rgba8;
use crate::png_io;

/// The whole `refs/` tree.
pub const REFS_BUDGET_TOTAL: u64 = 15 * 1024 * 1024;
/// One reference PNG.
pub const REFS_BUDGET_PER_FILE: u64 = 600 * 1024;

/// The committed self-golden of `case` in `dir`; [`CheckError::MissingRef`] when absent.
pub fn load_reference(dir: &Path, case: &Case) -> Result<Rgba8, CheckError> {
    let path = dir.join(format!("{}.png", case.file_stem()));
    if !path.is_file() {
        return Err(CheckError::MissingRef(path.display().to_string()));
    }
    png_io::decode(&path)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Every file under `root`, recursively (empty when `root` is absent).
    fn walk(root: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }

    const REFS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/refs");

    #[test]
    fn refs_size_budget() {
        let mut sizes: Vec<(u64, PathBuf)> = Vec::new();
        for path in walk(Path::new(REFS)) {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            assert!(
                matches!(name, "README.txt" | "REVIEW.md") || name.ends_with(".png"),
                "refs/ holds only PNGs, README.txt and REVIEW.md: {}",
                path.display()
            );
            if name.ends_with(".png") {
                sizes.push((std::fs::metadata(&path).expect("stat").len(), path));
            }
        }
        sizes.sort_by_key(|s| std::cmp::Reverse(s.0));
        let largest: Vec<String> =
            sizes.iter().take(5).map(|(n, p)| format!("{} ({n} B)", p.display())).collect();
        let total: u64 = sizes.iter().map(|(n, _)| n).sum();
        assert!(
            sizes.first().is_none_or(|(n, _)| *n <= REFS_BUDGET_PER_FILE),
            "a reference exceeds {REFS_BUDGET_PER_FILE} B; largest: {largest:?}"
        );
        assert!(
            total <= REFS_BUDGET_TOTAL,
            "refs/ is {total} B > {REFS_BUDGET_TOTAL} B; largest: {largest:?}"
        );
    }

    #[test]
    fn refs_dir_has_no_orphans() {
        let dir = Path::new(REFS).join("wgpu/linux-lavapipe");
        if !dir.is_dir() {
            return; // V12 commits the self-goldens.
        }
        let cases: Vec<String> =
            crate::corpus::all().iter().filter(|c| c.in_refs).map(Case::file_stem).collect();
        let stems: Vec<String> = walk(&dir)
            .iter()
            .filter(|p| p.extension().is_some_and(|e| e == "png"))
            .filter_map(|p| p.file_stem().and_then(|s| s.to_str()).map(str::to_owned))
            .collect();
        let orphans: Vec<&String> = stems.iter().filter(|s| !cases.contains(s)).collect();
        let missing: Vec<&String> = cases.iter().filter(|c| !stems.contains(c)).collect();
        assert!(orphans.is_empty(), "refs without an in_refs case: {orphans:?}");
        assert!(missing.is_empty(), "in_refs cases without a ref: {missing:?}");
    }

    #[test]
    fn missing_reference_is_typed() {
        let case = crate::corpus::all().into_iter().find(|c| c.in_refs).expect("an in_refs case");
        let err = load_reference(Path::new("/nonexistent"), &case).expect_err("no dir");
        assert!(matches!(err, CheckError::MissingRef(_)), "{err}");
    }
}
