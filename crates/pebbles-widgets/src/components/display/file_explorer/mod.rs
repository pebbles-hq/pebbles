//! [`FileExplorer`] — a VSCode-style file explorer built from separate,
//! composable pieces:
//!
//! * [`FileTree`] — the app-owned data model (wrap it in a `Signal`): stable
//!   node ids, and the mutation helpers (`insert`, `rename`, `delete`,
//!   `move_node`).
//! * [`file_explorer`] — a `Copy` controller over that signal: multi-selection,
//!   expansion, inline-rename and drag state, plus the action closures.
//! * [`FileExplorer::toolbar`] / [`FileExplorer::tree`] — the default widget
//!   pieces; skip them and compose your own buttons from the action closures
//!   (`new_file`, `new_folder`, `rename_selected`, `delete_selected`,
//!   `collapse_all`) wherever your layout wants them.
//!
//! The tree follows the desktop file-explorer standards: click selects,
//! **Ctrl/Cmd-click toggles**, **Shift-click range-selects**; double-click
//! renames inline (Enter/blur commits, Escape cancels); right-click opens a
//! context menu (new/rename/delete — new nodes land in the selected folder);
//! right-clicking empty space offers New File/New Folder; and **dragging a
//! selection onto a folder moves them all** (hovering a collapsed folder
//! during a drag expands it; dropping on empty space moves to the root).

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Cursor, IconKind};

use crate::components::{ButtonVariant, context_menu, icon, icon_button, menu_item, muted, text_field};
use crate::theme::{mix, theme};
use crate::widgets::{Container, Expanded, GestureDetector, Padding, column, gap_w, row, text};
use pebbles_core::children;
use pebbles_core::keyboard::{KeyInput, ctrl_held, shift_held};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animated, component_props, create_signal};

mod node;
mod tree;

pub use tree::{FileTree, FsKind, FsNode};
#[cfg(feature = "file-dialogs")]
pub use tree::pick_folder;

use node::{NodeProps, render_node};
use tree::{read_dir, unique_name};


// ---------------------------------------------------------------------------
// The explorer controller
// ---------------------------------------------------------------------------

/// The shared explorer state over an app-owned [`FileTree`] signal — `Copy`.
/// Wire your own buttons with the action closures; render the default pieces
/// with [`toolbar`](FileExplorer::toolbar) / [`tree`](FileExplorer::tree).
#[derive(Clone, Copy)]
pub struct FileExplorer {
    tree: Signal<FileTree>,
    selected: Signal<Vec<u64>>,
    expanded: Signal<HashSet<u64>>,
    renaming: Signal<Option<u64>>,
    dragging: Signal<bool>,
    drop_target: Signal<Option<u64>>,
    root_drop: Signal<bool>,
    hover_key: u64,
    /// Filesystem mode: the directory the explorer mirrors (None = in-memory).
    fs_root: Signal<Option<PathBuf>>,
    /// The last filesystem error, if any (read for toasts/inline hints).
    last_error: Signal<Option<String>>,
}

/// Create an explorer over `tree` (the app's `Signal<FileTree>`). Call inside
/// a component render.
pub fn file_explorer(tree: Signal<FileTree>) -> FileExplorer {
    FileExplorer {
        tree,
        selected: create_signal(Vec::new()),
        expanded: create_signal(HashSet::new()),
        renaming: create_signal(None),
        dragging: create_signal(false),
        drop_target: create_signal(None),
        root_drop: create_signal(false),
        hover_key: create_signal(()).raw_id(),
        fs_root: create_signal(None),
        last_error: create_signal(None),
    }
}

impl FileExplorer {
    /// The selected node ids (Ctrl/Cmd-click toggles, Shift-click ranges).
    pub fn selection(&self) -> Signal<Vec<u64>> {
        self.selected
    }

    /// The expansion set (read it, or drive it yourself).
    pub fn expanded(&self) -> Signal<HashSet<u64>> {
        self.expanded
    }

    /// The backing directory (filesystem mode), or `None` for the in-memory
    /// model. When set, expanding a folder reads its children from disk and
    /// every mutation hits the real filesystem.
    pub fn fs_root(&self) -> Signal<Option<PathBuf>> {
        self.fs_root
    }

    /// The last filesystem error (read it for toasts/inline hints).
    pub fn last_error(&self) -> Signal<Option<String>> {
        self.last_error
    }

