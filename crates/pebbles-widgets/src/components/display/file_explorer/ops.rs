//! File operations: open/refresh/watch, create, rename, move, copy,
//! trash/delete and the undo stack. Child module of `file_explorer`.

#[allow(clippy::wildcard_imports)]
use super::*;

impl FileExplorer {
    /// Load a REAL directory into the explorer (children only — folders read
    /// their own children lazily on first expand). Clears the selection and
    /// expansion; reports failures through [`last_error`](Self::last_error).
    pub fn open_folder(&self, path: impl AsRef<Path>) -> bool {
        match read_dir(path.as_ref(), self.load_filter.peek().as_deref()) {
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
        let keep = self.load_filter.peek().clone();
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
            match read_dir(&dir, self.load_filter.peek().as_deref()) {
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
    pub fn rename_node(&self, id: u64, name: impl Into<String>) -> bool {
        let name = name.into();
        let name = name.trim().trim_matches('/').to_string();
        if name.is_empty() {
            self.renaming.set(None);
            return false;
        }
        if let Err(e) = self.validate_name(id, &name) {
            self.last_error.set(Some(e));
            self.renaming.set(None);
            return false;
        }
        // "a/b/c.txt": create the intermediate folders, then the node lands in
        // the last one under the last segment's name (VS Code's nested create).
        let mut parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty()).collect();
        let leaf = parts.pop().unwrap_or_default().to_string();
        let mut parent = self.tree.peek().parent_of(id);
        for dir in parts {
            match self.ensure_child_folder(parent, dir) {
                Some(p) => parent = Some(p),
                None => {
                    self.renaming.set(None);
                    return false;
                }
            }
        }
        if parent != self.tree.peek().parent_of(id) {
            // Moving into the (new) folder must not hit an existing name there.
            if self.child_named(parent, &leaf).is_some() {
                self.last_error.set(Some(format!("'{leaf}' already exists")));
                self.renaming.set(None);
                return false;
            }
        }
        let old_path = self.path_of(id);
        let old_name = self.tree.peek().node(id).map(|n| n.name.clone()).unwrap_or_default();
        let old_parent = self.tree.peek().parent_of(id);
        let done = if let Some(root) = self.fs_root.peek().clone() {
            match old_path.clone() {
                Some(old) => {
                    let dir = match parent {
                        Some(p) => self.path_of(p),
                        None => Some(root.clone()),
                    };
                    let new = dir.map(|d| d.join(&leaf)).unwrap_or_else(|| old.with_file_name(&leaf));
                    // Never clobber: rename(2) silently replaces an existing file.
                    let case_only = new.parent() == old.parent()
                        && new.file_name().map(|n| n.to_string_lossy().to_lowercase())
                            == old.file_name().map(|n| n.to_string_lossy().to_lowercase());
                    if new.exists() && !case_only {
                        self.last_error.set(Some(format!("'{leaf}' already exists")));
                        false
                    } else {
                        match std::fs::rename(&old, &new) {
                            Ok(()) => true,
                            Err(e) => {
                                self.last_error.set(Some(format!("Could not rename: {e}")));
                                false
                            }
                        }
                    }
                }
                None => {
                    self.last_error.set(Some(format!("Could not resolve a path for a node in {root:?}")));
                    false
                }
            }
        } else {
            true
        };
        if done {
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
            self.push_history(FileOp::Rename { id, old_name, old_parent, paths: old_path.zip(new_path) });
        }
        self.renaming.set(None);
        done
    }

    /// Validate a rename / new name (VS Code's rules): not `.`/`..`, no NUL or
    /// backslash, no leading/trailing whitespace per segment, and no sibling
    /// already using it (a case-only change of the node's own name is fine).
    pub fn validate_name(&self, id: u64, name: &str) -> Result<(), String> {
        let name = name.trim_matches('/');
        if name.is_empty() {
            return Err("A file or folder name must be provided".into());
        }
        for seg in name.split('/').filter(|s| !s.is_empty()) {
            if seg == "." || seg == ".." {
                return Err(format!("'{seg}' is not a valid name"));
            }
            if seg.contains('\0') || seg.contains('\\') {
                return Err(format!("'{seg}' contains an invalid character"));
            }
            if seg.trim() != seg {
                return Err("Leading or trailing whitespace is not allowed".into());
            }
        }
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
    pub(super) fn ensure_child_folder(&self, parent: Option<u64>, name: &str) -> Option<u64> {
        if let Some(p) = parent {
            self.ensure_loaded(p);
        }
        if let Some(id) = self.child_named(parent, name) {
            let is_dir = self.tree.peek().node(id).is_some_and(|n| n.kind == FsKind::Folder);
            if !is_dir {
                self.last_error.set(Some(format!("'{name}' is a file")));
                return None;
            }
            return Some(id);
        }
        if let Some(root) = self.fs_root.peek().clone() {
            let dir = match parent {
                Some(p) => self.path_of(p)?,
                None => root,
            };
            if let Err(e) = std::fs::create_dir(dir.join(name)) {
                self.last_error.set(Some(format!("Could not create folder {name}: {e}")));
                return None;
            }
        }
        let cell = RefCell::new(None);
        self.tree.update(|t| *cell.borrow_mut() = Some(t.insert(parent, FsKind::Folder, name)));
        cell.take()
    }

    /// Delete nodes — on disk in filesystem mode, in the model otherwise.
    pub(super) fn delete_nodes(&self, ids: &[u64]) {
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
        }
        self.selected.set(Vec::new());
        self.active.set(None);
        self.renaming.set(None);
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

    /// A drop: validate, then (filesystem mode) refuse when disk moves are
    /// disabled, or ask for confirmation, then move. Returns whether a move
    /// was performed or is pending confirmation.
    pub fn request_move(&self, ids: &[u64], target: Option<u64>) -> bool {
        let plan = self.plan_move(ids, target);
        if plan.is_empty() {
            return false;
        }
        let fs = self.fs_root.peek().is_some();
        if fs && !self.fs_moves_allowed.peek() {
            self.last_error.set(Some("Moving files on disk is disabled".into()));
            return false;
        }
        if !fs || !self.confirm_moves.peek() {
            return self.move_nodes(&plan, target);
        }
        let (names, target_name) = {
            let t = self.tree.peek();
            let names = plan.iter().filter_map(|&id| t.node(id).map(|n| n.name.clone())).collect();
            let target_name = match target {
                Some(tg) => t.node(tg).map(|n| n.name.clone()).unwrap_or_default(),
                None => self
                    .fs_root
                    .peek()
                    .as_ref()
                    .and_then(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()))
                    .unwrap_or_else(|| "/".into()),
            };
            (names, target_name)
        };
        let req = MoveRequest { ids: plan, names, target, target_name };
        let this = *self;
        let ids = req.ids.clone();
        let resolve: Rc<dyn Fn(bool)> = Rc::new(move |ok| {
            if ok {
                this.move_nodes(&ids, target);
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
        true
    }

    /// Move nodes into a folder (or the root) — on disk in filesystem mode,
    /// in the model otherwise. No confirmation (programmatic); invalid moves
    /// (into itself/a descendant, no-ops, over an existing name) are skipped,
    /// and disk moves are refused when [`allow_fs_moves`](Self::allow_fs_moves)
    /// is off. Completed moves are recorded for [`undo`](Self::undo).
    /// Returns whether anything moved.
    pub fn move_nodes(&self, ids: &[u64], target: Option<u64>) -> bool {
        let plan = self.plan_move(ids, target);
        let fs_root = self.fs_root.peek().clone();
        if fs_root.is_some() && !self.fs_moves_allowed.peek() {
            if !plan.is_empty() {
                self.last_error.set(Some("Moving files on disk is disabled".into()));
            }
            return false;
        }
        // Load an unexpanded target first: a later lazy load would otherwise
        // replace its children and orphan the moved nodes' ids.
        if let Some(t) = target
            && !plan.is_empty()
        {
            self.ensure_loaded(t);
        }
        let mut done = Vec::new();
        for id in plan {
            let old_parent = self.tree.peek().parent_of(id);
            let mut paths = None;
            if let Some(root) = &fs_root {
                let Some(from) = self.path_of(id) else {
                    self.last_error.set(Some("Could not resolve a path for a node".into()));
                    continue;
                };
                let into = match target {
                    Some(t) => self.path_of(t),
                    None => Some(root.clone()),
                };
                let Some(into) = into else {
                    self.last_error.set(Some("Could not resolve the target folder".into()));
                    continue;
                };
                if &from == root || into.starts_with(&from) {
                    self.last_error.set(Some(format!("Refusing to move {} into itself", from.display())));
                    continue;
                }
                let to = into.join(from.file_name().unwrap_or_default());
                if to.exists() {
                    self.last_error.set(Some(format!("{} already exists", to.display())));
                    continue;
                }
                if let Err(e) = std::fs::rename(&from, &to) {
                    self.last_error.set(Some(format!("Could not move {}: {e}", from.display())));
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
        let any = !done.is_empty();
        if any {
            self.push_history(FileOp::Move(done));
        }
        any
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

    /// Move nodes to the trash (filesystem mode; undoable). In-memory trees
    /// just delete. Returns whether anything was removed.
    pub fn trash_nodes(&self, ids: &[u64]) -> bool {
        let ids = self.top_level(ids);
        if ids.is_empty() {
            return false;
        }
        if self.fs_root.peek().is_none() {
            self.delete_nodes(&ids);
            return true;
        }
        let mut done = Vec::new();
        let mut gone = Vec::new();
        for id in ids {
            let Some(path) = self.path_of(id) else { continue };
            if self.fs_root.peek().as_ref() == Some(&path) {
                continue;
            }
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
        let any = !done.is_empty();
        if any {
            self.push_history(FileOp::Trash(done));
        }
        any
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
    /// (Shift+Delete) or no trash available, confirm a permanent delete.
    /// In-memory trees delete immediately.
    pub fn request_delete(&self, permanent: bool) -> bool {
        let ids = self.top_level(&self.selected.peek());
        if ids.is_empty() {
            return false;
        }
        if self.fs_root.peek().is_none() {
            self.delete_nodes(&ids);
            return true;
        }
        let what = if ids.len() == 1 {
            let t = self.tree.peek();
            format!("'{}'", t.node(ids[0]).map(|n| n.name.clone()).unwrap_or_default())
        } else {
            format!("{} items", ids.len())
        };
        let this = *self;
        if !permanent && self.trash_dir().is_some() {
            crate::alert_dialog(format!("Are you sure you want to delete {what}?"))
                .description("You can restore from the Trash (or undo with Ctrl+Z).")
                .confirm("Move to Trash")
                .on_confirm(move || {
                    this.trash_nodes(&ids);
                })
                .open();
        } else {
            crate::alert_dialog(format!("Are you sure you want to permanently delete {what}?"))
                .description("This item will be deleted immediately. You can't undo this action.")
                .destructive(true)
                .confirm("Delete")
                .on_confirm(move || this.delete_nodes(&ids))
                .open();
        }
        true
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
                self.copy_nodes(&ids, target);
            }
        }
    }

    /// Copy nodes into `target` (`None` = root) — whole subtrees, on disk in
    /// filesystem mode, named like VS Code (`a copy.txt`). Undoable. Used by
    /// paste and Ctrl/Alt-drag. Returns the new node ids.
    pub fn copy_nodes(&self, ids: &[u64], target: Option<u64>) -> Vec<u64> {
        if let Some(t) = target {
            self.ensure_loaded(t);
        }
        let mut new_ids = Vec::new();
        let mut new_paths = Vec::new();
        for &id in ids {
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
                if dir.starts_with(&from) {
                    self.last_error.set(Some(format!("Cannot copy {} into itself", from.display())));
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
                    self.last_error.set(Some(format!("Could not copy {}: {e}", from.display())));
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
        }
        new_ids
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

    /// Create a node — on disk in filesystem mode (unique name probed against
    /// the real directory), then inserted into the model.
    pub(super) fn create_node(&self, parent: Option<u64>, kind: FsKind, base: &'static str) -> (u64, String) {
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
                    self.push_history(FileOp::Create { id, path: Some(target.clone()) });
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
            self.push_history(FileOp::Create { id, path: None });
            (id, base.to_string())
        }
    }

    pub(super) fn insert_at(&self, parent: Option<u64>, kind: FsKind, name: &'static str) -> u64 {
        let cell = RefCell::new(None);
        self.tree.update(|t| {
            *cell.borrow_mut() = Some(t.insert(parent, kind, name));
        });
        let id = cell.take().expect("inserted");
        self.reveal(id);
        id
    }
}
