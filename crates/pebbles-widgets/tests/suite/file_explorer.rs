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
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

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
    pebbles_core::keyboard::set_modifiers(false, true, false, false);
    tap_at(&mut ui, 135.0);
    assert_eq!(ex().selection().get(), vec![src_id(), readme_id()], "ctrl-click adds");
    tap_at(&mut ui, 135.0);
    assert_eq!(ex().selection().get(), vec![src_id()], "ctrl-click removes");

    // Shift-click range-selects every VISIBLE row between src and README —
    // main.rs included (src is expanded).
    pebbles_core::keyboard::set_modifiers(true, false, false, false);
    tap_at(&mut ui, 135.0);
    assert_eq!(
        ex().selection().get(),
        vec![src_id(), main_id(), readme_id()],
        "shift-click selects the visible range"
    );
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
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
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

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
    pebbles_core::keyboard::set_modifiers(false, true, false, false);
    for p in [readme, license] {
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
        frame(&mut ui, &mut env, win);
    }
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
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

// ---------------------------------------------------------------------------
// Filesystem mode: real directories, real mutations
// ---------------------------------------------------------------------------

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pebbles-explorer-{tag}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().subsec_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn filesystem_mode_reads_creates_renames_deletes_and_moves_on_disk() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let root = temp_dir("fs");
    std::fs::create_dir(root.join("docs")).unwrap();
    std::fs::write(root.join("README.md"), "readme").unwrap();
    std::fs::write(root.join("docs").join("guide.md"), "guide").unwrap();

    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    assert!(ex.open_folder(&root), "opens a real directory");
    assert!(ex.fs_root().get().is_some(), "filesystem mode engaged");

    let names = |t: &FileTree| t.root.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    let mut got = names(&tree.peek());
    got.sort();
    assert_eq!(got, vec!["README.md".to_string(), "docs".to_string()], "real entries, folders first");

    // Expanding a folder reads its children from disk (lazily).
    let docs = tree.peek().root.iter().find(|n| n.name == "docs").expect("docs").id;
    ex.select_only(docs);
    ex.toggle_folder(docs);
    let docs_children: Vec<String> = tree
        .peek()
        .node(docs)
        .map(|n| n.children.iter().map(|c| c.name.clone()).collect())
        .unwrap_or_default();
    assert_eq!(docs_children, vec!["guide.md".to_string()], "folder children read from disk");

    // Create a real file on disk (via the public action).
    ex.new_file()();
    let fid = tree.peek().node(docs).unwrap().children.iter().find(|c| c.name == "new_file.txt").expect("new file").id;
    assert!(root.join("docs/new_file.txt").exists(), "created on disk");

    // Rename it on disk.
    ex.rename_node(fid, "renamed.txt".to_string());
    assert!(root.join("docs/renamed.txt").exists(), "rename hit the disk");
    assert!(!root.join("docs/new_file.txt").exists());

    // Move it to the root (real fs move).
    ex.move_nodes(&[fid], None);
    assert!(root.join("renamed.txt").exists(), "move to root hit the disk");
    assert!(!root.join("docs/renamed.txt").exists());

    // Delete it for real.
    ex.select_only(fid);
    ex.delete_selected()();
    assert!(!root.join("renamed.txt").exists(), "delete removed the real file");

    std::fs::remove_dir_all(&root).ok();
}

// ---------------------------------------------------------------------------
// Right-click: the explorer's menu is independent of the global switch
// ---------------------------------------------------------------------------

#[test]
fn row_context_menu_opens_with_the_global_menu_disabled() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    assert!(!pebbles_widgets::is_global_menu_enabled(), "the global switch defaults OFF");

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Right-press README.md (row at ~94..122): the row's OWN menu claims the
    // press — selection syncs and the menu opens, global switch irrelevant.
    let readme = Offset::new(60.0, 105.0);
    let handled = ui.dispatch_secondary_tap_down(readme);
    frame(&mut ui, &mut env, win);
    assert!(handled, "the row menu claims the press (no global fallback)");
    assert!(pebbles_widgets::overlay::is_open(), "the row's context menu is open");
    assert_eq!(ex().selection().peek().len(), 1, "right-click selected the row");
}

// ---------------------------------------------------------------------------
// Keyboard: the VSCode set — and it declines when the explorer is idle
// ---------------------------------------------------------------------------

