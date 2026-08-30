//! Keyboard editing intents ([`KeyInput`]) + global modifier state.
//!
//! The shell translates raw winit key events (with Ctrl/Shift) into these
//! platform-neutral commands and routes them to the focused editor via
//! [`focus::dispatch_key`](crate::focus::dispatch_key). It also mirrors the live
//! modifier state here so pointer handlers can implement Shift-click, etc.

use std::cell::Cell;

/// A caret/selection movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// One grapheme left / right.
    Left,
    Right,
    /// One word left / right.
    WordLeft,
    WordRight,
    /// Start / end of the visual line (Home / End).
    LineStart,
    LineEnd,
    /// Up / down a line (multiline).
    Up,
    Down,
    /// Start / end of the whole text (Ctrl+Home / Ctrl+End).
    DocStart,
    DocEnd,
}

/// A single editing intent aimed at the focused text editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    /// Insert literal text at the caret, replacing any selection.
    Insert(String),
    /// IME composition update: the in-progress (preedit) text shown underlined at the
    /// caret. Replaces any previous preedit; an empty string clears composition. The
    /// text is *not* committed until an [`Insert`](KeyInput::Insert) (from `Ime::Commit`).
    Preedit(String),
    /// Delete the selection, or the grapheme before the caret.
    Backspace,
    /// Delete the selection, or the grapheme after the caret.
    Delete,
    /// Delete to the previous word boundary (Ctrl+Backspace).
    DeleteWordBack,
    /// Delete to the next word boundary (Ctrl+Delete).
    DeleteWordForward,
    /// Move (or, with `extend`, grow the selection by) one motion.
    Move { motion: Motion, extend: bool },
    /// Select the whole field (Ctrl+A).
    SelectAll,
    /// Copy the selection to the clipboard (Ctrl+C).
    Copy,
    /// Cut the selection to the clipboard (Ctrl+X).
    Cut,
    /// Paste clipboard text at the caret (Ctrl+V).
    Paste,
    /// Undo / redo (Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y).
    Undo,
    Redo,
    /// Enter/Return — newline in a multiline field, else submit.
    Enter,
    /// Escape — blur the field.
    Escape,
}

thread_local! {
    static SHIFT: Cell<bool> = const { Cell::new(false) };
    static CTRL: Cell<bool> = const { Cell::new(false) };
}

/// Update the live modifier state (called by the shell on every change).
pub fn set_modifiers(shift: bool, ctrl: bool) {
    SHIFT.with(|s| s.set(shift));
    CTRL.with(|c| c.set(ctrl));
}

/// Whether Shift is currently held (for Shift-click selection).
pub fn shift_held() -> bool {
    SHIFT.with(Cell::get)
}

/// Whether Ctrl is currently held.
pub fn ctrl_held() -> bool {
    CTRL.with(Cell::get)
}
