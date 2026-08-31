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

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Cursor, IconKind};

use crate::components::{ButtonVariant, context_menu, icon, icon_button, menu_item, muted, text_field};
use crate::theme::{mix, theme};
use crate::widgets::{Container, Expanded, GestureDetector, Padding, column, gap_w, row, text};
use pebbles_core::children;
use pebbles_core::keyboard::{KeyInput, ctrl_held, shift_held};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animated, component_props, create_signal};

// ---------------------------------------------------------------------------
// The data model
// ---------------------------------------------------------------------------

/// What a node is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FsKind {
    /// A directory — expandable, can hold children, can be a drop target.
    Folder,
    /// A leaf.
    File,
}

/// One node of the file tree. Identity is the stable `id` — rename, delete and
/// move all reference it, never the name.
#[derive(Clone, Debug)]
pub struct FsNode {
    pub id: u64,
    pub name: String,
    pub kind: FsKind,
    pub children: Vec<FsNode>,
}

impl FsNode {
    /// Create a folder node (the tree assigns the id when inserting).
    pub fn folder(name: impl Into<String>) -> Self {
        FsNode { id: 0, name: name.into(), kind: FsKind::Folder, children: Vec::new() }
    }
    /// Create a file node (the tree assigns the id when inserting).
    pub fn file(name: impl Into<String>) -> Self {
        FsNode { id: 0, name: name.into(), kind: FsKind::File, children: Vec::new() }
    }
}

/// The explorer's data model — app-owned: wrap it in a `Signal` and mutate it
/// through the helpers; the explorer re-renders from it each frame.
#[derive(Clone, Debug, Default)]
pub struct FileTree {
    pub root: Vec<FsNode>,
    next_id: u64,
}

