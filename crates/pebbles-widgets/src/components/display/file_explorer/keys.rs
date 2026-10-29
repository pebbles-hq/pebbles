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
        // Every binding is also gated by Capability::Keyboard.
        let create_shortcut_if = move |combo: &str, f: Box<dyn Fn() -> bool>| {
            // A handled key reveals the active row (VS Code: keyboard focus
            // never leaves the viewport — arrows, Home/End, type-to-find, F2).
            create_shortcut_if(combo, move || {
                let handled = self.caps.peek().allows(Capability::Keyboard) && f();
                if handled {
                    self.scroll_active_into_view();
                }
                handled
            });
        };
        let multi = move || self.caps.peek().allows(Capability::MultiSelect);
        // F2 — rename the active node.
        create_shortcut_if(
            "F2",
            Box::new(move || {
                if self.renaming.peek().is_some() {
                    return true; // already renaming — swallow the repeat
                }
                match self.active() {
                    Some(id) => {
                        if !self.caps.peek().allows(Capability::Rename) {
                            return true;
                        }
                        self.start_rename_for(id);
                        true
                    }
                    None => false,
                }
            }),
        );
        // Delete — confirm, then move the selection to the trash (undoable);
        // Shift+Delete — confirm a permanent delete. A single keypress never
        // destroys data. Model-only mode deletes immediately.
        create_shortcut_if(
            "Delete",
            Box::new(move || {
                let engaged = !self.selected.peek().is_empty();
                let _ = self.request_delete(false);
                engaged
            }),
        );
        create_shortcut_if(
            "Shift+Delete",
            Box::new(move || {
                let engaged = !self.selected.peek().is_empty();
                let _ = self.request_delete(true);
                engaged
            }),
        );
        // Mod+Z — undo the last file operation (VS Code); declines with nothing to undo,
        // and a focused editor's own undo wins first.
        create_shortcut_if(
            "Mod+Z",
            Box::new(move || {
                if self.history.peek().is_empty() {
                    return false;
                }
                self.undo();
                true
            }),
        );
        // Mod+A — select every visible node, from idle too (a focused editor's
        // own Select All never reaches here — editor precedence). Declines only
        // over an empty tree.
        create_shortcut_if(
            "Mod+A",
            Box::new(move || {
                let all = self.visible_ids();
                if all.is_empty() || !multi() {
                    return false;
                }
                self.selected.set(all);
                true
            }),
        );
        // Mod+C / Mod+X / Mod+V — the explorer clipboard (a focused editor's
        // clipboard intents win first, as everywhere).
        create_shortcut_if(
            "Mod+C",
            Box::new(move || {
                if self.selected.peek().is_empty() {
                    return false;
                }
                let _ = self.copy_selection();
                true
            }),
        );
        create_shortcut_if(
            "Mod+X",
            Box::new(move || {
                if self.selected.peek().is_empty() {
                    return false;
                }
                let _ = self.cut_selection();
                true
            }),
        );
        create_shortcut_if(
            "Mod+V",
            Box::new(move || {
                if self.clipboard.peek().is_none() {
                    return false;
                }
                let _ = self.paste_clipboard();
                true
            }),
        );
        // Home / End — jump to the first/last visible row (while engaged, so an
        // idle explorer never steals page scrolling).
        create_shortcut_if("Home", Box::new(move || self.key_jump(true)));
        create_shortcut_if("End", Box::new(move || self.key_jump(false)));
        // Mod+↑/↓ — move the focus row WITHOUT changing the selection, then
        // Mod+Space toggles the focused row in/out (one-by-one multi-select).
        create_shortcut_if("Mod+ArrowDown", Box::new(move || self.key_focus(1)));
        create_shortcut_if("Mod+ArrowUp", Box::new(move || self.key_focus(-1)));
        create_shortcut_if(
            "Mod+Space",
            Box::new(move || {
                let Some(id) = self.active() else { return false };
                if !multi() {
                    return false;
                }
                self.toggle_select(id);
                true
            }),
        );
        // Arrows — walk the visible rows; Shift extends the selection.
        create_shortcut_if("ArrowDown", Box::new(move || self.key_step(1, false)));
        create_shortcut_if("ArrowUp", Box::new(move || self.key_step(-1, false)));
        create_shortcut_if("Shift+ArrowDown", Box::new(move || self.key_step(1, multi())));
        create_shortcut_if("Shift+ArrowUp", Box::new(move || self.key_step(-1, multi())));
        // ArrowLeft — collapse the active folder, else jump to the parent.
        create_shortcut_if(
            "ArrowLeft",
            Box::new(move || {
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
            }),
        );
        // ArrowRight — expand the active folder, else step into its first child.
        create_shortcut_if(
            "ArrowRight",
            Box::new(move || {
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
            }),
        );
        // Enter — open the active file (pinned) / toggle a folder; Space —
        // preview it; Mod+Enter — Open to the Side (when the app plugs it).
        create_shortcut_if(
            "Enter",
            Box::new(move || {
                let Some(id) = self.active() else { return false };
                self.open_entry(id, OpenMode::Open);
                true
            }),
        );
        create_shortcut_if(
            "Space",
            Box::new(move || {
                let Some(id) = self.active() else { return false };
                self.open_entry(id, OpenMode::Preview);
                true
            }),
        );
        create_shortcut_if(
            "Mod+Enter",
            Box::new(move || {
                let Some(id) = self.active() else { return false };
                let h = self.menu_handlers.peek().get(menu_ids::OPEN_TO_SIDE).cloned();
                match (h, self.entry(id)) {
                    (Some(h), Some(e)) if !e.is_folder() => {
                        h(&MenuTarget { context: MenuContext::File, entries: vec![e] });
                        true
                    }
                    _ => false,
                }
            }),
        );
        // Type-to-find: printable keys jump to the next visible row whose
        // name starts with what was typed (the buffer resets after 1 s idle).
        for ch in "abcdefghijklmnopqrstuvwxyz0123456789._-".chars() {
            create_shortcut_if(&ch.to_string(), Box::new(move || self.type_to_find(ch)));
            if ch.is_ascii_alphabetic() {
                create_shortcut_if(&format!("Shift+{ch}"), Box::new(move || self.type_to_find(ch)));
            }
        }
        // Escape — cancel a pending cut/copy first (VSCode), then drop the
        // selection (disengages the key set).
        create_shortcut_if(
            "Escape",
            Box::new(move || {
                if self.clipboard.peek().is_some() {
                    self.clipboard.set(None);
                    true
                } else if self.selected.peek().is_empty() {
                    false
                } else {
                    self.selected.set(Vec::new());
                    true
                }
            }),
        );
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

    /// Type-to-find (see `install_keys`). Declines while idle.
    pub fn type_to_find(&self, ch: char) -> bool {
        if self.active().is_none() {
            return false;
        }
        let now = std::time::Instant::now();
        let mut buf = self.find_buf.peek().clone();
        if buf.1.is_none_or(|t| now.duration_since(t).as_secs_f64() > 1.0) {
            buf.0.clear();
        }
        buf.0.push(ch.to_ascii_lowercase());
        buf.1 = Some(now);
        let prefix = buf.0.clone();
        self.find_buf.set(buf);
        let visible = self.visible_ids();
        let t = self.tree.peek();
        let name = |id: u64| t.node(id).map(|n| n.name.to_lowercase()).unwrap_or_default();
        let start = self.active().and_then(|a| visible.iter().position(|&v| v == a)).unwrap_or(0);
        // A fresh single letter moves past the current row (cycling); a longer
        // prefix may stay on it.
        let from = if prefix.chars().count() == 1 { start + 1 } else { start };
        let hit = (0..visible.len())
            .map(|k| visible[(from + k) % visible.len()])
            .find(|&id| name(id).starts_with(&prefix));
        drop(t);
        if let Some(id) = hit {
            self.select_only(id);
        }
        true
    }
}
