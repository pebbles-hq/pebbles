//! The Markdown reader/editor (feature `markdown`): source-level task toggling,
//! live re-render on bound-source edits, and the editor's mode switching —
//! headless through a real Ui.

use pebbles_core::{IntoWidget, Ui, component, create_signal};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{MarkdownMode, View, markdown, markdown_editor, toggle_task};

// ---------------------------------------------------------------------------
// toggle_task — the Obsidian source rewrite
// ---------------------------------------------------------------------------

#[test]
fn toggle_task_flips_the_right_checkbox_in_source() {
    let src = "# t\n\n- [ ] first\n- [x] second\n  - [ ] nested\n\n1. [ ] ordered\n";
    // Ordinal 0: check "first".
    let s = toggle_task(src, 0).expect("first task");
    assert!(s.contains("- [x] first"), "{s}");
    // Ordinal 1: uncheck "second".
    let s = toggle_task(src, 1).expect("second task");
    assert!(s.contains("- [ ] second"), "{s}");
    // Ordinal 2: the nested one keeps its indentation.
    let s = toggle_task(src, 2).expect("nested task");
    assert!(s.contains("  - [x] nested"), "{s}");
    // Ordinal 3: ordered-list tasks work too.
    let s = toggle_task(src, 3).expect("ordered task");
    assert!(s.contains("1. [x] ordered"), "{s}");
    // Out of range → None; non-task bullets are not counted.
    assert!(toggle_task(src, 4).is_none());
    assert!(toggle_task("- plain bullet\n", 0).is_none());
    // Uppercase X unchecks as well.
    assert_eq!(toggle_task("- [X] done", 0).as_deref(), Some("- [ ] done"));
}

// ---------------------------------------------------------------------------
// Rendering: a bound source re-renders live
// ---------------------------------------------------------------------------

thread_local! {
    static SRC: std::cell::RefCell<Option<pebbles_core::Signal<String>>> =
        const { std::cell::RefCell::new(None) };
    static MODE: std::cell::RefCell<Option<pebbles_core::Signal<MarkdownMode>>> =
        const { std::cell::RefCell::new(None) };
}

const DOC: &str = "# Title\n\nSome **bold** and a [link](https://x.y).\n\n- [ ] task\n\n```rust\nfn x() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n> quoted\n";

fn reader_root() -> impl IntoWidget {
    let src = create_signal(DOC.to_string());
    SRC.with(|c| *c.borrow_mut() = Some(src));
    markdown("").bind(src)
}

#[test]
fn bound_markdown_renders_and_rerenders_on_source_and_task_toggle() {
    pebbles_widgets::theme::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(600.0, 800.0);
    ui.mount_root(View::new(palette::WHITE, component(reader_root)).into_widget());
    ui.layout(&mut env, win);
    let before = ui.element_count();
    assert!(before > 20, "the GFM document produced a real tree ({before} elements)");

    // Append a heading → the view re-renders with more elements.
    let src = SRC.with(|c| c.borrow().expect("src"));
    src.update(|s| s.push_str("\n## More\n\nwords here\n"));
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    assert!(ui.element_count() > before, "live edit re-rendered the view");

    // A source-level task toggle round-trips through the same signal.
    let toggled = toggle_task(&src.peek(), 0).expect("task present");
    assert!(toggled.contains("- [x] task"));
    src.set(toggled);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
}

fn editor_root() -> impl IntoWidget {
    let src = create_signal(DOC.to_string());
    let mode = create_signal(MarkdownMode::Edit);
    SRC.with(|c| *c.borrow_mut() = Some(src));
    MODE.with(|c| *c.borrow_mut() = Some(mode));
    markdown_editor(src).mode_signal(mode).lines(8)
}

#[test]
fn editor_switches_modes_via_the_external_signal() {
    pebbles_widgets::theme::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(700.0, 600.0);
    ui.mount_root(View::new(palette::WHITE, component(editor_root)).into_widget());
    ui.layout(&mut env, win);
    let edit_count = ui.element_count();

    let mode = MODE.with(|c| c.borrow().expect("mode"));
    mode.set(MarkdownMode::Read);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let read_count = ui.element_count();
    assert_ne!(edit_count, read_count, "Read renders the document, not the source pane");

    mode.set(MarkdownMode::Split);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    assert!(ui.element_count() > read_count, "Split shows both panes");
}

// ---------------------------------------------------------------------------
// Word spacing: parley trims trailing whitespace from a line's width, so word
// chunks must be separated by the wrap's horizontal spacing — not a (collapsed)
// trailing space — or the text jams together ("alphabetagamma").
// ---------------------------------------------------------------------------

#[test]
fn words_in_a_paragraph_are_spaced_not_jammed() {
    use pebbles_render::RenderParagraph;
    pebbles_widgets::theme::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(palette::WHITE, component(|| markdown("alpha beta gamma"))).into_widget(),
    );
    ui.layout(&mut env, Size::new(600.0, 400.0));
    let tree = ui.render_tree();
    let mut words: Vec<(f64, f64)> = tree
        .find_all::<RenderParagraph>()
        .into_iter()
        .map(|id| (tree.absolute_offset(id).x, tree.size_of(id).width))
        .collect();
    words.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    assert!(words.len() >= 3, "three words → three chunks, got {}", words.len());
    for pair in words.windows(2) {
        let (x0, w0) = pair[0];
        let (x1, _) = pair[1];
        let gap = x1 - (x0 + w0);
        assert!(gap > 1.0, "words jammed together: only {gap:.1}px between chunks");
    }
}
