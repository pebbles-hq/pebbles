//! Expandable files: rows an app shows *inside* a file (a low-code flow's parameters, a
//! module's symbols…). They are not part of the [`FileTree`]: never on disk, never
//! selected, renamed, moved, deleted or dragged by the explorer's own operations — the
//! app supplies them ([`FileExplorer::set_file_rows`]) and handles their clicks and
//! context menus.

#[allow(clippy::wildcard_imports)]
use super::*;
use std::collections::HashMap;

/// One row inside an expandable file. A row with `children` is a group (it has its own
/// twistie; groups start expanded).
#[derive(Clone, Debug, PartialEq)]
pub struct FileRow {
    /// The app's identifier for the row (handed back to the hooks).
    pub tag: String,
    pub label: String,
    /// Muted text after the label (a type, a count).
    pub detail: Option<String>,
    /// The row's icon (an app glyph, or an icon theme's image).
    pub icon: Option<ExplorerIcon>,
    pub children: Vec<FileRow>,
}

impl FileRow {
    pub fn new(tag: impl Into<String>, label: impl Into<String>) -> Self {
        FileRow { tag: tag.into(), label: label.into(), detail: None, icon: None, children: Vec::new() }
    }
    pub fn detail(mut self, d: impl Into<String>) -> Self {
        self.detail = Some(d.into());
        self
    }
    pub fn icon(mut self, i: impl Into<ExplorerIcon>) -> Self {
        self.icon = Some(i.into());
        self
    }
    pub fn children(mut self, c: Vec<FileRow>) -> Self {
        self.children = c;
        self
    }
}

pub(super) type ExpandableFn = Rc<dyn Fn(&Path) -> bool>;
pub(super) type FileExpandFn = Rc<dyn Fn(&Path)>;
pub(super) type RowOpenFn = Rc<dyn Fn(&Path, &str)>;
pub(super) type RowMenuFn = Rc<dyn Fn(&Path, &str) -> Vec<crate::components::MenuEntry>>;

/// The expandable-files state (one per explorer).
#[derive(Clone, Default)]
pub(in crate::components::display) struct FileRows {
    pub(super) expandable: Option<ExpandableFn>,
    pub(super) on_expand: Option<FileExpandFn>,
    pub(super) on_open: Option<RowOpenFn>,
    pub(super) menu: Option<RowMenuFn>,
}

impl FileExplorer {
    /// Make files expandable: `pred` says which (keep it cheap — it runs while rendering,
    /// e.g. an extension check). Expanding one calls `on_expand(path)`; the app then
    /// supplies its rows with [`set_file_rows`](Self::set_file_rows) (read them off the UI
    /// thread — rendering never touches the disk).
    pub fn set_expandable_files(
        &self,
        pred: impl Fn(&Path) -> bool + 'static,
        on_expand: impl Fn(&Path) + 'static,
    ) {
        self.file_rows_cfg.update(|c| {
            c.expandable = Some(Rc::new(pred));
            c.on_expand = Some(Rc::new(on_expand));
        });
    }

    /// The rows shown inside the expandable file `path` (`None` = not known yet: the
    /// expanded file shows "Loading…").
    pub fn set_file_rows(&self, path: &Path, rows: Option<Vec<FileRow>>) {
        let same = self.file_rows.with(|m| m.get(path) == rows.as_ref());
        if same {
            return;
        }
        self.file_rows.update(|m| match rows {
            Some(r) => {
                m.insert(path.to_path_buf(), r);
            }
            None => {
                m.remove(path);
            }
        });
    }

    /// Called when a row inside a file is clicked: `(file, tag)`.
    pub fn on_file_row_open(&self, f: impl Fn(&Path, &str) + 'static) {
        self.file_rows_cfg.update(|c| c.on_open = Some(Rc::new(f)));
    }

    /// The context menu of a row inside a file: `(file, tag)` → entries.
    pub fn set_file_row_menu(&self, f: impl Fn(&Path, &str) -> Vec<crate::components::MenuEntry> + 'static) {
        self.file_rows_cfg.update(|c| c.menu = Some(Rc::new(f)));
    }