    /// Load a REAL directory into the explorer (children only — folders read
    /// their own children lazily on first expand). Clears the selection and
    /// expansion; reports failures through [`last_error`](Self::last_error).
    pub fn open_folder(&self, path: impl AsRef<Path>) -> bool {
        match read_dir(path.as_ref()) {
            Ok(children) => {
                self.fs_root.set(Some(path.as_ref().to_path_buf()));
                self.tree.update(|t| {
                    let mut children = children;
                    t.assign_ids(&mut children);
                    t.root = children;
                });
                self.selected.set(Vec::new());
                self.expanded.set(HashSet::new());
                self.renaming.set(None);
                self.last_error.set(None);
                true
            }
            Err(e) => {
                self.last_error.set(Some(format!("Could not open {}: {e}", path.as_ref().display())));
                false
            }
        }
    }

    /// The absolute path of a node (filesystem mode only — in-memory nodes
    /// have none).
    pub fn path_of(&self, id: u64) -> Option<PathBuf> {
        let root = self.fs_root.get()?;
        // Walk up through the parents, collecting names.
        let mut names = vec![self.tree.peek().node(id)?.name.clone()];
        let mut cur = self.tree.peek().parent_of(id);
        while let Some(p) = cur {
            names.push(self.tree.peek().node(p)?.name.clone());
            cur = self.tree.peek().parent_of(p);
        }
        names.reverse();
        Some(names.iter().fold(root, |acc, n| acc.join(n)))
    }

    /// Read a folder's children from disk into the model (filesystem mode).
    fn ensure_loaded(&self, id: u64) {
        if self.fs_root.get().is_none() {
            return;
        }
        let needs = self
            .tree
            .peek()
            .node(id)
            .is_some_and(|n| n.kind == FsKind::Folder && !n.loaded);
        if !needs {
            return;
        }
        if let Some(dir) = self.path_of(id) {
            match read_dir(&dir) {
                Ok(children) => {
                    self.tree.update(|t| {
                        let mut children = children;
                        t.assign_ids(&mut children);
                        if let Some(n) = t.node_mut(id) {
                            n.children = children;
                            n.loaded = true;
                        }
                    });
                }
                Err(e) => {
                    self.last_error.set(Some(format!("Could not read {}: {e}", dir.display())));
                }
            }
        }
    }

    /// Expand/collapse a folder (loading its children from disk first, in
    /// filesystem mode).
    pub fn toggle_folder(&self, id: u64) {
        self.ensure_loaded(id);
        self.expanded.update(|ex| {
            if !ex.remove(&id) {
                ex.insert(id);
            }
        });
    }

