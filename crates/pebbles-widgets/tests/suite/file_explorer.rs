//! [`FileExplorer`]: the model mutates correctly (insert/rename/delete/move with
//! the descendant + file-target guards), and the UI drives it — New File creates +
//! starts the inline rename, custom buttons delete, and drag-pan onto a folder
//! moves the node.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Ui, component, create_signal};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_testing::draw_frame as frame;
use pebbles_widgets::{FileTree, FsKind, OverlayHost, View, button, column, container, file_explorer};

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
            container().height(220.0).child(ex.tree()).into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

#[test]
fn new_file_creates_and_inline_rename_commits() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    pebbles_widgets::Theme::tailwind().make_current();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    ui.dispatch_pan_update(target, src_row); // past the drag slop
    ui.dispatch_hover(src_row); // hovering the folder marks the drop target
    frame(&mut ui, &mut env, win);
    ui.dispatch_pan_end(target, src_row);
    frame(&mut ui, &mut env, win);

    let t = tree_sig().peek();
    let src = t.root.iter().find(|n| n.name == "src").expect("src folder");
    assert!(src.children.iter().any(|n| n.name == "README.md"), "README.md moved into src");
    assert!(!t.root.iter().any(|n| n.name == "README.md"), "and left the root");
}

#[test]
fn ctrl_click_toggles_and_shift_click_ranges() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

    let mut ui = Ui::new();
    pebbles_widgets::Theme::tailwind().make_current();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    ui.dispatch_pan_update(target, src_row); // past the drag slop
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
    pebbles_widgets::Theme::tailwind().make_current();
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

    // New File: an inline editor first — nothing on disk until committed.
    ex.new_file()();
    let pending = ex.renaming().get().expect("inline editor open");
    assert!(!root.join("docs/new_file.txt").exists(), "not created before the name is committed");
    ex.commit_rename(pending, "").unwrap(); // empty = the suggested name
    let fid = tree
        .peek()
        .node(docs)
        .unwrap()
        .children
        .iter()
        .find(|c| c.name == "new_file.txt")
        .expect("new file")
        .id;
    assert!(root.join("docs/new_file.txt").exists(), "created on disk");

    // Rename it on disk.
    ex.rename_node(fid, "renamed.txt".to_string()).unwrap();
    assert!(root.join("docs/renamed.txt").exists(), "rename hit the disk");
    assert!(!root.join("docs/new_file.txt").exists());

    // Move it to the root (real fs move).
    ex.move_nodes(&[fid], None).unwrap();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    pebbles_widgets::Theme::tailwind().make_current();
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
    assert!(
        kids.contains(&"main.rs") && kids.contains(&"main copy.rs"),
        "copy duplicated with a deduped name: {kids:?}"
    );

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
    use pebbles_render::{IconKind, tabler};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();

    let mut ui = Ui::new();
    pebbles_widgets::Theme::tailwind().make_current();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    let t = tree_sig().peek();
    let folder = t.node(0).expect("src").clone();
    let file = t.node(1).expect("main.rs").clone();

    use pebbles_widgets::{ExplorerIcon, IconTheme};
    let g = |d| ExplorerIcon::Glyph(d, None);
    // Defaults: closed/open folder glyphs differ; plain file glyph.
    assert_eq!(ex().resolved_icon(&folder, false), g(IconKind::Folder.data()));
    assert_eq!(ex().resolved_icon(&folder, true), g(tabler::FOLDER_OPEN), "open folders get their own glyph");
    assert_eq!(ex().resolved_icon(&file, false), g(IconKind::File.data()));

    // An installed theme wins over the defaults; unset slots fall through.
    ex().set_icon_theme(
        IconTheme::new().file_extension("rs", ExplorerIcon::colored(tabler::FILE_CODE, palette::WHITE)),
    );
    assert_eq!(ex().resolved_icon(&file, false), ExplorerIcon::colored(tabler::FILE_CODE, palette::WHITE));
    assert_eq!(ex().resolved_icon(&folder, false), g(IconKind::Folder.data()), "unset keeps the default");

    // A per-node override wins over the theme.
    let mut starred = file.clone();
    starred.icon = Some(tabler::STAR);
    starred.color = Some(palette::BLACK);
    assert_eq!(ex().resolved_icon(&starred, false), ExplorerIcon::colored(tabler::STAR, palette::BLACK));

    // The resolver wins over everything.
    ex().set_icon_resolver(|e, _| (e.name == "main.rs").then(|| ExplorerIcon::glyph(tabler::BUG)));
    assert_eq!(ex().resolved_icon(&starred, false), g(tabler::BUG));
    ex().clear_icon_resolver();

    // Clearing restores the defaults.
    ex().clear_icon_theme();
    assert_eq!(ex().resolved_icon(&file, false), g(IconKind::File.data()));
}

#[test]
fn icon_theme_precedence_matches_vs_code() {
    use pebbles_render::tabler as t;
    use pebbles_widgets::{ExplorerIcon, IconTheme};
    let gl = ExplorerIcon::glyph;
    let th = IconTheme::new()
        .file(gl(t::FILE))
        .folder(gl(t::FOLDER), Some(gl(t::FOLDER_OPEN)))
        .root_folder(gl(t::FOLDER_ROOT), Some(gl(t::FOLDER_UP)))
        .file_name("Cargo.toml", gl(t::PACKAGE))
        .file_extension("toml", gl(t::SETTINGS))
        .file_extension("ts", gl(t::BRAND_TYPESCRIPT))
        .file_extension("d.ts", gl(t::FILE_TYPE_TS))
        .file_extension("test.ts", gl(t::TEST_PIPE))
        .file_extension("tar.gz", gl(t::FILE_ZIP))
        .file_extension("gz", gl(t::ARCHIVE))
        .language("rust", gl(t::BRAND_RUST))
        .folder_name("src", gl(t::FOLDER_CODE), Some(gl(t::FOLDERS)))
        .folder_name("tests", gl(t::FLASK), None);
    let file = |n: &str| th.resolve(n, false, false, false);
    // file name > extension (case-insensitive like VS Code)
    assert_eq!(file("Cargo.toml"), Some(gl(t::PACKAGE)));
    assert_eq!(file("CARGO.TOML"), Some(gl(t::PACKAGE)));
    assert_eq!(file("other.toml"), Some(gl(t::SETTINGS)));
    // longest compound extension first
    assert_eq!(file("index.d.ts"), Some(gl(t::FILE_TYPE_TS)));
    assert_eq!(file("app.test.ts"), Some(gl(t::TEST_PIPE)));
    assert_eq!(file("app.ts"), Some(gl(t::BRAND_TYPESCRIPT)));
    assert_eq!(file("x.tar.gz"), Some(gl(t::FILE_ZIP)));
    assert_eq!(file("x.gz"), Some(gl(t::ARCHIVE)));
    assert_eq!(file("APP.TS"), Some(gl(t::BRAND_TYPESCRIPT)));
    // extension > language id > default
    assert_eq!(file("main.rs"), Some(gl(t::BRAND_RUST)), "language id via the built-in map");
    assert_eq!(file("notes.unknown"), Some(gl(t::FILE)));
    assert_eq!(file(".env"), Some(gl(t::FILE)), "a leading dot is not an extension");
    // custom language resolver
    let th2 = th.clone().language_resolver(|n| n.ends_with(".rsx").then(|| "rust".to_string()));
    assert_eq!(th2.resolve("view.rsx", false, false, false), Some(gl(t::BRAND_RUST)));
    // folders: name (expanded variant) > root > default
    assert_eq!(th.resolve("src", true, false, false), Some(gl(t::FOLDER_CODE)));
    assert_eq!(th.resolve("SRC", true, true, false), Some(gl(t::FOLDERS)), "expanded variant");
    assert_eq!(th.resolve("tests", true, true, false), Some(gl(t::FLASK)), "no expanded variant → closed");
    assert_eq!(th.resolve("proj", true, false, true), Some(gl(t::FOLDER_ROOT)));
    assert_eq!(th.resolve("proj", true, true, true), Some(gl(t::FOLDER_UP)));
    assert_eq!(th.resolve("other", true, false, false), Some(gl(t::FOLDER)));
    assert_eq!(th.resolve("other", true, true, false), Some(gl(t::FOLDER_OPEN)));
    // The built-in theme covers the common cases.
    let d = IconTheme::default_theme();
    for n in [
        "Cargo.toml",
        "package.json",
        "README.md",
        "LICENSE",
        "Dockerfile",
        ".gitignore",
        "a.rs",
        "a.py",
        "a.go",
        "a.png",
    ] {
        assert!(
            d.resolve(n, false, false, false).is_some_and(|i| matches!(i, ExplorerIcon::Glyph(_, Some(_)))),
            "{n} coloured"
        );
    }
    for n in ["src", "tests", "docs", "target", "node_modules", ".github"] {
        assert!(d.resolve(n, true, false, false).is_some(), "{n}");
    }
}

