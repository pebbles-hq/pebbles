//! [`FileExplorer`] — a VS Code-grade file explorer built from separate,
//! composable pieces:
//!
//! * [`FileTree`] — the app-owned data model (wrap it in a `Signal`): stable
//!   node ids, and the mutation helpers (`insert`, `rename`, `delete`,
//!   `move_node`).
//! * [`file_explorer`] — a `Copy` controller over that signal: selection,
//!   expansion, inline rename, drag state, file operations, undo, menus.
//! * [`FileExplorer::toolbar`] / [`FileExplorer::tree`] — the default widget
//!   pieces; skip them and compose your own chrome from the action closures
//!   (`new_file`, `new_folder`, `rename_selected`, `collapse_all`, …).
//!
//! In **filesystem mode** ([`open_folder`](FileExplorer::open_folder)) every
//! operation hits the real disk; otherwise the tree is in-memory.
//!
//! ```ignore
//! let tree = create_signal(FileTree::new());
//! let ex = file_explorer(tree);
//! ex.set_icon_theme(IconTheme::default_theme());
//! ex.set_load_filter(|p, is_dir| !(is_dir && p.ends_with("target")));
//! ex.open_folder("/path/to/project");
//! ex.watch(Some(2.0));                                // pick up external changes
//! ex.on_open(|e, mode| open_in_editor(e.path.as_deref(), mode));
//! column(children![ex.toolbar(), ex.tree()])
//! ```
//!
//! # Interaction (VS Code's)
//!
//! Click selects and **previews** a file (folders toggle); double-click /
//! **Enter** opens; **Space** previews; **Ctrl/Cmd-click** toggles,
//! **Shift-click** ranges, **Mod+A** selects all, **Shift+↑/↓** extend;
//! **←/→** collapse/expand, **Home/End**, **Mod+↑/↓** + **Mod+Space** walk and
//! toggle one by one; **F2** renames inline (prefilled, stem selected,
//! validated, never overwrites, `a/b/c.txt` creates folders); **Delete**
//! moves to the trash after a confirmation, **Shift+Delete** deletes
//! permanently after one; **Mod+C/X/V** copy/cut/paste (`x copy.txt` on
//! conflicts); **Mod+Z** undoes the last file operation; typing letters jumps
//! to matching rows. New File / New Folder open an inline name editor and
//! create on disk only once the name is committed.
//!
//! **Drag and drop** — a press only becomes a drag past an 8 px slop (a click
//! never moves anything), drops need an explicit highlighted target (a folder,
//! or empty space for the root), moving into the current parent / itself / a
//! descendant is refused, disk moves ask "Move 'x' into 'y'?"
//! ([`set_confirm_move`](FileExplorer::set_confirm_move) to supply your own,
//! [`confirm_moves`](FileExplorer::confirm_moves) to skip) and are undoable;
//! Ctrl/Alt-drag copies; files dragged in from the OS are copied in.
//!
//! # Context menus
//!
//! VS Code's file / folder / empty-space / multi-selection menus are a list
//! of [`ExplorerMenuItem`]s. App-specific items (Open to the Side, Open With…,
//! Reveal in File Manager, Open in Integrated Terminal, Find in Folder,
//! Compare) appear once you plug them:
//!
//! ```ignore
//! ex.set_menu_handler(menu_ids::OPEN_IN_TERMINAL, |t| terminal.cd(t.entries[0].path.clone()));
//! ex.remove_menu_item(menu_ids::COPY_RELATIVE_PATH);
//! ex.move_menu_item(menu_ids::DELETE, 0);
//! ex.insert_menu_item_after(menu_ids::OPEN, ExplorerMenuItem::new("app.run", "Run File", |t| run(t))
//!     .visible_when(|t| t.entry().is_some_and(|e| e.name.ends_with(".rs")))
//!     .enabled_when(|_| !busy()));
//! ```
//!
//! # Controls for developers
//!
//! Every capability is switchable ([`ExplorerCapabilities`]), with a
//! read-only preset; disabled actions grey out in the menus, their shortcuts
//! decline and the API refuses them ([`ExplorerError::NotAllowed`]) before
//! touching the disk. Per-item predicates and before/after hooks cover the
//! rest:
//!
//! ```ignore
//! let ex = file_explorer(tree).read_only();                       // browse/select/open only
//! let ex = file_explorer(tree)
//!     .capabilities(ExplorerCapabilities::all().allow_delete(false).show_hidden(false));
//! ex.can_rename(|e| e.name != "Cargo.toml");
//! ex.can_drop_into(|target, _| target.is_none_or(|t| t.name != "vendor"));
//! ex.on_before_delete(|entries, permanent| !permanent || confirm_twice(entries));
//! ex.on_after_op(|op| log::info!("{op:?}"));
//! ```
//!
//! # Look
//!
//! Icons follow VS Code icon themes ([`IconTheme`]: file name > compound
//! extension > extension > language id > default; folder name with open/closed
//! icons > root > default), any icon can be a glyph, an image or a widget
//! ([`ExplorerIcon`]), and per-path overrides
//! ([`set_path_icon`](FileExplorer::set_path_icon)) plus a resolver that wins
//! over everything ([`set_icon_resolver`](FileExplorer::set_icon_resolver))
//! exist for custom logic. Every colour and size comes from [`ExplorerStyle`]
//! tokens derived from the Pebbles theme (override with
//! [`set_style`](FileExplorer::set_style)). Rows show app-supplied
//! [`Decoration`]s (git status, problems, dirty, badges) and can compact
//! single-child folder chains ([`compact_folders`](FileExplorer::compact_folders)).
//!
//! **Right-click is built in** — the explorer's menus work regardless of the
//! app-wide global-menu switch ([`set_global_menu_enabled`](crate::set_global_menu_enabled)).

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{Border, BoxDecoration, Cursor, IconData, IconKind, tabler};

