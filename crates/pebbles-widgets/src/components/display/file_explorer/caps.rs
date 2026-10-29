//! Developer controls: [`ExplorerCapabilities`] (every capability switchable,
//! plus a [`read_only`](ExplorerCapabilities::read_only) preset), per-item
//! predicates, before/after operation hooks, and [`ExplorerError`].

#[allow(clippy::wildcard_imports)]
use super::*;

/// One switchable explorer capability (see [`ExplorerCapabilities`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    /// New File (menu, toolbar, API).
    CreateFile,
    /// New Folder (and the intermediate folders of a nested `a/b/c.txt` name).
    CreateFolder,
    /// Inline rename / F2 / [`FileExplorer::rename_node`].
    Rename,
    /// Permanent delete (Shift+Delete, or Delete with no trash).
    Delete,
    /// Move to the trash (Delete).
    Trash,
    /// Drag-and-drop moves, cut/paste, [`FileExplorer::move_nodes`].
    Move,
    /// Copy/cut/paste and Ctrl/Alt-drag copies.
    CopyPaste,
    /// Accept files dropped in from the OS.
    DropFromOs,
    /// Drag rows out to other apps (where the platform supports it).
    DragOut,
    /// Ctrl/Shift multi-select, Mod+A, Shift+arrows.
    MultiSelect,
    /// The built-in right-click menus.
    ContextMenu,
    /// Show dot-files (hidden entries). Off = they are not loaded.
    ShowHidden,
    /// [`FileExplorer::watch`] polling for external changes.
    FileWatching,
    /// The explorer's keyboard shortcuts.
    Keyboard,
}

impl Capability {
    /// Every capability.
    pub const ALL: [Capability; 14] = [
        Capability::CreateFile,
        Capability::CreateFolder,
        Capability::Rename,
        Capability::Delete,
        Capability::Trash,
        Capability::Move,
        Capability::CopyPaste,
        Capability::DropFromOs,
        Capability::DragOut,
        Capability::MultiSelect,
        Capability::ContextMenu,
        Capability::ShowHidden,
        Capability::FileWatching,
        Capability::Keyboard,
    ];

    /// Whether this capability changes files (what [`read_only`](ExplorerCapabilities::read_only) turns off).
    pub fn mutates(self) -> bool {
        matches!(
            self,
            Capability::CreateFile
                | Capability::CreateFolder
                | Capability::Rename
                | Capability::Delete
                | Capability::Trash
                | Capability::Move
                | Capability::CopyPaste
                | Capability::DropFromOs
        )
    }

    fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// Which explorer capabilities are on. Default: everything. Disabled actions
/// are greyed out in the menus, their shortcuts decline, and the API refuses
/// them with [`ExplorerError::NotAllowed`] without touching the disk.
///
/// ```ignore
/// // Browse/select/open only:
/// let ex = file_explorer(tree).capabilities(ExplorerCapabilities::read_only());
/// // Everything except deleting and dragging:
/// let ex = file_explorer(tree)
///     .capabilities(ExplorerCapabilities::all().allow_delete(false).allow_trash(false).allow_move(false));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplorerCapabilities(u32);

impl Default for ExplorerCapabilities {
    fn default() -> Self {
        Self::all()
    }
}

macro_rules! cap_setters {
    ($($fn:ident => $cap:ident),* $(,)?) => {
        $(
            #[doc = concat!("Switch [`Capability::", stringify!($cap), "`] on or off.")]
            #[must_use]
            pub fn $fn(self, on: bool) -> Self {
                self.with(Capability::$cap, on)
            }
        )*
    };
}

impl ExplorerCapabilities {
    /// Every capability on (the default).
    pub fn all() -> Self {
        Self(Capability::ALL.iter().fold(0, |b, c| b | c.bit()))
    }

    /// Nothing on.
    pub fn none() -> Self {
        Self(0)
    }

    /// Browse, select (multi too), open, context menu (read-only items), keyboard
    /// navigation, hidden files and file watching — but no mutation of any kind.
    pub fn read_only() -> Self {
        Capability::ALL.iter().fold(Self::all(), |c, cap| if cap.mutates() { c.with(*cap, false) } else { c })
    }