#[test]
fn keyboard_drives_the_explorer_and_declines_when_idle() {
    use pebbles_core::{Mods, ShortcutKey, shortcuts};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    let w = ui.window_id();
    let none = Mods::default();
    // Ids from tree_sig(): src=0, main.rs=1, README.md=2.

    // Idle (no selection): every binding declines — nothing is hijacked.
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::ArrowDown), "arrows fall through");
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::Delete), "Delete falls through");
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::Escape), "Escape falls through");

    // Engage on the collapsed folder; ArrowRight expands it.
    ex().select_only(0);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowRight));
    assert!(ex().expanded().peek().contains(&0), "ArrowRight expanded src");
    frame(&mut ui, &mut env, win);

    // ArrowDown walks into the folder; Shift+ArrowDown extends the selection.
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowDown));
    assert_eq!(ex().selection().peek(), vec![1], "stepped to main.rs");
    let shift = Mods { shift: true, ..Mods::default() };
    assert!(shortcuts::dispatch(w, shift, ShortcutKey::ArrowDown));
    assert_eq!(ex().selection().peek(), vec![1, 2], "Shift+Down extends");

    // ArrowLeft from a file jumps to the parent; again on the folder collapses.
    ex().select_only(1);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowLeft));
    assert_eq!(ex().selection().peek(), vec![0], "Left jumps to the parent");
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowLeft));
    assert!(!ex().expanded().peek().contains(&0), "Left collapses the folder");

    // F2 starts the inline rename of the active node.
    assert!(shortcuts::dispatch(w, none, ShortcutKey::F(2)));
    assert_eq!(ex().renaming().peek(), Some(0), "F2 renames the active node");
    ex().renaming().set(None); // cancel — the editor plays no part here

    // Mod+A selects all visible; Escape clears (and disengages).
    #[cfg(target_os = "macos")]
    let modk = Mods { meta: true, ..Mods::default() };
    #[cfg(not(target_os = "macos"))]
    let modk = Mods { ctrl: true, ..Mods::default() };
    ex().select_only(2);
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('a')));
    assert_eq!(ex().selection().peek().len(), 2, "all visible rows (src collapsed)");
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Escape));
    assert!(ex().selection().peek().is_empty(), "Escape clears the selection");

    // Delete removes the selection.
    ex().select_only(2);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Delete));
    frame(&mut ui, &mut env, win);
    let names = tree_sig().peek().root.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    assert!(!names.contains(&"README.md".to_string()), "Delete removed the file");
}

// ---------------------------------------------------------------------------
// Per-node customization: each folder/file styles individually
// ---------------------------------------------------------------------------

#[test]
fn per_node_icon_and_color_are_individually_customizable() {
    use pebbles_render::IconKind;
    use pebbles_widgets::FsNode;

    let mut t = FileTree::new();
    // Builders on a hand-built node (insert_node assigns subtree ids + dedups).
    let mut custom = FsNode::folder("src").icon(IconKind::File).color(palette::WHITE);
    custom.children.push(FsNode::file("main.rs").color(palette::BLACK));
    let src = t.insert_node(None, custom);
    let n = t.node(src).expect("inserted");
    assert_eq!(n.icon, Some(IconKind::File.data()));
    assert_eq!(n.color, Some(palette::WHITE));
    let child = n.children.first().expect("child got an id too");
    assert!(child.id != 0 || src != 0, "subtree ids assigned");
    assert_eq!(child.color, Some(palette::BLACK));
    assert!(child.icon.is_none(), "unset fields keep the kind's default");

    // Plain nodes default to no overrides; node_mut customizes in place.
    let plain = t.insert(None, FsKind::File, "a.txt");
    assert!(t.node(plain).expect("plain").icon.is_none());
    t.node_mut(plain).expect("mutable").icon = Some(IconKind::Folder.data());
    assert_eq!(t.node(plain).expect("plain").icon, Some(IconKind::Folder.data()));

    // insert_node still de-duplicates names against siblings.
    let dup = t.insert_node(None, FsNode::file("a.txt"));
    assert_eq!(t.node(dup).expect("dup").name, "a 2.txt");
}

// ---------------------------------------------------------------------------
// Rename UX: the editor opens PREFILLED with the current name, stem selected
// ---------------------------------------------------------------------------

#[test]
fn rename_prefills_the_current_name_with_the_stem_selected() {
    use pebbles_core::{Mods, ShortcutKey, shortcuts};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);
    let w = ui.window_id();

    // F2 on README.md (id 2): the editor mounts with "README.md" prefilled and
    // the stem "README" selected — typing replaces ONLY the stem.
    ex().select_only(2);
    assert!(shortcuts::dispatch(w, Mods::default(), ShortcutKey::F(2)));
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Insert("GUIDE".to_string()));
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui, &mut env, win);

    let names = tree_sig().peek().root.iter().map(|n| n.name.clone()).collect::<Vec<_>>();
    assert!(
        names.contains(&"GUIDE.md".to_string()),
        "typing replaced the selected stem, keeping the extension: {names:?}"
    );
    assert!(!names.contains(&"README.md".to_string()), "the old name is gone");
}

