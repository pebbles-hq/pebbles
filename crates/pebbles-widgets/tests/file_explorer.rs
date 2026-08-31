//! [`FileExplorer`]: the model mutates correctly (insert/rename/delete/move with
//! the descendant + file-target guards), and the UI drives it — New File creates
//! + starts the inline rename, custom buttons delete, and drag-pan onto a folder
//! moves the node.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Ui, component, create_signal};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    Container, FileTree, FsKind, OverlayHost, View, button, column, file_explorer,
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
    assert!(t.move_node(f, Some(b)), "move into a sibling folder");
    assert_eq!(t.parent_of(f), Some(b));
    assert!(!t.move_node(a, Some(a)), "moving into itself is rejected");
    assert!(!t.move_node(a, Some(child)), "moving into a descendant is rejected");
    assert!(!t.move_node(b, Some(f)), "moving into a file is rejected");
    assert!(t.move_node(f, None), "move to the root");
    assert_eq!(t.parent_of(f), None, "now a root node");

    // Names de-duplicate against siblings.
    let x = t.insert(None, FsKind::File, "readme.txt");
    let y = t.insert(None, FsKind::File, "readme.txt");
    assert_eq!(t.node(x).map(|n| n.name.as_str()), Some("readme.txt"));
    assert_eq!(t.node(y).map(|n| n.name.as_str()), Some("readme 2.txt"));

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
    static EX: RefCell<Option<pebbles_widgets::FileExplorer>> = const { RefCell::new(None) };
}

fn ex() -> pebbles_widgets::FileExplorer {
    EX.with(|c| c.borrow().expect("explorer stored"))
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
    EX.with(|c| *c.borrow_mut() = Some(ex));
    OverlayHost::wrap(
        column(vec![
            button("New file").on_pressed(ex.new_file()).into_widget(),
            button("Delete").on_pressed(ex.delete_selected()).into_widget(),
            // A tall panel so the explorer has empty space (right-click there).
            Container::new()
                .height(220.0)
                .child(ex.tree())
                .into_widget(),
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

#[test]
fn ctrl_click_toggles_and_shift_click_ranges() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Rows: src (72..94), README.md (94..122).
    let mut tap_at = |ui: &mut Ui, y: f64| {
        let p = Offset::new(60.0, y);
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
        frame(ui, &mut env, win);
    };

    // Plain clicks select single. (Tapping src expands it, shifting README
    // down to 122..150 — tap README first.)
    tap_at(&mut ui, 105.0);
    assert_eq!(ex().selection().get(), vec![readme_id()], "plain click selects one");
    tap_at(&mut ui, 80.0);
    assert_eq!(ex().selection().get(), vec![src_id()], "and another replaces it");

    // Ctrl-click toggles the other node into the selection (README now at
    // 122..150 under the expanded src).
    pebbles_core::keyboard::set_modifiers(false, true);
    tap_at(&mut ui, 135.0);
    assert_eq!(ex().selection().get(), vec![src_id(), readme_id()], "ctrl-click adds");
    tap_at(&mut ui, 135.0);
    assert_eq!(ex().selection().get(), vec![src_id()], "ctrl-click removes");

    // Shift-click range-selects every VISIBLE row between src and README —
    // main.rs included (src is expanded).
    pebbles_core::keyboard::set_modifiers(true, false);
    tap_at(&mut ui, 135.0);
    assert_eq!(
        ex().selection().get(),
        vec![src_id(), main_id(), readme_id()],
        "shift-click selects the visible range"
    );
    pebbles_core::keyboard::set_modifiers(false, false);
}

fn src_id() -> u64 {
    tree_sig().peek().root.iter().find(|n| n.name == "src").expect("src").id
}

fn main_id() -> u64 {
    let t = tree_sig().peek();
    let src = t.root.iter().find(|n| n.name == "src").expect("src");
    src.children.iter().find(|n| n.name == "main.rs").expect("main").id
}

fn readme_id() -> u64 {
    tree_sig().peek().root.iter().find(|n| n.name == "README.md").expect("readme").id
}

#[test]
fn dragging_a_selection_moves_them_all() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false);

    // Add a second file next to README for a two-file selection.
    tree_sig().update(|t| {
        t.insert(None, FsKind::File, "LICENSE");
    });

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Rows: src (72..94), README.md (94..122), LICENSE (122..150).
    let readme = Offset::new(60.0, 105.0);
    let license = Offset::new(60.0, 133.0);
    let src_row = Offset::new(60.0, 80.0);

    // Ctrl-click both files.
    pebbles_core::keyboard::set_modifiers(false, true);
    for p in [readme, license] {
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
        frame(&mut ui, &mut env, win);
    }
    pebbles_core::keyboard::set_modifiers(false, false);
    assert_eq!(ex().selection().get().len(), 2, "two files selected");

    // Drag README (one of the selected) onto src → BOTH move.
    let target = ui.pan_target_at(readme).expect("a draggable row");
    ui.dispatch_pan_start(target, readme);
    ui.dispatch_pan_update(target, readme);
    ui.dispatch_hover(src_row);
    frame(&mut ui, &mut env, win);
    ui.dispatch_pan_end(target, src_row);
    frame(&mut ui, &mut env, win);

    let t = tree_sig().peek();
    let src = t.root.iter().find(|n| n.name == "src").expect("src");
    let in_src: Vec<&str> = src.children.iter().map(|c| c.name.as_str()).collect();
    assert!(in_src.contains(&"README.md") && in_src.contains(&"LICENSE"), "both moved: {in_src:?}");
    assert!(!t.root.iter().any(|n| n.name == "README.md" || n.name == "LICENSE"));
}

#[test]
fn right_clicking_empty_space_offers_new_nodes() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Below the last row is empty explorer space (y ≈ 200). The PRESS claims
    // it (the shell then skips the global menu).
    let p = Offset::new(60.0, 200.0);
    let handled = ui.dispatch_secondary_tap_down(p);
    frame(&mut ui, &mut env, win);
    assert!(handled, "the explorer claims the right-click");
    assert!(pebbles_widgets::overlay::is_open(), "its menu opens (New File / New Folder)");
    pebbles_widgets::overlay::hide_overlay();
}