    /// Set one capability.
    #[must_use]
    pub fn with(self, cap: Capability, on: bool) -> Self {
        if on { Self(self.0 | cap.bit()) } else { Self(self.0 & !cap.bit()) }
    }

    /// Whether `cap` is on.
    pub fn allows(self, cap: Capability) -> bool {
        self.0 & cap.bit() != 0
    }

    /// Whether every mutating capability is off.
    pub fn is_read_only(self) -> bool {
        Capability::ALL.iter().all(|c| !c.mutates() || !self.allows(*c))
    }

    cap_setters! {
        allow_create_file => CreateFile,
        allow_create_folder => CreateFolder,
        allow_rename => Rename,
        allow_delete => Delete,
        allow_trash => Trash,
        allow_move => Move,
        allow_copy_paste => CopyPaste,
        allow_drop_from_os => DropFromOs,
        allow_drag_out => DragOut,
        allow_multi_select => MultiSelect,
        show_context_menu => ContextMenu,
        show_hidden => ShowHidden,
        file_watching => FileWatching,
        keyboard_shortcuts => Keyboard,
    }
}

/// Why an explorer operation was refused or failed. Every refusal happens
/// BEFORE the disk is touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplorerError {
    /// The capability is switched off ([`ExplorerCapabilities`]).
    NotAllowed(Capability),
    /// A per-item predicate (`can_rename`, `can_delete`, `can_move`,
    /// `can_drop_into`) refused it — the message names the item.
    Refused(String),
    /// A before-hook ([`FileExplorer::on_before_op`]) vetoed it.
    Vetoed,
    /// The request itself is invalid (bad name, into itself, already exists,
    /// nothing to do, no such node).
    Invalid(String),
    /// The filesystem operation failed.
    Io(String),
}

impl std::fmt::Display for ExplorerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExplorerError::NotAllowed(c) => write!(f, "{c:?} is not allowed here"),
            ExplorerError::Refused(m) | ExplorerError::Invalid(m) | ExplorerError::Io(m) => f.write_str(m),
            ExplorerError::Vetoed => f.write_str("The operation was cancelled"),
        }
    }
}

impl std::error::Error for ExplorerError {}

/// A node as seen by predicates and hooks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerEntry {
    pub id: u64,
    pub name: String,
    pub kind: FsKind,
    /// The parent folder's id (`None` = a root entry).
    pub parent: Option<u64>,
    /// The on-disk path (filesystem mode only).
    pub path: Option<PathBuf>,
}

impl ExplorerEntry {
    pub fn is_folder(&self) -> bool {
        self.kind == FsKind::Folder
    }
}

/// A file operation about to happen (before-hooks may veto it) or that just
/// happened (after-hooks). `target`/`parent` `None` = the explorer root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplorerOp {
    Create {
        parent: Option<ExplorerEntry>,
        kind: FsKind,
        name: String,
    },
    Rename {
        entry: ExplorerEntry,
        new_name: String,
    },
    /// `permanent: false` = to the trash.
    Delete {
        entries: Vec<ExplorerEntry>,
        permanent: bool,
    },
    Move {
        entries: Vec<ExplorerEntry>,
        target: Option<ExplorerEntry>,
    },
    Copy {
        entries: Vec<ExplorerEntry>,
        target: Option<ExplorerEntry>,
    },
}

pub(super) type EntryPred = Rc<dyn Fn(&ExplorerEntry) -> bool>;
pub(super) type DropPred = Rc<dyn Fn(Option<&ExplorerEntry>, &[ExplorerEntry]) -> bool>;
pub(super) type BeforeHook = Rc<dyn Fn(&ExplorerOp) -> bool>;
pub(super) type AfterHook = Rc<dyn Fn(&ExplorerOp)>;

/// The explorer's developer controls (one signal so the struct stays `Copy`).
#[derive(Clone, Default)]
pub(super) struct Controls {
    pub(super) can_rename: Option<EntryPred>,
    pub(super) can_delete: Option<EntryPred>,
    pub(super) can_move: Option<EntryPred>,
    pub(super) can_drop_into: Option<DropPred>,
    pub(super) before: Vec<BeforeHook>,
    pub(super) after: Vec<AfterHook>,
}

