//! The bundled-font attribution (K06).
use repo_guard::{read, workspace_root};

#[test]
fn notice_attributes_every_bundled_font() {
    let notice = read("NOTICE");
    assert!(notice.contains("SIL Open Font"), "NOTICE must name the OFL");
    let dir = workspace_root().join("crates/pebbles-render/assets/fonts");
    for e in std::fs::read_dir(dir).expect("fonts dir").flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.ends_with(".ttf") || name.ends_with("-OFL.txt") {
            assert!(notice.contains(&name), "NOTICE does not mention {name}");
        }
    }
}