#[test]
fn per_path_override_and_widget_icons() {
    use pebbles_render::tabler as t;
    use pebbles_widgets::{ExplorerIcon, IconTheme};
    let (dir, tree, ex) = sample_project("pathicon");
    ex.set_icon_theme(IconTheme::default_theme());
    let main = tree.peek().node(id_of(tree, "src/main.rs")).cloned().unwrap();
    assert!(matches!(ex.resolved_icon(&main, false), ExplorerIcon::Glyph(d, _) if d == t::BRAND_RUST));
    ex.set_path_icon(dir.join("src/main.rs"), Some(ExplorerIcon::glyph(t::STAR)));
    assert_eq!(ex.resolved_icon(&main, false), ExplorerIcon::glyph(t::STAR));
    let w = ExplorerIcon::widget(|size| pebbles_widgets::gap_w(size).into_widget());
    ex.set_path_icon(dir.join("src/main.rs"), Some(w.clone()));
    assert_eq!(ex.resolved_icon(&main, false), w);
    ex.set_path_icon(dir.join("src/main.rs"), None);
    assert!(matches!(ex.resolved_icon(&main, false), ExplorerIcon::Glyph(d, _) if d == t::BRAND_RUST));
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Focus walk (Mod+arrows + Mod+Space) and the external filter
// ---------------------------------------------------------------------------

#[test]
fn mod_arrows_walk_focus_and_mod_space_toggles_one_by_one() {
    use pebbles_core::{Mods, ShortcutKey, shortcuts};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

    let mut ui = Ui::new();
    pebbles_widgets::Theme::tailwind().make_current();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);
    let w = ui.window_id();
    #[cfg(target_os = "macos")]
    let modk = Mods { meta: true, ..Mods::default() };
    #[cfg(not(target_os = "macos"))]
    let modk = Mods { ctrl: true, ..Mods::default() };
    // Ids: src=0 (collapsed folder), main.rs=1 (inside), README.md=2.

    // Select src, then walk the FOCUS down without touching the selection.
    ex().select_only(0);
    assert_eq!(ex().active_row().peek(), Some(0), "selecting focuses");
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::ArrowDown), "Mod+Down moves focus");
    assert_eq!(ex().active_row().peek(), Some(2), "focus walked to README (src collapsed)");
    assert_eq!(ex().selection().peek(), vec![0], "…while the selection stayed put");

    // Mod+Space toggles the focused row INTO the selection (one-by-one).
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Space), "Mod+Space toggles");
    assert_eq!(ex().selection().peek(), vec![0, 2], "focused row joined the selection");
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::Space));
    assert_eq!(ex().selection().peek(), vec![0], "…and toggles back out");

    // Mod+Up walks back.
    assert!(shortcuts::dispatch(w, modk, ShortcutKey::ArrowUp));
    assert_eq!(ex().active_row().peek(), Some(0));
}

#[test]
fn external_filter_prunes_the_tree_and_keyboard_follows() {
    use pebbles_core::{Mods, ShortcutKey, shortcuts};
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = tree_sig();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);

    let mut ui = Ui::new();
    pebbles_widgets::Theme::tailwind().make_current();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);
    let w = ui.window_id();
    let none = Mods::default();
    // Tree: src=0 { main.rs=1 }, README.md=2.

    // "rs" matches main.rs → src (its ancestor) + main.rs stay; README is pruned.
    // Folders are force-expanded while filtering, so main.rs is reachable even
    // though src was never expanded.
    ex().filter().set("rs".to_string());
    frame(&mut ui, &mut env, win);
    ex().select_only(0);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowDown));
    assert_eq!(ex().selection().peek(), vec![1], "keyboard walks the FILTERED rows");
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowDown));
    assert_eq!(ex().selection().peek(), vec![1], "README is filtered out — the walk stops");

    // A folder-name match keeps the folder's whole subtree visible.
    ex().filter().set("src".to_string());
    ex().select_only(0);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::ArrowDown));
    assert_eq!(ex().selection().peek(), vec![1], "children ride an ancestor match");

    // No matches → nothing to walk; clearing restores everything.
    ex().filter().set("zzz".to_string());
    assert!(shortcuts::dispatch(w, none, ShortcutKey::End), "engaged: consumed at the edge");
    ex().filter().set(String::new());
    frame(&mut ui, &mut env, win);
    ex().select_only(0);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::End));
    assert_eq!(ex().selection().peek(), vec![2], "unfiltered again: End reaches README");
}

#[test]
fn load_filter_skips_ignored_entries_on_open_and_lazy_expand() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let root = temp_dir("filter");
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::create_dir(root.join("target")).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    std::fs::write(root.join("src").join("main.rs"), "").unwrap();
    std::fs::write(root.join("src").join("main.rs.bak"), "").unwrap();

    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    ex.set_load_filter(|path, is_dir| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_dir { !matches!(name, ".git" | "target") } else { !name.ends_with(".bak") }
    });
    assert!(ex.open_folder(&root));
    let names: Vec<String> = tree.peek().root.iter().map(|n| n.name.clone()).collect();
    assert_eq!(names, vec!["src".to_string(), "Cargo.toml".to_string()], "ignored dirs skipped");

    let src = tree.peek().root[0].id;
    ex.toggle_folder(src);
    let kids: Vec<String> = tree.peek().node(src).unwrap().children.iter().map(|c| c.name.clone()).collect();
    assert_eq!(kids, vec!["main.rs".to_string()], "filter applies to lazy folder loads too");
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// Data-loss regression: a click (press → tiny jitter → release, exactly as
// the shell runner dispatches it) must never move anything on disk.
// ---------------------------------------------------------------------------

thread_local! {
    static FS_EX: RefCell<Option<pebbles_widgets::FileExplorer>> = const { RefCell::new(None) };
    static TREE_FS: RefCell<Option<pebbles_core::Signal<FileTree>>> = const { RefCell::new(None) };
}

fn fs_root_widget() -> impl IntoWidget {
    let ex = FS_EX.with(|c| c.borrow().expect("fs explorer stored"));
    OverlayHost::wrap(container().height(260.0).child(ex.tree()))
}

/// Mount an explorer over a temp dir holding `outer/inner/deep.txt` with
/// `outer` expanded. Returns (ui, env, root dir, explorer).
fn mount_fs(tag: &str) -> (Ui, TextEnv, std::path::PathBuf, pebbles_widgets::FileExplorer) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let dir = temp_dir(tag);
    std::fs::create_dir_all(dir.join("outer/inner")).unwrap();
    std::fs::write(dir.join("outer/inner/deep.txt"), "x").unwrap();
    std::fs::create_dir(dir.join("zother")).unwrap();
    let mut ui = Ui::new();
    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    ex.set_trash_dir(dir.with_extension("trash")); // never the real trash
    assert!(ex.open_folder(&dir));
    let outer = tree.peek().root.iter().find(|n| n.name == "outer").unwrap().id;
    ex.toggle_folder(outer);
    FS_EX.with(|c| *c.borrow_mut() = Some(ex));
    TREE_FS.with(|c| *c.borrow_mut() = Some(tree));
    pebbles_widgets::Theme::tailwind().make_current();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(fs_root_widget)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);
    (ui, env, dir, ex)
}

/// Find the row (by y probe) whose pan target differs per row: returns the y
/// of the nth row (0-based), probing every 2px.
fn row_points(ui: &mut Ui) -> Vec<Offset> {
    let mut out: Vec<Offset> = Vec::new();
    let mut last = None;
    let mut y = 1.0;
    while y < 250.0 {
        let p = Offset::new(60.0, y);
        let t = ui.pan_target_at(p);
        if t.is_some() && t != last {
            out.push(Offset::new(60.0, y + 6.0));
        }
        last = t;
        y += 2.0;
    }
    out
}

/// The shell runner's exact click sequence: pan_start at PRESS, hover moves
/// while held (with pan updates), pan_end + tap at release.
fn runner_click(ui: &mut Ui, env: &mut TextEnv, at: Offset, jitter: &[Offset]) {
    let win = Size::new(400.0, 300.0);
    let target = ui.pan_target_at(at);
    let armed = ui.tap_target_at(at);
    if let Some(t) = target {
        ui.dispatch_pan_start(t, at);
    }
    ui.dispatch_pointer_down(at);
    frame(ui, env, win);
    let mut end = at;
    for j in jitter {
        end = *j;
        ui.dispatch_hover(end);
        if let Some(t) = target {
            ui.dispatch_pan_update(t, end);
        }
        frame(ui, env, win);
    }
    if let Some(t) = target {
        ui.dispatch_pan_end(t, end);
    }
    ui.dispatch_pointer_up(end);
    // The runner taps only when released over the widget it pressed.
    if armed.is_some() && ui.tap_target_at(end) == armed {
        ui.dispatch_tap(end);
    }
    frame(ui, env, win);
}