impl FileTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a node (any depth).
    pub fn node(&self, id: u64) -> Option<&FsNode> {
        fn walk(nodes: &[FsNode], id: u64) -> Option<&FsNode> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(found) = walk(&n.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.root, id)
    }

    fn node_mut(&mut self, id: u64) -> Option<&mut FsNode> {
        fn walk(nodes: &mut [FsNode], id: u64) -> Option<&mut FsNode> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(found) = walk(&mut n.children, id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&mut self.root, id)
    }

    /// The id of the folder containing `id`, if any (root nodes have none).
    pub fn parent_of(&self, id: u64) -> Option<u64> {
        fn walk(nodes: &[FsNode], parent: Option<u64>, id: u64) -> Option<u64> {
            for n in nodes {
                if n.id == id {
                    return parent;
                }
                if let Some(found) = walk(&n.children, Some(n.id), id) {
                    return Some(found);
                }
            }
            None
        }
        walk(&self.root, None, id)
    }

    /// Whether `id` sits inside `parent`'s subtree (or is `parent` itself).
    pub fn is_descendant(&self, parent: u64, id: u64) -> bool {
        fn walk(nodes: &[FsNode], id: u64) -> bool {
            nodes.iter().any(|n| n.id == id || walk(&n.children, id))
        }
        match self.node(parent) {
            Some(n) => n.id == id || walk(&n.children, id),
            None => false,
        }
    }

    /// Insert `kind` `name` into `parent` (`None` = the root) and return the
    /// new node's id. The name is de-duplicated against its siblings
    /// (`new_file.txt`, `new_file 2.txt`, …).
    pub fn insert(&mut self, parent: Option<u64>, kind: FsKind, name: impl Into<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let base = name.into();
        let siblings: Vec<&str> = match parent {
            Some(p) => self.node(p).map(|n| n.children.iter().map(|c| c.name.as_str()).collect()),
            None => Some(self.root.iter().map(|n| n.name.as_str()).collect()),
        }
        .unwrap_or_default();
        let name = unique_name(&base, &siblings);
        let node = FsNode { id, name, kind, children: Vec::new() };
        match parent {
            Some(p) => {
                if let Some(pn) = self.node_mut(p) {
                    pn.children.push(node);
                } else {
                    self.root.push(node);
                }
            }
            None => self.root.push(node),
        }
        id
    }

    /// Rename a node (empty names are ignored).
    pub fn rename(&mut self, id: u64, name: impl Into<String>) -> bool {
        let name = name.into();
        if name.trim().is_empty() {
            return false;
        }
        match self.node_mut(id) {
            Some(n) => {
                n.name = name;
                true
            }
            None => false,
        }
    }

    /// Delete a node and its subtree.
    pub fn delete(&mut self, id: u64) -> bool {
        fn remove(nodes: &mut Vec<FsNode>, id: u64) -> bool {
            if let Some(i) = nodes.iter().position(|n| n.id == id) {
                nodes.remove(i);
                return true;
            }
            nodes.iter_mut().any(|n| remove(&mut n.children, id))
        }
        remove(&mut self.root, id)
    }

    /// Move `id` into `new_parent` (`None` = the root). Rejected (no-op,
    /// `false`) when the target is the node itself, one of its descendants, a
    /// file, or missing.
    pub fn move_node(&mut self, id: u64, new_parent: Option<u64>) -> bool {
        match new_parent {
            None => {
                if self.parent_of(id).is_none() {
                    return true; // already at the root
                }
                fn detach(nodes: &mut Vec<FsNode>, id: u64) -> Option<FsNode> {
                    if let Some(i) = nodes.iter().position(|n| n.id == id) {
                        return Some(nodes.remove(i));
                    }
                    nodes.iter_mut().find_map(|n| detach(&mut n.children, id))
                }
                if let Some(node) = detach(&mut self.root, id) {
                    self.root.push(node);
                    true
                } else {
                    false
                }
            }
            Some(new_parent) => {
                if id == new_parent {
                    return false;
                }
                let target_is_folder =
                    matches!(self.node(new_parent), Some(n) if n.kind == FsKind::Folder);
                if !target_is_folder || self.is_descendant(id, new_parent) {
                    return false;
                }
                fn detach(nodes: &mut Vec<FsNode>, id: u64) -> Option<FsNode> {
                    if let Some(i) = nodes.iter().position(|n| n.id == id) {
                        return Some(nodes.remove(i));
                    }
                    nodes.iter_mut().find_map(|n| detach(&mut n.children, id))
                }
                let node = detach(&mut self.root, id);
                match (node, self.node_mut(new_parent)) {
                    (Some(n), Some(p)) => {
                        p.children.push(n);
                        true
                    }
                    (Some(n), None) => {
                        self.root.push(n);
                        false
                    }
                    (None, _) => false,
                }
            }
        }
    }
}