use crate::components::{ButtonVariant, context_menu, icon, icon_button, menu_item, muted, text_field};
use crate::theme::{mix, theme};
use crate::widgets::{Container, Expanded, GestureDetector, Padding, column, gap_w, row, text};
use pebbles_core::children;
use pebbles_core::keyboard::{KeyInput, ctrl_held, shift_held};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animated, component_props, create_shortcut_if, create_signal};

mod caps;
mod deco;
mod icons;
mod keys;
mod menus;
mod node;
mod ops;
mod style;
mod tree;

pub use caps::{Capability, ExplorerCapabilities, ExplorerEntry, ExplorerError, ExplorerOp};
pub use deco::{Decoration, GitStatus};
pub use icons::{ExplorerIcon, IconTheme, LanguageResolver, language_of};
pub use menus::{ExplorerMenuItem, MenuAction, MenuContext, MenuTarget, OpenMode, menu_ids};
pub use style::ExplorerStyle;
#[cfg(feature = "file-dialogs")]
pub use tree::pick_folder;
pub use tree::{FileTree, FsKind, FsNode, natural_cmp};

use node::{NodeProps, render_node};
use tree::{copy_path, read_dir};

// ---------------------------------------------------------------------------
// The explorer controller
// ---------------------------------------------------------------------------