#[test]
fn click_with_jitter_on_a_nested_row_never_moves_anything() {
    let (mut ui, mut env, dir, _ex) = mount_fs("jitter");
    let rows = row_points(&mut ui);
    assert!(rows.len() >= 3, "rows: outer, inner, zother ({rows:?})");
    let inner = rows[1];
    // Plain click, then a click with 1–3 px of jitter.
    runner_click(&mut ui, &mut env, inner, &[]);
    assert!(dir.join("outer/inner/deep.txt").exists(), "plain click moved outer/inner");
    runner_click(
        &mut ui,
        &mut env,
        inner,
        &[Offset::new(inner.x + 1.0, inner.y + 1.0), Offset::new(inner.x + 3.0, inner.y + 2.0)],
    );
    assert!(dir.join("outer/inner/deep.txt").exists(), "jittery click moved outer/inner");
    assert!(!dir.join("inner").exists(), "nothing landed at the root");
    std::fs::remove_dir_all(&dir).ok();
}

/// A real drag: press on `from`, travel past the slop to `to` (hovering it),
/// release there.
fn runner_drag(ui: &mut Ui, env: &mut TextEnv, from: Offset, to: Offset) {
    let mid = Offset::new(from.x, (from.y + to.y) / 2.0);
    runner_click(ui, env, from, &[Offset::new(from.x + 2.0, from.y), mid, to]);
}

fn set_confirm(ex: &pebbles_widgets::FileExplorer, answer: bool) -> std::rc::Rc<std::cell::Cell<usize>> {
    let asked = std::rc::Rc::new(std::cell::Cell::new(0));
    let a = asked.clone();
    ex.set_confirm_move(move |req, resolve| {
        assert_eq!(req.names, vec!["inner".to_string()]);
        assert_eq!(req.target_name, "zother");
        a.set(a.get() + 1);
        resolve(answer);
    });
    asked
}

