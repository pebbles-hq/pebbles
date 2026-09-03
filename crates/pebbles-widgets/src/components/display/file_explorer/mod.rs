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
//!
//! **Right-click is built in and always on** — the explorer's own context
//! menus work regardless of the app-wide global-menu switch
//! ([`set_global_menu_enabled`](crate::set_global_menu_enabled)); that switch
//! only controls the *fallback* menu on unclaimed surfaces. Widget-specific
//! always wins.
//!
//! **Keyboard** (every binding is conditional — it declines when it doesn't
//! apply, so nothing is hijacked): **↑/↓** walk the visible rows (**Shift**
//! extends), **Mod+↑/↓** move the FOCUS ring without selecting and
//! **Mod+Space** toggles the focused row in/out (one-by-one multi-select, the
//! Windows/VSCode pattern), **→** expands / steps into, **←** collapses /
//! jumps to the parent, **Home/End** jump to the first/last row, **F2**
//! renames (the editor opens PREFILLED with the current name, stem selected),
//! **Delete** deletes, **Mod+A** selects all visible (works from idle),
//! **Mod+C/X/V** copy/cut/paste (cut rows dim; Copy duplicates whole subtrees,
//! on disk too in filesystem mode), **Escape** cancels a pending cut/copy,
//! then clears the selection. A focused editor always wins its own keys first.
//!
//! **Controllable from outside** — the widget ships the tree, you ship the
//! chrome: bind any input to [`filter`](FileExplorer::filter) (live pruning,
//! matched folders keep their subtree, folders force-expand while filtering,
//! the keyboard walks exactly the filtered rows), drive
//! [`selection`](FileExplorer::selection)/[`expanded`](FileExplorer::expanded)/
//! [`active_row`](FileExplorer::active_row) directly, and use
//! [`reveal`](FileExplorer::reveal) + [`select_only`](FileExplorer::select_only)
//! for "Reveal in Explorer", [`expand_all`](FileExplorer::expand_all)/
//! [`collapse_all`](FileExplorer::collapse_all), [`open_folder`](FileExplorer::open_folder)/
//! [`detach_folder`](FileExplorer::detach_folder).
//!
//! **Icons are themable** ([`set_icon_theme`](FileExplorer::set_icon_theme) —
//! the hook an IDE's icon theming plugs into): a resolver maps every node to
//! any bundled lucide glyph + color, per-node [`FsNode::icon`]/[`FsNode::color`]
//! overrides win, and the defaults show open/closed folder glyphs.
//!
//! Rows carry the standard state set: hover tint, selected (accent), active
//! (focus ring), cut (dimmed), drop target.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::path::{Path, PathBuf};

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, BoxDecoration, Cursor, IconData, IconKind, lucide};

use crate::components::{ButtonVariant, context_menu, icon, icon_button, menu_item, muted, text_field};
use crate::theme::{mix, theme};
use crate::widgets::{Container, Expanded, GestureDetector, Padding, column, gap_w, row, text};
use pebbles_core::children;
use pebbles_core::keyboard::{KeyInput, ctrl_held, shift_held};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animated, component_props, create_shortcut_if, create_signal};

mod node;
mod tree;

pub use tree::{FileTree, FsKind, FsNode};
#[cfg(feature = "file-dialogs")]
pub use tree::pick_folder;

use node::{NodeProps, render_node};
use tree::{copy_path, read_dir, unique_name};


// ---------------------------------------------------------------------------
// The explorer controller
// ---------------------------------------------------------------------------

/// An icon theme: resolves a node (+ whether it renders expanded) to a glyph
/// and an optional color. Return `None` to fall through to the default
/// folder/file look for that node. Installed with
/// [`FileExplorer::set_icon_theme`] — the hook an IDE's icon theming plugs into.
pub(super) type IconTheme = Rc<dyn Fn(&FsNode, bool) -> Option<(IconData, Option<Color>)>>;

/// Whether `n`'s own name matches the (lowercased) filter query.
pub(super) fn name_matches(n: &FsNode, q: &str) -> bool {
    !q.is_empty() && n.name.to_lowercase().contains(q)
}

/// The filter rule: with a non-empty query a node stays visible when its name
/// matches, an ANCESTOR's name matched (`anc` — a matched folder shows its
/// contents), or any descendant matches.
pub(super) fn filter_keeps(n: &FsNode, q: &str, anc: bool) -> bool {
    fn descendant(n: &FsNode, q: &str) -> bool {
        name_matches(n, q) || n.children.iter().any(|c| descendant(c, q))
    }
    q.is_empty() || anc || descendant(n, q)
}

