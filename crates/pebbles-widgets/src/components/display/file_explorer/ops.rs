//! File operations: open/refresh/watch, create, rename, move, copy,
//! trash/delete and the undo stack. Child module of `file_explorer`.

use super::caps::ItemPred;
#[allow(clippy::wildcard_imports)]
use super::*;

impl FileExplorer {
    /// Load a REAL directory into the explorer (children only — folders read
    /// their own children lazily on first expand). Clears the selection and
    /// expansion; reports failures through [`last_error`](Self::last_error).
    pub fn open_folder(&self, path: impl AsRef<Path>) -> bool {
        match read_dir(path.as_ref(), self.keep_fn().as_deref()) {
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

    /// Re-read every loaded folder from disk and merge: existing entries keep
    /// their ids (selection, expansion, open tabs stay put), new ones appear,
    /// vanished ones go. A no-op when nothing changed. Returns whether the
    /// tree changed. (The Refresh button; also what [`watch`](Self::watch) polls.)
    pub fn refresh(&self) -> bool {
        let Some(root) = self.fs_root.peek().clone() else { return false };
        let keep = self.keep_fn();
        fn rebuild(dir: &Path, old: &[FsNode], keep: Option<&tree::KeepEntry>) -> Option<Vec<FsNode>> {
            let fresh = read_dir(dir, keep).ok()?;
            Some(
                fresh
                    .into_iter()
                    .map(|f| match old.iter().find(|o| o.name == f.name && o.kind == f.kind) {
                        Some(o) => {
                            let mut n = o.clone();
                            if o.kind == FsKind::Folder && o.loaded {
                                n.children = rebuild(&dir.join(&o.name), &o.children, keep)
                                    .unwrap_or_else(|| o.children.clone());
                            }
                            n
                        }
                        // Marked new (ids start at 0, so 0 is a real id).
                        None => FsNode { id: u64::MAX, ..f },
                    })
                    .collect(),
            )
        }
        fn same(a: &[FsNode], b: &[FsNode]) -> bool {
            a.len() == b.len()
                && a.iter().zip(b).all(|(x, y)| {
                    x.id == y.id && x.name == y.name && x.kind == y.kind && same(&x.children, &y.children)
                })
        }
        fn fresh_ids(t: &mut FileTree, nodes: &mut [FsNode]) {
            for n in nodes {
                if n.id == u64::MAX {
                    t.assign_ids(std::slice::from_mut(n));
                } else {
                    fresh_ids(t, &mut n.children);
                }
            }
        }
        let old = self.tree.peek().root.clone();
        let Some(new) = rebuild(&root, &old, keep.as_deref()) else {
            self.last_error.set(Some(format!("Could not read {}", root.display())));
            return false;
        };
        if same(&old, &new) {
            return false;
        }
        self.tree.update(|t| {
            let mut new = new;
            fresh_ids(t, &mut new);
            t.root = new;
        });
        // Drop selection entries that vanished.
        let t = self.tree.peek();
        let sel: Vec<u64> = self.selected.peek().iter().copied().filter(|&i| t.node(i).is_some()).collect();
        drop(t);
        if sel.len() != self.selected.peek().len() {
            self.selected.set(sel);
        }
        true
    }

    /// Watch the opened folder for external changes by polling every `secs`
    /// seconds ([`refresh`](Self::refresh) — only loaded folders are read, and
    /// nothing re-renders unless something changed). `None` stops watching.
    pub fn watch(&self, secs: Option<f64>) {
        let secs = secs.filter(|_| self.caps.peek().allows(Capability::FileWatching));
        self.watch_secs.set(secs);
        match secs {
            Some(s) => self.arm_watch(s),
            None => pebbles_core::animation::clear_timeout(self.watch_key),
        }
    }

    pub(super) fn arm_watch(&self, secs: f64) {
        let this = *self;
        pebbles_core::animation::set_timeout(self.watch_key, secs, move || {
            // try_peek: the explorer may have unmounted.
            if let Some(Some(s)) = this.watch_secs.try_peek() {
                this.refresh();
                this.arm_watch(s);
            }
        });
    }

    /// The effective load filter: the app's [`set_load_filter`](Self::set_load_filter)
    /// plus the hidden-files switch ([`Capability::ShowHidden`]).
    pub(super) fn keep_fn(&self) -> Option<LoadFilter> {
        let app = self.load_filter.peek().clone();
        if self.caps.peek().allows(Capability::ShowHidden) {
            return app;
        }
        Some(Rc::new(move |p: &Path, is_dir: bool| {
            let hidden = p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
            !hidden && app.as_ref().is_none_or(|f| f(p, is_dir))
        }))
    }

    /// Leave filesystem mode: detach from the disk. The model stays as-is and
    /// mutations become in-memory only (pair with setting a fresh tree).
    pub fn detach_folder(&self) {
        self.fs_root.set(None);
    }

    /// Read a folder's children from disk into the model (filesystem mode).
    pub(super) fn ensure_loaded(&self, id: u64) {
        if self.fs_root.get().is_none() {
            return;
        }
        let needs = self.tree.peek().node(id).is_some_and(|n| n.kind == FsKind::Folder && !n.loaded);
        if !needs {
            return;
        }
        if let Some(dir) = self.path_of(id) {
            match read_dir(&dir, self.keep_fn().as_deref()) {
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

    /// Rename a node — on disk in filesystem mode, in the model otherwise.
    /// A `a/b/c.txt` name creates the intermediate folders and moves the node
    /// into the last one. Never overwrites an existing entry. Undoable.
    pub fn rename_node(&self, id: u64, name: impl Into<String>) -> Result<(), ExplorerError> {
        let r = self.rename_inner(id, name.into());
        self.renaming.set(None);
        r
    }

    fn rename_inner(&self, id: u64, name: String) -> Result<(), ExplorerError> {
        let name = name.trim().trim_matches('/').to_string();
        self.require(Capability::Rename)?;
        let Some(entry) = self.entry(id) else {
            return self.fail(ExplorerError::Invalid("No such node".into()));
        };
        if name == entry.name {
            return Ok(());
        }
        if let Err(e) = self.validate_name(id, &name) {
            return self.fail(ExplorerError::Invalid(e));
        }
        self.check_items(ItemPred::Rename, std::slice::from_ref(&entry))?;
        let mut parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        let leaf = parts.pop().unwrap_or_default().to_string();
        if !parts.is_empty() {
            self.require(Capability::CreateFolder)?;
        }
        let op = ExplorerOp::Rename { entry: entry.clone(), new_name: name.clone() };
        self.before(&op)?;
        // "a/b/c.txt": create the intermediate folders (VS Code's nested create).
        let old_parent = entry.parent;
        let mut parent = old_parent;
        for dir in parts {
            parent = Some(self.ensure_child_folder(parent, dir)?);
        }
        if parent != old_parent && self.child_named(parent, &leaf).is_some() {
            return self.fail(ExplorerError::Invalid(format!("'{leaf}' already exists")));
        }
        let old_path = entry.path.clone();
        if let Some(root) = self.fs_root.peek().clone() {
            let Some(old) = old_path.clone() else {
                return self.fail(ExplorerError::Invalid("Could not resolve a path for the node".into()));
            };
            let dir = match parent {
                Some(p) => self.path_of(p),
                None => Some(root),
            };
            let new = dir.map(|d| d.join(&leaf)).unwrap_or_else(|| old.with_file_name(&leaf));
            // Never clobber: rename(2) silently replaces an existing file.
            let case_only = new.parent() == old.parent()
                && new.file_name().map(|n| n.to_string_lossy().to_lowercase())
                    == old.file_name().map(|n| n.to_string_lossy().to_lowercase());
            if new.exists() && !case_only {
                return self.fail(ExplorerError::Invalid(format!("'{leaf}' already exists")));
            }
            if let Err(e) = std::fs::rename(&old, &new) {
                return self.fail(ExplorerError::Io(format!("Could not rename: {e}")));
            }
        }
        self.tree.update(|t| {
            if parent != old_parent {
                t.move_node(id, parent);
            }
            t.rename(id, leaf.clone());
        });
        if let Some(p) = parent {
            self.expanded.update(|e| {
                e.insert(p);
            });
        }
        let new_path = self.path_of(id);
        self.push_history(FileOp::Rename {
            id,
            old_name: entry.name,
            old_parent,
            paths: old_path.zip(new_path),
        });
        self.after(&op);
        Ok(())
    }

    /// Validate a rename / new name (VS Code's rules): not `.`/`..`, no NUL or
    /// backslash, no leading/trailing whitespace per segment, and no sibling
    /// already using it (a case-only change of the node's own name is fine).
    pub fn validate_name(&self, id: u64, name: &str) -> Result<(), String> {
        let name = name.trim_matches('/');
        tree::validate_segments(name)?;
        if !name.contains('/') {
            let parent = self.tree.peek().parent_of(id);
            if let Some(other) = self.child_named(parent, name)
                && other != id
            {
                return Err(format!("A file or folder '{name}' already exists here"));
            }
        }
        Ok(())
    }

    /// The child of `parent` (`None` = root) named exactly `name`.
    pub(super) fn child_named(&self, parent: Option<u64>, name: &str) -> Option<u64> {
        let t = self.tree.peek();
        let kids = match parent {
            Some(p) => &t.node(p)?.children,
            None => &t.root,
        };
        kids.iter().find(|n| n.name == name).map(|n| n.id)
    }

    /// The folder `name` under `parent`, created (on disk too) when missing.
    pub(super) fn ensure_child_folder(&self, parent: Option<u64>, name: &str) -> Result<u64, ExplorerError> {
        if let Some(p) = parent {
            self.ensure_loaded(p);
        }
        if let Some(id) = self.child_named(parent, name) {
            let is_dir = self.tree.peek().node(id).is_some_and(|n| n.kind == FsKind::Folder);
            if !is_dir {
                return self.fail(ExplorerError::Invalid(format!("'{name}' is a file")));
            }
            return Ok(id);
        }
        if let Some(root) = self.fs_root.peek().clone() {
            let dir = match parent {
                Some(p) => self.path_of(p).unwrap_or(root),
                None => root,
            };
            if let Err(e) = std::fs::create_dir(dir.join(name)) {
                return self.fail(ExplorerError::Io(format!("Could not create folder {name}: {e}")));
            }
        }
        let cell = RefCell::new(None);
        self.tree.update(|t| *cell.borrow_mut() = Some(t.insert(parent, FsKind::Folder, name)));
        Ok(cell.take().expect("inserted"))
    }

    /// Delete nodes — on disk in filesystem mode, in the model otherwise.
    pub(super) fn delete_nodes(&self, ids: &[u64]) -> usize {
        let mut n = 0;
        for id in ids {
            if self.fs_root.get().is_some() {
                if let Some(path) = self.path_of(*id) {
                    let is_dir = self.tree.peek().node(*id).is_some_and(|n| n.kind == FsKind::Folder);
                    let res =
                        if is_dir { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
                    if let Err(e) = res {
                        self.last_error.set(Some(format!("Could not delete {}: {e}", path.display())));
                        continue;
                    }
                } else {
                    self.last_error.set(Some("Could not resolve a path for a node".to_string()));
                    continue;
                }
            }
            self.tree.update(|t| {
                t.delete(*id);
            });
            n += 1;
        }
        self.selected.set(Vec::new());
        self.active.set(None);
        self.renaming.set(None);
        n
    }

    /// Permanently delete nodes (no trash, no undo) — gated by
    /// [`Capability::Delete`], `can_delete` and the before-hooks. The
    /// confirmation dialog is the caller's job ([`request_delete`](Self::request_delete) asks).
    pub fn delete_permanently(&self, ids: &[u64]) -> Result<usize, ExplorerError> {
        let ids = self.top_level(ids);
        if ids.is_empty() {
            return self.fail(ExplorerError::Invalid("Nothing to delete".into()));
        }
        // In-memory trees: either removal capability allows it.
        if self.fs_root.peek().is_some() || !self.caps.peek().allows(Capability::Trash) {
            self.require(Capability::Delete)?;
        }
        let entries = self.entries(&ids);
        self.refuse_root(&entries)?;
        self.check_items(ItemPred::Delete, &entries)?;
        let op = ExplorerOp::Delete { entries, permanent: true };
        self.before(&op)?;
        let n = self.delete_nodes(&ids);
        self.after(&op);
        Ok(n)
    }

    /// Refuse operating on the opened root folder itself.
    pub(super) fn refuse_root(&self, entries: &[ExplorerEntry]) -> Result<(), ExplorerError> {
        let root = self.fs_root.peek().clone();
        if root.is_some() && entries.iter().any(|e| e.path.is_some() && e.path == root) {
            return self.fail(ExplorerError::Invalid("The root folder can't be changed".into()));
        }
        Ok(())
    }

    /// Allow (default) or refuse moves that touch the disk. With `false`,
    /// drag-and-drop and [`move_nodes`](Self::move_nodes) never rename on disk
    /// in filesystem mode (in-memory trees still move).
    pub fn allow_fs_moves(self, allow: bool) -> Self {
        self.fs_moves_allowed.set(allow);
        self
    }

    /// Ask before drag-and-drop moves on disk (default `true`, like VS Code's
    /// `explorer.confirmDragAndDrop`).
    pub fn confirm_moves(self, confirm: bool) -> Self {
        self.confirm_moves.set(confirm);
        self
    }

    /// Supply your own move confirmation (replaces the default dialog
    /// "Move 'x' into 'y'?"). See [`ConfirmMoveFn`].
    pub fn set_confirm_move(&self, f: impl Fn(&MoveRequest, Rc<dyn Fn(bool)>) + 'static) {
        self.confirm_hook.set(Some(Rc::new(f)));
    }

    /// Validate a move: drop missing nodes, nodes whose ancestor is also
    /// moving, no-ops (already in `target`), and moves into the node itself or
    /// a descendant. A file target is refused outright.
    pub(super) fn plan_move(&self, ids: &[u64], target: Option<u64>) -> Vec<u64> {
        let t = self.tree.peek();
        if let Some(tg) = target {
            match t.node(tg) {
                Some(n) if n.kind == FsKind::Folder => {}
                _ => return Vec::new(),
            }
        }
        ids.iter()
            .copied()
            .filter(|&id| t.node(id).is_some())
            .filter(|&id| !ids.iter().any(|&o| o != id && t.is_descendant(o, id)))
            .filter(|&id| t.parent_of(id) != target)
            .filter(|&id| target.is_none_or(|tg| !t.is_descendant(id, tg)))
            .collect()
    }

    /// Every gate a move must pass (capability, disk switch, predicates,
    /// hooks) — returns the validated plan and the op for the after-event.
    fn gate_move(&self, ids: &[u64], target: Option<u64>) -> Result<(Vec<u64>, ExplorerOp), ExplorerError> {
        self.require(Capability::Move)?;
        if let Some(t) = target
            && self.tree.peek().node(t).is_none_or(|n| n.kind != FsKind::Folder)
        {
            return self.fail(ExplorerError::Invalid("The target is not a folder".into()));
        }
        let plan = self.plan_move(ids, target);
        if plan.is_empty() {
            return self
                .fail(ExplorerError::Invalid("Nothing to move (already there, or into itself)".into()));
        }
        if self.fs_root.peek().is_some() && !self.fs_moves_allowed.peek() {
            return self.fail(ExplorerError::Invalid("Moving files on disk is disabled".into()));
        }
        let entries = self.entries(&plan);
        self.refuse_root(&entries)?;
        self.check_items(ItemPred::Move, &entries)?;
        self.check_drop(target, &entries)?;
        let op = ExplorerOp::Move { entries, target: target.and_then(|t| self.entry(t)) };
        self.before(&op)?;
        Ok((plan, op))
    }

    /// A drop: validate and gate, then (filesystem mode) ask for confirmation,
    /// then move. `Ok` = moved, or pending the confirmation.
    pub fn request_move(&self, ids: &[u64], target: Option<u64>) -> Result<(), ExplorerError> {
        let (plan, op) = self.gate_move(ids, target)?;
        if self.fs_root.peek().is_none() || !self.confirm_moves.peek() {
            return self.move_planned(&plan, target, &op).map(|_| ());
        }
        let ExplorerOp::Move { entries, target: tentry } = &op else { unreachable!() };
        let names = entries.iter().map(|e| e.name.clone()).collect();
        let target_name = match tentry {
            Some(t) => t.name.clone(),
            None => self
                .fs_root
                .peek()
                .as_ref()
                .and_then(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "/".into()),
        };
        let req = MoveRequest { ids: plan.clone(), names, target, target_name };
        let this = *self;
        let resolve: Rc<dyn Fn(bool)> = Rc::new(move |ok| {
            if ok {
                let _ = this.move_planned(&plan, target, &op);
            }
        });
        if let Some(hook) = self.confirm_hook.peek().clone() {
            hook(&req, resolve);
        } else {
            let what = if req.names.len() == 1 {
                format!("'{}'", req.names[0])
            } else {
                format!("{} items", req.names.len())
            };
            let r2 = resolve.clone();
            crate::alert_dialog(format!("Move {what} into '{}'?", req.target_name))
                .description("The files are moved on disk. You can undo this with Ctrl+Z.")
                .confirm("Move")
                .on_confirm(move || r2(true))
                .on_cancel(move || resolve(false))
                .open();
        }
        Ok(())
    }

    /// Move nodes into a folder (or the root) — on disk in filesystem mode,
    /// in the model otherwise. No confirmation (programmatic), but every gate
    /// applies ([`Capability::Move`], [`allow_fs_moves`](Self::allow_fs_moves),
    /// `can_move`, `can_drop_into`, before-hooks). Moves into itself/a
    /// descendant, no-ops and the root are refused; an existing name at the
    /// destination is never overwritten. Undoable. Returns how many moved.
    pub fn move_nodes(&self, ids: &[u64], target: Option<u64>) -> Result<usize, ExplorerError> {
        let (plan, op) = self.gate_move(ids, target)?;
        self.move_planned(&plan, target, &op)
    }

    fn move_planned(
        &self,
        plan: &[u64],
        target: Option<u64>,
        op: &ExplorerOp,
    ) -> Result<usize, ExplorerError> {
        let fs_root = self.fs_root.peek().clone();
        // Load an unexpanded target first: a later lazy load would otherwise
        // replace its children and orphan the moved nodes' ids.
        if let Some(t) = target {
            self.ensure_loaded(t);
        }
        let mut done = Vec::new();
        let mut err = None;
        for &id in plan {
            let old_parent = self.tree.peek().parent_of(id);
            let mut paths = None;
            if let Some(root) = &fs_root {
                let (Some(from), Some(into)) = (
                    self.path_of(id),
                    match target {
                        Some(t) => self.path_of(t),
                        None => Some(root.clone()),
                    },
                ) else {
                    err = Some(ExplorerError::Invalid("Could not resolve a path".into()));
                    continue;
                };
                if &from == root || into.starts_with(&from) {
                    err = Some(ExplorerError::Invalid(format!(
                        "Refusing to move {} into itself",
                        from.display()
                    )));
                    continue;
                }
                let to = into.join(from.file_name().unwrap_or_default());
                if to.exists() {
                    err = Some(ExplorerError::Invalid(format!("{} already exists", to.display())));
                    continue;
                }
                if let Err(e) = std::fs::rename(&from, &to) {
                    err = Some(ExplorerError::Io(format!("Could not move {}: {e}", from.display())));
                    continue;
                }
                paths = Some((from, to));
            }
            let mut moved = false;
            self.tree.update(|t| {
                moved = t.move_node(id, target);
            });
            if moved {
                done.push(MoveRecord { id, old_parent, paths });
            }
        }
        if let Some(t) = target {
            self.expanded.update(|e| {
                e.insert(t);
            });
        }
        let n = done.len();
        if n > 0 {
            self.push_history(FileOp::Move(done));
            self.after(op);
        }
        match err {
            Some(e) if n == 0 => self.fail(e),
            Some(e) => {
                self.last_error.set(Some(e.to_string()));
                Ok(n)
            }
            None => Ok(n),
        }
    }

    pub(super) fn push_history(&self, op: FileOp) {
        self.history.update(|h| {
            h.push(op);
            if h.len() > 100 {
                h.remove(0);
            }
        });
    }

    /// Whether there is a file operation to undo (reactive).
    pub fn can_undo(&self) -> bool {
        !self.history.get().is_empty()
    }

    /// Undo the most recent file operation (move, rename, …) — on disk too,
    /// never overwriting anything that has reappeared since. Returns whether
    /// anything was restored. Mod+Z in the tree.
    pub fn undo(&self) -> bool {
        let mut op = None;
        self.history.update(|h| op = h.pop());
        let Some(op) = op else { return false };
        let mut any = false;
        match op {
            FileOp::Move(batch) => {
                for rec in batch.into_iter().rev() {
                    if let Some((from, to)) = &rec.paths
                        && !self.undo_rename_on_disk(from, to)
                    {
                        continue;
                    }
                    self.tree.update(|t| {
                        any |= t.move_node(rec.id, rec.old_parent);
                    });
                }
            }
            FileOp::Rename { id, old_name, old_parent, paths } => {
                if let Some((from, to)) = &paths
                    && !self.undo_rename_on_disk(from, to)
                {
                    return false;
                }
                self.tree.update(|t| {
                    if t.parent_of(id) != old_parent {
                        t.move_node(id, old_parent);
                    }
                    any = t.rename(id, old_name);
                });
            }
            FileOp::Create { id, path } => {
                if let Some(path) = &path {
                    let empty = if path.is_dir() {
                        std::fs::read_dir(path).map(|mut d| d.next().is_none()).unwrap_or(false)
                    } else {
                        std::fs::metadata(path).map(|m| m.len() == 0).unwrap_or(false)
                    };
                    if !empty {
                        self.last_error
                            .set(Some(format!("Not undoing: {} is no longer empty", path.display())));
                        return false;
                    }
                    let res =
                        if path.is_dir() { std::fs::remove_dir(path) } else { std::fs::remove_file(path) };
                    if let Err(e) = res {
                        self.last_error.set(Some(format!("Could not undo: {e}")));
                        return false;
                    }
                }
                self.tree.update(|t| any = t.delete(id));
            }
            FileOp::Copy { ids, paths } => {
                // The copies are ours — remove them (to the trash when there is one).
                if !paths.is_empty() && self.trash_paths(&paths).is_none() {
                    return false;
                }
                self.tree.update(|t| {
                    for id in &ids {
                        any |= t.delete(*id);
                    }
                });
            }
            FileOp::Trash(entries) => {
                for (orig, trashed, info) in entries.iter().rev() {
                    if orig.exists() {
                        self.last_error.set(Some(format!("Cannot restore: {} exists", orig.display())));
                        continue;
                    }
                    if let Err(e) = std::fs::rename(trashed, orig) {
                        self.last_error.set(Some(format!("Could not restore {}: {e}", orig.display())));
                        continue;
                    }
                    let _ = std::fs::remove_file(info);
                    any = true;
                }
                if any {
                    self.refresh();
                }
            }
        }
        any
    }

    /// Use `dir` as the trash (freedesktop layout: `dir/files`, `dir/info`).
    /// By default the explorer uses the freedesktop home trash on Linux/BSD
    /// (`$XDG_DATA_HOME/Trash`, else `~/.local/share/Trash`) and has no trash
    /// elsewhere (Delete then deletes permanently, after confirmation).
    pub fn set_trash_dir(&self, dir: impl Into<PathBuf>) {
        self.trash_dir.set(Some(dir.into()));
    }

    /// The trash directory in use, if any.
    pub fn trash_dir(&self) -> Option<PathBuf> {
        if let Some(d) = self.trash_dir.peek().clone() {
            return Some(d);
        }
        #[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
        {
            let base = std::env::var_os("XDG_DATA_HOME")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
            Some(base.join("Trash"))
        }
        #[cfg(not(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android")))))]
        {
            None
        }
    }

    /// Move `paths` to the trash; returns the (orig, trashed, info) entries,
    /// or `None` when there is no trash or any item failed (those that made it
    /// stay trashed and are still returned via the error path's history).
    pub(super) fn trash_paths(&self, paths: &[PathBuf]) -> Option<Vec<(PathBuf, PathBuf, PathBuf)>> {
        let Some(trash) = self.trash_dir() else {
            self.last_error.set(Some("No trash available".into()));
            return None;
        };
        let (files, info) = (trash.join("files"), trash.join("info"));
        if let Err(e) = std::fs::create_dir_all(&files).and_then(|_| std::fs::create_dir_all(&info)) {
            self.last_error.set(Some(format!("Could not open the trash: {e}")));
            return None;
        }
        let mut out = Vec::new();
        for path in paths {
            let base = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut name = base.clone();
            let mut n = 2;
            while files.join(&name).exists() || info.join(format!("{name}.trashinfo")).exists() {
                name = format!("{base}.{n}");
                n += 1;
            }
            let info_path = info.join(format!("{name}.trashinfo"));
            let abs = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let body = format!(
                "[Trash Info]\nPath={}\nDeletionDate={}\n",
                tree::percent_encode_path(&abs),
                tree::local_iso_now()
            );
            if let Err(e) = std::fs::write(&info_path, body) {
                self.last_error.set(Some(format!("Could not write the trash info: {e}")));
                return None;
            }
            let dest = files.join(&name);
            if let Err(e) = std::fs::rename(path, &dest) {
                let _ = std::fs::remove_file(&info_path);
                self.last_error.set(Some(format!(
                    "Could not move {} to the trash ({e}) — use Shift+Delete to delete permanently",
                    path.display()
                )));
                if !out.is_empty() {
                    self.push_history(FileOp::Trash(out));
                }
                return None;
            }
            out.push((path.clone(), dest, info_path));
        }
        Some(out)
    }

    /// Move nodes to the trash (filesystem mode; undoable) — gated by
    /// [`Capability::Trash`], `can_delete` and the before-hooks. In-memory
    /// trees just delete. Returns how many were removed.
    pub fn trash_nodes(&self, ids: &[u64]) -> Result<usize, ExplorerError> {
        let ids = self.top_level(ids);
        if ids.is_empty() {
            return self.fail(ExplorerError::Invalid("Nothing to delete".into()));
        }
        if self.fs_root.peek().is_some() || !self.caps.peek().allows(Capability::Delete) {
            self.require(Capability::Trash)?;
        }
        let entries = self.entries(&ids);
        self.refuse_root(&entries)?;
        self.check_items(ItemPred::Delete, &entries)?;
        let op = ExplorerOp::Delete { entries, permanent: false };
        self.before(&op)?;
        if self.fs_root.peek().is_none() {
            let n = self.delete_nodes(&ids);
            self.after(&op);
            return Ok(n);
        }
        let mut done = Vec::new();
        let mut gone = Vec::new();
        for id in ids {
            let Some(path) = self.path_of(id) else { continue };
            match self.trash_paths(std::slice::from_ref(&path)) {
                Some(mut e) => {
                    done.append(&mut e);
                    gone.push(id);
                }
                None => break,
            }
        }
        self.tree.update(|t| {
            for id in &gone {
                t.delete(*id);
            }
        });
        self.selected.set(Vec::new());
        self.active.set(None);
        let n = gone.len();
        if n == 0 {
            let msg = self.last_error.peek().clone().unwrap_or_else(|| "Could not move to the trash".into());
            return Err(ExplorerError::Io(msg));
        }
        self.push_history(FileOp::Trash(done));
        self.after(&op);
        Ok(n)
    }

    /// `ids` without the ones whose ancestor is also in `ids`.
    pub(super) fn top_level(&self, ids: &[u64]) -> Vec<u64> {
        let t = self.tree.peek();
        ids.iter()
            .copied()
            .filter(|&id| t.node(id).is_some())
            .filter(|&id| !ids.iter().any(|&o| o != id && t.is_descendant(o, id)))
            .collect()
    }

    /// The Delete action (key, menu): in filesystem mode, confirm then move
    /// the selection to the trash (VS Code) — or, with `permanent`
    /// (Shift+Delete) or no trash (none available, or [`Capability::Trash`]
    /// off), confirm a permanent delete. In-memory trees delete immediately.
    /// Gates are checked BEFORE the dialog opens.
    pub fn request_delete(&self, permanent: bool) -> Result<(), ExplorerError> {
        let ids = self.top_level(&self.selected.peek());
        if ids.is_empty() {
            return Err(ExplorerError::Invalid("Nothing selected".into()));
        }
        if self.fs_root.peek().is_none() {
            return if permanent { self.delete_permanently(&ids) } else { self.trash_nodes(&ids) }
                .map(|_| ());
        }
        let caps = self.caps.peek();
        let use_trash = !permanent && caps.allows(Capability::Trash) && self.trash_dir().is_some();
        self.require(if use_trash { Capability::Trash } else { Capability::Delete })?;
        let entries = self.entries(&ids);
        self.refuse_root(&entries)?;
        self.check_items(ItemPred::Delete, &entries)?;
        let what =
            if ids.len() == 1 { format!("'{}'", entries[0].name) } else { format!("{} items", ids.len()) };
        let this = *self;
        if use_trash {
            crate::alert_dialog(format!("Are you sure you want to delete {what}?"))
                .description("You can restore from the Trash (or undo with Ctrl+Z).")
                .confirm("Move to Trash")
                .on_confirm(move || {
                    let _ = this.trash_nodes(&ids);
                })
                .open();
        } else {
            crate::alert_dialog(format!("Are you sure you want to permanently delete {what}?"))
                .description("This item will be deleted immediately. You can't undo this action.")
                .destructive(true)
                .confirm("Delete")
                .on_confirm(move || {
                    let _ = this.delete_permanently(&ids);
                })
                .open();
        }
        Ok(())
    }

    /// Put `to` back at `from` (refusing when `from` exists again).
    pub(super) fn undo_rename_on_disk(&self, from: &Path, to: &Path) -> bool {
        let case_only = from.parent() == to.parent()
            && from.to_string_lossy().to_lowercase() == to.to_string_lossy().to_lowercase();
        if from.exists() && !case_only {
            self.last_error.set(Some(format!("Cannot undo: {} exists again", from.display())));
            return false;
        }
        if let Err(e) = std::fs::rename(to, from) {
            self.last_error.set(Some(format!("Could not undo {}: {e}", to.display())));
            return false;
        }
        true
    }

    /// Copy the selection to the explorer clipboard (paste with Mod+V / the menu).
    pub fn copy_selection(&self) -> Result<(), ExplorerError> {
        self.require(Capability::CopyPaste)?;
        let ids = self.selected.peek().clone();
        if ids.is_empty() {
            return Err(ExplorerError::Invalid("Nothing selected".into()));
        }
        self.clipboard.set(Some((ids, ClipMode::Copy)));
        Ok(())
    }

    /// Cut the selection: pasting MOVES it. Cut rows render dimmed until pasted
    /// or cancelled (Escape). Needs [`Capability::CopyPaste`] and [`Capability::Move`].
    pub fn cut_selection(&self) -> Result<(), ExplorerError> {
        self.require(Capability::CopyPaste)?;
        self.require(Capability::Move)?;
        let ids = self.selected.peek().clone();
        if ids.is_empty() {
            return Err(ExplorerError::Invalid("Nothing selected".into()));
        }
        let entries = self.entries(&ids);
        self.check_items(ItemPred::Move, &entries)?;
        self.clipboard.set(Some((ids, ClipMode::Cut)));
        Ok(())
    }

    /// Paste the clipboard into the active folder (the selected folder, else the
    /// selected file's parent, else the root — VSCode's target rules). Cut moves;
    /// Copy duplicates the whole subtree (on disk too) named `x copy.txt`.
    pub fn paste_clipboard(&self) -> Result<(), ExplorerError> {
        self.require(Capability::CopyPaste)?;
        let Some((ids, mode)) = self.clipboard.peek().clone() else {
            return Err(ExplorerError::Invalid("The clipboard is empty".into()));
        };
        let target = self.insertion_parent();
        match mode {
            ClipMode::Cut => {
                let r = self.move_nodes(&ids, target).map(|_| ());
                self.clipboard.set(None);
                r
            }
            ClipMode::Copy => self.copy_nodes(&ids, target).map(|_| ()),
        }
    }

    /// Copy nodes into `target` (`None` = root) — whole subtrees, on disk in
    /// filesystem mode, named like VS Code (`a copy.txt`). Gated by
    /// [`Capability::CopyPaste`], `can_drop_into` and the before-hooks.
    /// Undoable. Used by paste and Ctrl/Alt-drag. Returns the new node ids.
    pub fn copy_nodes(&self, ids: &[u64], target: Option<u64>) -> Result<Vec<u64>, ExplorerError> {
        self.require(Capability::CopyPaste)?;
        if let Some(t) = target
            && self.tree.peek().node(t).is_none_or(|n| n.kind != FsKind::Folder)
        {
            return self.fail(ExplorerError::Invalid("The target is not a folder".into()));
        }
        let ids = self.top_level(ids);
        if ids.is_empty() {
            return self.fail(ExplorerError::Invalid("Nothing to copy".into()));
        }
        let entries = self.entries(&ids);
        self.check_drop(target, &entries)?;
        let op = ExplorerOp::Copy { entries, target: target.and_then(|t| self.entry(t)) };
        self.before(&op)?;
        if let Some(t) = target {
            self.ensure_loaded(t);
        }
        let mut new_ids = Vec::new();
        let mut new_paths = Vec::new();
        let mut err = None;
        for &id in &ids {
            // Snapshot the subtree first, so pasting into itself is safe.
            let Some(mut src) = self.tree.peek().node(id).cloned() else { continue };
            if let Some(root) = self.fs_root.peek().clone() {
                let (Some(from), Some(dir)) = (
                    self.path_of(id),
                    match target {
                        Some(t) => self.path_of(t),
                        None => Some(root),
                    },
                ) else {
                    err = Some(ExplorerError::Invalid("Could not resolve a path".into()));
                    continue;
                };
                if dir.starts_with(&from) {
                    err = Some(ExplorerError::Invalid(format!("Cannot copy {} into itself", from.display())));
                    continue;
                }
                // De-dup against the REAL directory, then mirror on disk.
                let taken = std::fs::read_dir(&dir)
                    .map(|rd| {
                        rd.filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let taken_refs: Vec<&str> = taken.iter().map(|s| s.as_str()).collect();
                let name = tree::copy_name(&src.name, &taken_refs);
                if let Err(e) = copy_path(&from, &dir.join(&name)) {
                    err = Some(ExplorerError::Io(format!("Could not copy {}: {e}", from.display())));
                    continue;
                }
                new_paths.push(dir.join(&name));
                src.name = name;
            } else {
                let t = self.tree.peek();
                let sibs = match target {
                    Some(p) => t.node(p).map(|n| n.children.clone()).unwrap_or_default(),
                    None => t.root.clone(),
                };
                let taken: Vec<&str> = sibs.iter().map(|n| n.name.as_str()).collect();
                src.name = tree::copy_name(&src.name, &taken);
            }
            new_ids.push(self.insert_subtree(target, src));
        }
        if !new_ids.is_empty() {
            self.push_history(FileOp::Copy { ids: new_ids.clone(), paths: new_paths });
            self.after(&op);
        }
        match err {
            Some(e) if new_ids.is_empty() => self.fail(e),
            Some(e) => {
                self.last_error.set(Some(e.to_string()));
                Ok(new_ids)
            }
            None => Ok(new_ids),
        }
    }

    /// Insert a fully-built node (fresh subtree ids, name deduped) and reveal it.
    pub(super) fn insert_subtree(&self, parent: Option<u64>, node: FsNode) -> u64 {
        let cell = RefCell::new(None);
        self.tree.update(|t| {
            *cell.borrow_mut() = Some(t.insert_node(parent, node));
        });
        let id = cell.take().expect("inserted");
        self.reveal(id);
        id
    }

    /// Create a file or folder named `name` (a nested `a/b/c.txt` creates the
    /// intermediate folders) in `parent` (`None` = root) — on disk in
    /// filesystem mode. Gated by [`Capability::CreateFile`] /
    /// [`Capability::CreateFolder`] and the before-hooks; never overwrites.
    /// Undoable. Returns the new node's id.
    pub fn create_entry(&self, parent: Option<u64>, kind: FsKind, name: &str) -> Result<u64, ExplorerError> {
        let name = name.trim().trim_matches('/').to_string();
        self.require(if kind == FsKind::Folder { Capability::CreateFolder } else { Capability::CreateFile })?;
        if let Some(p) = parent
            && self.tree.peek().node(p).is_none_or(|n| n.kind != FsKind::Folder)
        {
            return self.fail(ExplorerError::Invalid("The parent is not a folder".into()));
        }
        if let Err(e) = tree::validate_segments(&name) {
            return self.fail(ExplorerError::Invalid(e));
        }
        let mut parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        let leaf = parts.pop().unwrap_or_default().to_string();
        if !parts.is_empty() {
            self.require(Capability::CreateFolder)?;
        }
        let op = ExplorerOp::Create { parent: parent.and_then(|p| self.entry(p)), kind, name: name.clone() };
        self.before(&op)?;
        let mut dir_id = parent;
        for d in parts {
            dir_id = Some(self.ensure_child_folder(dir_id, d)?);
        }
        if let Some(p) = dir_id {
            self.ensure_loaded(p);
        }
        if self.child_named(dir_id, &leaf).is_some() {
            return self.fail(ExplorerError::Invalid(format!("'{leaf}' already exists")));
        }
        let mut path = None;
        if let Some(root) = self.fs_root.peek().clone() {
            let dir = match dir_id {
                Some(p) => self.path_of(p).unwrap_or(root),
                None => root,
            };
            let target = dir.join(&leaf);
            let created = if kind == FsKind::Folder {
                std::fs::create_dir(&target)
            } else {
                std::fs::OpenOptions::new().write(true).create_new(true).open(&target).map(|_| ())
            };
            if let Err(e) = created {
                return self.fail(ExplorerError::Io(format!("Could not create {}: {e}", target.display())));
            }
            path = Some(target);
        }
        let id = self.insert_at(dir_id, kind, &leaf);
        self.push_history(FileOp::Create { id, path });
        self.after(&op);
        Ok(id)
    }

    pub(super) fn insert_at(&self, parent: Option<u64>, kind: FsKind, name: &str) -> u64 {
        let cell = RefCell::new(None);
        self.tree.update(|t| {
            *cell.borrow_mut() = Some(t.insert(parent, kind, name));
        });
        let id = cell.take().expect("inserted");
        self.reveal(id);
        id
    }

    /// Copy files/folders from elsewhere on disk (an OS drag-and-drop) into
    /// `target` (`None` = root), named like VS Code on conflicts (`a copy.txt`
    /// — never overwrites). Gated by [`Capability::DropFromOs`],
    /// `can_drop_into` and the before-hooks (as an [`ExplorerOp::Copy`] whose
    /// entries have `id == u64::MAX`). Undoable. Returns the new node ids.
    pub fn import_paths(&self, paths: &[PathBuf], target: Option<u64>) -> Result<Vec<u64>, ExplorerError> {
        self.require(Capability::DropFromOs)?;
        let Some(root) = self.fs_root.peek().clone() else {
            return self.fail(ExplorerError::Invalid("Not in filesystem mode".into()));
        };
        let dir = match target {
            Some(t) => match self.tree.peek().node(t) {
                Some(n) if n.kind == FsKind::Folder => self.path_of(t).unwrap_or(root.clone()),
                _ => return self.fail(ExplorerError::Invalid("The target is not a folder".into())),
            },
            None => root.clone(),
        };
        let entries: Vec<ExplorerEntry> = paths
            .iter()
            .map(|p| ExplorerEntry {
                id: u64::MAX,
                name: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                kind: if p.is_dir() { FsKind::Folder } else { FsKind::File },
                parent: None,
                path: Some(p.clone()),
            })
            .collect();
        if entries.is_empty() {
            return self.fail(ExplorerError::Invalid("Nothing to import".into()));
        }
        self.check_drop(target, &entries)?;
        let op = ExplorerOp::Copy { entries, target: target.and_then(|t| self.entry(t)) };
        self.before(&op)?;
        if let Some(t) = target {
            self.ensure_loaded(t);
        }
        let mut made = Vec::new();
        let mut err = None;
        for from in paths {
            if dir.starts_with(from) {
                err = Some(ExplorerError::Invalid(format!("Cannot copy {} into itself", from.display())));
                continue;
            }
            let taken: Vec<String> = std::fs::read_dir(&dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string()).collect()
                })
                .unwrap_or_default();
            let taken_refs: Vec<&str> = taken.iter().map(|s| s.as_str()).collect();
            let base = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let name = tree::copy_name(&base, &taken_refs);
            let to = dir.join(&name);
            if let Err(e) = copy_path(from, &to) {
                err = Some(ExplorerError::Io(format!("Could not copy {}: {e}", from.display())));
                continue;
            }
            made.push((name, to));
        }
        // Mirror into the model (the target folder is loaded, the root always is).
        let mut ids = Vec::new();
        let mut new_paths = Vec::new();
        for (name, to) in made {
            let kind = if to.is_dir() { FsKind::Folder } else { FsKind::File };
            let mut node = if kind == FsKind::Folder { FsNode::folder(name) } else { FsNode::file(name) };
            node.loaded = kind != FsKind::Folder;
            ids.push(self.insert_subtree(target, node));
            new_paths.push(to);
        }
        if !ids.is_empty() {
            self.push_history(FileOp::Copy { ids: ids.clone(), paths: new_paths });
            self.after(&op);
        }
        match err {
            Some(e) if ids.is_empty() => self.fail(e),
            Some(e) => {
                self.last_error.set(Some(e.to_string()));
                Ok(ids)
            }
            None => Ok(ids),
        }
    }
}