#[test]
fn drag_past_slop_onto_a_folder_moves_after_confirm_and_undo_restores() {
    let (mut ui, mut env, dir, ex) = mount_fs("dragok");
    let asked = set_confirm(&ex, true);
    let rows = row_points(&mut ui);
    runner_drag(&mut ui, &mut env, rows[1], rows[2]);
    assert_eq!(asked.get(), 1, "confirmation asked once");
    assert!(dir.join("zother/inner/deep.txt").exists(), "moved into zother on disk");
    assert!(!dir.join("outer/inner").exists());
    assert!(ex.can_undo());
    assert!(ex.undo(), "undo restores");
    assert!(dir.join("outer/inner/deep.txt").exists(), "back in outer on disk");
    assert!(!dir.join("zother/inner").exists());
    assert!(!ex.can_undo());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn rejected_confirmation_moves_nothing() {
    let (mut ui, mut env, dir, ex) = mount_fs("dragno");
    let asked = set_confirm(&ex, false);
    let rows = row_points(&mut ui);
    runner_drag(&mut ui, &mut env, rows[1], rows[2]);
    assert_eq!(asked.get(), 1);
    assert!(dir.join("outer/inner/deep.txt").exists(), "rejected → untouched");
    assert!(!ex.can_undo());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn disallowed_fs_moves_move_nothing() {
    let (mut ui, mut env, dir, ex) = mount_fs("dragoff");
    let ex = ex.allow_fs_moves(false);
    let asked = set_confirm(&ex, true);
    let rows = row_points(&mut ui);
    runner_drag(&mut ui, &mut env, rows[1], rows[2]);
    assert_eq!(asked.get(), 0, "never even asks");
    assert!(dir.join("outer/inner/deep.txt").exists(), "disabled → untouched");
    // Programmatic moves are refused too.
    let inner = rows_id(&ex, "outer_inner");
    assert!(ex.move_nodes(&[inner], None).is_err());
    assert!(dir.join("outer/inner/deep.txt").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn drops_without_an_explicit_target_and_invalid_moves_do_nothing() {
    let (mut ui, mut env, dir, ex) = mount_fs("notarget");
    let asked = std::rc::Rc::new(std::cell::Cell::new(0));
    let a = asked.clone();
    ex.set_confirm_move(move |_, resolve| {
        a.set(a.get() + 1);
        resolve(true);
    });
    let rows = row_points(&mut ui);
    // Drag `inner` past the slop but release back over itself / its parent row
    // / outside the window: no target → nothing.
    runner_drag(&mut ui, &mut env, rows[1], Offset::new(rows[1].x + 30.0, rows[1].y));
    runner_drag(&mut ui, &mut env, rows[1], rows[0]); // its own parent: no-op
    runner_drag(&mut ui, &mut env, rows[1], Offset::new(60.0, 900.0));
    assert_eq!(asked.get(), 0, "nothing to confirm");
    assert!(dir.join("outer/inner/deep.txt").exists());
    // A folder into itself / its descendant is refused; so is a no-op.
    let inner = rows_id(&ex, "outer_inner");
    let outer = rows_id(&ex, "outer");
    assert!(ex.move_nodes(&[outer], Some(inner)).is_err(), "into a descendant");
    assert!(ex.move_nodes(&[outer], Some(outer)).is_err(), "into itself");
    assert!(ex.move_nodes(&[inner], Some(outer)).is_err(), "already there");
    assert!(dir.join("outer/inner/deep.txt").exists());
    std::fs::remove_dir_all(&dir).ok();
}

fn rows_id(ex: &pebbles_widgets::FileExplorer, name: &str) -> u64 {
    let _ = ex;
    let tree = TREE_FS.with(|t| t.borrow().unwrap());
    let t = tree.peek();
    let outer = t.root.iter().find(|n| n.name == "outer").unwrap();
    if name == "outer_inner" {
        return outer.children.iter().find(|n| n.name == "inner").unwrap().id;
    }
    t.root.iter().find(|n| n.name == name).unwrap().id
}

#[test]
fn natural_sort_folders_first() {
    use std::cmp::Ordering;
    assert_eq!(pebbles_widgets::natural_cmp("file2", "file10"), Ordering::Less);
    assert_eq!(pebbles_widgets::natural_cmp("B.txt", "a.txt"), Ordering::Greater);
    let dir = temp_dir("natsort");
    for f in ["file10.txt", "file2.txt", "File1.txt"] {
        std::fs::write(dir.join(f), "").unwrap();
    }
    std::fs::create_dir(dir.join("z10")).unwrap();
    std::fs::create_dir(dir.join("z9")).unwrap();
    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    assert!(ex.open_folder(&dir));
    let names: Vec<String> = tree.peek().root.iter().map(|n| n.name.clone()).collect();
    assert_eq!(names, ["z9", "z10", "File1.txt", "file2.txt", "file10.txt"]);
    std::fs::remove_dir_all(&dir).ok();
}

fn fs_explorer(
    tag: &str,
) -> (std::path::PathBuf, pebbles_core::Signal<FileTree>, pebbles_widgets::FileExplorer) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let dir = temp_dir(tag);
    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    ex.set_trash_dir(dir.with_extension("trash")); // never the real trash
    (dir, tree, ex)
}

fn id_of(tree: pebbles_core::Signal<FileTree>, path: &str) -> u64 {
    let t = tree.peek();
    let mut nodes = &t.root;
    let mut id = 0;
    for seg in path.split('/') {
        let n = nodes.iter().find(|n| n.name == seg).unwrap_or_else(|| panic!("{path}: no {seg}"));
        id = n.id;
        nodes = &n.children;
    }
    id
}

#[test]
fn rename_validates_never_clobbers_and_creates_nested_paths_and_undoes() {
    let (dir, tree, ex) = fs_explorer("renamev");
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    std::fs::write(dir.join("b.txt"), "B").unwrap();
    assert!(ex.open_folder(&dir));
    let a = id_of(tree, "a.txt");
    // Onto an existing sibling: refused, nothing overwritten.
    assert!(ex.rename_node(a, "b.txt").is_err());
    assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "B");
    assert!(ex.last_error().get().is_some());
    for bad in ["..", ".", " x", "a\\b"] {
        assert!(ex.validate_name(a, bad).is_err(), "{bad:?} rejected");
    }
    // Case-only rename of itself is allowed.
    assert!(ex.validate_name(a, "A.txt").is_ok());
    // Nested path: folders created, file moved in.
    assert!(ex.rename_node(a, "x/y/c.txt").is_ok());
    assert_eq!(std::fs::read_to_string(dir.join("x/y/c.txt")).unwrap(), "A");
    assert!(!dir.join("a.txt").exists());
    assert_eq!(id_of(tree, "x/y/c.txt"), a);
    // Undo puts it back (the created folders stay, as in VS Code).
    assert!(ex.can_undo());
    assert!(ex.undo());
    assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "A");
    assert_eq!(id_of(tree, "a.txt"), a);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn refresh_merges_external_changes_keeping_ids_and_watch_polls() {
    let (dir, tree, ex) = fs_explorer("refresh");
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.rs"), "").unwrap();
    std::fs::write(dir.join("gone.txt"), "").unwrap();
    assert!(ex.open_folder(&dir));
    let src = id_of(tree, "src");
    ex.toggle_folder(src);
    let a = id_of(tree, "src/a.rs");
    assert!(!ex.refresh(), "nothing changed → no-op");
    std::fs::write(dir.join("src/b.rs"), "").unwrap();
    std::fs::remove_file(dir.join("gone.txt")).unwrap();
    assert!(ex.refresh());
    assert_eq!(id_of(tree, "src"), src, "ids survive");
    assert_eq!(id_of(tree, "src/a.rs"), a);
    let _ = id_of(tree, "src/b.rs");
    assert!(!tree.peek().root.iter().any(|n| n.name == "gone.txt"));
    // The polling watcher picks up the next change on its own.
    ex.watch(Some(1.0));
    std::fs::write(dir.join("new.txt"), "").unwrap();
    let t0 = 1.0e6;
    pebbles_core::animation::tick(t0);
    pebbles_core::animation::tick(t0 + 1.5);
    assert!(tree.peek().root.iter().any(|n| n.name == "new.txt"), "watcher saw new.txt");
    ex.watch(None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn trash_writes_freedesktop_entries_and_undo_restores() {
    let (dir, tree, ex) = fs_explorer("trash");
    let trash = temp_dir("trashbin");
    ex.set_trash_dir(&trash);
    std::fs::create_dir(dir.join("d")).unwrap();
    std::fs::write(dir.join("d/x.txt"), "x").unwrap();
    std::fs::write(dir.join("f.txt"), "f").unwrap();
    assert!(ex.open_folder(&dir));
    let (d, f) = (id_of(tree, "d"), id_of(tree, "f.txt"));
    assert!(ex.trash_nodes(&[d, f]).is_ok());
    assert!(!dir.join("d").exists() && !dir.join("f.txt").exists());
    assert_eq!(std::fs::read_to_string(trash.join("files/d/x.txt")).unwrap(), "x");
    let info = std::fs::read_to_string(trash.join("info/f.txt.trashinfo")).unwrap();
    assert!(info.starts_with("[Trash Info]\nPath=") && info.contains("f.txt\nDeletionDate="), "{info}");
    #[cfg(unix)]
    assert!(info.starts_with("[Trash Info]\nPath=/"), "absolute path: {info}");
    assert!(tree.peek().root.is_empty());
    assert!(ex.undo());
    assert_eq!(std::fs::read_to_string(dir.join("d/x.txt")).unwrap(), "x");
    assert_eq!(std::fs::read_to_string(dir.join("f.txt")).unwrap(), "f");
    assert!(!trash.join("info/f.txt.trashinfo").exists());
    assert_eq!(tree.peek().root.len(), 2, "back in the tree");
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&trash).ok();
}

#[test]
fn paste_copy_uses_copy_suffix_and_undo_removes_copies_and_creates() {
    let (dir, tree, ex) = fs_explorer("copysfx");
    let trash = temp_dir("copysfx-trash");
    ex.set_trash_dir(&trash);
    std::fs::write(dir.join("a.txt"), "A").unwrap();
    assert!(ex.open_folder(&dir));
    let a = id_of(tree, "a.txt");
    ex.select_only(a);
    ex.copy_selection().unwrap();
    ex.selection().set(Vec::new());
    ex.paste_clipboard().unwrap();
    ex.paste_clipboard().unwrap();
    assert!(dir.join("a copy.txt").exists() && dir.join("a copy 2.txt").exists());
    assert!(ex.undo(), "undo the second paste");
    assert!(!dir.join("a copy 2.txt").exists() && dir.join("a copy.txt").exists());
    // New File → undo removes the (still empty) file.
    ex.selection().set(Vec::new());
    ex.new_file()();
    let pending = ex.renaming().get().expect("inline editor open");
    ex.commit_rename(pending, "").unwrap();
    assert!(dir.join("new_file.txt").exists());
    assert!(ex.undo());
    assert!(!dir.join("new_file.txt").exists());
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&trash).ok();
}

#[test]
fn ctrl_drag_copies_instead_of_moving() {
    let (mut ui, mut env, dir, ex) = mount_fs("ctrldrag");
    let asked = set_confirm(&ex, true);
    let rows = row_points(&mut ui);
    pebbles_core::keyboard::set_modifiers(false, true, false, false);
    // Press without Ctrl semantics on select: start the drag then hold Ctrl at drop.
    runner_drag(&mut ui, &mut env, rows[1], rows[2]);
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
    assert_eq!(asked.get(), 0, "copies need no move confirmation");
    assert!(dir.join("outer/inner/deep.txt").exists(), "original stays");
    assert!(dir.join("zother/inner/deep.txt").exists(), "copied into zother");
    assert!(ex.undo(), "undo removes the copy");
    assert!(!dir.join("zother/inner").exists());
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(dir.with_extension("trash")).ok();
}

// ---------------------------------------------------------------------------
// Developer controls: capabilities, read-only, predicates, hooks
// ---------------------------------------------------------------------------

fn snapshot(dir: &std::path::Path) -> Vec<String> {
    fn walk(d: &std::path::Path, base: &std::path::Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap().flatten() {
            let p = e.path();
            out.push(format!(
                "{}={}",
                p.strip_prefix(base).unwrap().display(),
                std::fs::read_to_string(&p).unwrap_or_default()
            ));
            if p.is_dir() {
                walk(&p, base, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn sample_project(
    tag: &str,
) -> (std::path::PathBuf, pebbles_core::Signal<FileTree>, pebbles_widgets::FileExplorer) {
    let (dir, tree, ex) = fs_explorer(tag);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]").unwrap();
    std::fs::write(dir.join(".env"), "SECRET=1").unwrap();
    assert!(ex.open_folder(&dir));
    let src = id_of(tree, "src");
    ex.toggle_folder(src);
    (dir, tree, ex)
}

#[test]
fn read_only_refuses_every_mutation_without_touching_disk() {
    use pebbles_widgets::{Capability, ExplorerCapabilities, ExplorerError};
    let (dir, tree, ex) = sample_project("readonly");
    let ex = ex.read_only();
    assert!(ex.get_capabilities().is_read_only());
    assert_eq!(ExplorerCapabilities::read_only(), ex.get_capabilities());
    let before = snapshot(&dir);
    let (main, cargo, src, docs) =
        (id_of(tree, "src/main.rs"), id_of(tree, "Cargo.toml"), id_of(tree, "src"), id_of(tree, "docs"));
    assert_eq!(ex.rename_node(main, "x.rs"), Err(ExplorerError::NotAllowed(Capability::Rename)));
    assert_eq!(ex.move_nodes(&[main], Some(docs)), Err(ExplorerError::NotAllowed(Capability::Move)));
    assert_eq!(ex.copy_nodes(&[main], Some(docs)), Err(ExplorerError::NotAllowed(Capability::CopyPaste)));
    assert_eq!(ex.trash_nodes(&[cargo]), Err(ExplorerError::NotAllowed(Capability::Trash)));
    assert_eq!(ex.delete_permanently(&[cargo]), Err(ExplorerError::NotAllowed(Capability::Delete)));
    assert_eq!(
        ex.create_entry(Some(src), FsKind::File, "new.rs"),
        Err(ExplorerError::NotAllowed(Capability::CreateFile))
    );
    assert_eq!(
        ex.create_entry(None, FsKind::Folder, "x"),
        Err(ExplorerError::NotAllowed(Capability::CreateFolder))
    );
    ex.select_only(main);
    assert!(ex.copy_selection().is_err() && ex.cut_selection().is_err());
    assert!(ex.request_delete(false).is_err() && ex.request_delete(true).is_err());
    ex.new_file()();
    assert!(ex.renaming().get().is_none(), "New File does nothing");
    ex.rename_selected()();
    assert!(ex.renaming().get().is_none(), "Rename does nothing");
    ex.delete_selected()();
    assert_eq!(snapshot(&dir), before, "disk untouched");
    // Browsing and selection still work.
    ex.select_only(cargo);
    assert_eq!(ex.selection().get(), vec![cargo]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn individual_capabilities_switch_independently() {
    use pebbles_widgets::{Capability, ExplorerCapabilities, ExplorerError};
    let (dir, tree, ex) = sample_project("caps");
    assert!(tree.peek().root.iter().any(|n| n.name == ".env"), "hidden shown by default");
    ex.set_capabilities(ExplorerCapabilities::all().allow_delete(false).show_hidden(false));
    assert!(!tree.peek().root.iter().any(|n| n.name == ".env"), "hidden files filtered");
    let cargo = id_of(tree, "Cargo.toml");
    assert_eq!(ex.delete_permanently(&[cargo]), Err(ExplorerError::NotAllowed(Capability::Delete)));
    assert!(dir.join("Cargo.toml").exists());
    // Rename still allowed.
    assert!(ex.rename_node(cargo, "Cargo2.toml").is_ok());
    assert!(ex.allows(Capability::Rename) && !ex.allows(Capability::Delete));
    // Multi-select off: Ctrl-toggle behaves as a single select.
    ex.set_capabilities(ExplorerCapabilities::all().allow_multi_select(false));
    let (docs, src) = (id_of(tree, "docs"), id_of(tree, "src"));
    ex.select_only(docs);
    ex.toggle_selected(src);
    assert_eq!(ex.selection().get(), vec![src], "no multi-select: replaces");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn per_item_predicates_protect_specific_entries() {
    use pebbles_widgets::ExplorerError;
    let (dir, tree, ex) = sample_project("preds");
    ex.can_rename(|e| e.name != "Cargo.toml");
    ex.can_delete(|e| e.name != "src");
    ex.can_move(|e| !e.name.ends_with(".toml"));
    ex.can_drop_into(|target, _items| target.is_none_or(|t| t.name != "docs"));
    let (cargo, src, main, docs) =
        (id_of(tree, "Cargo.toml"), id_of(tree, "src"), id_of(tree, "src/main.rs"), id_of(tree, "docs"));
    assert!(matches!(ex.rename_node(cargo, "x.toml"), Err(ExplorerError::Refused(_))));
    assert!(matches!(ex.trash_nodes(&[src]), Err(ExplorerError::Refused(_))));
    assert!(matches!(ex.move_nodes(&[cargo], Some(src)), Err(ExplorerError::Refused(_))));
    assert!(matches!(ex.move_nodes(&[main], Some(docs)), Err(ExplorerError::Refused(_))));
    assert!(matches!(ex.copy_nodes(&[main], Some(docs)), Err(ExplorerError::Refused(_))));
    assert!(dir.join("Cargo.toml").exists() && dir.join("src/main.rs").exists());
    // Unprotected entries still work.
    assert!(ex.move_nodes(&[main], None).is_ok());
    assert!(dir.join("main.rs").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn before_hooks_veto_and_after_events_fire() {
    use pebbles_widgets::{ExplorerError, ExplorerOp};
    let (dir, tree, ex) = sample_project("hooks");
    let log = std::rc::Rc::new(RefCell::new(Vec::<String>::new()));
    let l = log.clone();
    ex.on_after_op(move |op| {
        l.borrow_mut().push(match op {
            ExplorerOp::Create { name, .. } => format!("create {name}"),
            ExplorerOp::Rename { entry, new_name } => format!("rename {} {new_name}", entry.name),
            ExplorerOp::Delete { entries, permanent } => format!("delete {} {permanent}", entries[0].name),
            ExplorerOp::Move { entries, .. } => format!("move {}", entries[0].name),
            ExplorerOp::Copy { entries, .. } => format!("copy {}", entries[0].name),
        })
    });
    ex.on_before_rename(|_, new| !new.contains("bad"));
    ex.on_before_delete(|_, permanent| !permanent);
    ex.on_before_create(|_, _, name| name != "nope.txt");
    ex.on_before_move(|_, target| target.is_some());
    ex.on_before_copy(|items, _| items.len() == 1);
    let (cargo, main, docs, src) =
        (id_of(tree, "Cargo.toml"), id_of(tree, "src/main.rs"), id_of(tree, "docs"), id_of(tree, "src"));
    assert_eq!(ex.rename_node(cargo, "bad.toml"), Err(ExplorerError::Vetoed));
    assert_eq!(ex.delete_permanently(&[cargo]), Err(ExplorerError::Vetoed));
    assert_eq!(ex.create_entry(None, FsKind::File, "nope.txt"), Err(ExplorerError::Vetoed));
    assert_eq!(ex.move_nodes(&[main], None), Err(ExplorerError::Vetoed));
    assert_eq!(ex.copy_nodes(&[main, cargo], Some(docs)), Err(ExplorerError::Vetoed));
    assert!(log.borrow().is_empty(), "nothing happened");
    assert!(
        !dir.join("nope.txt").exists() && dir.join("Cargo.toml").exists() && dir.join("src/main.rs").exists()
    );
    // Allowed ones go through and report.
    ex.rename_node(cargo, "Good.toml").unwrap();
    ex.create_entry(Some(src), FsKind::File, "lib.rs").unwrap();
    ex.move_nodes(&[main], Some(docs)).unwrap();
    ex.copy_nodes(&[main], Some(src)).unwrap();
    ex.trash_nodes(&[id_of(tree, "Good.toml")]).unwrap();
    assert_eq!(
        *log.borrow(),
        [
            "rename Cargo.toml Good.toml",
            "create lib.rs",
            "move main.rs",
            "copy main.rs",
            "delete Good.toml false"
        ]
    );
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(dir.with_extension("trash")).ok();
}

#[test]
fn style_tokens_derive_from_the_theme_follow_light_dark_and_can_be_overridden() {
    use pebbles_widgets::{ExplorerStyle, Theme};
    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    Theme::light().make_current();
    let light = ex.style();
    assert_eq!(light, ExplorerStyle::from_theme(&Theme::light()));
    assert_eq!(light.selection_active_bg, Theme::light().colors.accent);
    Theme::dark().make_current();
    let dark = ex.style();
    assert_ne!(light.row_bg, dark.row_bg, "re-derived for dark");
    assert_ne!(light.git_modified, dark.git_modified);
    // App override, still a function of the theme (so it stays live).
    ex.set_style(|th| ExplorerStyle { indent: 9.0, row_height: Some(30.0), ..ExplorerStyle::from_theme(th) });
    assert_eq!(ex.style().indent, 9.0);
    assert_eq!(ex.style().row_bg, Theme::dark().colors.background);
    Theme::light().make_current();
    assert_eq!(ex.style().row_bg, Theme::light().colors.background, "override follows the theme");
    ex.clear_style();
    assert_eq!(ex.style().indent, 14.0);
}

#[test]
fn fixed_row_height_token_lays_rows_out_at_that_height() {
    use pebbles_widgets::ExplorerStyle;
    let (mut ui, mut env, dir, ex) = mount_fs("rowh");
    ex.set_style(|th| ExplorerStyle { row_height: Some(40.0), ..ExplorerStyle::from_theme(th) });
    let win = Size::new(400.0, 300.0);
    frame(&mut ui, &mut env, win);
    let rows = row_points(&mut ui);
    assert!(rows.len() >= 3);
    let gap = rows[1].y - rows[0].y;
    assert!((gap - 40.0).abs() < 2.5, "rows are 40px apart: {rows:?}");
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Context menus (VS Code sets, pluggable, customizable)
// ---------------------------------------------------------------------------

fn labels(ex: &pebbles_widgets::FileExplorer, t: &pebbles_widgets::MenuTarget) -> Vec<String> {
    ex.menu_for(t).iter().map(|i| if i.is_separator() { "-".to_string() } else { i.label.clone() }).collect()
}

#[test]
fn context_menus_match_vs_code_per_context() {
    use pebbles_widgets::menu_ids as m;
    let (dir, tree, ex) = sample_project("menus");
    let (main, src, cargo) = (id_of(tree, "src/main.rs"), id_of(tree, "src"), id_of(tree, "Cargo.toml"));
    // Without app handlers the app-specific items stay hidden.
    let file = ex.menu_target(Some(main));
    assert_eq!(
        labels(&ex, &file),
        [
            "Open",
            "-",
            "Cut",
            "Copy",
            "Paste",
            "-",
            "Copy Path",
            "Copy Relative Path",
            "-",
            "Rename…",
            "Delete"
        ]
    );
    // Plug the app actions (what Gravel wires).
    for id in [
        m::OPEN_TO_SIDE,
        m::OPEN_WITH,
        m::REVEAL_IN_FILE_MANAGER,
        m::OPEN_IN_TERMINAL,
        m::FIND_IN_FOLDER,
        m::COMPARE_WITH_SELECTED,
    ] {
        ex.set_menu_handler(id, |_| {});
    }
    assert_eq!(
        labels(&ex, &file),
        [
            "Open",
            "Open to the Side",
            "Open With…",
            "-",
            "Reveal in File Manager",
            "Open in Integrated Terminal",
            "-",
            "Select for Compare",
            "-",
            "Cut",
            "Copy",
            "Paste",
            "-",
            "Copy Path",
            "Copy Relative Path",
            "-",
            "Rename…",
            "Delete"
        ]
    );
    let folder = ex.menu_target(Some(src));
    assert_eq!(
        labels(&ex, &folder),
        [
            "New File…",
            "New Folder…",
            "Reveal in File Manager",
            "Open in Integrated Terminal",
            "-",
            "Find in Folder…",
            "-",
            "Cut",
            "Copy",
            "Paste",
            "-",
            "Copy Path",
            "Copy Relative Path",
            "-",
            "Rename…",
            "Delete",
            "Collapse"
        ]
    );
    let empty = ex.menu_target(None);
    assert_eq!(
        labels(&ex, &empty),
        ["New File…", "New Folder…", "-", "Paste", "-", "Refresh", "Collapse All"]
    );
    // Multi-selection: two files.
    ex.select_only(main);
    ex.toggle_selected(cargo);
    let multi = ex.menu_target(Some(main));
    assert_eq!(multi.entries.len(), 2);
    assert_eq!(
        labels(&ex, &multi),
        [
            "Open to the Side",
            "-",
            "Compare Selected",
            "-",
            "Cut",
            "Copy",
            "-",
            "Copy Path",
            "Copy Relative Path",
            "-",
            "Delete"
        ]
    );
    // Paste is greyed until something is on the clipboard.
    let paste = ex.menu_for(&empty).into_iter().find(|i| i.id == m::PASTE).unwrap();
    assert!(!paste.is_enabled(&empty));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn read_only_greys_mutating_menu_items_and_copy_path_works() {
    use pebbles_widgets::menu_ids as m;
    let (dir, tree, ex) = sample_project("menuro");
    let ex = ex.read_only();
    let main = id_of(tree, "src/main.rs");
    let t = ex.menu_target(Some(main));
    for i in ex.menu_for(&t) {
        if i.is_separator() {
            continue;
        }
        let should = matches!(i.id.as_str(), m::OPEN | m::COPY_PATH | m::COPY_RELATIVE_PATH);
        assert_eq!(i.is_enabled(&t), should, "{} enabled={}", i.label, i.is_enabled(&t));
    }
    let item = |id: &str| ex.menu_for(&t).into_iter().find(|i| i.id == id).unwrap();
    item(m::COPY_PATH).run(&t);
    assert_eq!(pebbles_core::clipboard::read(), dir.join("src").join("main.rs").display().to_string());
    item(m::COPY_RELATIVE_PATH).run(&t);
    assert_eq!(
        pebbles_core::clipboard::read(),
        std::path::Path::new("src").join("main.rs").display().to_string()
    );
    // Running a greyed item still can't mutate: the API refuses.
    item(m::DELETE).run(&t);
    assert!(dir.join("src/main.rs").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn menus_are_customizable_and_compare_flows_through_the_handler() {
    use pebbles_widgets::{ExplorerMenuItem, MenuContext, menu_ids as m};
    let (dir, tree, ex) = sample_project("menucustom");
    let (main, cargo) = (id_of(tree, "src/main.rs"), id_of(tree, "Cargo.toml"));
    ex.remove_menu_item(m::COPY_RELATIVE_PATH);
    ex.move_menu_item(m::DELETE, 0);
    let hits = std::rc::Rc::new(std::cell::Cell::new(0));
    let h = hits.clone();
    ex.insert_menu_item_after(
        m::OPEN,
        ExplorerMenuItem::new("app.runFile", "Run File", move |_| h.set(h.get() + 1))
            .visible_when(|t| t.entry().is_some_and(|e| e.name.ends_with(".rs")))
            .enabled_when(|t| t.context == MenuContext::File),
    );
    let t = ex.menu_target(Some(main));
    let ls = labels(&ex, &t);
    assert_eq!(ls[0], "Delete", "reordered");
    assert_eq!(ls[1..3], ["Open".to_string(), "Run File".to_string()]);
    assert!(!ls.contains(&"Copy Relative Path".to_string()), "removed");
    ex.menu_for(&t).into_iter().find(|i| i.id == "app.runFile").unwrap().run(&t);
    assert_eq!(hits.get(), 1);
    assert!(
        !labels(&ex, &ex.menu_target(Some(cargo))).contains(&"Run File".to_string()),
        "visible predicate"
    );
    // Compare: select one, compare with another — the handler gets both.
    let got = std::rc::Rc::new(RefCell::new(Vec::<String>::new()));
    let g = got.clone();
    ex.set_menu_handler(m::COMPARE_WITH_SELECTED, move |t| {
        *g.borrow_mut() = t.entries.iter().map(|e| e.name.clone()).collect();
    });
    let tm = ex.menu_target(Some(main));
    ex.menu_for(&tm).into_iter().find(|i| i.id == m::SELECT_FOR_COMPARE).unwrap().run(&tm);
    ex.select_only(cargo);
    let tc = ex.menu_target(Some(cargo));
    ex.menu_for(&tc).into_iter().find(|i| i.id == m::COMPARE_WITH_SELECTED).expect("offered").run(&tc);
    assert_eq!(*got.borrow(), ["main.rs", "Cargo.toml"]);
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Keyboard: Enter/Space open, type-to-find, keyboard switch, auto-reveal
// ---------------------------------------------------------------------------

thread_local! {
    static KB_DIR: RefCell<Option<std::path::PathBuf>> = const { RefCell::new(None) };
    static KB_EX: RefCell<Option<(pebbles_widgets::FileExplorer, pebbles_core::Signal<FileTree>)>> = const { RefCell::new(None) };
}

fn kb_root() -> impl IntoWidget {
    // The explorer is created INSIDE the component so its shortcuts register
    // for this window (as in an app).
    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    ex.set_trash_dir(KB_DIR.with(|d| d.borrow().clone().unwrap()).with_extension("trash"));
    ex.open_folder(KB_DIR.with(|d| d.borrow().clone().unwrap()));
    KB_EX.with(|c| *c.borrow_mut() = Some((ex, tree)));
    OverlayHost::wrap(container().height(260.0).child(ex.tree()))
}

fn mount_kb(
    tag: &str,
) -> (Ui, std::path::PathBuf, pebbles_widgets::FileExplorer, pebbles_core::Signal<FileTree>) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
    let dir = temp_dir(tag);
    for f in ["alpha.rs", "beta.rs", "bravo.txt", "charlie.md"] {
        std::fs::write(dir.join(f), "").unwrap();
    }
    std::fs::create_dir_all(dir.join("deep/er")).unwrap();
    std::fs::write(dir.join("deep/er/target.txt"), "").unwrap();
    KB_DIR.with(|d| *d.borrow_mut() = Some(dir.clone()));
    let mut ui = Ui::new();
    pebbles_widgets::Theme::tailwind().make_current();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(kb_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);
    let (ex, tree) = KB_EX.with(|c| c.borrow().unwrap());
    (ui, dir, ex, tree)
}

#[test]
fn enter_space_open_type_to_find_and_the_keyboard_switch() {
    use pebbles_core::{Mods, ShortcutKey, shortcuts};
    use pebbles_widgets::{ExplorerCapabilities, OpenMode};
    let (ui, dir, ex, tree) = mount_kb("kbopen");
    let w = ui.window_id();
    let none = Mods::default();
    let opened = std::rc::Rc::new(RefCell::new(Vec::<(String, OpenMode)>::new()));
    let o = opened.clone();
    ex.on_open(move |e, m| o.borrow_mut().push((e.name.clone(), m)));
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::Char('b')), "idle: typing falls through");
    ex.select_only(id_of(tree, "alpha.rs"));
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Enter));
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Space));
    assert_eq!(
        *opened.borrow(),
        [("alpha.rs".to_string(), OpenMode::Open), ("alpha.rs".to_string(), OpenMode::Preview)]
    );
    // Type-to-find: "b" jumps to beta.rs, then "br" refines to bravo.txt.
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Char('b')));
    assert_eq!(ex.selection().get(), vec![id_of(tree, "beta.rs")]);
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Char('r')));
    assert_eq!(ex.selection().get(), vec![id_of(tree, "bravo.txt")]);
    // Keyboard switched off: every binding declines.
    ex.set_capabilities(ExplorerCapabilities::all().keyboard_shortcuts(false));
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::Enter));
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::ArrowDown));
    assert!(!shortcuts::dispatch(w, none, ShortcutKey::Delete));
    assert_eq!(opened.borrow().len(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn reveal_path_expands_lazily_and_selects() {
    let (_ui, dir, ex, tree) = mount_kb("reveal");
    let id = ex.reveal_path(dir.join("deep/er/target.txt")).expect("revealed");
    assert_eq!(ex.selection().get(), vec![id]);
    assert_eq!(id_of(tree, "deep/er/target.txt"), id);
    assert!(
        ex.expanded().get().contains(&id_of(tree, "deep"))
            && ex.expanded().get().contains(&id_of(tree, "deep/er"))
    );
    assert!(ex.reveal_path("/definitely/elsewhere").is_none());
    std::fs::remove_dir_all(&dir).ok();
}

// ---------------------------------------------------------------------------
// Decorations and compact folders
// ---------------------------------------------------------------------------

#[test]
fn decorations_color_badge_and_propagate_to_folders() {
    use pebbles_widgets::{Decoration, ExplorerStyle, GitStatus, Theme};
    let (mut ui, mut env, dir, ex) = mount_fs("deco");
    let tree = TREE_FS.with(|t| t.borrow().unwrap());
    let status = create_signal(std::collections::HashMap::<String, Decoration>::new());
    ex.set_decorations(move |e| status.get().get(&e.name).cloned());
    status.update(|m| {
        m.insert("inner".into(), Decoration { git: Some(GitStatus::Modified), ..Default::default() });
        m.insert("zother".into(), Decoration { errors: 2, warnings: 1, ..Default::default() });
    });
    let st = ExplorerStyle::from_theme(&Theme::tailwind());
    let inner = ex.decoration(id_of(tree, "outer/inner")).unwrap();
    assert_eq!(inner.badge_text().as_deref(), Some("M"));
    assert_eq!(inner.label_color(&st), Some(st.git_modified));
    let z = ex.decoration(id_of(tree, "zother")).unwrap();
    assert_eq!(z.badge_text().as_deref(), Some("3"));
    assert_eq!(z.label_color(&st), Some(st.error), "errors win over git");
    // outer has no decoration of its own: it inherits a dot from inner.
    let outer = ex.decoration(id_of(tree, "outer")).unwrap();
    assert_eq!(outer.badge_text().as_deref(), Some("•"));
    assert_eq!(outer.git, Some(GitStatus::Modified));
    // Dirty badge, and the row actually renders the badge text.
    status.update(|m| {
        m.insert("inner".into(), Decoration { dirty: true, ..Default::default() });
    });
    frame(&mut ui, &mut env, Size::new(400.0, 300.0));
    assert_eq!(ex.decoration(id_of(tree, "outer/inner")).unwrap().badge_text().as_deref(), Some("●"));
    let rt = ui.render_tree();
    let painted = rt.find_all::<pebbles_render::RenderParagraph>().into_iter().any(|id| {
        rt.object_ref(id).downcast_ref::<pebbles_render::RenderParagraph>().is_some_and(|p| p.text == "●")
    });
    assert!(painted, "the ● badge is painted");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn compact_folders_render_chains_as_one_row() {
    let (dir, tree, ex) = fs_explorer("compact");
    std::fs::create_dir_all(dir.join("a/b/c")).unwrap();
    std::fs::write(dir.join("a/b/c/leaf.txt"), "").unwrap();
    std::fs::write(dir.join("top.txt"), "").unwrap();
    assert!(ex.open_folder(&dir));
    let ex = ex.compact_folders(true);
    let a = id_of(tree, "a");
    ex.toggle_folder(a); // expanding the head loads and opens the whole chain
    let c = id_of(tree, "a/b/c");
    let leaf = id_of(tree, "a/b/c/leaf.txt");
    let visible = ex.visible_rows();
    assert_eq!(visible, vec![c, leaf, id_of(tree, "top.txt")], "a/b/c is ONE row");
    // Collapsing the row collapses the chain.
    ex.toggle_folder(c);
    assert_eq!(ex.visible_rows(), vec![c, id_of(tree, "top.txt")]);
    // Off again: the plain tree.
    let ex = ex.compact_folders(false);
    ex.toggle_folder(a);
    assert_eq!(ex.visible_rows()[0], a);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn files_dropped_from_the_os_are_copied_in_and_undo_removes_them() {
    use pebbles_widgets::{Capability, ExplorerCapabilities, ExplorerError, file_drop};
    let (mut ui, mut env, dir, ex) = mount_fs("osdrop");
    let tree = TREE_FS.with(|t| t.borrow().unwrap());
    let outside = temp_dir("osdrop-src");
    std::fs::write(outside.join("photo.png"), "png").unwrap();
    std::fs::create_dir_all(outside.join("pkg/sub")).unwrap();
    std::fs::write(outside.join("pkg/sub/x.txt"), "x").unwrap();
    std::fs::write(dir.join("photo.png"), "existing").unwrap();
    ex.refresh();
    // Through the OS drag-and-drop path (the lone drop target catches it).
    let win = Size::new(400.0, 300.0);
    frame(&mut ui, &mut env, win);
    frame(&mut ui, &mut env, win);
    file_drop::hover(outside.join("photo.png"), Offset::new(100.0, 250.0));
    file_drop::drop_file(outside.join("photo.png"), Offset::new(100.0, 250.0));
    file_drop::drop_file(outside.join("pkg"), Offset::new(100.0, 250.0));
    assert!(file_drop::flush());
    assert_eq!(std::fs::read_to_string(dir.join("photo.png")).unwrap(), "existing", "never overwritten");
    assert_eq!(std::fs::read_to_string(dir.join("photo copy.png")).unwrap(), "png");
    assert_eq!(std::fs::read_to_string(dir.join("pkg/sub/x.txt")).unwrap(), "x");
    assert!(outside.join("pkg/sub/x.txt").exists(), "the source is untouched (copied, not moved)");
    let _ = id_of(tree, "pkg");
    // Into a specific folder via the API.
    let z = id_of(tree, "zother");
    ex.import_paths(&[outside.join("photo.png")], Some(z)).unwrap();
    assert!(dir.join("zother/photo.png").exists());
    assert!(ex.undo());
    assert!(!dir.join("zother/photo.png").exists());
    // Switched off: refused.
    ex.set_capabilities(ExplorerCapabilities::all().allow_drop_from_os(false));
    assert_eq!(
        ex.import_paths(&[outside.join("photo.png")], Some(z)),
        Err(ExplorerError::NotAllowed(Capability::DropFromOs))
    );
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&outside).ok();
    std::fs::remove_dir_all(dir.with_extension("trash")).ok();
}

thread_local! {
    static RERENDER: RefCell<Option<pebbles_core::Signal<u32>>> = const { RefCell::new(None) };
    static RR_EX: RefCell<Option<pebbles_widgets::FileExplorer>> = const { RefCell::new(None) };
}

fn rerender_root() -> impl IntoWidget {
    let bump = RERENDER.with(|r| r.borrow().unwrap());
    let _ = bump.get(); // re-run this component on demand
    let tree = create_signal(FileTree::new());
    let ex = file_explorer(tree);
    RR_EX.with(|c| *c.borrow_mut() = Some(ex));
    OverlayHost::wrap(container().height(100.0).child(ex.tree()))
}

#[test]
fn menu_customizations_survive_component_re_renders() {
    use pebbles_widgets::ExplorerMenuItem;
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let bump = create_signal(0u32);
    RERENDER.with(|r| *r.borrow_mut() = Some(bump));
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(rerender_root)).into_widget());
    frame(&mut ui, &mut env, win);
    let ex = RR_EX.with(|c| c.borrow().unwrap());
    ex.add_menu_item(ExplorerMenuItem::new("app.x", "X", |_| {}));
    bump.set(1);
    frame(&mut ui, &mut env, win);
    let ex = RR_EX.with(|c| c.borrow().unwrap());
    assert!(ex.menu_items().iter().any(|i| i.id == "app.x"), "custom item kept across a re-render");
}

// ---------------------------------------------------------------------------
// Bounded box: rows are clipped to the explorer's viewport and scroll
// ---------------------------------------------------------------------------

fn bounded_root() -> impl IntoWidget {
    let mut t = FileTree::new();
    for i in 0..30 {
        t.insert(None, FsKind::File, format!("file{i:02}.txt"));
    }
    let ex = file_explorer(create_signal(t));
    // A 120 px box — far fewer rows fit than the 30 in the tree. Below it, a
    // sibling the rows must not paint over (the gallery's next heading).
    column(vec![
        container().height(120.0).child(ex.tree()).into_widget(),
        pebbles_widgets::text("below").into_widget(),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
}

#[test]
fn rows_clip_and_scroll_inside_a_bounded_box() {
    use pebbles_render::RenderScroll;
    pebbles_widgets::overlay::init();
    let mut h = pebbles_testing::Harness::new().window(300.0, 400.0);
    h.mount(bounded_root);
    h.draw();

    // The tree is a scroll viewport no taller than the box, with content to scroll.
    let sid = h.find::<RenderScroll>().expect("the explorer rows sit in a scroll viewport");
    let tree = h.ui.render_tree();
    let top = tree.absolute_offset(sid).y;
    let vh = h.size_of(sid).height;
    assert!(vh <= 120.0 + 0.5, "viewport is bounded by the box, got {vh}");
    let max = h.object::<RenderScroll>(sid).max_offset;
    assert!(max > 0.0, "30 rows overflow a 120 px box, so the tree scrolls (max_offset {max})");

    // The heading below starts after the box, not under the overflowing rows.
    let below = h.find_all_text("below")[0];
    assert!(tree.absolute_offset(below).y >= top + vh - 0.5, "the next sibling sits below the viewport");

    // The last row is outside the viewport (clipped) before scrolling…
    let last = h.find_all_text("file29.txt")[0];
    let y0 = tree.absolute_offset(last).y;
    assert!(y0 >= top + vh, "the last row starts below the viewport before scrolling ({y0})");

    // …and a wheel scroll over the tree brings it into view.
    h.scroll(Offset::new(100.0, top + 20.0), 10_000.0);
    h.settle();
    h.draw();
    let tree = h.ui.render_tree();
    let off = h.object::<RenderScroll>(sid).offset;
    assert!((off - max).abs() < 1.0, "scrolled to the end (offset {off}, max {max})");
    let last = h.find_all_text("file29.txt")[0];
    let y1 = tree.absolute_offset(last).y;
    let rh = h.size_of(last).height;
    assert!(y1 >= top - 0.5 && y1 + rh <= top + vh + 0.5, "the last row is inside the viewport ({y1})");
}

// ---------------------------------------------------------------------------
// Reveal: keyboard focus never leaves a scrolled viewport (VS Code)
// ---------------------------------------------------------------------------

thread_local! {
    static RV_EX: RefCell<Option<pebbles_widgets::FileExplorer>> = const { RefCell::new(None) };
}

fn reveal_root() -> impl IntoWidget {
    let mut t = FileTree::new();
    for i in 0..30 {
        t.insert(None, FsKind::File, format!("file{i:02}.txt"));
    }
    let ex = file_explorer(create_signal(t));
    RV_EX.with(|c| *c.borrow_mut() = Some(ex));
    ex.set_decorations(|e| {
        Some(pebbles_widgets::Decoration { errors: 1 + e.name.len() % 7, ..Default::default() })
    });
    column(vec![container().height(120.0).child(ex.tree()).into_widget()])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
}

/// (viewport top, viewport height, scroll offset).
fn viewport(h: &pebbles_testing::Harness) -> (f64, f64, f64) {
    use pebbles_render::RenderScroll;
    let sid = h.find::<RenderScroll>().expect("scroll viewport");
    let top = h.ui.render_tree().absolute_offset(sid).y;
    (top, h.size_of(sid).height, h.object::<RenderScroll>(sid).offset)
}

/// Whether the row labelled `name` is fully inside the viewport.
fn row_visible(h: &pebbles_testing::Harness, name: &str) -> bool {
    let (top, vh, _) = viewport(h);
    let Some(&t) = h.find_all_text(name).first() else { return false };
    let y = h.ui.render_tree().absolute_offset(t).y;
    let rh = h.size_of(t).height;
    y >= top - 0.5 && y + rh <= top + vh + 0.5
}

#[test]
fn keyboard_navigation_reveals_the_active_row_with_minimal_scroll() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
    let mut h = pebbles_testing::Harness::new().window(300.0, 400.0);
    h.mount(reveal_root);
    h.draw();
    let ex = RV_EX.with(|c| c.borrow().unwrap());
    let ids = ex.visible_rows();
    let name = |i: usize| format!("file{i:02}.txt");

    // End jumps to the last row and scrolls it into view (bottom-aligned).
    ex.select_only(ids[0]);
    h.press("End");
    h.settle();
    h.draw();
    assert!(row_visible(&h, &name(29)), "End revealed the last row");
    let (_, _, off_end) = viewport(&h);
    assert!(off_end > 0.0, "the viewport scrolled ({off_end})");

    // ArrowUp within the viewport moves the focus without scrolling.
    h.press("ArrowUp");
    h.settle();
    h.draw();
    assert!((viewport(&h).2 - off_end).abs() < 0.5, "a visible row needs no scroll");

    // Home scrolls back to the top.
    h.press("Home");
    h.settle();
    h.draw();
    assert!(viewport(&h).2.abs() < 0.5, "Home scrolled to the top");
    assert!(row_visible(&h, &name(0)));

    // Walking down past the bottom edge scrolls by the minimum: the active row
    // sits exactly at the bottom of the viewport each step.
    for i in 1..12 {
        h.press("ArrowDown");
        h.settle();
        h.draw();
        assert!(row_visible(&h, &name(i)), "ArrowDown kept row {i} in view");
    }
    // Exactly minimal: row 11's bottom edge is the viewport's bottom edge.
    let (top, vh, off) = viewport(&h);
    let y = |i: usize| h.ui.render_tree().absolute_offset(h.find_all_text(&name(i))[0]).y;
    let rh = y(11) - y(10);
    assert!(
        (off - (12.0 * rh - vh)).abs() < 1.0,
        "minimal scroll (offset {off}, row {rh}, viewport {vh}, top {top})"
    );

    // Type-to-find jumps to (and reveals) a far row.
    h.press("Home");
    h.settle();
    h.draw();
    ex.select_only(ids[0]);
    for ch in "file25".chars() {
        h.press(&ch.to_string());
    }
    h.settle();
    h.draw();
    assert_eq!(ex.selection().peek(), vec![ids[25]], "type-to-find selected file25");
    assert!(row_visible(&h, &name(25)), "type-to-find revealed it");

    // The programmatic reveal (what auto-reveal of the active editor's file
    // calls via `reveal_path`) does the same minimal scroll.
    h.press("Home");
    h.settle();
    h.draw();
    ex.scroll_into_view(ids[15]);
    h.settle();
    h.draw();
    assert!(row_visible(&h, &name(15)), "scroll_into_view revealed row 15");

    // F2 rename reveals the row being renamed.
    h.press("Home");
    h.settle();
    h.draw();
    ex.select_only(ids[20]);
    h.press("F2");
    h.settle();
    h.draw();
    assert_eq!(ex.renaming().peek(), Some(ids[20]));
    assert!(viewport(&h).2 > 0.0, "F2 revealed the renamed row");
}

#[test]
fn new_file_input_is_scrolled_into_view() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut h = pebbles_testing::Harness::new().window(300.0, 400.0);
    h.mount(reveal_root);
    h.draw();
    let ex = RV_EX.with(|c| c.borrow().unwrap());
    let ids = ex.visible_rows();
    // Active row at the bottom while the viewport shows the top.
    ex.select_only(ids[29]);
    h.draw();
    assert!(viewport(&h).2.abs() < 0.5);
    ex.begin_create(FsKind::File);
    h.settle();
    h.draw();
    assert!(ex.renaming().peek().is_some(), "inline create started");
    assert!(viewport(&h).2 > 0.0, "the viewport scrolled to the new-file input");
    let pending = ex.renaming().peek().unwrap();
    let pos = ex.visible_rows().iter().position(|&v| v == pending).unwrap();
    // Rows are uniform: the input row (last) is bottom-aligned in the viewport.
    let y = |n: &str| h.ui.render_tree().absolute_offset(h.find_all_text(n)[0]).y;
    let rh = y("file29.txt") - y("file28.txt");
    let (_, vh, off) = viewport(&h);
    assert!(
        pos as f64 * rh >= off - 0.5 && (pos as f64 + 1.0) * rh <= off + vh + 0.5,
        "the new-file input row (#{pos}) is inside the viewport (offset {off})"
    );
}

#[test]
fn decoration_badges_stay_clear_of_the_scrollbar() {
    use pebbles_render::RenderScroll;
    pebbles_widgets::overlay::init();
    let mut h = pebbles_testing::Harness::new().window(300.0, 400.0);
    h.mount(reveal_root);
    h.draw();
    let sid = h.find::<RenderScroll>().unwrap();
    let tree = h.ui.render_tree();
    let right = tree.absolute_offset(sid).x + h.size_of(sid).width;
    let sb = pebbles_render::ScrollbarStyle::default();
    let bar_left = right - sb.margin - sb.thickness;
    let mut checked = 0;
    for b in ["1", "2", "3", "4", "5", "6", "7"] {
        for t in h.find_all_text(b) {
            let x1 = tree.absolute_offset(t).x + h.size_of(t).width;
            assert!(x1 <= bar_left + 0.5, "badge {b} ends at {x1}, under the scrollbar (starts {bar_left})");
            checked += 1;
        }
    }
    assert!(checked > 0, "badges rendered");
}