/// What a clipboard entry does on paste.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ClipMode {
    Copy,
    Cut,
}

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
    /// The inline-rename buffer — prefilled with the current name at rename start.
    pub(super) rename_buf: Signal<String>,
    /// The explorer clipboard: node ids + copy/cut, filled by Mod+C/X and the menus.
    pub(super) clipboard: Signal<Option<(Vec<u64>, ClipMode)>>,
    /// The installed icon theme, if any (see [`set_icon_theme`](Self::set_icon_theme)).
    pub(super) icon_theme: Signal<Option<IconTheme>>,
    /// The keyboard-focus row (the ring) — independent of the selection, so
    /// Mod+↑/↓ can walk rows without selecting (toggle with Mod+Space).
    pub(super) active: Signal<Option<u64>>,
    /// The live filter query (case-insensitive substring) — bind your own input.
    pub(super) filter: Signal<String>,
}

/// Create an explorer over `tree` (the app's `Signal<FileTree>`). Call inside
/// a component render.
pub fn file_explorer(tree: Signal<FileTree>) -> FileExplorer {
    let explorer = FileExplorer {
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
        rename_buf: create_signal(String::new()),
        clipboard: create_signal(None),
        icon_theme: create_signal(None),
        active: create_signal(None),
        filter: create_signal(String::new()),
    };
    explorer.install_keys();
    explorer
}

impl FileExplorer {
    /// The selected node ids (Ctrl/Cmd-click toggles, Shift-click ranges).
    pub fn selection(&self) -> Signal<Vec<u64>> {
        self.selected
    }

    /// The node being inline-renamed, if any (double-click / F2 / menu Rename).
    pub fn renaming(&self) -> Signal<Option<u64>> {
        self.renaming
    }

    /// Install an icon theme — the VSCode-style theming hook. `f` maps a node
    /// (+ whether it renders expanded) to a glyph and optional color; return
    /// `None` per node to keep the default look. Per-node [`FsNode::icon`]
    /// overrides always win over the theme. Rows re-render on theme change.
    ///
    /// ```ignore
    /// explorer.set_icon_theme(|n, _open| match n.name.rsplit('.').next() {
    ///     Some("rs") => Some((lucide::FILE_CODE, None)),
    ///     _ => None,
    /// });
    /// ```
    pub fn set_icon_theme(
        &self,
        f: impl Fn(&FsNode, bool) -> Option<(IconData, Option<Color>)> + 'static,
    ) {
        self.icon_theme.set(Some(Rc::new(f)));
    }

    /// Remove the installed icon theme (back to the default folder/file glyphs).
    pub fn clear_icon_theme(&self) {
        self.icon_theme.set(None);
    }

    /// Resolve a node's glyph: per-node override → icon theme → the defaults
    /// (open/closed folder, plain file). A `None` color means "the theme's
    /// muted foreground".
    pub fn resolved_icon(&self, node: &FsNode, expanded: bool) -> (IconData, Option<Color>) {
        if let Some(d) = node.icon {
            return (d, node.color);
        }
        if let Some(theme) = self.icon_theme.get()
            && let Some((d, c)) = theme(node, expanded)
        {
            return (d, c.or(node.color));
        }
        let d = if node.kind == FsKind::Folder {
            if expanded { lucide::FOLDER_OPEN } else { IconKind::Folder.data() }
        } else {
            IconKind::File.data()
        };
        (d, node.color)
    }

    /// The live filter query — **bind your own input to it**:
    /// `text_field().bind(explorer.filter())`. Non-empty: only nodes whose name
    /// matches (case-insensitive substring), sits under a matching folder, or
    /// contains a match are shown, with folders force-expanded; the keyboard
    /// walks the filtered rows.
    pub fn filter(&self) -> Signal<String> {
        self.filter
    }

    /// The keyboard-focus row (rendered with the ring), independent of the
    /// selection. Move it with Mod+↑/↓; Mod+Space toggles it into the selection.
    pub fn active_row(&self) -> Signal<Option<u64>> {
        self.active
    }