// ---------------------------------------------------------------------------
// Clipboard: Mod+C/X/V, Escape-cancel, Mod+A from idle, Home/End
// ---------------------------------------------------------------------------

#[test]
fn clipboard_copy_cut_paste_and_the_remaining_common_shortcuts() {
    use pebbles_core::{Mods, ShortcutKey, shortcuts};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);
    let w = ui.window_id();
    let none = Mods::default();
    #[cfg(target_os = "macos")]
    let modk = Mods { meta: true, ..Mods::default() };
    #[cfg(not(target_os = "macos"))]
    let modk = Mods { ctrl: true, ..Mods::default() };
    // Ids: src=0 (folder), main.rs=1 (inside src), README.md=2.

    // Mod+A works from IDLE (no selection): selects every visible row.
    assert!(ex().selection().peek().is_empty(), "starts idle");
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('a')), "Ctrl+A engages from idle");
    assert_eq!(ex().selection().peek().len(), 2, "all visible rows (src collapsed + README)");

    // Home/End jump to the first/last visible row.
    assert!(shortcuts::dispatch(w, none, ShortcutKey::End));
    assert_eq!(ex().selection().peek(), vec![2], "End selects the last row");
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Home));
    assert_eq!(ex().selection().peek(), vec![0], "Home selects the first row");

    // COPY: duplicate main.rs next to itself (paste targets its parent folder).
    ex().select_only(1);
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('c')), "Ctrl+C copies");
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('v')), "Ctrl+V pastes");
    frame(&mut ui, &mut env, win);
    let t = tree_sig().peek();
    let src = t.root.iter().find(|n| n.name == "src").expect("src");
    let kids: Vec<&str> = src.children.iter().map(|n| n.name.as_str()).collect();
    assert!(kids.contains(&"main.rs") && kids.contains(&"main 2.rs"), "copy duplicated with a deduped name: {kids:?}");

    // CUT: move README.md into src (select the folder as the paste target).
    ex().select_only(2);
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('x')), "Ctrl+X cuts");
    ex().select_only(0);
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('v')), "Ctrl+V pastes the cut");
    frame(&mut ui, &mut env, win);
    let t = tree_sig().peek();
    assert!(!t.root.iter().any(|n| n.name == "README.md"), "cut left the root");
    let src = t.root.iter().find(|n| n.name == "src").expect("src");
    assert!(src.children.iter().any(|n| n.name == "README.md"), "…and moved into src");
    // The cut clipboard is consumed: another paste declines.
    assert!(!shortcuts::dispatch(w, modk, ShortcutKey::Char('v')), "cut clipboard consumed");

    // Escape cancels a pending cut (before it clears the selection).
    ex().select_only(1);
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Char('x')));
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Escape), "Escape cancels the cut");
    assert!(!shortcuts::dispatch(w, modk, ShortcutKey::Char('v')), "nothing left to paste");
    assert_eq!(ex().selection().peek(), vec![1], "the selection survived the cancel");
}

// ---------------------------------------------------------------------------
// Icon themes: per-node override → installed theme → the defaults
// ---------------------------------------------------------------------------

#[test]
fn icon_theme_resolution_priority() {
    use pebbles_render::{IconKind, lucide};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    let t = tree_sig().peek();
    let folder = t.node(0).expect("src").clone();
    let file = t.node(1).expect("main.rs").clone();

    // Defaults: closed/open folder glyphs differ; plain file glyph.
    assert_eq!(ex().resolved_icon(&folder, false).0, IconKind::Folder.data());
    assert_eq!(ex().resolved_icon(&folder, true).0, lucide::FOLDER_OPEN, "open folders get their own glyph");
    assert_eq!(ex().resolved_icon(&file, false).0, IconKind::File.data());

    // An installed theme wins over the defaults; returning None falls through.
    ex().set_icon_theme(|n, _open| {
        n.name.ends_with(".rs").then_some((lucide::FILE_CODE, Some(palette::WHITE)))
    });
    assert_eq!(ex().resolved_icon(&file, false), (lucide::FILE_CODE, Some(palette::WHITE)));
    assert_eq!(ex().resolved_icon(&folder, false).0, IconKind::Folder.data(), "None keeps the default");

    // A per-node override wins over everything.
    let mut starred = file.clone();
    starred.icon = Some(lucide::STAR);
    starred.color = Some(palette::BLACK);
    assert_eq!(ex().resolved_icon(&starred, false), (lucide::STAR, Some(palette::BLACK)));

    // Clearing restores the defaults.
    ex().clear_icon_theme();
    assert_eq!(ex().resolved_icon(&file, false).0, IconKind::File.data());
}
