//! Folder items: a folder the app presents as ONE row (a low-code item stored as
//! `Greeting.gravel/` holding `Greeting.gravel` and friends, a bundle, a package…).
//!
//! The row shows the app's label and icon, never its folder contents; a click opens the
//! item's main file; it expands only when that file is an expandable file (see
//! [`set_expandable_files`](FileExplorer::set_expandable_files)) — its rows are the
//! file's rows. Rename, delete, cut, copy, paste, drag, Reveal and Copy Path act on the
//! folder itself (the node IS the folder); it is never a drop or paste target.

#[allow(clippy::wildcard_imports)]
use super::*;

/// How a folder item shows (see [`FileExplorer::set_folder_items`]).
#[derive(Clone, Debug, PartialEq)]
pub struct FolderItem {
    /// The row's text (`Greeting`). When the folder's name starts with it, the rest
    /// (`.gravel`) is kept on rename: the editor shows the label, the folder becomes
    /// `<typed><rest>`.
    pub label: String,
    /// The file a click opens, and whose rows the item expands to.
    pub open: PathBuf,
    /// The row's icon (`None` = the icon the main file would get).
    pub icon: Option<ExplorerIcon>,
}

impl FolderItem {
    pub fn new(label: impl Into<String>, open: impl Into<PathBuf>) -> Self {
        FolderItem { label: label.into(), open: open.into(), icon: None }
    }
    pub fn icon(mut self, i: impl Into<ExplorerIcon>) -> Self {
        self.icon = Some(i.into());
        self
    }
}

pub(super) type ItemFn = Rc<dyn Fn(&Path) -> Option<FolderItem>>;

impl FileExplorer {
    /// Present some folders as single rows: `f(folder path)` returns how, or `None` for a
    /// normal folder. It runs while rendering, so keep it cheap and IO-free (a name check).
    pub fn set_folder_items(&self, f: impl Fn(&Path) -> Option<FolderItem> + 'static) {
        self.item_fn.set(Some(Rc::new(f)));
    }

    /// The folder item `id` is, if any (filesystem mode, a folder, the hook says so).
    pub fn folder_item(&self, id: u64) -> Option<FolderItem> {
        let f = self.item_fn.get()?;
        if self.tree.peek().node(id)?.kind != FsKind::Folder {
            return None;
        }
        f(&self.path_of(id)?)
    }

    /// Whether `id` is a folder items can be put into (a folder that isn't an item).
    pub(super) fn is_container(&self, id: u64) -> bool {
        self.tree.peek().node(id).is_some_and(|n| n.kind == FsKind::Folder) && self.folder_item(id).is_none()
    }

    /// An item's icon: the app's, else what its main file would get.
    pub(super) fn item_icon(&self, id: u64, it: &FolderItem) -> ExplorerIcon {
        if let Some(i) = &it.icon {
            return i.clone();
        }
        let name = it.open.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(r) = self.icon_resolver.get() {
            let e = ExplorerEntry { id, name: name.clone(), kind: FsKind::File, parent: None, path: Some(it.open.clone()) };
            if let Some(i) = r(&e, false) {
                return i;
            }
        }
        self.resolved_icon(&FsNode::file(name), false)
    }

    /// Expand / collapse an item: its main file's rows (when that file is expandable).
    pub(super) fn toggle_item(&self, id: u64, it: &FolderItem) {
        if !self.is_expandable_file(&it.open) {
            return;
        }
        let open = self.expanded.peek().contains(&id);
        self.expanded.update(|e| {
            if !e.remove(&id) {
                e.insert(id);
            }
        });
        if !open {
            let hook = self.file_rows_cfg.with(|c| c.on_expand.clone());
            if let Some(h) = hook {
                h(&it.open);
            }
        }
    }
}