    /// The selected row inside a file (`(file, tag)`), if any (reactive). A click on a
    /// row selects it (and clears the tree's selection); selecting a file row clears it.
    pub fn selected_file_row(&self) -> Option<(PathBuf, String)> {
        self.file_row_selected.get()
    }

    /// Select a row inside a file (`None` = none).
    pub fn select_file_row(&self, sel: Option<(PathBuf, String)>) {
        if self.file_row_selected.peek() != sel {
            self.file_row_selected.set(sel);
        }
    }

    /// The expandable files currently expanded (their paths), e.g. to refresh their rows.
    pub fn expanded_files(&self) -> Vec<PathBuf> {
        let ids: Vec<u64> = self.expanded.peek().iter().copied().collect();
        let t = self.tree.peek();
        let kinds: Vec<(u64, FsKind)> =
            ids.into_iter().filter_map(|i| t.node(i).map(|n| (i, n.kind))).collect();
        drop(t);
        // A folder item's rows are its main file's.
        kinds
            .into_iter()
            .filter_map(|(i, k)| match k {
                FsKind::File => self.path_of(i),
                FsKind::Folder => self.folder_item(i).map(|it| it.open),
            })
            .filter(|p| self.is_expandable_file(p))
            .collect()
    }

    /// Whether `path` (a file) is expandable.
    pub fn is_expandable_file(&self, path: &Path) -> bool {
        let pred = self.file_rows_cfg.with(|c| c.expandable.clone());
        pred.is_some_and(|f| f(path))
    }

    /// Expand / collapse an expandable file.
    pub fn toggle_file(&self, id: u64) {
        let open = self.expanded.peek().contains(&id);
        self.expanded.update(|e| {
            if open {
                e.remove(&id);
            } else {
                e.insert(id);
            }
        });
        if !open && let Some(p) = self.path_of(id) {
            let hook = self.file_rows_cfg.with(|c| c.on_expand.clone());
            if let Some(h) = hook {
                h(&p);
            }
        }
    }
}

