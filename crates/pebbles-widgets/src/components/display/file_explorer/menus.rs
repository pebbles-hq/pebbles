//! The explorer's context menus — VS Code's file / folder / empty-space /
//! multi-selection menus as a customizable list of [`ExplorerMenuItem`]s:
//! remove or reorder built-ins, add your own with visible/enabled predicates,
//! and plug app-specific actions (Open to the Side, Open in Integrated
//! Terminal, Find in Folder, Compare, Reveal in File Manager, Open With…)
//! with [`FileExplorer::set_menu_handler`].

#[allow(clippy::wildcard_imports)]
use super::*;

/// Built-in menu item ids (use them to remove, reorder or plug handlers).
pub mod menu_ids {
    pub const OPEN: &str = "explorer.open";
    pub const OPEN_TO_SIDE: &str = "explorer.openToSide";
    pub const OPEN_WITH: &str = "explorer.openWith";
    pub const NEW_FILE: &str = "explorer.newFile";
    pub const NEW_FOLDER: &str = "explorer.newFolder";
    pub const REVEAL_IN_FILE_MANAGER: &str = "explorer.revealInFileManager";
    pub const OPEN_IN_TERMINAL: &str = "explorer.openInTerminal";
    pub const FIND_IN_FOLDER: &str = "explorer.findInFolder";
    pub const SELECT_FOR_COMPARE: &str = "explorer.selectForCompare";
    pub const COMPARE_WITH_SELECTED: &str = "explorer.compareWithSelected";
    pub const COMPARE_SELECTED: &str = "explorer.compareSelected";
    pub const CUT: &str = "explorer.cut";
    pub const COPY: &str = "explorer.copy";
    pub const PASTE: &str = "explorer.paste";
    pub const COPY_PATH: &str = "explorer.copyPath";
    pub const COPY_RELATIVE_PATH: &str = "explorer.copyRelativePath";
    pub const RENAME: &str = "explorer.rename";
    pub const DELETE: &str = "explorer.delete";
    pub const COLLAPSE: &str = "explorer.collapse";
    pub const REFRESH: &str = "explorer.refresh";
    pub const COLLAPSE_ALL: &str = "explorer.collapseAll";
}

/// Where a menu opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuContext {
    /// On a file row (single selection).
    File,
    /// On a folder row (single selection).
    Folder,
    /// On a row while several items are selected.
    Multi,
    /// On empty space below the rows.
    Empty,
}

/// What a menu item acts on: the context and the entries (the selection when
/// the clicked row is part of it, else just the clicked row; empty for
/// [`MenuContext::Empty`]).
#[derive(Clone, Debug)]
pub struct MenuTarget {
    pub context: MenuContext,
    pub entries: Vec<ExplorerEntry>,
}

impl MenuTarget {
    /// The single entry (file/folder contexts).
    pub fn entry(&self) -> Option<&ExplorerEntry> {
        if self.entries.len() == 1 { self.entries.first() } else { None }
    }
    /// Whether every entry is a file.
    pub fn all_files(&self) -> bool {
        !self.entries.is_empty() && self.entries.iter().all(|e| !e.is_folder())
    }
}

type Pred = Rc<dyn Fn(&MenuTarget) -> bool>;
/// A menu action.
pub type MenuAction = Rc<dyn Fn(&MenuTarget)>;

/// One context-menu row (or a separator). Build with [`new`](Self::new).
#[derive(Clone)]
pub struct ExplorerMenuItem {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
    pub destructive: bool,
    separator: bool,
    visible: Pred,
    enabled: Pred,
    action: Option<MenuAction>,
}

impl std::fmt::Debug for ExplorerMenuItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.separator { f.write_str("---") } else { write!(f, "{} ({})", self.label, self.id) }
    }
}