/// An installed load filter (see [`FileExplorer::set_load_filter`]).
pub(super) type LoadFilter = Rc<dyn Fn(&Path, bool) -> bool>;
/// An installed icon resolver (see [`FileExplorer::set_icon_resolver`]).
pub(super) type IconResolver = Rc<dyn Fn(&ExplorerEntry, bool) -> Option<ExplorerIcon>>;

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
    /// The row pressed (pointer down) and where (global) — a press only turns
    /// into a drag once the pointer travels past [`DRAG_SLOP`].
    pub(super) press: Signal<Option<(u64, Offset)>>,
    /// The row currently under the pointer (any row, file or folder) — the
    /// empty-space root target is live only while this is `None`.
    pub(super) row_hover: Signal<Option<u64>>,
    /// Whether moves may touch the disk (filesystem mode). `false` refuses them.
    fs_moves_allowed: Signal<bool>,
    /// Whether filesystem moves ask for confirmation first (default `true`).
    confirm_moves: Signal<bool>,
    /// The app's confirm hook (default: an alert dialog).
    confirm_hook: Signal<Option<ConfirmMoveFn>>,
    /// The trash directory (freedesktop layout: `files/` + `info/`); `None` =
    /// the platform default (see [`set_trash_dir`](Self::set_trash_dir)).
    trash_dir: Signal<Option<PathBuf>>,
    /// Which capabilities are on (see [`ExplorerCapabilities`]).
    caps: Signal<ExplorerCapabilities>,
    /// Predicates and hooks (see [`caps`]).
    controls: Signal<caps::Controls>,
    /// Undo stack of completed file operations (Mod+Z / [`undo`](Self::undo)).
    history: Signal<Vec<FileOp>>,
    hover_key: u64,
    /// Timer key for the polling file watcher ([`watch`](Self::watch)).
    watch_key: u64,
    /// The watcher interval in seconds (`None` = not watching).
    watch_secs: Signal<Option<f64>>,
    /// Filesystem mode: the directory the explorer mirrors (None = in-memory).
    fs_root: Signal<Option<PathBuf>>,
    /// The last filesystem error, if any (read for toasts/inline hints).
    last_error: Signal<Option<String>>,
    /// The placeholder row of an inline New File/New Folder (not on disk yet).
    pub(super) pending_create: Signal<Option<u64>>,
    /// The inline-rename buffer — prefilled with the current name at rename start.
    pub(super) rename_buf: Signal<String>,
    /// The explorer clipboard: node ids + copy/cut, filled by Mod+C/X and the menus.
    pub(super) clipboard: Signal<Option<(Vec<u64>, ClipMode)>>,
    /// The installed icon theme, if any (see [`set_icon_theme`](Self::set_icon_theme)).
    pub(super) icon_theme: Signal<Option<Rc<IconTheme>>>,
    /// The icon resolver that wins over everything (see [`set_icon_resolver`](Self::set_icon_resolver)).
    pub(super) icon_resolver: Signal<Option<IconResolver>>,
    /// The style override (see [`set_style`](Self::set_style)).
    pub(super) style_fn: Signal<Option<style::StyleFn>>,
    /// The context-menu items (built-ins first; see [`menus`]).
    pub(super) menu: Signal<Vec<ExplorerMenuItem>>,
    /// App handlers for the pluggable built-in items.
    pub(super) menu_handlers: Signal<std::collections::HashMap<String, MenuAction>>,
    /// "Select for Compare" source.
    pub(super) compare_source: Signal<Option<u64>>,
    /// The decoration hook (see [`set_decorations`](Self::set_decorations)).
    pub(super) deco_fn: Signal<Option<deco::DecoFn>>,
    /// Compact folders on/off.
    pub(super) compact: Signal<bool>,
    /// Type-to-find buffer and the time of the last key.
    pub(super) find_buf: Signal<(String, Option<std::time::Instant>)>,
    /// The open hook (see [`on_open`](Self::on_open)).
    pub(super) open_hook: Signal<Option<menus::OpenHook>>,
    /// Per-path icon overrides.
    pub(super) path_icons: Signal<std::collections::HashMap<PathBuf, ExplorerIcon>>,
    /// The keyboard-focus row (the ring) — independent of the selection, so
    /// Mod+↑/↓ can walk rows without selecting (toggle with Mod+Space).
    pub(super) active: Signal<Option<u64>>,
    /// The live filter query (case-insensitive substring) — bind your own input.
    pub(super) filter: Signal<String>,
    /// Filesystem mode: which disk entries to load (see [`set_load_filter`](Self::set_load_filter)).
    pub(super) load_filter: Signal<Option<LoadFilter>>,
}

/// How far (logical px) the pointer must travel from the press before a row
/// press becomes a drag — the desktop drag threshold (GTK's default
/// `gtk-dnd-drag-threshold`, which Chromium/VS Code use on Linux). Below it a
/// press is a click and never moves anything.
const DRAG_SLOP: f64 = 8.0;

/// A move awaiting confirmation (see [`FileExplorer::set_confirm_move`]).
#[derive(Clone, Debug)]
pub struct MoveRequest {
    /// The nodes to move (already validated: no no-ops, no self/descendant moves).
    pub ids: Vec<u64>,
    /// Their display names, parallel to `ids`.
    pub names: Vec<String>,
    /// The target folder (`None` = the explorer root).
    pub target: Option<u64>,
    /// The target's display name (the root folder's name for `None`).
    pub target_name: String,
}

