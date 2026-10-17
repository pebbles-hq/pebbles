//! Documentation invariants of the wgpu migration (04, 12).
use repo_guard::{read, workspace_root};

/// Public docs must not advertise the deleted classic `vello` backend (C02/K09).
#[test]
fn public_docs_do_not_advertise_classic_vello() {
    let files = [
        "README.md",
        "ARCHITECTURE.md",
        "DISTRIBUTING.md",
        "PLATFORMS.md",
        "WIDGETS.md",
        "crates/pebbles-render/README.md",
        "crates/pebbles-shell/README.md",
    ];
    for f in files {
        let text = read(f);
        assert!(!text.contains("--renderer vello"), "{f}");
        assert!(!text.contains("GPU compute") && !text.contains("GPU-compute"), "{f}");
        assert!(!text.contains("classic Vello") && !text.contains("classic\n**Vello**"), "{f}");
        for (i, _) in text.match_indices("features vello") {
            let next = text[i + "features vello".len()..].chars().next();
            assert!(matches!(next, Some('-') | Some('_')), "{f}: `--features vello` advertised");
        }
    }
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