/// `base` made unique against `taken` (`name`, `name 2`, `name 3`, …) — the
/// extension stays put: `readme.txt` → `readme 2.txt`.
fn unique_name(base: &str, taken: &[&str]) -> String {
    if !taken.contains(&base) {
        return base.to_string();
    }
    let (stem, ext) = match base.rfind('.') {
        Some(i) if i > 0 => (&base[..i], &base[i..]),
        _ => (base, ""),
    };
    let stem = match stem.rfind(' ') {
        Some(i) if stem[i + 1..].chars().all(|c| c.is_ascii_digit()) => &stem[..i],
        _ => stem,
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem} {n}{ext}");
        if !taken.contains(&candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

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

    fn select_only(&self, id: u64) {
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

    /// Create a file (in the active folder) and start renaming it.
    pub fn new_file(self) -> impl Fn() + 'static {
        move || {
            let parent = self.insertion_parent();
            let id = self.insert_at(parent, FsKind::File, "new_file.txt");
            self.start_rename_for(id);
        }
    }

    /// Create a folder (in the active folder) and start renaming it.
    pub fn new_folder(self) -> impl Fn() + 'static {
        move || {
            let parent = self.insertion_parent();
            let id = self.insert_at(parent, FsKind::Folder, "New Folder");
            self.start_rename_for(id);
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

    /// Delete the selection (all selected nodes, subtrees included).
    pub fn delete_selected(self) -> impl Fn() + 'static {
        move || {
            let ids = self.selected.get();
            self.tree.update(|t| {
                for id in &ids {
                    t.delete(*id);
                }
            });
            self.selected.set(Vec::new());
            self.renaming.set(None);
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

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Props for one tree row.
struct NodeProps {
    explorer: FileExplorer,
    node: FsNode,
    depth: usize,
}

/// One tree row: indent, twistie, glyph, label — with select/expand (Ctrl/Shift
/// multi), inline rename, a context menu, and drag-to-move (a selection drags
/// together; folders highlight as drop targets and expand on hover-hold).
fn render_node(p: &NodeProps) -> AnyWidget {
    let c = theme().colors;
    let explorer = p.explorer;
    let node = &p.node;
    let is_folder = node.kind == FsKind::Folder;
    let expanded = is_folder && explorer.expanded.get().contains(&node.id);
    let selected = explorer.selected.get().contains(&node.id);
    let renaming = explorer.renaming.get() == Some(node.id);
    let dragging = explorer.dragging.get();
    let dragged = dragging && selected;
    let drop_target = dragging && explorer.drop_target.get() == Some(node.id);
    let hovered = create_signal(false);
    let rename_buf = create_signal(String::new());

    // Row background: selection tint, hover tint, drop-target highlight.
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let mut bg = c.background;
    if selected {
        bg = mix(bg, c.accent, 0.12);
    }
    if drop_target {
        bg = mix(bg, c.accent, 0.24);
    }
    bg = mix(bg, c.foreground, 0.04 * hv as f32);

    let indent = gap_w(p.depth as f64 * 14.0);
    let twistie: AnyWidget = if is_folder {
        icon(if expanded { IconKind::ChevronDown } else { IconKind::ChevronRight })
            .size(14.0)
            .color(c.muted_foreground)
            .into_widget()
    } else {
        gap_w(14.0).into_widget()
    };
    let glyph = icon(if is_folder { IconKind::Folder } else { IconKind::File })
        .size(16.0)
        .color(c.muted_foreground);

    let label: AnyWidget = if renaming {
        component_props(
            render_rename_editor,
            RenameProps { explorer, id: node.id, buf: rename_buf, placeholder: node.name.clone() },
        )
        .into_widget()
    } else {
        text(node.name.clone())
            .size(13.5)
            .color(if dragged { c.muted_foreground } else { c.foreground })
            .into_widget()
    };

    let body = Container::new()
        .color(bg)
        .padding(EdgeInsets::symmetric(6.0, 3.0))
        .child(
            row(children![indent, twistie, gap_w(4.0), glyph, gap_w(6.0), Expanded::new(label)])
                .main_axis_size(MainAxisSize::Min),
        );

    // Renaming: no row gestures — the editor owns the input.
    let id = node.id;
    let row_widget: AnyWidget = if renaming {
        body.into_widget()
    } else {
        let g = GestureDetector::new(body)
            .cursor(Cursor::Pointer)
            .on_hover_enter({
                let hovered = hovered;
                move || hovered.set(true)
            })
            .on_hover_exit({
                let hovered = hovered;
                move || hovered.set(false)
            })
            .on_secondary_tap_down({
                move || {
                    // Right-clicking an unselected node selects just it (keeps
                    // the selection otherwise) — the standard behavior.
                    if !explorer.selected.get().contains(&id) {
                        explorer.select_only(id);
                    }
                }
            })
            .on_hover_enter({
                move || {
                    // Hovering a folder while dragging marks it as the drop
                    // target; hovering a collapsed one also starts the
                    // expand-on-hold timer.
                    if explorer.dragging.get() && is_folder {
                        explorer.drop_target.set(Some(id));
                        explorer.root_drop.set(false);
                        if !explorer.expanded.get().contains(&id) {
                            let ex = explorer;
                            pebbles_core::animation::set_timeout(explorer.hover_key, 0.6, move || {
                                if ex.dragging.get() && ex.drop_target.get() == Some(id) {
                                    ex.expanded.update(|e| {
                                        e.insert(id);
                                    });
                                }
                            });
                        }
                    }
                }
            })
            .on_hover_exit({
                move || {
                    if explorer.drop_target.get() == Some(id) {
                        explorer.drop_target.set(None);
                    }
                    pebbles_core::animation::clear_timeout(explorer.hover_key);
                }
            })
            .on_pan_start({
                move || {
                    // Dragging an unselected node selects it; a selected node
                    // drags the whole selection with it.
                    if !explorer.selected.get().contains(&id) {
                        explorer.select_only(id);
                    }
                    explorer.dragging.set(true);
                }
            })
            .on_pan_end({
                move || {
                    if !explorer.dragging.get() {
                        return;
                    }
                    let ids = explorer.selected.get();
                    let target = explorer.drop_target.get();
                    let root = explorer.root_drop.get();
                    explorer.tree.update(|t| {
                        for id in &ids {
                            if let Some(tid) = target {
                                t.move_node(*id, Some(tid));
                            } else if root {
                                t.move_node(*id, None);
                            }
                        }
                    });
                    if let Some(tid) = target {
                        explorer.expanded.update(|e| {
                            e.insert(tid);
                        });
                    }
                    explorer.dragging.set(false);
                    explorer.drop_target.set(None);
                    explorer.root_drop.set(false);
                }
            })
            .on_tap({
                move || {
                    if ctrl_held() {
                        explorer.toggle_select(id);
                    } else if shift_held() {
                        explorer.range_select(id);
                    } else {
                        explorer.select_only(id);
                        if is_folder {
                            explorer.expanded.update(|ex| {
                                if !ex.remove(&id) {
                                    ex.insert(id);
                                }
                            });
                        }
                    }
                }
            })
            .on_double_tap(move || explorer.start_rename_for(id));
        context_menu(g)
            .item(menu_item("New File").on_select(explorer.new_file()))
            .item(menu_item("New Folder").on_select(explorer.new_folder()))
            .separator()
            .item(menu_item("Rename").on_select(explorer.rename_selected()))
            .item(menu_item("Delete").destructive().on_select(explorer.delete_selected()))
            .into_widget()
    };

    let mut kids = vec![row_widget];
    if is_folder && expanded && !renaming {
        for child in &node.children {
            kids.push(
                component_props(
                    render_node,
                    NodeProps { explorer, node: child.clone(), depth: p.depth + 1 },
                )
                .into_widget(),
            );
        }
    }
    column(kids)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

/// Props for the inline rename editor.
struct RenameProps {
    explorer: FileExplorer,
    id: u64,
    buf: Signal<String>,
    placeholder: String,
}

/// The inline rename field: Enter/blur commits, Escape cancels.
fn render_rename_editor(p: &RenameProps) -> AnyWidget {
    let explorer = p.explorer;
    let id = p.id;
    let buf = p.buf;
    let commit = move || {
        let name = buf.peek().trim().to_string();
        if !name.is_empty() {
            explorer.tree.update(|t| {
                t.rename(id, name);
            });
        }
        explorer.renaming.set(None);
    };
    text_field()
        .placeholder(p.placeholder.clone())
        .bind(buf)
        .autofocus()
        .on_submit(move |_| commit())
        .on_focus_change(move |focused| {
            if !focused {
                commit();
            }
        })
        .on_nav(move |k| {
            if matches!(k, KeyInput::Escape) {
                explorer.renaming.set(None);
                true
            } else {
                false
            }
        })
        .into_widget()
}