/// The app's move-confirmation hook: called with the request and a `resolve`
/// callback — call `resolve(true)` to perform the move, `resolve(false)` to
/// drop it (may be called later, e.g. from your own dialog).
pub type ConfirmMoveFn = Rc<dyn Fn(&MoveRequest, Rc<dyn Fn(bool)>)>;

/// One completed node move (for undo).
#[derive(Clone, Debug)]
struct MoveRecord {
    id: u64,
    old_parent: Option<u64>,
    /// Filesystem mode: the on-disk paths before / after.
    paths: Option<(PathBuf, PathBuf)>,
}

/// One undoable file operation.
#[derive(Clone, Debug)]
enum FileOp {
    Move(Vec<MoveRecord>),
    Rename {
        id: u64,
        old_name: String,
        old_parent: Option<u64>,
        paths: Option<(PathBuf, PathBuf)>,
    },
    /// A New File / New Folder (the node and, on disk, its path).
    Create {
        id: u64,
        path: Option<PathBuf>,
    },
    /// Pasted copies (node ids and, on disk, their paths).
    Copy {
        ids: Vec<u64>,
        paths: Vec<PathBuf>,
    },
    /// Moved to the trash: (original path, trashed path, .trashinfo path).
    Trash(Vec<(PathBuf, PathBuf, PathBuf)>),
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
        press: create_signal(None),
        row_hover: create_signal(None),
        fs_moves_allowed: create_signal(true),
        confirm_moves: create_signal(true),
        confirm_hook: create_signal(None),
        history: create_signal(Vec::new()),
        trash_dir: create_signal(None),
        caps: create_signal(ExplorerCapabilities::all()),
        controls: create_signal(caps::Controls::default()),
        hover_key: create_signal(()).raw_id(),
        watch_key: create_signal(()).raw_id(),
        watch_secs: create_signal(None),
        fs_root: create_signal(None),
        last_error: create_signal(None),
        rename_buf: create_signal(String::new()),
        pending_create: create_signal(None),
        clipboard: create_signal(None),
        icon_theme: create_signal(None),
        icon_resolver: create_signal(None),
        style_fn: create_signal(None),
        menu: create_signal(Vec::new()),
        menu_handlers: create_signal(std::collections::HashMap::new()),
        compare_source: create_signal(None),
        open_hook: create_signal(None),
        find_buf: create_signal((String::new(), None)),
        deco_fn: create_signal(None),
        compact: create_signal(false),
        path_icons: create_signal(std::collections::HashMap::new()),
        active: create_signal(None),
        filter: create_signal(String::new()),
        load_filter: create_signal(None),
    };
    explorer.menu.set(explorer.default_menu());
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

    /// Install a filesystem load filter — the ignore hook (VS Code `files.exclude`).
    /// `f(path, is_dir)` is called for every disk entry as a folder's children are read
    /// ([`open_folder`](Self::open_folder) and lazy expansion); return `false` to skip it
    /// (`.git`, `target/`, `node_modules`, ...). Applies to subsequent loads, so install
    /// it before `open_folder`.
    ///
    /// ```ignore
    /// explorer.set_load_filter(|path, is_dir| {
    ///     let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    ///     !(is_dir && matches!(name, ".git" | "target" | "node_modules"))
    /// });
    /// ```
    pub fn set_load_filter(&self, f: impl Fn(&Path, bool) -> bool + 'static) {
        self.load_filter.set(Some(Rc::new(f)));
    }

    /// Remove the load filter (load every entry again on subsequent reads).
    pub fn clear_load_filter(&self) {
        self.load_filter.set(None);
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

    /// Expand/collapse a folder (loading its children from disk first, in
    /// filesystem mode).
    pub fn toggle_folder(&self, id: u64) {
        self.ensure_loaded(id);
        if self.compact.peek() {
            // A compact row toggles its whole chain (head … id).
            let head = self.compact_head(id);
            let chain: Vec<u64> = {
                let t = self.tree.peek();
                t.node(head).map(|n| self.compact_chain(n).iter().map(|c| c.id).collect()).unwrap_or_default()
            };
            let open = chain.iter().all(|c| self.expanded.peek().contains(c));
            self.expanded.update(|ex| {
                for c in &chain {
                    if open {
                        ex.remove(c);
                    } else {
                        ex.insert(*c);
                    }
                }
            });
            if !open {
                self.extend_compact_chain(*chain.last().unwrap_or(&id));
            }
            return;
        }
        self.expanded.update(|ex| {
            if !ex.remove(&id) {
                ex.insert(id);
            }
        });
    }

    /// The first folder of the compact chain `id` belongs to.
    pub(super) fn compact_head(&self, id: u64) -> u64 {
        let t = self.tree.peek();
        let mut head = id;
        while let Some(p) = t.parent_of(head) {
            let only_child = t.node(p).is_some_and(|n| n.loaded && n.children.len() == 1);
            if !only_child {
                break;
            }
            head = p;
        }
        head
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
    /// The rows on screen, top to bottom (node ids; a compact chain is its
    /// last folder) — what the keyboard walks.
    pub fn visible_rows(&self) -> Vec<u64> {
        self.visible_ids()
    }

    fn visible_ids(&self) -> Vec<u64> {
        let q = self.filter.peek().trim().to_lowercase();
        let compact = self.compact.peek();
        let walk = |nodes: &[FsNode], expanded: &HashSet<u64>, out: &mut Vec<u64>| {
            fn go(
                ex: &FileExplorer,
                compact: bool,
                nodes: &[FsNode],
                expanded: &HashSet<u64>,
                q: &str,
                anc: bool,
                out: &mut Vec<u64>,
            ) {
                for n in nodes {
                    if !filter_keeps(n, q, anc) {
                        continue;
                    }
                    // Compact folders: the chain is ONE row (its last folder).
                    let chain = if compact && q.is_empty() { ex.compact_chain(n) } else { vec![n.clone()] };
                    let last = chain.last().expect("non-empty");
                    out.push(last.id);
                    let open =
                        if q.is_empty() { chain.iter().all(|c| expanded.contains(&c.id)) } else { true };
                    if last.kind == FsKind::Folder && open {
                        go(ex, compact, &last.children, expanded, q, anc || name_matches(n, q), out);
                    }
                }
            }
            go(self, compact, nodes, expanded, &q, false, out);
        };
        let mut out = Vec::new();
        let root = self.tree.peek().root.clone();
        walk(&root, &self.expanded.peek(), &mut out);
        out
    }

    pub fn select_only(&self, id: u64) {
        self.selected.set(vec![id]);
        self.active.set(Some(id));
    }

    /// Ctrl/Cmd-click semantics: toggle `id` in/out of the selection (a plain
    /// select when [`Capability::MultiSelect`] is off).
    pub fn toggle_selected(&self, id: u64) {
        self.toggle_select(id);
    }

    /// Shift-click semantics: select the range from the anchor to `id`.
    pub fn select_range_to(&self, id: u64) {
        self.range_select(id);
    }

    fn toggle_select(&self, id: u64) {
        if !self.caps.peek().allows(Capability::MultiSelect) {
            self.select_only(id);
            return;
        }
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
        if !self.caps.peek().allows(Capability::MultiSelect) {
            self.select_only(id);
            return;
        }
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
    /// Auto-reveal: expand (loading folders lazily) down to `path`, select it
    /// and make it the focus row — what an IDE calls when the active editor
    /// changes. Returns the node id, or `None` when the path isn't under the
    /// opened folder (or is filtered out).
    pub fn reveal_path(&self, path: impl AsRef<Path>) -> Option<u64> {
        let root = self.fs_root.peek().clone()?;
        let rel = path.as_ref().strip_prefix(&root).ok()?;
        let mut parent: Option<u64> = None;
        let mut id = None;
        for comp in rel.components() {
            let name = comp.as_os_str().to_string_lossy().to_string();
            if let Some(p) = parent {
                self.ensure_loaded(p);
            }
            let found = self.child_named(parent, &name)?;
            id = Some(found);
            parent = Some(found);
        }
        let id = id?;
        self.reveal(id);
        if self.selected.peek().as_slice() != [id] {
            self.select_only(id);
        }
        Some(id)
    }

    pub fn reveal(&self, id: u64) {
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

    /// New File: an inline name editor appears in the active folder; the file
    /// is created (on disk in filesystem mode) only when the name is
    /// committed (Enter/blur — empty keeps the suggested `new_file.txt`;
    /// `a/b/c.txt` creates the folders). Escape cancels. VS Code's flow.
    pub fn new_file(self) -> impl Fn() + 'static {
        move || self.begin_create(FsKind::File)
    }

    /// New Folder: like [`new_file`](Self::new_file), for a folder.
    pub fn new_folder(self) -> impl Fn() + 'static {
        move || self.begin_create(FsKind::Folder)
    }

    /// Start an inline create of `kind` in the active folder.
    pub fn begin_create(&self, kind: FsKind) {
        let cap = if kind == FsKind::Folder { Capability::CreateFolder } else { Capability::CreateFile };
        if self.require(cap).is_err() {
            return;
        }
        self.cancel_pending_create();
        let parent = self.insertion_parent();
        if let Some(p) = parent {
            self.ensure_loaded(p);
        }
        let base = if kind == FsKind::Folder { "New Folder" } else { "new_file.txt" };
        // A placeholder row (model only) carries the editor; nothing is on disk yet.
        let id = self.insert_at(parent, kind, base);
        self.pending_create.set(Some(id));
        self.start_rename_for(id);
        // A NEW node starts from an empty field (the suggestion shows as the
        // placeholder) — only renames prefill.
        self.rename_buf.set(String::new());
    }

    /// Commit the inline editor for `id`: finishes a pending create (empty =
    /// the suggested name) or renames. Errors land in [`last_error`](Self::last_error).
    pub fn commit_rename(&self, id: u64, text: &str) -> Result<(), ExplorerError> {
        let text = text.trim();
        if self.pending_create.peek() == Some(id) {
            self.pending_create.set(None);
            let (parent, kind, suggested) = {
                let t = self.tree.peek();
                let n = t.node(id).cloned();
                (t.parent_of(id), n.as_ref().map(|n| n.kind), n.map(|n| n.name))
            };
            self.tree.update(|t| {
                t.delete(id);
            });
            self.renaming.set(None);
            let (Some(kind), Some(suggested)) = (kind, suggested) else { return Ok(()) };
            let name = if text.is_empty() { suggested } else { text.to_string() };
            let new = self.create_entry(parent, kind, &name)?;
            self.select_only(new);
            return Ok(());
        }
        if text.is_empty() {
            self.renaming.set(None);
            return Ok(());
        }
        self.rename_node(id, text)
    }

    /// Cancel the inline editor (Escape): drops a pending create's placeholder.
    pub fn cancel_rename(&self) {
        self.cancel_pending_create();
        self.renaming.set(None);
    }

    fn cancel_pending_create(&self) {
        if let Some(id) = self.pending_create.peek() {
            self.pending_create.set(None);
            self.tree.update(|t| {
                t.delete(id);
            });
            self.renaming.set(None);
        }
    }

    /// Start renaming the active node (a no-op with no selection, or with
    /// [`Capability::Rename`] off).
    pub fn rename_selected(self) -> impl Fn() + 'static {
        move || {
            if let Some(id) = self.active()
                && self.require(Capability::Rename).is_ok()
            {
                self.start_rename_for(id);
            }
        }
    }

    /// Permanently delete the selection (subtrees included; on disk in
    /// filesystem mode) WITHOUT a confirmation — for your own confirmed UI.
    /// All gates apply. The Delete key/menu use [`request_delete`](Self::request_delete).
    pub fn delete_selected(self) -> impl Fn() + 'static {
        move || {
            let ids = self.selected.get();
            let _ = self.delete_permanently(&ids);
        }
    }

    /// Collapse every folder.
    pub fn collapse_all(self) -> impl Fn() + 'static {
        move || self.expanded.set(HashSet::new())
    }

    /// The default action row (VS Code's explorer title bar): New File ·
    /// New Folder · Refresh (filesystem mode) · Collapse All. Buttons for
    /// switched-off capabilities are disabled. Optional — compose your own
    /// from the action closures wherever you want.
    pub fn toolbar(self) -> impl IntoWidget {
        let caps = self.caps.get();
        let mut kids: Vec<AnyWidget> = vec![
            icon_button(tabler::FILE_PLUS)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .label("New File…")
                .disabled(!caps.allows(Capability::CreateFile))
                .on_pressed(self.new_file())
                .into_widget(),
            gap_w(2.0).into_widget(),
            icon_button(tabler::FOLDER_PLUS)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .label("New Folder…")
                .disabled(!caps.allows(Capability::CreateFolder))
                .on_pressed(self.new_folder())
                .into_widget(),
            gap_w(2.0).into_widget(),
        ];
        if self.fs_root.get().is_some() {
            kids.push(
                icon_button(tabler::REFRESH)
                    .variant(ButtonVariant::Ghost)
                    .size(15.0)
                    .label("Refresh Explorer")
                    .on_pressed(move || {
                        self.refresh();
                    })
                    .into_widget(),
            );
            kids.push(gap_w(2.0).into_widget());
        }
        kids.push(
            icon_button(tabler::FOLD)
                .variant(ButtonVariant::Ghost)
                .size(15.0)
                .label("Collapse Folders in Explorer")
                .on_pressed(self.collapse_all())
                .into_widget(),
        );
        row(kids).main_axis_size(MainAxisSize::Min)
    }

    /// The tree: select (Ctrl/Shift multi), expand, inline rename, context
    /// menus, and drag-to-move (multi-drag included). Right-clicking empty
    /// space offers New File/New Folder; dropping on empty space moves to the
    /// root.
    pub fn tree(self) -> impl IntoWidget {
        // Files dragged in from the OS (filesystem mode, DropFromOs on): drop
        // onto a folder row → into it; onto a file → its folder; else the root.
        if self.fs_root.get().is_some() && self.caps.get().allows(Capability::DropFromOs) {
            let tint = self.style().drop_target_bg;
            let strength = self.style().drop_target_strength;
            return crate::widgets::file_drop_target(move |hovering| {
                let body = self.tree_inner();
                if hovering {
                    Container::new().color(tint.multiply_alpha(strength)).child(body).into_widget()
                } else {
                    body
                }
            })
            .on_drop(move |paths| {
                let _ = self.import_paths(&paths, self.os_drop_target());
            })
            .into_widget();
        }
        self.tree_inner()
    }

    /// Where an OS drop lands: the folder row under the pointer, a file row's
    /// folder, else the root.
    fn os_drop_target(&self) -> Option<u64> {
        let h = self.row_hover.peek()?;
        let t = self.tree.peek();
        match t.node(h) {
            Some(n) if n.kind == FsKind::Folder => Some(h),
            Some(_) => t.parent_of(h),
            None => None,
        }
    }

    fn tree_inner(self) -> AnyWidget {
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
            let hint = if q.is_empty() { "Empty — add a file or folder." } else { "No matches." };
            kids.push(Padding::new(EdgeInsets::all(12.0), muted(hint)).into_widget());
        }
        // Fill the parent's height when it gives one (a panel), shrink-wrap
        // otherwise — the empty space stays part of the explorer (right-click
        // there = New File/New Folder; drop there = move to root).
        let body =
            column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Max);

        // Empty-space right-click → New File / New Folder; empty-space drop
        // (while dragging) → move to the root.
        // Every hover region under the pointer gets events (rows sit inside this one), so
        // the root is the drop target only while no row has claimed it.
        let root_gesture = GestureDetector::new(body)
            .on_hover_move({
                move || {
                    // Only over EMPTY space: any row under the pointer (a
                    // file, the dragged row itself) is not a root drop.
                    let want = self.dragging.peek()
                        && self.drop_target.peek().is_none()
                        && self.row_hover.peek().is_none()
                        && self.drop_ok(None);
                    if self.root_drop.peek() != want {
                        self.root_drop.set(want);
                    }
                }
            })
            .on_hover_exit(move || self.root_drop.set(false));
        self.with_menu(root_gesture.into_widget(), self.menu_target(None), None)
    }
}