    /// Expand every folder.
    pub fn expand_all(&self) {
        fn collect(nodes: &[FsNode], out: &mut HashSet<u64>) {
            for n in nodes {
                if n.kind == FsKind::Folder {
                    out.insert(n.id);
                    collect(&n.children, out);
                }
            }
        }
        let mut all = HashSet::new();
        collect(&self.tree.peek().root, &mut all);
        self.expanded.set(all);
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

    /// Leave filesystem mode: detach from the disk. The model stays as-is and
    /// mutations become in-memory only (pair with setting a fresh tree).
    pub fn detach_folder(&self) {
        self.fs_root.set(None);
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
        self.active.set(None);
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

    /// The active node: the keyboard-focus row when set (and still present),
    /// else the LAST selected one (rename/new-node/paste targets).
    fn active(&self) -> Option<u64> {
        self.active
            .peek()
            .filter(|id| self.tree.peek().node(*id).is_some())
            .or_else(|| self.selected.peek().last().copied())
    }

    /// The visible nodes in display order (folders expanded), honoring the
    /// filter with the SAME rules the renderer uses — keyboard navigation always
    /// walks exactly what's on screen.
    fn visible_ids(&self) -> Vec<u64> {
        let q = self.filter.peek().trim().to_lowercase();
        fn walk(nodes: &[FsNode], expanded: &HashSet<u64>, q: &str, anc: bool, out: &mut Vec<u64>) {
            for n in nodes {
                if !filter_keeps(n, q, anc) {
                    continue;
                }
                out.push(n.id);
                let open = if q.is_empty() { expanded.contains(&n.id) } else { true };
                if n.kind == FsKind::Folder && open {
                    walk(&n.children, expanded, q, anc || name_matches(n, q), out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.tree.peek().root, &self.expanded.peek(), &q, false, &mut out);
        out
    }

    pub fn select_only(&self, id: u64) {
        self.selected.set(vec![id]);
        self.active.set(Some(id));
    }

    fn toggle_select(&self, id: u64) {
        self.active.set(Some(id));
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
        // The range anchors on the focus row only while it is still SELECTED
        // (a ctrl-click deselect moves focus but must not anchor), else on the
        // last selected node.
        let anchor_id = self
            .active
            .peek()
            .filter(|a| self.selected.peek().contains(a))
            .or_else(|| self.selected.peek().last().copied());
        let anchor = anchor_id.and_then(|a| visible.iter().position(|&v| v == a));
        let end = visible.iter().position(|&v| v == id);
        match (anchor, end) {
            (Some(a), Some(b)) => {
                let (lo, hi) = (a.min(b), a.max(b));
                self.selected.set(visible[lo..=hi].to_vec());
                self.active.set(Some(id));
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

    /// Expand every ancestor of `id` so it is visible — the "Reveal in
    /// Explorer" hook (pair with [`select_only`](Self::select_only)).
    pub fn reveal(&self, id: u64) {
        let mut cur = self.tree.peek().parent_of(id);
        while let Some(p) = cur {
            self.expanded.update(|e| {
                e.insert(p);
            });
            cur = self.tree.peek().parent_of(p);
        }
    }

    /// The VSCode key set. Every binding is CONDITIONAL: it acts only while
    /// the explorer is engaged (non-empty selection; F2 also engages via the
    /// active node) and declines otherwise, so arrows/Delete/etc. fall through
    /// to whatever else wants them. While the rename editor is focused, the
    /// editor's own key precedence applies — none of these fire.
    fn install_keys(self) {
        // F2 — rename the active node.
        create_shortcut_if("F2", move || {
            if self.renaming.peek().is_some() {
                return true; // already renaming — swallow the repeat
            }
            match self.active() {
                Some(id) => {
                    self.start_rename_for(id);
                    true
                }
                None => false,
            }
        });
        // Delete — delete the selection.
        create_shortcut_if("Delete", move || {
            let ids = self.selected.peek().clone();
            if ids.is_empty() {
                return false;
            }
            self.delete_nodes(&ids);
            true
        });
        // Mod+A — select every visible node, from idle too (a focused editor's
        // own Select All never reaches here — editor precedence). Declines only
        // over an empty tree.
        create_shortcut_if("Mod+A", move || {
            let all = self.visible_ids();
            if all.is_empty() {
                return false;
            }
            self.selected.set(all);
            true
        });
        // Mod+C / Mod+X / Mod+V — the explorer clipboard (a focused editor's
        // clipboard intents win first, as everywhere).
        create_shortcut_if("Mod+C", move || {
            if self.selected.peek().is_empty() {
                return false;
            }
            self.copy_selection();
            true
        });
        create_shortcut_if("Mod+X", move || {
            if self.selected.peek().is_empty() {
                return false;
            }
            self.cut_selection();
            true
        });
        create_shortcut_if("Mod+V", move || {
            if self.clipboard.peek().is_none() {
                return false;
            }
            self.paste_clipboard();
            true
        });
        // Home / End — jump to the first/last visible row (while engaged, so an
        // idle explorer never steals page scrolling).
        create_shortcut_if("Home", move || self.key_jump(true));
        create_shortcut_if("End", move || self.key_jump(false));
        // Mod+↑/↓ — move the focus row WITHOUT changing the selection, then
        // Mod+Space toggles the focused row in/out (one-by-one multi-select).
        create_shortcut_if("Mod+ArrowDown", move || self.key_focus(1));
        create_shortcut_if("Mod+ArrowUp", move || self.key_focus(-1));
        create_shortcut_if("Mod+Space", move || {
            let Some(id) = self.active() else { return false };
            self.toggle_select(id);
            true
        });
        // Arrows — walk the visible rows; Shift extends the selection.
        create_shortcut_if("ArrowDown", move || self.key_step(1, false));
        create_shortcut_if("ArrowUp", move || self.key_step(-1, false));
        create_shortcut_if("Shift+ArrowDown", move || self.key_step(1, true));
        create_shortcut_if("Shift+ArrowUp", move || self.key_step(-1, true));
        // ArrowLeft — collapse the active folder, else jump to the parent.
        create_shortcut_if("ArrowLeft", move || {
            let Some(id) = self.active() else { return false };
            let is_open = self.tree.peek().node(id).is_some_and(|n| n.kind == FsKind::Folder)
                && self.expanded.peek().contains(&id);
            if is_open {
                self.expanded.update(|e| {
                    e.remove(&id);
                });
            } else if let Some(parent) = self.tree.peek().parent_of(id) {
                self.select_only(parent);
            }
            true
        });
        // ArrowRight — expand the active folder, else step into its first child.
        create_shortcut_if("ArrowRight", move || {
            let Some(id) = self.active() else { return false };
            let node = self.tree.peek().node(id).cloned();
            let Some(node) = node else { return false };
            if node.kind != FsKind::Folder {
                return true; // engaged but nothing to expand — consume, like VSCode
            }
            if !self.expanded.peek().contains(&id) {
                self.toggle_folder(id);
            } else if let Some(first) = node.children.first() {
                self.select_only(first.id);
            }
            true
        });
        // Escape — cancel a pending cut/copy first (VSCode), then drop the
        // selection (disengages the key set).
        create_shortcut_if("Escape", move || {
            if self.clipboard.peek().is_some() {
                self.clipboard.set(None);
                true
            } else if self.selected.peek().is_empty() {
                false
            } else {
                self.selected.set(Vec::new());
                true
            }
        });
    }

    /// Home/End — select the first/last visible row (declines while idle).
    fn key_jump(&self, first: bool) -> bool {
        if self.selected.peek().is_empty() {
            return false;
        }
        let visible = self.visible_ids();
        let target = if first { visible.first() } else { visible.last() };
        if let Some(&id) = target {
            self.select_only(id);
        }
        true
    }

    /// Copy the selection to the explorer clipboard (paste with Mod+V / the menu).
    pub fn copy_selection(&self) {
        let ids = self.selected.peek().clone();
        if !ids.is_empty() {
            self.clipboard.set(Some((ids, ClipMode::Copy)));
        }
    }

    /// Cut the selection: pasting MOVES it. Cut rows render dimmed until pasted
    /// or cancelled (Escape).
    pub fn cut_selection(&self) {
        let ids = self.selected.peek().clone();
        if !ids.is_empty() {
            self.clipboard.set(Some((ids, ClipMode::Cut)));
        }
    }

    /// Paste the clipboard into the active folder (the selected folder, else the
    /// selected file's parent, else the root — VSCode's target rules). Cut moves;
    /// Copy duplicates the whole subtree (on disk too, in filesystem mode) with
    /// sibling-deduped names.
    pub fn paste_clipboard(&self) {
        let Some((ids, mode)) = self.clipboard.peek().clone() else { return };
        let target = self.insertion_parent();
        match mode {
            ClipMode::Cut => {
                self.move_nodes(&ids, target);
                self.clipboard.set(None);
            }
            ClipMode::Copy => {
                for id in ids {
                    // Snapshot the subtree first, so pasting into itself is safe.
                    let Some(mut src) = self.tree.peek().node(id).cloned() else { continue };
                    if self.fs_root.get().is_some() {
                        let Some(from) = self.path_of(id) else {
                            self.last_error.set(Some("Could not resolve a path for a node".into()));
                            continue;
                        };
                        let dir = match target {
                            Some(t) => self.path_of(t),
                            None => self.fs_root.get(),
                        };
                        let Some(dir) = dir else {
                            self.last_error.set(Some("Could not resolve the target folder".into()));
                            continue;
                        };
                        // De-dup against the REAL directory, then mirror on disk.
                        let taken = std::fs::read_dir(&dir)
                            .map(|rd| {
                                rd.filter_map(|e| e.ok())
                                    .map(|e| e.file_name().to_string_lossy().to_string())
                                    .collect::<Vec<_>>()
                            })
                            .unwrap_or_default();
                        let taken_refs: Vec<&str> = taken.iter().map(|s| s.as_str()).collect();
                        let name = unique_name(&src.name, &taken_refs);
                        if let Err(e) = copy_path(&from, &dir.join(&name)) {
                            self.last_error.set(Some(format!("Could not copy {}: {e}", from.display())));
                            continue;
                        }
                        src.name = name;
                    }
                    self.insert_subtree(target, src);
                }
            }
        }
    }

    /// Insert a fully-built node (fresh subtree ids, name deduped) and reveal it.
    fn insert_subtree(&self, parent: Option<u64>, node: FsNode) -> u64 {
        let cell = RefCell::new(None);
        self.tree.update(|t| {
            *cell.borrow_mut() = Some(t.insert_node(parent, node));
        });
        let id = cell.take().expect("inserted");
        self.reveal(id);
        id
    }

    /// Move the active row `dir` steps through the visible order; `extend`
    /// (Shift) grows the selection instead of replacing it. Declines (`false`)
    /// with no selection so plain arrows keep scrolling the page.
    fn key_step(&self, dir: i64, extend: bool) -> bool {
        let Some(active) = self.active() else { return false };
        let visible = self.visible_ids();
        let Some(pos) = visible.iter().position(|&v| v == active) else { return false };
        let next = pos as i64 + dir;
        if next < 0 || next as usize >= visible.len() {
            return true; // at the edge — consumed, no move
        }
        let next = visible[next as usize];
        if extend {
            self.selected.update(|s| {
                s.retain(|&v| v != next);
                s.push(next); // becomes the new active end of the selection
            });
            self.active.set(Some(next));
        } else {
            self.select_only(next);
        }
        true
    }

    /// Mod+↑/↓ — move the FOCUS row without touching the selection (the
    /// Windows/VSCode one-by-one pattern; Mod+Space then toggles it in).
    fn key_focus(&self, dir: i64) -> bool {
        let Some(cur) = self.active() else { return false };
        let visible = self.visible_ids();
        let Some(pos) = visible.iter().position(|&v| v == cur) else { return false };
        let next = pos as i64 + dir;
        if next >= 0 && (next as usize) < visible.len() {
            self.active.set(Some(visible[next as usize]));
        }
        true
    }

    fn start_rename_for(&self, id: u64) {
        self.select_only(id);
        // Prefill the editor with the CURRENT name (the standard rename UX —
        // the field selects the stem so typing replaces it, and arrow keys let
        // you edit in place instead of retyping the whole name).
        let current = self.tree.peek().node(id).map(|n| n.name.clone()).unwrap_or_default();
        self.rename_buf.set(current);
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
            // A NEW node starts from an empty field (the created default shows as
            // the placeholder; committing empty keeps it) — only renames prefill.
            self.rename_buf.set(String::new());
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
            self.rename_buf.set(String::new());
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
                self.start_rename_for(id);
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
            icon_button(lucide::FILE_PLUS)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .on_pressed(self.new_file()),
            gap_w(2.0),
            icon_button(lucide::FOLDER_PLUS)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .on_pressed(self.new_folder()),
            gap_w(2.0),
            icon_button(lucide::CHEVRONS_DOWN_UP)
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
        let q = self.filter.get().trim().to_lowercase();
        let mut kids: Vec<AnyWidget> = Vec::new();
        for node in &model.root {
            if !filter_keeps(node, &q, false) {
                continue;
            }
            kids.push(
                component_props(
                    render_node,
                    NodeProps { explorer: self, node: node.clone(), depth: 0, anc_match: false },
                )
                .into_widget(),
            );
        }
        if kids.is_empty() {
            let hint =
                if q.is_empty() { "Empty — add a file or folder." } else { "No matches." };
            kids.push(Padding::new(EdgeInsets::all(12.0), muted(hint)).into_widget());
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
            .separator()
            .item(
                menu_item("Paste")
                    .disabled(self.clipboard.get().is_none())
                    .on_select(move || self.paste_clipboard()),
            )
    }
}
