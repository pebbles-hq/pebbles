//! [`FileExplorer`]: the model mutates correctly (insert/rename/delete/move with
//! the descendant + file-target guards), and the UI drives it — New File creates
//! + starts the inline rename, custom buttons delete, and drag-pan onto a folder
//! moves the node.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Ui, component, create_signal};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    FileTree, FsKind, OverlayHost, View, button, column, file_explorer,
};

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

#[test]
fn model_insert_rename_delete_move() {
    let mut t = FileTree::new();
    let a = t.insert(None, FsKind::Folder, "a");
    let b = t.insert(None, FsKind::Folder, "b");
    let f = t.insert(Some(a), FsKind::File, "f.txt");
    let child = t.insert(Some(a), FsKind::Folder, "child");

    assert_eq!(t.node(f).map(|n| n.name.as_str()), Some("f.txt"));
    assert_eq!(t.parent_of(f), Some(a));
    assert!(t.rename(f, "g.txt"));
    assert_eq!(t.node(f).map(|n| n.name.as_str()), Some("g.txt"));
    assert!(!t.rename(f, "   "), "blank renames are rejected");

    // Move into another folder works; guardrails reject the rest.
    assert!(t.move_node(f, b), "move into a sibling folder");
    assert_eq!(t.parent_of(f), Some(b));
    assert!(!t.move_node(a, a), "moving into itself is rejected");
    assert!(!t.move_node(a, child), "moving into a descendant is rejected");
    assert!(!t.move_node(b, f), "moving into a file is rejected");

    // Delete removes the subtree.
    assert!(t.delete(a));
    assert!(t.node(a).is_none() && t.node(child).is_none());
    assert_eq!(t.node(f).map(|n| n.name.as_str()), Some("g.txt"), "sibling survives");
}

// ---------------------------------------------------------------------------
// UI: create + rename
// ---------------------------------------------------------------------------

thread_local! {
    static TREE: RefCell<Option<pebbles_core::Signal<FileTree>>> = const { RefCell::new(None) };
}

fn tree_sig() -> pebbles_core::Signal<FileTree> {
    TREE.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            let mut t = FileTree::new();
            let src = t.insert(None, FsKind::Folder, "src");
            t.insert(Some(src), FsKind::File, "main.rs");
            t.insert(None, FsKind::File, "README.md");
            *c = Some(create_signal(t));
        }
        c.unwrap()
    })
}

fn explorer() -> pebbles_widgets::FileExplorer {
    file_explorer(tree_sig())
}

fn root() -> impl IntoWidget {
    let ex = explorer();
    OverlayHost::wrap(
        column(vec![
            button("New file").on_pressed(ex.new_file()).into_widget(),
            button("Delete").on_pressed(ex.delete_selected()).into_widget(),
            ex.tree().into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
}

#[test]
fn new_file_creates_and_inline_rename_commits() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    let names = |t: &FileTree| t.root.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    let before = names(&tree_sig().peek());

    // Tap "New file" (first button, top-left).
    let p = Offset::new(30.0, 16.0);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    frame(&mut ui, &mut env, win);

    let after = names(&tree_sig().peek());
    assert_eq!(after.len(), before.len() + 1, "a new node lands in the root");
    assert!(after.contains(&"new_file.txt".to_string()), "with the default name");

    // The rename editor is live: type a name and press Enter.
    ui.dispatch_key(KeyInput::Insert("hello.rs".to_string()));
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui, &mut env, win);

    let after = names(&tree_sig().peek());
    assert!(after.contains(&"hello.rs".to_string()), "Enter commits the rename");
    assert!(!after.contains(&"new_file.txt".to_string()), "the default name is gone");
}

#[test]
fn select_then_custom_button_deletes() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Buttons are stacked (column): New file y 0..36, Delete y 36..72; the tree
    // starts at ~72. Collapsed rows: src (≈72..94), README.md (≈94..122).
    // Tap README.md to select it, then the Delete button.
    let readme = Offset::new(60.0, 105.0);
    ui.dispatch_pointer_down(readme);
    ui.dispatch_tap(readme);
    ui.dispatch_pointer_up(readme);
    frame(&mut ui, &mut env, win);

    let del = Offset::new(30.0, 54.0);
    ui.dispatch_pointer_down(del);
    ui.dispatch_tap(del);
    ui.dispatch_pointer_up(del);
    frame(&mut ui, &mut env, win);

    let names = tree_sig().peek().root.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    assert!(!names.contains(&"README.md".to_string()), "the selected file was deleted");
    assert!(names.contains(&"src".to_string()), "siblings survive");
}

#[test]
fn drag_onto_folder_moves() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Collapsed rows: src (≈72..94), README.md (≈94..122).
    // Pan-drag README.md onto src.
    let readme = Offset::new(60.0, 105.0);
    let src_row = Offset::new(60.0, 80.0);
    let target = ui.pan_target_at(readme).expect("a draggable row");
    ui.dispatch_pan_start(target, readme);
    ui.dispatch_pan_update(target, readme);
    ui.dispatch_hover(src_row); // hovering the folder marks the drop target
    frame(&mut ui, &mut env, win);
    ui.dispatch_pan_end(target, src_row);
    frame(&mut ui, &mut env, win);

    let t = tree_sig().peek();
    let src = t.root.iter().find(|n| n.name == "src").expect("src folder");
    assert!(
        src.children.iter().any(|n| n.name == "README.md"),
        "README.md moved into src"
    );
    assert!(!t.root.iter().any(|n| n.name == "README.md"), "and left the root");
}