/// The rows inside the expanded file `path`, at `depth`.
pub(super) fn file_rows_widget(explorer: &FileExplorer, path: PathBuf, depth: usize) -> AnyWidget {
    let rows = explorer.file_rows.with(|m| m.get(&path).cloned());
    let mut out: Vec<AnyWidget> = Vec::new();
    match rows {
        None => out.push(
            component_props(
                render_file_row,
                RowP {
                    explorer: *explorer,
                    path,
                    row: FileRow::new("", "Loading…"),
                    depth,
                    key: String::new(),
                    muted: true,
                },
            )
            .into_widget(),
        ),
        Some(rows) => push_rows(explorer, &path, &rows, depth, "", &mut out),
    }
    column(out)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn push_rows(
    explorer: &FileExplorer,
    path: &Path,
    rows: &[FileRow],
    depth: usize,
    parent: &str,
    out: &mut Vec<AnyWidget>,
) {
    for r in rows {
        let key = format!("{parent}/{}", r.tag);
        let collapsed = explorer.file_rows_collapsed.with(|c| c.contains(&(path.to_path_buf(), key.clone())));
        out.push(
            crate::widgets::keyed(
                key.clone(),
                component_props(
                    render_file_row,
                    RowP {
                        explorer: *explorer,
                        path: path.to_path_buf(),
                        row: r.clone(),
                        depth,
                        key: key.clone(),
                        muted: false,
                    },
                ),
            )
            .into_widget(),
        );
        if !r.children.is_empty() && !collapsed {
            push_rows(explorer, path, &r.children, depth + 1, &key, out);
        }
    }
}

struct RowP {
    explorer: FileExplorer,
    path: PathBuf,
    row: FileRow,
    depth: usize,
    /// The row's identity within the file (ancestor tags joined), for its fold state.
    key: String,
    muted: bool,
}

fn render_file_row(p: &RowP) -> AnyWidget {
    let st = p.explorer.style();
    let explorer = p.explorer;
    let group = !p.row.children.is_empty();
    let (path, key) = (p.path.clone(), p.key.clone());
    let collapsed = {
        let k = (path.clone(), key.clone());
        explorer.file_rows_collapsed.with(|c| c.contains(&k))
    };
    let hovered = create_signal(false);
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let me = (path.clone(), p.row.tag.clone());
    let selected = !p.muted
        && pebbles_core::create_memo(move || explorer.file_row_selected.get().as_ref() == Some(&me)).get();
    let base = if selected { st.selection_active_bg } else { st.row_bg };
    let bg = if selected && st.hover_strength >= 1.0 {
        base
    } else {
        mix(base, st.hover_bg, st.hover_strength * hv as f32)
    };
    let fg = if selected { st.selection_active_fg } else { st.text };
    let twistie: AnyWidget = if group {
        icon(if collapsed { IconKind::ChevronRight } else { IconKind::ChevronDown })
            .size(st.twistie_size)
            .color(st.icon_tint)
            .into_widget()
    } else {
        gap_w(st.twistie_size).into_widget()
    };
    let glyph: AnyWidget = match p.row.icon.clone() {
        Some(i) => i.build(st.icon_size, if selected { fg } else { st.icon_tint }),
        None => gap_w(st.icon_size).into_widget(),
    };
    let mut label =
        text(p.row.label.clone()).size(st.font_size as f32).color(if p.muted { st.muted_text } else { fg });
    if let Some(f) = &st.font_family {
        label = label.font_family(f.clone());
    }
    let mut parts: Vec<AnyWidget> =
        vec![twistie, gap_w(4.0).into_widget(), glyph, gap_w(6.0).into_widget(), label.into_widget()];
    if let Some(d) = &p.row.detail {
        parts.push(gap_w(6.0).into_widget());
        parts.push(text(d.clone()).size((st.font_size - 1.0) as f32).color(st.muted_text).into_widget());
    }
    let content = row(parts).main_axis_size(MainAxisSize::Min);
    let mut pad = st.row_padding;
    pad.right += scrollbar_gutter();
    let mut inner = Container::new().padding(pad).child(content);
    if let Some(h) = st.row_height {
        inner = inner.height(h).alignment(pebbles_foundation::Alignment::CENTER_LEFT);
    }
    let body = Container::new()
        .color(bg)
        .child(row(children![gap_w(p.depth as f64 * st.indent), Expanded::new(inner)]));
    if p.muted {
        return body.into_widget();
    }
    let tag = p.row.tag.clone();
    let (path_t, key_t, tag_t) = (path.clone(), key.clone(), tag.clone());
    let g = GestureDetector::new(body)
        .cursor(crate::design::theme::action_cursor())
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_tap(move || {
            if group {
                let k = (path_t.clone(), key_t.clone());
                explorer.file_rows_collapsed.update(|c| {
                    if !c.remove(&k) {
                        c.insert(k);
                    }
                });
            } else {
                explorer.selected.set(Vec::new());
                explorer.select_file_row(Some((path_t.clone(), tag_t.clone())));
                let hook = explorer.file_rows_cfg.with(|c| c.on_open.clone());
                if let Some(h) = hook {
                    h(&path_t, &tag_t);
                }
            }
        });
    let menu = explorer.file_rows_cfg.with(|c| c.menu.clone());
    let w = crate::widgets::semantics(crate::widgets::SemanticsRole::Button, p.row.label.clone(), g);
    match menu {
        Some(_) if explorer.caps.get().allows(Capability::ContextMenu) => context_menu(w)
            .entries_with(move |_| {
                let m = explorer.file_rows_cfg.with(|c| c.menu.clone());
                m.map(|f| f(&path, &tag)).unwrap_or_default()
            })
            .into_widget(),
        _ => w.into_widget(),
    }
}

pub(super) type RowsMap = HashMap<PathBuf, Vec<FileRow>>;
