//! Keyboard: the explorer's shortcuts and row navigation.

#[allow(clippy::wildcard_imports)]
use super::*;

impl FileExplorer {
    /// The VSCode key set. Every binding is CONDITIONAL: it acts only while
    /// the explorer is engaged (non-empty selection; F2 also engages via the
    /// active node) and declines otherwise, so arrows/Delete/etc. fall through
    /// to whatever else wants them. While the rename editor is focused, the
    /// editor's own key precedence applies — none of these fire.
    pub(super) fn install_keys(self) {
        // F2 — rename the active node.
        create_shortcut_if("F2", move || {
            if self.renaming.peek().is_some() {
                return true; // already renaming — swallow the repeat
            }
            match self.active() {
                Some(id) => {
                    self.start_rename_for(id);
                    true
                }
                None => false,
            }
        });
        // Delete — delete the selection. On disk this is an irreversible recursive
        // remove (no trash, no undo), so confirm first; a single Delete keypress must
        // not be able to destroy user data. Model-only mode deletes immediately.
        create_shortcut_if("Delete", move || self.request_delete(false));
        create_shortcut_if("Shift+Delete", move || self.request_delete(true));
        // Mod+Z — undo the last file operation (VS Code); declines with nothing to undo,
        // and a focused editor's own undo wins first.
        create_shortcut_if("Mod+Z", move || {
            if self.history.peek().is_empty() {
                return false;
            }
            self.undo();
            true
        });
        // Mod+A — select every visible node, from idle too (a focused editor's
        // own Select All never reaches here — editor precedence). Declines only
        // over an empty tree.
        create_shortcut_if("Mod+A", move || {
            let all = self.visible_ids();
            if all.is_empty() {
                return false;
            }
            self.selected.set(all);
            true
        });
        // Mod+C / Mod+X / Mod+V — the explorer clipboard (a focused editor's
        // clipboard intents win first, as everywhere).
        create_shortcut_if("Mod+C", move || {
            if self.selected.peek().is_empty() {
                return false;
            }
            self.copy_selection();
            true
        });
        create_shortcut_if("Mod+X", move || {
            if self.selected.peek().is_empty() {
                return false;
            }
            self.cut_selection();
            true
        });
        create_shortcut_if("Mod+V", move || {
            if self.clipboard.peek().is_none() {
                return false;
            }
            self.paste_clipboard();
            true
        });
        // Home / End — jump to the first/last visible row (while engaged, so an
        // idle explorer never steals page scrolling).
        create_shortcut_if("Home", move || self.key_jump(true));
        create_shortcut_if("End", move || self.key_jump(false));
        // Mod+↑/↓ — move the focus row WITHOUT changing the selection, then
        // Mod+Space toggles the focused row in/out (one-by-one multi-select).
        create_shortcut_if("Mod+ArrowDown", move || self.key_focus(1));
        create_shortcut_if("Mod+ArrowUp", move || self.key_focus(-1));
        create_shortcut_if("Mod+Space", move || {
            let Some(id) = self.active() else { return false };
            self.toggle_select(id);
            true
        });
        // Arrows — walk the visible rows; Shift extends the selection.
        create_shortcut_if("ArrowDown", move || self.key_step(1, false));
        create_shortcut_if("ArrowUp", move || self.key_step(-1, false));
        create_shortcut_if("Shift+ArrowDown", move || self.key_step(1, true));
        create_shortcut_if("Shift+ArrowUp", move || self.key_step(-1, true));
        // ArrowLeft — collapse the active folder, else jump to the parent.
        create_shortcut_if("ArrowLeft", move || {
            let Some(id) = self.active() else { return false };
            let is_open = self.tree.peek().node(id).is_some_and(|n| n.kind == FsKind::Folder)
                && self.expanded.peek().contains(&id);
            if is_open {
                self.expanded.update(|e| {
                    e.remove(&id);
                });
            } else if let Some(parent) = self.tree.peek().parent_of(id) {
                self.select_only(parent);
            }
            true
        });
        // ArrowRight — expand the active folder, else step into its first child.
        create_shortcut_if("ArrowRight", move || {
            let Some(id) = self.active() else { return false };
            let node = self.tree.peek().node(id).cloned();
            let Some(node) = node else { return false };
            if node.kind != FsKind::Folder {
                return true; // engaged but nothing to expand — consume, like VSCode
            }
            if !self.expanded.peek().contains(&id) {
                self.toggle_folder(id);
            } else if let Some(first) = node.children.first() {
                self.select_only(first.id);
            }
            true
        });
        // Escape — cancel a pending cut/copy first (VSCode), then drop the
        // selection (disengages the key set).
        create_shortcut_if("Escape", move || {
            if self.clipboard.peek().is_some() {
                self.clipboard.set(None);
                true
            } else if self.selected.peek().is_empty() {
                false
            } else {
                self.selected.set(Vec::new());
                true
            }
        });
    }

    /// Home/End — select the first/last visible row (declines while idle).
    pub(super) fn key_jump(&self, first: bool) -> bool {
        if self.selected.peek().is_empty() {
            return false;
        }
        let visible = self.visible_ids();
        let target = if first { visible.first() } else { visible.last() };
        if let Some(&id) = target {
            self.select_only(id);
        }
        true
    }

    /// Move the active row `dir` steps through the visible order; `extend`
    /// (Shift) grows the selection instead of replacing it. Declines (`false`)
    /// with no selection so plain arrows keep scrolling the page.
    pub(super) fn key_step(&self, dir: i64, extend: bool) -> bool {
        let Some(active) = self.active() else { return false };
        let visible = self.visible_ids();
        let Some(pos) = visible.iter().position(|&v| v == active) else { return false };
        let next = pos as i64 + dir;
        if next < 0 || next as usize >= visible.len() {
            return true; // at the edge — consumed, no move
        }
        let next = visible[next as usize];
        if extend {
            self.selected.update(|s| {
                s.retain(|&v| v != next);
                s.push(next); // becomes the new active end of the selection
            });
            self.active.set(Some(next));
        } else {
            self.select_only(next);
        }
        true
    }

    /// Mod+↑/↓ — move the FOCUS row without touching the selection (the
    /// Windows/VSCode one-by-one pattern; Mod+Space then toggles it in).
    pub(super) fn key_focus(&self, dir: i64) -> bool {
        let Some(cur) = self.active() else { return false };
        let visible = self.visible_ids();
        let Some(pos) = visible.iter().position(|&v| v == cur) else { return false };
        let next = pos as i64 + dir;
        if next >= 0 && (next as usize) < visible.len() {
            self.active.set(Some(visible[next as usize]));
        }
        true
    }
}
