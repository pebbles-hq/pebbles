//! [`ListNav`] — the shared keyboard-navigation model for a menu/option list
//! (SI-4). It owns a reactive **active-row index** and turns a focused editor's
//! key intents into list moves: Up/Down change the active row, Enter picks it,
//! Escape dismisses. Reading [`ListNav::active`] in a render highlights the row.
//!
//! It is substrate-agnostic: it never touches the overlay itself — the caller
//! supplies `on_pick(i)` and `on_escape()`. Wired into the combobox search list
//! and the command palette via [`TextField::on_nav`](super::TextField::on_nav).

use pebbles_core::{KeyInput, Motion, Signal, create_signal};

/// The active-row model for a keyboard-navigable list. `Copy` (a signal handle),
/// so it threads freely into closures.
#[derive(Clone, Copy)]
pub struct ListNav {
    active: Signal<Option<usize>>,
}

/// Create a [`ListNav`] with no active row yet.
pub fn list_nav() -> ListNav {
    ListNav { active: create_signal(None) }
}

impl ListNav {
    /// The active row index (reactive read — highlight the matching row).
    pub fn active(&self) -> Option<usize> {
        self.active.get()
    }

    /// Set the active row (e.g. mouse hover syncing the keyboard cursor).
    pub fn set_active(&self, i: Option<usize>) {
        self.active.set(i);
    }

    /// Keep the active row in range after the visible list is re-filtered to
    /// `len` rows: clear it when the list is empty, clamp it to the last row
    /// otherwise. Call once per render before building the handler.
    pub fn clamp(&self, len: usize) {
        let a = self.active.peek();
        if len == 0 {
            if a.is_some() {
                self.active.set(None);
            }
        } else if let Some(i) = a
            && i >= len
        {
            self.active.set(Some(len - 1));
        }
    }

    /// A key handler for the owning editor: Up/Down move the active row within
    /// `0..len` (Down from none → first, Up from none → last), Enter fires
    /// `on_pick(active)`, Escape fires `on_escape`. Returns `true` when it
    /// consumed the key (so the field skips its own edit). Rebuild it each render
    /// so `len` tracks the current filtered count.
    pub fn handler(
        &self,
        len: usize,
        on_pick: impl Fn(usize) + 'static,
        on_escape: impl Fn() + 'static,
    ) -> impl Fn(KeyInput) -> bool + 'static {
        let active = self.active;
        move |k: KeyInput| match k {
            KeyInput::Move { motion: Motion::Down, .. } => {
                if len == 0 {
                    return true;
                }
                let next = match active.peek() {
                    None => 0,
                    Some(i) => (i + 1).min(len - 1),
                };
                active.set(Some(next));
                true
            }
            KeyInput::Move { motion: Motion::Up, .. } => {
                if len == 0 {
                    return true;
                }
                let next = match active.peek() {
                    None => len - 1,
                    Some(i) => i.saturating_sub(1),
                };
                active.set(Some(next));
                true
            }
            KeyInput::Enter => {
                if let Some(i) = active.peek()
                    && i < len
                {
                    on_pick(i);
                }
                true
            }
            KeyInput::Escape => {
                on_escape();
                true
            }
            _ => false,
        }
    }
}