    /// Rename a node — on disk in filesystem mode, in the model otherwise.
    pub fn rename_node(&self, id: u64, name: String) -> bool {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.renaming.set(None);
            return false;
        }
        let done = if let Some(root) = self.fs_root.get() {
            let old = self.path_of(id);
            match old {
                Some(old) => {
                    let new = old.with_file_name(&name);
                    match std::fs::rename(&old, &new) {
                        Ok(()) => {
                            self.tree.update(|t| {
                                t.rename(id, name);
                            });
                            true
                        }
                        Err(e) => {
                            self.last_error.set(Some(format!("Could not rename: {e}")));
                            false
                        }
                    }
                }
                None => {
                    self.last_error.set(Some(format!("Could not resolve a path for a node in {root:?}")));
                    false
                }
            }
        } else {
            self.tree.update(|t| {
                t.rename(id, name);
            });
            true
        };
        self.renaming.set(None);
        done
    }

    /// Delete nodes — on disk in filesystem mode, in the model otherwise.
    fn delete_nodes(&self, ids: &[u64]) {
        for id in ids {
            if self.fs_root.get().is_some() {
                if let Some(path) = self.path_of(*id) {
                    let is_dir = self.tree.peek().node(*id).is_some_and(|n| n.kind == FsKind::Folder);
                    let res = if is_dir {
                        std::fs::remove_dir_all(&path)
                    } else {
                        std::fs::remove_file(&path)
                    };
                    if let Err(e) = res {
                        self.last_error.set(Some(format!("Could not delete {}: {e}", path.display())));
                        continue;
                    }
                } else {
                    self.last_error.set(Some(format!("Could not resolve a path for a node")));
                    continue;
                }
            }
            self.tree.update(|t| {
                t.delete(*id);
            });
        }
        self.selected.set(Vec::new());
        self.renaming.set(None);
    }

    /// Move nodes into a folder (or the root) — on disk in filesystem mode,
    /// in the model otherwise. Also used programmatically.
    pub fn move_nodes(&self, ids: &[u64], target: Option<u64>) {
        let fs = self.fs_root.get().is_some();
        for id in ids {
            if fs {
                let Some(from) = self.path_of(*id) else {
                    self.last_error.set(Some("Could not resolve a path for a node".into()));
                    continue;
                };
                let into = match target {
                    Some(t) => self.path_of(t),
                    None => self.fs_root.get(),
                };
                let Some(into) = into else {
                    self.last_error.set(Some("Could not resolve the target folder".into()));
                    continue;
                };
                let to = into.join(from.file_name().unwrap_or_default());
                if let Err(e) = std::fs::rename(&from, &to) {
                    self.last_error.set(Some(format!("Could not move {}: {e}", from.display())));
                    continue;
                }
            }
            self.tree.update(|t| {
                t.move_node(*id, target);
            });
        }
        if let Some(t) = target {
            self.expanded.update(|e| {
                e.insert(t);
            });
        }
    }

    /// The active node — the LAST selected one (rename/new-node targets).
    fn active(&self) -> Option<u64> {
        self.selected.get().last().copied()
    }

    /// The visible nodes in display order (folders expanded).
    fn visible_ids(&self) -> Vec<u64> {
        fn walk(nodes: &[FsNode], expanded: &HashSet<u64>, out: &mut Vec<u64>) {
            for n in nodes {
                out.push(n.id);
                if n.kind == FsKind::Folder && expanded.contains(&n.id) {
                    walk(&n.children, expanded, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.tree.peek().root, &self.expanded.peek(), &mut out);
        out
    }

    pub fn select_only(&self, id: u64) {
        self.selected.set(vec![id]);
    }

    fn toggle_select(&self, id: u64) {
        self.selected.update(|s| {
            if let Some(i) = s.iter().position(|&v| v == id) {
                s.remove(i);
            } else {
                s.push(id);
            }
        });
    }

    fn range_select(&self, id: u64) {
        let visible = self.visible_ids();
        let anchor = self.active().and_then(|a| visible.iter().position(|&v| v == a));
        let end = visible.iter().position(|&v| v == id);
        match (anchor, end) {
            (Some(a), Some(b)) => {
                let (lo, hi) = (a.min(b), a.max(b));
                self.selected.set(visible[lo..=hi].to_vec());
            }
            _ => self.select_only(id),
        }
    }

    /// Where new nodes land: the active folder, else the active file's parent,
    /// else the root.
    fn insertion_parent(&self) -> Option<u64> {
        match self.active() {
            Some(id) => match self.tree.peek().node(id) {
                Some(n) if n.kind == FsKind::Folder => Some(id),
                Some(_) => self.tree.peek().parent_of(id),
                None => None,
            },
            None => None,
        }
    }

    /// Expand every ancestor of `id` so it is visible.
    fn reveal(&self, id: u64) {
        let mut cur = self.tree.peek().parent_of(id);
        while let Some(p) = cur {
            self.expanded.update(|e| {
                e.insert(p);
            });
            cur = self.tree.peek().parent_of(p);
        }
    }

    fn start_rename_for(&self, id: u64) {
        self.select_only(id);
        // Drop any held focus (e.g. the button that was just clicked) so the
        // rename editor's one-shot autofocus actually grabs the keyboard.
        pebbles_core::focus::set_focus(None);
        self.renaming.set(Some(id));
    }

    /// Create a file (in the active folder) and start renaming it. In
    /// filesystem mode the file is created on disk.
    pub fn new_file(self) -> impl Fn() + 'static {
        move || {
            let parent = self.insertion_parent();
            let (id, name) = self.create_node(parent, FsKind::File, "new_file.txt");
            self.start_rename_for(id);
            let _ = name;
        }
    }

    /// Create a folder (in the active folder) and start renaming it. In
    /// filesystem mode the folder is created on disk.
    pub fn new_folder(self) -> impl Fn() + 'static {
        move || {
            let parent = self.insertion_parent();
            let (id, _name) = self.create_node(parent, FsKind::Folder, "New Folder");
            self.start_rename_for(id);
        }
    }

    /// Create a node — on disk in filesystem mode (unique name probed against
    /// the real directory), then inserted into the model.
    fn create_node(&self, parent: Option<u64>, kind: FsKind, base: &'static str) -> (u64, String) {
        if let Some(root) = self.fs_root.get() {
            let dir = match parent {
                Some(p) => self.path_of(p).unwrap_or_else(|| root.clone()),
                None => root,
            };
            let taken = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let taken_refs: Vec<&str> = taken.iter().map(|s| s.as_str()).collect();
            let name = unique_name(base, &taken_refs);
            let target = dir.join(&name);
            let created = if kind == FsKind::Folder {
                std::fs::create_dir(&target).map_err(|e| e.to_string())
            } else {
                std::fs::File::create(&target).map(|_| ()).map_err(|e| e.to_string())
            };
            match created {
                Ok(()) => {
                    let id = self.insert_at(parent, kind, Box::leak(name.clone().into_boxed_str()));
                    (id, name)
                }
                Err(e) => {
                    self.last_error.set(Some(format!("Could not create {}: {e}", target.display())));
                    let id = self.insert_at(parent, kind, base);
                    (id, base.to_string())
                }
            }
        } else {
            let id = self.insert_at(parent, kind, base);
            (id, base.to_string())
        }
    }

    fn insert_at(&self, parent: Option<u64>, kind: FsKind, name: &'static str) -> u64 {
        let cell = RefCell::new(None);
        self.tree.update(|t| {
            *cell.borrow_mut() = Some(t.insert(parent, kind, name));
        });
        let id = cell.take().expect("inserted");
        self.reveal(id);
        id
    }

    /// Start renaming the active node (a no-op with no selection).
    pub fn rename_selected(self) -> impl Fn() + 'static {
        move || {
            if let Some(id) = self.active() {
                self.renaming.set(Some(id));
            }
        }
    }

    /// Delete the selection (all selected nodes, subtrees included) — on
    /// disk in filesystem mode.
    pub fn delete_selected(self) -> impl Fn() + 'static {
        move || {
            let ids = self.selected.get();
            self.delete_nodes(&ids);
        }
    }

    /// Collapse every folder.
    pub fn collapse_all(self) -> impl Fn() + 'static {
        move || self.expanded.set(HashSet::new())
    }

    /// The default action row: New File · New Folder · Collapse All. Optional —
    /// compose your own buttons from the action closures wherever you want.
    pub fn toolbar(self) -> impl IntoWidget {
        row(children![
            icon_button(pebbles_render::lucide::FILE_PLUS)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .on_pressed(self.new_file()),
            gap_w(2.0),
            icon_button(pebbles_render::lucide::FOLDER_PLUS)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .on_pressed(self.new_folder()),
            gap_w(2.0),
            icon_button(pebbles_render::lucide::CHEVRONS_DOWN_UP)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .on_pressed(self.collapse_all()),
        ])
        .main_axis_size(MainAxisSize::Min)
    }

    /// The tree: select (Ctrl/Shift multi), expand, inline rename, context
    /// menus, and drag-to-move (multi-drag included). Right-clicking empty
    /// space offers New File/New Folder; dropping on empty space moves to the
    /// root.
    pub fn tree(self) -> impl IntoWidget {
        let model = self.tree.get();
        let mut kids: Vec<AnyWidget> = Vec::new();
        for node in &model.root {
            kids.push(
                component_props(
                    render_node,
                    NodeProps { explorer: self, node: node.clone(), depth: 0 },
                )
                .into_widget(),
            );
        }
        if kids.is_empty() {
            kids.push(
                Padding::new(EdgeInsets::all(12.0), muted("Empty — add a file or folder.")).into_widget(),
            );
        }
        // Fill the parent's height when it gives one (a panel), shrink-wrap
        // otherwise — the empty space stays part of the explorer (right-click
        // there = New File/New Folder; drop there = move to root).
        let body = column(kids)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Max);

        // Empty-space right-click → New File / New Folder; empty-space drop
        // (while dragging) → move to the root.
        let root_gesture = GestureDetector::new(body)
            .on_hover_enter({
                move || {
                    if self.dragging.get() {
                        self.root_drop.set(true);
                    }
                }
            })
            .on_hover_exit({
                move || self.root_drop.set(false)
            });
        context_menu(root_gesture)
            .item(menu_item("New File").on_select(self.new_file()))
            .item(menu_item("New Folder").on_select(self.new_folder()))
    }
}