impl ExplorerMenuItem {
    /// An item running `action` (visible and enabled everywhere until narrowed).
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        action: impl Fn(&MenuTarget) + 'static,
    ) -> Self {
        ExplorerMenuItem {
            id: id.into(),
            label: label.into(),
            shortcut: None,
            destructive: false,
            separator: false,
            visible: Rc::new(|_| true),
            enabled: Rc::new(|_| true),
            action: Some(Rc::new(action)),
        }
    }
    /// A divider (adjacent / leading / trailing dividers collapse).
    pub fn separator() -> Self {
        ExplorerMenuItem {
            id: String::new(),
            label: String::new(),
            shortcut: None,
            destructive: false,
            separator: true,
            visible: Rc::new(|_| true),
            enabled: Rc::new(|_| true),
            action: None,
        }
    }
    /// Show only when `f` holds.
    #[must_use]
    pub fn visible_when(mut self, f: impl Fn(&MenuTarget) -> bool + 'static) -> Self {
        self.visible = Rc::new(f);
        self
    }
    /// Show only in these contexts.
    #[must_use]
    pub fn in_contexts(self, contexts: &'static [MenuContext]) -> Self {
        self.visible_when(move |t| contexts.contains(&t.context))
    }
    /// Grey out unless `f` holds.
    #[must_use]
    pub fn enabled_when(mut self, f: impl Fn(&MenuTarget) -> bool + 'static) -> Self {
        self.enabled = Rc::new(f);
        self
    }
    /// A trailing shortcut hint.
    #[must_use]
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    /// Danger colour.
    #[must_use]
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }
    pub fn is_separator(&self) -> bool {
        self.separator
    }
    /// Whether it shows for `t`.
    pub fn is_visible(&self, t: &MenuTarget) -> bool {
        (self.visible)(t)
    }
    /// Whether it is enabled for `t`.
    pub fn is_enabled(&self, t: &MenuTarget) -> bool {
        (self.enabled)(t)
    }
    /// Run the action for `t`.
    pub fn run(&self, t: &MenuTarget) {
        if let Some(a) = &self.action {
            a(t);
        }
    }
}

/// The installed open hook.
pub(super) type OpenHook = Rc<dyn Fn(&ExplorerEntry, OpenMode)>;

/// How an item was asked to open (see [`FileExplorer::on_open`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenMode {
    /// Enter, double-click, menu Open.
    Open,
    /// Space / single click: a preview (VS Code's preview editor).
    Preview,
}

