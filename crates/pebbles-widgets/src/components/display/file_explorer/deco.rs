//! Row decorations (git status, problems, dirty, custom badges — the app
//! supplies them through a hook) and VS Code's compact folders.

#[allow(clippy::wildcard_imports)]
use super::*;

/// A file's git status (VS Code's gitDecoration kinds).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
    Ignored,
    Conflict,
    Renamed,
}

impl GitStatus {
    /// VS Code's badge letter.
    pub fn letter(self) -> &'static str {
        match self {
            GitStatus::Modified => "M",
            GitStatus::Added => "A",
            GitStatus::Deleted => "D",
            GitStatus::Untracked => "U",
            GitStatus::Ignored => "I",
            GitStatus::Conflict => "!",
            GitStatus::Renamed => "R",
        }
    }

    /// Its colour from the style tokens.
    pub fn color(self, st: &ExplorerStyle) -> Color {
        match self {
            GitStatus::Modified => st.git_modified,
            GitStatus::Added => st.git_added,
            GitStatus::Deleted => st.git_deleted,
            GitStatus::Untracked => st.git_untracked,
            GitStatus::Ignored => st.git_ignored,
            GitStatus::Conflict => st.git_conflict,
            GitStatus::Renamed => st.git_renamed,
        }
    }

    fn weight(self) -> u8 {
        match self {
            GitStatus::Conflict => 6,
            GitStatus::Deleted => 5,
            GitStatus::Modified => 4,
            GitStatus::Added | GitStatus::Renamed => 3,
            GitStatus::Untracked => 2,
            GitStatus::Ignored => 1,
        }
    }
}

/// What the app says about one row (all optional). Problems win the label
/// colour over git (as in VS Code), errors over warnings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decoration {
    pub git: Option<GitStatus>,
    pub errors: usize,
    pub warnings: usize,
    /// Unsaved changes in an open editor (a ● badge).
    pub dirty: bool,
    /// A custom badge (overrides the computed one).
    pub badge: Option<String>,
    /// A custom label colour (overrides the computed one).
    pub color: Option<Color>,
    /// Render dimmed (e.g. ignored files).
    pub faded: bool,
}

impl Decoration {
    /// The label colour (custom > errors > warnings > git).
    pub fn label_color(&self, st: &ExplorerStyle) -> Option<Color> {
        self.color.or(if self.errors > 0 {
            Some(st.error)
        } else if self.warnings > 0 {
            Some(st.warning)
        } else {
            self.git.map(|g| g.color(st))
        })
    }

    /// The trailing badge text (custom > problem count > ● dirty > git letter).
    pub fn badge_text(&self) -> Option<String> {
        if let Some(b) = &self.badge {
            return Some(b.clone());
        }
        let n = self.errors + self.warnings;
        if n > 0 {
            return Some(if n > 9 { "9+".to_string() } else { n.to_string() });
        }
        if self.dirty {
            return Some("●".to_string());
        }
        self.git.map(|g| g.letter().to_string())
    }

    /// The decoration a folder inherits from its (loaded) descendants: the
    /// strongest git status and summed problems, shown as a dot (VS Code
    /// propagates colours to parents).
    fn merge(&mut self, child: &Decoration) {
        if child.git.is_some_and(|g| g != GitStatus::Ignored)
            && self.git.is_none_or(|s| child.git.is_some_and(|c| c.weight() > s.weight()))
        {
            self.git = child.git;
        }
        self.errors += child.errors;
        self.warnings += child.warnings;
    }
}

/// The installed decoration hook.
pub(super) type DecoFn = Rc<dyn Fn(&ExplorerEntry) -> Option<Decoration>>;

impl FileExplorer {
    /// Supply row decorations (git status, problems, dirty, custom badges).
    /// Called per row on render; read your own signals inside and rows update
    /// when they change. Folders without their own decoration inherit the
    /// strongest status of their loaded descendants (shown as a `•`).
    pub fn set_decorations(&self, f: impl Fn(&ExplorerEntry) -> Option<Decoration> + 'static) {
        self.deco_fn.set(Some(Rc::new(f)));
    }

    /// Remove the decoration hook.
    pub fn clear_decorations(&self) {
        self.deco_fn.set(None);
    }

    /// The decoration for `id` (its own, or a folder's propagated one).
    pub fn decoration(&self, id: u64) -> Option<Decoration> {
        let f = self.deco_fn.get()?;
        let e = self.entry(id)?;
        if let Some(d) = f(&e) {
            return Some(d);
        }
        if !e.is_folder() {
            return None;
        }
        let node = self.tree.peek().node(id).cloned()?;
        let mut acc = Decoration::default();
        let mut any = false;
        fn walk(ex: &FileExplorer, f: &DecoFn, n: &FsNode, acc: &mut Decoration, any: &mut bool) {
            for c in &n.children {
                if let Some(e) = ex.entry(c.id)
                    && let Some(d) = f(&e)
                {
                    acc.merge(&d);
                    *any = true;
                }
                walk(ex, f, c, acc, any);
            }
        }
        walk(self, &f, &node, &mut acc, &mut any);
        if !any || (acc.git.is_none() && acc.errors == 0 && acc.warnings == 0) {
            return None;
        }
        acc.badge = Some("•".to_string());
        Some(acc)
    }

    /// Show single-child folder chains as one row (`a/b/c`) — VS Code's
    /// `explorer.compactFolders` (default off here).
    #[must_use]
    pub fn compact_folders(self, on: bool) -> Self {
        self.compact.set(on);
        self
    }

    /// The compacted chain starting at folder `node` (just `[node]` when
    /// compact folders is off or there is nothing to compact). Only loaded
    /// folders compact.
    pub(super) fn compact_chain(&self, node: &FsNode) -> Vec<FsNode> {
        let mut chain = vec![node.clone()];
        if !self.compact.get() || node.kind != FsKind::Folder {
            return chain;
        }
        loop {
            let last = chain.last().expect("non-empty");
            if !last.loaded || last.children.len() != 1 || last.children[0].kind != FsKind::Folder {
                break;
            }
            let next = last.children[0].clone();
            chain.push(next);
        }
        chain
    }

    /// Expanding a compact row loads the single-folder chain below it so it
    /// can compact further (VS Code resolves the chain eagerly).
    pub(super) fn extend_compact_chain(&self, id: u64) {
        if !self.compact.peek() {
            return;
        }
        let mut cur = id;
        loop {
            self.ensure_loaded(cur);
            let next = self.tree.peek().node(cur).and_then(|n| {
                (n.children.len() == 1 && n.children[0].kind == FsKind::Folder).then(|| n.children[0].id)
            });
            match next {
                Some(n) => {
                    self.expanded.update(|e| {
                        e.insert(n);
                    });
                    cur = n;
                }
                None => break,
            }
        }
    }
}
