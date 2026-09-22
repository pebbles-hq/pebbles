//! Scene-graph golden tests (§8): a deterministic, headless fingerprint of the
//! rendered render tree — each node's `debug_name` + integer-rounded window rect,
//! indented by depth — diffed against a committed golden. Catches "wrong widget
//! painted / missing nodes / design language silently changed the geometry" without
//! a GPU (structural + geometric, not pixels). The three `DesignLanguage` fixtures
//! are the point: Compact (radius 0, dense padding) vs Tailwind vs Material produce
//! visibly different geometry, so a token regression flips a golden.
//!
//! Regenerate after an intended layout change: `PEBBLES_UPDATE_GOLDENS=1 cargo test
//! -p pebbles-widgets --test suite goldens`.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::{RenderId, RenderTree, TextEnv};
use pebbles_testing::draw_frame;
use pebbles_widgets::theme::{DesignLanguage, Theme, set_design, set_theme};
use pebbles_widgets::{OverlayHost, View, button, dialog, tabs, text};

/// A canonical string fingerprint of the laid-out render tree.
fn fingerprint(tree: &RenderTree) -> String {
    let mut out = String::new();
    if let Some(root) = tree.root {
        walk(tree, root, 0, &mut out);
    }
    out
}

fn walk(tree: &RenderTree, id: RenderId, depth: usize, out: &mut String) {
    let off = tree.absolute_offset(id);
    let sz = tree.size_of(id);
    out.push_str(&format!(
        "{:indent$}{} @{},{} {}x{}\n",
        "",
        tree.debug_name(id),
        off.x.round() as i64,
        off.y.round() as i64,
        sz.width.round() as i64,
        sz.height.round() as i64,
        indent = depth * 2,
    ));
    for &child in tree.children_of(id) {
        walk(tree, child, depth + 1, out);
    }
}

/// Mount `root`, lay out + settle at `win`, and return the tree fingerprint.
fn render_fingerprint<W: IntoWidget + 'static>(win: Size, root: fn() -> W) -> String {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    for _ in 0..2 {
        draw_frame(&mut ui, &mut env, win);
    }
    fingerprint(ui.render_tree())
}

/// Compare `actual` to `tests/suite/goldens/<name>.txt`, or write it when
/// `PEBBLES_UPDATE_GOLDENS` is set.
fn check_golden(name: &str, actual: &str) {
    let dir = format!("{}/tests/suite/goldens", env!("CARGO_MANIFEST_DIR"));
    let path = format!("{dir}/{name}.txt");
    if std::env::var("PEBBLES_UPDATE_GOLDENS").is_ok() {
        std::fs::create_dir_all(&dir).expect("create goldens dir");
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden {path} — regen with PEBBLES_UPDATE_GOLDENS=1"));
    assert_eq!(
        actual.trim_end(),
        expected.trim_end(),
        "scene-graph golden '{name}' changed — if the layout change is intended, regenerate with \
         PEBBLES_UPDATE_GOLDENS=1"
    );
}

fn a_button() -> impl IntoWidget {
    button("Save").on_pressed(|| {})
}

#[test]
fn golden_button() {
    set_theme(Theme::light());
    check_golden("button", &render_fingerprint(Size::new(200.0, 80.0), a_button));
}

#[test]
fn golden_tabs() {
    set_theme(Theme::light());
    let tree = render_fingerprint(Size::new(400.0, 200.0), || {
        tabs(0).tab("A", text("a"), || {}).tab("B", text("b"), || {})
    });
    check_golden("tabs", &tree);
}

fn dialog_over_page() -> impl IntoWidget {
    OverlayHost::wrap(button("Behind"))
}

#[test]
fn golden_dialog_over_overlay() {
    pebbles_widgets::overlay::init();
    pebbles_widgets::dialog::init();
    set_theme(Theme::light());
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(dialog_over_page)).into_widget());
    ui.layout(&mut env, win);
    dialog(button("OK")).open();
    for _ in 0..2 {
        draw_frame(&mut ui, &mut env, win);
    }
    check_golden("dialog_over_overlay", &fingerprint(ui.render_tree()));
}

// The design-language fixtures: the SAME button under each language must produce a
// DIFFERENT golden (Compact is square + dense, Material is rounded + roomier), which
// is what makes a token regression cheap to catch.

#[test]
fn golden_design_compact() {
    set_theme(Theme::light());
    set_design(DesignLanguage::Compact);
    check_golden("design_compact", &render_fingerprint(Size::new(200.0, 80.0), a_button));
}

#[test]
fn golden_design_tailwind() {
    set_theme(Theme::light());
    set_design(DesignLanguage::Tailwind);
    check_golden("design_tailwind", &render_fingerprint(Size::new(200.0, 80.0), a_button));
}

#[test]
fn golden_design_material() {
    set_theme(Theme::light());
    set_design(DesignLanguage::Material);
    check_golden("design_material", &render_fingerprint(Size::new(200.0, 80.0), a_button));
}