impl FileExplorer {
    /// The built-in VS Code menu set (in VS Code's order).
    pub(super) fn default_menu(self) -> Vec<ExplorerMenuItem> {
        use MenuContext::*;
        use menu_ids as m;
        let ex = self;
        let has = move |id: &'static str| move |_: &MenuTarget| ex.menu_handlers.peek().contains_key(id);
        let cap = move |c: Capability| move |_: &MenuTarget| ex.caps.peek().allows(c);
        let handler = move |id: &'static str| {
            move |t: &MenuTarget| {
                let h = ex.menu_handlers.peek().get(id).cloned();
                if let Some(h) = h {
                    h(t);
                }
            }
        };
        let sep = ExplorerMenuItem::separator;
        vec![
            ExplorerMenuItem::new(m::OPEN, "Open", move |t| {
                for e in &t.entries {
                    ex.open_entry(e.id, OpenMode::Open);
                }
            })
            .in_contexts(&[File]),
            ExplorerMenuItem::new(m::OPEN_TO_SIDE, "Open to the Side", handler(m::OPEN_TO_SIDE))
                .shortcut("Ctrl+Enter")
                .visible_when(move |t| {
                    (t.context == File || (t.context == Multi && t.all_files())) && has(m::OPEN_TO_SIDE)(t)
                }),
            ExplorerMenuItem::new(m::OPEN_WITH, "Open With…", handler(m::OPEN_WITH))
                .visible_when(move |t| t.context == File && has(m::OPEN_WITH)(t)),
            sep(),
            ExplorerMenuItem::new(m::NEW_FILE, "New File…", move |t| {
                ex.new_in(t, FsKind::File);
            })
            .in_contexts(&[Folder, Empty])
            .enabled_when(cap(Capability::CreateFile)),
            ExplorerMenuItem::new(m::NEW_FOLDER, "New Folder…", move |t| {
                ex.new_in(t, FsKind::Folder);
            })
            .in_contexts(&[Folder, Empty])
            .enabled_when(cap(Capability::CreateFolder)),
            ExplorerMenuItem::new(
                m::REVEAL_IN_FILE_MANAGER,
                "Reveal in File Manager",
                handler(m::REVEAL_IN_FILE_MANAGER),
            )
            .shortcut("Shift+Alt+R")
            .visible_when(move |t| matches!(t.context, File | Folder) && has(m::REVEAL_IN_FILE_MANAGER)(t)),
            ExplorerMenuItem::new(
                m::OPEN_IN_TERMINAL,
                "Open in Integrated Terminal",
                handler(m::OPEN_IN_TERMINAL),
            )
            .visible_when(move |t| matches!(t.context, File | Folder) && has(m::OPEN_IN_TERMINAL)(t)),
            sep(),
            ExplorerMenuItem::new(m::FIND_IN_FOLDER, "Find in Folder…", handler(m::FIND_IN_FOLDER))
                .shortcut("Shift+Alt+F")
                .visible_when(move |t| t.context == Folder && has(m::FIND_IN_FOLDER)(t)),
            sep(),
            ExplorerMenuItem::new(m::SELECT_FOR_COMPARE, "Select for Compare", move |t| {
                ex.compare_source.set(t.entry().map(|e| e.id));
            })
            .visible_when(move |t| t.context == File && has(m::COMPARE_WITH_SELECTED)(t)),
            ExplorerMenuItem::new(m::COMPARE_WITH_SELECTED, "Compare with Selected", move |t| {
                let src = ex.compare_source.peek().and_then(|id| ex.entry(id));
                let h = ex.menu_handlers.peek().get(m::COMPARE_WITH_SELECTED).cloned();
                if let (Some(src), Some(e), Some(h)) = (src, t.entry(), h) {
                    h(&MenuTarget { context: Multi, entries: vec![src, e.clone()] });
                }
            })
            .visible_when(move |t| {
                t.context == File
                    && has(m::COMPARE_WITH_SELECTED)(t)
                    && ex.compare_source.peek().is_some_and(|s| t.entry().is_some_and(|e| e.id != s))
            }),
            ExplorerMenuItem::new(m::COMPARE_SELECTED, "Compare Selected", handler(m::COMPARE_WITH_SELECTED))
                .visible_when(move |t| {
                    t.context == Multi
                        && t.entries.len() == 2
                        && t.all_files()
                        && has(m::COMPARE_WITH_SELECTED)(t)
                }),
            sep(),
            ExplorerMenuItem::new(m::CUT, "Cut", move |_| {
                let _ = ex.cut_selection();
            })
            .shortcut("Ctrl+X")
            .in_contexts(&[File, Folder, Multi])
            .enabled_when(move |t| cap(Capability::CopyPaste)(t) && cap(Capability::Move)(t)),
            ExplorerMenuItem::new(m::COPY, "Copy", move |_| {
                let _ = ex.copy_selection();
            })
            .shortcut("Ctrl+C")
            .in_contexts(&[File, Folder, Multi])
            .enabled_when(cap(Capability::CopyPaste)),
            ExplorerMenuItem::new(m::PASTE, "Paste", move |_| {
                let _ = ex.paste_clipboard();
            })
            .shortcut("Ctrl+V")
            .in_contexts(&[File, Folder, Empty])
            .enabled_when(move |t| cap(Capability::CopyPaste)(t) && ex.clipboard.peek().is_some()),
            sep(),
            ExplorerMenuItem::new(m::COPY_PATH, "Copy Path", move |t| {
                ex.copy_paths(t, false);
            })
            .shortcut("Shift+Alt+C")
            .visible_when(move |t| t.context != Empty && ex.fs_root.peek().is_some()),
            ExplorerMenuItem::new(m::COPY_RELATIVE_PATH, "Copy Relative Path", move |t| {
                ex.copy_paths(t, true);
            })
            .shortcut("Ctrl+K Ctrl+Shift+Alt+C")
            .visible_when(move |t| t.context != Empty && ex.fs_root.peek().is_some()),
            sep(),
            ExplorerMenuItem::new(m::RENAME, "Rename…", move |t| {
                if let Some(e) = t.entry() {
                    ex.start_rename_for(e.id);
                }
            })
            .shortcut("F2")
            .in_contexts(&[File, Folder])
            .enabled_when(move |t| cap(Capability::Rename)(t) && ex.item_allowed(caps::ItemPred::Rename, t)),
            ExplorerMenuItem::new(m::DELETE, "Delete", move |_| {
                let _ = ex.request_delete(false);
            })
            .shortcut("Delete")
            .destructive()
            .in_contexts(&[File, Folder, Multi])
            .enabled_when(move |t| {
                (cap(Capability::Trash)(t) || cap(Capability::Delete)(t))
                    && ex.item_allowed(caps::ItemPred::Delete, t)
            }),
            ExplorerMenuItem::new(m::COLLAPSE, "Collapse", move |t| {
                if let Some(e) = t.entry() {
                    ex.collapse_subtree(e.id);
                }
            })
            .visible_when(move |t| {
                t.context == Folder && t.entry().is_some_and(|e| ex.expanded.peek().contains(&e.id))
            }),
            ExplorerMenuItem::new(m::REFRESH, "Refresh", move |_| {
                ex.refresh();
            })
            .in_contexts(&[Empty])
            .visible_when(move |t| t.context == Empty && ex.fs_root.peek().is_some()),
            ExplorerMenuItem::new(m::COLLAPSE_ALL, "Collapse All", move |_| ex.expanded.set(HashSet::new()))
                .in_contexts(&[Empty]),
        ]
    }

    /// The current context-menu items (built-ins + yours, in order).
    pub fn menu_items(&self) -> Vec<ExplorerMenuItem> {
        self.menu.peek().clone()
    }

    /// Replace the whole menu list.
    pub fn set_menu_items(&self, items: Vec<ExplorerMenuItem>) {
        self.menu.set(items);
    }

    /// Remove an item by id (a built-in from [`menu_ids`] or yours).
    pub fn remove_menu_item(&self, id: &str) {
        self.menu.update(|m| m.retain(|i| i.separator || i.id != id));
    }

    /// Append an item.
    pub fn add_menu_item(&self, item: ExplorerMenuItem) {
        self.menu.update(|m| m.push(item));
    }

    /// Insert an item right after `after_id` (appended if it isn't there).
    pub fn insert_menu_item_after(&self, after_id: &str, item: ExplorerMenuItem) {
        self.menu.update(|m| {
            let at =
                m.iter().position(|i| !i.separator && i.id == after_id).map(|i| i + 1).unwrap_or(m.len());
            m.insert(at, item);
        });
    }

    /// Insert an item right before `before_id` (appended if it isn't there).
    pub fn insert_menu_item_before(&self, before_id: &str, item: ExplorerMenuItem) {
        self.menu.update(|m| {
            let at = m.iter().position(|i| !i.separator && i.id == before_id).unwrap_or(m.len());
            m.insert(at, item);
        });
    }

    /// Move an item to position `index` (clamped).
    pub fn move_menu_item(&self, id: &str, index: usize) {
        self.menu.update(|m| {
            if let Some(from) = m.iter().position(|i| !i.separator && i.id == id) {
                let it = m.remove(from);
                let to = index.min(m.len());
                m.insert(to, it);
            }
        });
    }

    /// Plug an app-specific action into a built-in item ([`menu_ids`]):
    /// `OPEN_TO_SIDE`, `OPEN_WITH`, `REVEAL_IN_FILE_MANAGER`,
    /// `OPEN_IN_TERMINAL`, `FIND_IN_FOLDER`, `COMPARE_WITH_SELECTED` (gets
    /// the two files to compare). Those items show only once a handler exists.
    pub fn set_menu_handler(&self, id: &str, f: impl Fn(&MenuTarget) + 'static) {
        let id = id.to_string();
        self.menu_handlers.update(|h| {
            h.insert(id, Rc::new(f));
        });
    }

    /// Called to open a file (Enter, double-click, Space = preview, menu Open).
    /// Without it, opening just activates the row ([`active_row`](Self::active_row)).
    pub fn on_open(&self, f: impl Fn(&ExplorerEntry, OpenMode) + 'static) {
        self.open_hook.set(Some(Rc::new(f)));
    }

    /// Open `id` (see [`on_open`](Self::on_open)). Folders toggle.
    pub fn open_entry(&self, id: u64, mode: OpenMode) {
        let Some(e) = self.entry(id) else { return };
        if e.is_folder() {
            self.toggle_folder(id);
            return;
        }
        self.select_only(id);
        let hook = self.open_hook.peek().clone();
        if let Some(h) = hook {
            h(&e, mode);
        }
    }

    /// The menu target for a right-click on `id` (`None` = empty space): the
    /// selection when `id` is part of it, else just `id`.
    pub fn menu_target(&self, id: Option<u64>) -> MenuTarget {
        let Some(id) = id else { return MenuTarget { context: MenuContext::Empty, entries: Vec::new() } };
        let sel = self.selected.get();
        let ids = if sel.contains(&id) && sel.len() > 1 { self.top_level(&sel) } else { vec![id] };
        let entries = self.entries(&ids);
        let context = if entries.len() > 1 {
            MenuContext::Multi
        } else if entries.first().is_some_and(|e| e.is_folder()) {
            MenuContext::Folder
        } else {
            MenuContext::File
        };
        MenuTarget { context, entries }
    }

    /// The visible rows of the menu for `t` (separators collapsed).
    pub fn menu_for(&self, t: &MenuTarget) -> Vec<ExplorerMenuItem> {
        let mut out: Vec<ExplorerMenuItem> = Vec::new();
        for it in self.menu.get().iter() {
            if it.separator {
                if out.last().is_some_and(|l| !l.separator) {
                    out.push(it.clone());
                }
            } else if it.is_visible(t) {
                out.push(it.clone());
            }
        }
        while out.last().is_some_and(|l| l.separator) {
            out.pop();
        }
        out
    }

    /// Wrap `child` in the context menu for `t` (or return it bare when the
    /// [`Capability::ContextMenu`] is off).
    pub(super) fn with_menu(
        self,
        child: AnyWidget,
        t: MenuTarget,
        on_open: Option<Rc<dyn Fn()>>,
    ) -> AnyWidget {
        if !self.caps.get().allows(Capability::ContextMenu) {
            return child;
        }
        let mut cm = context_menu(child);
        if let Some(f) = on_open {
            cm = cm.on_open(move |_| f());
        }
        let t = Rc::new(t);
        for it in self.menu_for(&t) {
            if it.separator {
                cm = cm.separator();
                continue;
            }
            let mut mi = menu_item(it.label.clone()).disabled(!it.is_enabled(&t));
            if let Some(s) = &it.shortcut {
                mi = mi.shortcut(s.clone());
            }
            if it.destructive {
                mi = mi.destructive();
            }
            let t2 = t.clone();
            mi = mi.on_select(move || it.run(&t2));
            cm = cm.item(mi);
        }
        cm.into_widget()
    }

    /// Whether a per-item predicate allows every entry of `t` (greys items).
    pub(super) fn item_allowed(&self, which: caps::ItemPred, t: &MenuTarget) -> bool {
        let c = self.controls.peek();
        let p = match which {
            caps::ItemPred::Rename => c.can_rename.clone(),
            caps::ItemPred::Delete => c.can_delete.clone(),
            caps::ItemPred::Move => c.can_move.clone(),
        };
        drop(c);
        p.is_none_or(|p| t.entries.iter().all(|e| p(e)))
    }

    /// Start an inline create inside the menu's folder (or the root).
    fn new_in(&self, t: &MenuTarget, kind: FsKind) {
        match t.entry() {
            Some(e) if e.is_folder() => self.select_only(e.id),
            _ if t.context == MenuContext::Empty => self.selected.set(Vec::new()),
            _ => {}
        }
        self.begin_create(kind);
    }

    /// Copy Path / Copy Relative Path (one per line, like VS Code).
    pub fn copy_paths(&self, t: &MenuTarget, relative: bool) {
        let root = self.fs_root.peek().clone();
        let lines: Vec<String> = t
            .entries
            .iter()
            .filter_map(|e| e.path.clone())
            .map(|p| match (&root, relative) {
                (Some(r), true) => p.strip_prefix(r).map(|x| x.to_path_buf()).unwrap_or(p),
                _ => p,
            })
            .map(|p| p.display().to_string())
            .collect();
        if !lines.is_empty() {
            pebbles_core::clipboard::write(&lines.join("\n"));
        }
    }

    /// Collapse `id` and every folder under it.
    pub fn collapse_subtree(&self, id: u64) {
        let t = self.tree.peek();
        let mut ids = vec![id];
        if let Some(n) = t.node(id) {
            fn walk(n: &FsNode, out: &mut Vec<u64>) {
                for c in &n.children {
                    out.push(c.id);
                    walk(c, out);
                }
            }
            walk(n, &mut ids);
        }
        drop(t);
        self.expanded.update(|e| {
            for i in &ids {
                e.remove(i);
            }
        });
    }
}
