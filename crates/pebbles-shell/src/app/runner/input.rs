//! Event → intent translation: winit keys/cursors to Pebbles [`KeyInput`] commands,
//! shortcut tokens, cursor icons, and the overlay-aware wheel router. Pure functions
//! over the parent's imports.

#[allow(clippy::wildcard_imports)]
use super::*;

/// C6 — route a wheel `dy` when an overlay popover is open, so a scroll behind the
/// popover slides it to stay glued to its trigger (and dismisses it when nothing
/// scrolls). Shared by the main window and every secondary window — the overlay
/// signals are per-window, so `ui` must be the current window first (both callers
/// make it current before calling this). Returns whether a repaint is needed.
pub(super) fn wheel_with_overlay(ui: &mut Ui, cursor: Offset, dy: f64) -> bool {
    use pebbles_widgets::overlay;
    if !overlay::is_open() {
        return ui.dispatch_scroll(cursor, dy);
    }
    if overlay::over_panel(cursor.x, cursor.y) {
        // Wheel over the popover itself → scroll its own content only.
        ui.dispatch_scroll(cursor, dy)
    } else if ui.dispatch_scroll(cursor, dy) {
        // Wheel over the page behind the popover → slide the popover with it.
        overlay::shift(0.0, -dy);
        true
    } else {
        // Nowhere to scroll → dismiss so it never floats detached.
        overlay::hide_overlay();
        true
    }
}

pub(super) fn to_winit_cursor(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
    }
}

/// Translate a winit key press (+ Ctrl/Shift) into an editing command, or `None`
/// if it isn't one the focused editor cares about. Shift extends selections; Ctrl
/// switches arrows to word motion and enables the clipboard/undo/select-all set.
pub(super) fn to_command(event: &KeyEvent, ctrl: bool, shift: bool) -> Option<KeyInput> {
    use KeyInput::*;
    use Motion::*;
    let mv = |m: Motion| Some(Move { motion: m, extend: shift });
    match event.logical_key.as_ref() {
        Key::Named(NamedKey::Backspace) => Some(if ctrl { DeleteWordBack } else { Backspace }),
        Key::Named(NamedKey::Delete) => Some(if ctrl { DeleteWordForward } else { Delete }),
        Key::Named(NamedKey::ArrowLeft) => mv(if ctrl { WordLeft } else { Left }),
        Key::Named(NamedKey::ArrowRight) => mv(if ctrl { WordRight } else { Right }),
        Key::Named(NamedKey::ArrowUp) => mv(Up),
        Key::Named(NamedKey::ArrowDown) => mv(Down),
        Key::Named(NamedKey::Home) => mv(if ctrl { DocStart } else { LineStart }),
        Key::Named(NamedKey::End) => mv(if ctrl { DocEnd } else { LineEnd }),
        Key::Named(NamedKey::Enter) => Some(Enter),
        Key::Named(NamedKey::Escape) => Some(Escape),
        Key::Named(NamedKey::Space) if !ctrl => Some(Insert(" ".to_string())),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => Some(SelectAll),
            "c" => Some(Copy),
            "x" => Some(Cut),
            "v" => Some(Paste),
            "z" => Some(if shift { Redo } else { Undo }),
            "y" => Some(Redo),
            _ => None,
        },
        Key::Character(s) if s.chars().all(|ch| !ch.is_control()) => Some(Insert(s.to_string())),
        _ => None,
    }
}

/// Map a winit key to a [`ShortcutKey`] token (B2) — `None` for keys outside
/// the shortcut grammar.
pub(super) fn to_shortcut_key(event: &KeyEvent) -> Option<pebbles_core::ShortcutKey> {
    use pebbles_core::ShortcutKey as SK;
    use winit::keyboard::{Key, NamedKey};
    match event.logical_key.as_ref() {
        Key::Named(n) => match n {
            NamedKey::Enter => Some(SK::Enter),
            NamedKey::Escape => Some(SK::Escape),
            NamedKey::Space => Some(SK::Space),
            NamedKey::Tab => Some(SK::Tab),
            NamedKey::ArrowUp => Some(SK::ArrowUp),
            NamedKey::ArrowDown => Some(SK::ArrowDown),
            NamedKey::ArrowLeft => Some(SK::ArrowLeft),
            NamedKey::ArrowRight => Some(SK::ArrowRight),
            NamedKey::Home => Some(SK::Home),
            NamedKey::End => Some(SK::End),
            NamedKey::Delete => Some(SK::Delete),
            NamedKey::Backspace => Some(SK::Backspace),
            NamedKey::F1 => Some(SK::F(1)),
            NamedKey::F2 => Some(SK::F(2)),
            NamedKey::F3 => Some(SK::F(3)),
            NamedKey::F4 => Some(SK::F(4)),
            NamedKey::F5 => Some(SK::F(5)),
            NamedKey::F6 => Some(SK::F(6)),
            NamedKey::F7 => Some(SK::F(7)),
            NamedKey::F8 => Some(SK::F(8)),
            NamedKey::F9 => Some(SK::F(9)),
            NamedKey::F10 => Some(SK::F(10)),
            NamedKey::F11 => Some(SK::F(11)),
            NamedKey::F12 => Some(SK::F(12)),
            _ => None,
        },
        Key::Character(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => Some(SK::Char(c.to_ascii_lowercase())),
                _ => None,
            }
        }
        _ => None,
    }
}
