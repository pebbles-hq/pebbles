//! The explorer's filesystem data model: [`FsNode`]/[`FsKind`]/[`FileTree`] plus the
//! disk helpers ([`read_dir`], [`unique_name`]) and the OS folder picker. Pure data —
//! no widgets here.

use std::path::Path;

use pebbles_foundation::Color;
use pebbles_render::IconData;
#[cfg(feature = "file-dialogs")]
use std::path::PathBuf;

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
    /// Filesystem mode only: whether this folder's children have been read
    /// from disk yet (folders load lazily on first expand). In-memory nodes
    /// are always loaded.
    pub loaded: bool,
    /// Per-node glyph override (`None` = the icon theme, then the kind's
    /// default). Any of the ~1800 bundled lucide icons (or custom [`IconData`]).
    pub icon: Option<IconData>,
    /// Per-node glyph color override (`None` = the theme's muted foreground).
    pub color: Option<Color>,
}

impl FsNode {
    /// Create a folder node (the tree assigns the id when inserting).
    pub fn folder(name: impl Into<String>) -> Self {
        FsNode {
            id: 0,
            name: name.into(),
            kind: FsKind::Folder,
            children: Vec::new(),
            loaded: true,
            icon: None,
            color: None,
        }
    }
    /// Create a file node (the tree assigns the id when inserting).
    pub fn file(name: impl Into<String>) -> Self {
        FsNode {
            id: 0,
            name: name.into(),
            kind: FsKind::File,
            children: Vec::new(),
            loaded: true,
            icon: None,
            color: None,
        }
    }
    /// Give THIS node its own glyph (each node is customizable individually —
    /// e.g. a `.rs` file gets a code icon, `src/` a special folder). Accepts an
    /// [`IconKind`](pebbles_render::IconKind) or any `pebbles_render::lucide::*`
    /// const.
    pub fn icon(mut self, icon: impl Into<IconData>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    /// Give THIS node its own glyph color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
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

    /// Mutable access to a node — customize it in place (icon, color, …):
    /// `tree.update(|t| { if let Some(n) = t.node_mut(id) { n.icon = Some(..); } })`.
    pub fn node_mut(&mut self, id: u64) -> Option<&mut FsNode> {
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

    /// Assign fresh unique ids to `nodes` and their descendants (read_dir
    /// output carries placeholder ids — adopt it through this).
    pub(crate) fn assign_ids(&mut self, nodes: &mut [FsNode]) {
        for n in nodes {
            n.id = self.next_id;
            self.next_id += 1;
            self.assign_ids(&mut n.children);
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
        let node = FsNode { id, name, kind, children: Vec::new(), loaded: true, icon: None, color: None };
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

    /// Insert a fully-built [`FsNode`] (with its per-node `icon`/`color` and any
    /// pre-built children) into `parent` (`None` = the root). Ids are assigned to
    /// the node and its whole subtree; the name is de-duplicated against its
    /// siblings. Returns the new node's id.
    ///
    /// ```ignore
    /// t.insert_node(None, FsNode::folder("src").icon(IconKind::FolderOpen));
    /// t.insert_node(src, FsNode::file("main.rs").color(palette::AMBER_500));
    /// ```
    pub fn insert_node(&mut self, parent: Option<u64>, mut node: FsNode) -> u64 {
        node.id = self.next_id;
        self.next_id += 1;
        self.assign_ids(&mut node.children);
        let siblings: Vec<&str> = match parent {
            Some(p) => self.node(p).map(|n| n.children.iter().map(|c| c.name.as_str()).collect()),
            None => Some(self.root.iter().map(|n| n.name.as_str()).collect()),
        }
        .unwrap_or_default();
        node.name = unique_name(&node.name, &siblings);
        let id = node.id;
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
                let target_is_folder = matches!(self.node(new_parent), Some(n) if n.kind == FsKind::Folder);
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

/// Read a real directory into nodes: folders first, then files, both sorted
/// by name (folders start unloaded — their children load on expand).
pub(super) fn read_dir(path: &Path) -> std::io::Result<Vec<FsNode>> {
    let mut folders: Vec<FsNode> = Vec::new();
    let mut files: Vec<FsNode> = Vec::new();
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let ft = entry.file_type()?;
        if ft.is_dir() {
            folders.push(FsNode {
                id: 0,
                name,
                kind: FsKind::Folder,
                children: Vec::new(),
                loaded: false,
                icon: None,
                color: None,
            });
        } else if ft.is_file() {
            files.push(FsNode {
                id: 0,
                name,
                kind: FsKind::File,
                children: Vec::new(),
                loaded: true,
                icon: None,
                color: None,
            });
        }
    }
    folders.sort_by_key(|a| a.name.to_lowercase());
    files.sort_by_key(|a| a.name.to_lowercase());
    folders.extend(files);
    Ok(folders)
}

/// Open the OS folder picker (blocking, on a background thread) and deliver the
/// chosen path (or `None` on cancel) to `on_picked` on the UI thread. Needs the
/// `file-dialogs` feature.
#[cfg(feature = "file-dialogs")]
pub fn pick_folder(on_picked: impl Fn(Option<PathBuf>) + 'static) {
    // Web has no native folder picker (rfd/xdg/GTK don't apply); a browser
    // `<input type=file>`-backed picker is a follow-up. Resolve as "cancelled" so
    // the symbol exists and callers still link and behave sanely.
    #[cfg(target_family = "wasm")]
    return on_picked(None);

    #[cfg(not(target_family = "wasm"))]
    {
        // Under the synthetic-input monkey (PEBBLES_INPUT_STORM) never open a real
        // OS dialog — resolve as "cancelled" so burn-in runs stay unattended.
        if std::env::var("PEBBLES_INPUT_STORM").is_ok_and(|v| !v.is_empty() && v != "0") {
            on_picked(None);
            return;
        }
        pebbles_core::spawn(|| rfd::FileDialog::new().pick_folder(), on_picked);
    }
}

/// Recursively copy a file or a whole directory (filesystem-mode paste).
pub(super) fn copy_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::metadata(from)?.is_dir() {
        std::fs::create_dir(to)?;
        for entry in std::fs::read_dir(from)? {
            let entry = entry?;
            copy_path(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

/// `base` made unique against `taken` (`name`, `name 2`, `name 3`, …) — the
/// extension stays put: `readme.txt` → `readme 2.txt`.
pub(super) fn unique_name(base: &str, taken: &[&str]) -> String {
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