impl FileExplorer {
    /// Set the capabilities (builder). See [`ExplorerCapabilities`].
    #[must_use]
    pub fn capabilities(self, caps: ExplorerCapabilities) -> Self {
        self.set_capabilities(caps);
        self
    }

    /// The read-only preset: browse, select and open only (builder).
    #[must_use]
    pub fn read_only(self) -> Self {
        self.capabilities(ExplorerCapabilities::read_only())
    }

    /// Change the capabilities at runtime.
    pub fn set_capabilities(&self, caps: ExplorerCapabilities) {
        let hidden_changed =
            self.caps.peek().allows(Capability::ShowHidden) != caps.allows(Capability::ShowHidden);
        self.caps.set(caps);
        if !caps.allows(Capability::FileWatching) {
            self.watch(None);
        }
        if hidden_changed && let Some(root) = self.fs_root.peek().clone() {
            self.open_folder(root);
        }
    }

    /// The current capabilities (reactive).
    pub fn get_capabilities(&self) -> ExplorerCapabilities {
        self.caps.get()
    }

    /// Whether `cap` is on (reactive).
    pub fn allows(&self, cap: Capability) -> bool {
        self.caps.get().allows(cap)
    }

    /// Per-item rename predicate (e.g. protect `Cargo.toml`).
    pub fn can_rename(&self, f: impl Fn(&ExplorerEntry) -> bool + 'static) {
        self.controls.update(|c| c.can_rename = Some(Rc::new(f)));
    }

    /// Per-item delete/trash predicate.
    pub fn can_delete(&self, f: impl Fn(&ExplorerEntry) -> bool + 'static) {
        self.controls.update(|c| c.can_delete = Some(Rc::new(f)));
    }

    /// Per-item move predicate (drag, cut/paste, [`move_nodes`](Self::move_nodes)).
    pub fn can_move(&self, f: impl Fn(&ExplorerEntry) -> bool + 'static) {
        self.controls.update(|c| c.can_move = Some(Rc::new(f)));
    }

    /// Drop-target predicate: may `items` be moved/copied/dropped into
    /// `target` (`None` = the root)? Refused targets don't highlight.
    pub fn can_drop_into(&self, f: impl Fn(Option<&ExplorerEntry>, &[ExplorerEntry]) -> bool + 'static) {
        self.controls.update(|c| c.can_drop_into = Some(Rc::new(f)));
    }

    /// Register a before-hook: called with every create/rename/delete/move/copy
    /// before anything happens; return `false` to veto it
    /// ([`ExplorerError::Vetoed`]). Several hooks may be registered; any veto wins.
    pub fn on_before_op(&self, f: impl Fn(&ExplorerOp) -> bool + 'static) {
        self.controls.update(|c| c.before.push(Rc::new(f)));
    }

    /// Register an after-event: called once an operation has happened.
    pub fn on_after_op(&self, f: impl Fn(&ExplorerOp) + 'static) {
        self.controls.update(|c| c.after.push(Rc::new(f)));
    }

    /// Before-hook for creates only: `(parent, kind, name) -> allow`.
    pub fn on_before_create(&self, f: impl Fn(Option<&ExplorerEntry>, FsKind, &str) -> bool + 'static) {
        self.on_before_op(move |op| match op {
            ExplorerOp::Create { parent, kind, name } => f(parent.as_ref(), *kind, name),
            _ => true,
        });
    }

    /// Before-hook for renames only: `(entry, new_name) -> allow`.
    pub fn on_before_rename(&self, f: impl Fn(&ExplorerEntry, &str) -> bool + 'static) {
        self.on_before_op(move |op| match op {
            ExplorerOp::Rename { entry, new_name } => f(entry, new_name),
            _ => true,
        });
    }

    /// Before-hook for deletes (trash and permanent): `(entries, permanent) -> allow`.
    pub fn on_before_delete(&self, f: impl Fn(&[ExplorerEntry], bool) -> bool + 'static) {
        self.on_before_op(move |op| match op {
            ExplorerOp::Delete { entries, permanent } => f(entries, *permanent),
            _ => true,
        });
    }

    /// Before-hook for moves only: `(entries, target) -> allow`.
    pub fn on_before_move(&self, f: impl Fn(&[ExplorerEntry], Option<&ExplorerEntry>) -> bool + 'static) {
        self.on_before_op(move |op| match op {
            ExplorerOp::Move { entries, target } => f(entries, target.as_ref()),
            _ => true,
        });
    }

    /// Before-hook for copies only: `(entries, target) -> allow`.
    pub fn on_before_copy(&self, f: impl Fn(&[ExplorerEntry], Option<&ExplorerEntry>) -> bool + 'static) {
        self.on_before_op(move |op| match op {
            ExplorerOp::Copy { entries, target } => f(entries, target.as_ref()),
            _ => true,
        });
    }

    /// The [`ExplorerEntry`] for a node.
    pub fn entry(&self, id: u64) -> Option<ExplorerEntry> {
        let t = self.tree.peek();
        let n = t.node(id)?;
        Some(ExplorerEntry {
            id,
            name: n.name.clone(),
            kind: n.kind,
            parent: t.parent_of(id),
            path: {
                drop(t);
                self.path_of(id)
            },
        })
    }

    pub(super) fn entries(&self, ids: &[u64]) -> Vec<ExplorerEntry> {
        ids.iter().filter_map(|&id| self.entry(id)).collect()
    }

    /// Record and return an error (shown via [`last_error`](Self::last_error)).
    pub(super) fn fail<T>(&self, e: ExplorerError) -> Result<T, ExplorerError> {
        self.last_error.set(Some(e.to_string()));
        Err(e)
    }

    /// Capability gate.
    pub(super) fn require(&self, cap: Capability) -> Result<(), ExplorerError> {
        if self.caps.peek().allows(cap) { Ok(()) } else { self.fail(ExplorerError::NotAllowed(cap)) }
    }

    /// Per-item predicate gate (`which` names the predicate).
    pub(super) fn check_items(
        &self,
        which: ItemPred,
        entries: &[ExplorerEntry],
    ) -> Result<(), ExplorerError> {
        let c = self.controls.peek();
        let pred = match which {
            ItemPred::Rename => c.can_rename.clone(),
            ItemPred::Delete => c.can_delete.clone(),
            ItemPred::Move => c.can_move.clone(),
        };
        drop(c);
        if let Some(p) = pred
            && let Some(bad) = entries.iter().find(|e| !p(e))
        {
            return self.fail(ExplorerError::Refused(format!("'{}' can't be {}", bad.name, which.verb())));
        }
        Ok(())
    }

    /// Drop-target predicate gate.
    pub(super) fn check_drop(
        &self,
        target: Option<u64>,
        entries: &[ExplorerEntry],
    ) -> Result<(), ExplorerError> {
        let p = self.controls.peek().can_drop_into.clone();
        if let Some(p) = p {
            let t = target.and_then(|t| self.entry(t));
            if !p(t.as_ref(), entries) {
                let name = t.map(|t| t.name).unwrap_or_else(|| "the root".into());
                return self.fail(ExplorerError::Refused(format!("Can't drop into '{name}'")));
            }
        }
        Ok(())
    }

    /// Whether a drop of the current selection into `target` would be accepted
    /// (drives the drop-target highlight).
    pub(super) fn drop_ok(&self, target: Option<u64>) -> bool {
        let p = self.controls.peek().can_drop_into.clone();
        match p {
            None => true,
            Some(p) => {
                let items = self.entries(&self.selected.peek());
                let t = target.and_then(|t| self.entry(t));
                p(t.as_ref(), &items)
            }
        }
    }

    /// Run the before-hooks.
    pub(super) fn before(&self, op: &ExplorerOp) -> Result<(), ExplorerError> {
        let hooks = self.controls.peek().before.clone();
        if hooks.iter().all(|h| h(op)) { Ok(()) } else { self.fail(ExplorerError::Vetoed) }
    }

    /// Fire the after-events.
    pub(super) fn after(&self, op: &ExplorerOp) {
        let hooks = self.controls.peek().after.clone();
        for h in hooks {
            h(op);
        }
    }
}

/// Which per-item predicate [`FileExplorer::check_items`] consults.
#[derive(Clone, Copy)]
pub(super) enum ItemPred {
    Rename,
    Delete,
    Move,
}

impl ItemPred {
    fn verb(self) -> &'static str {
        match self {
            ItemPred::Rename => "renamed",
            ItemPred::Delete => "deleted",
            ItemPred::Move => "moved",
        }
    }
}
