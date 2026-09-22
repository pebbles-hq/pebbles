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
///
/// A thin wrapper over [`command_for`], which owns the pure logical-key → intent
/// mapping (so it can be unit-tested without constructing a winit `KeyEvent`).
pub(super) fn to_command(event: &KeyEvent, ctrl: bool, shift: bool) -> Option<KeyInput> {
    command_for(event.logical_key.as_ref(), ctrl, shift)
}

/// The pure logical-key → editing-command mapping behind [`to_command`]. Kept free
/// of the winit `KeyEvent` wrapper (whose `platform_specific` field is `pub(crate)`
/// and so can't be built in a test) — it takes only the borrowed logical [`Key`] plus
/// the Ctrl/Shift flags, which ARE constructible headlessly.
fn command_for(key: Key<&str>, ctrl: bool, shift: bool) -> Option<KeyInput> {
    use KeyInput::*;
    use Motion::*;
    let mv = |m: Motion| Some(Move { motion: m, extend: shift });
    match key {
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
        Key::Named(NamedKey::F12) => Some(GoToDefinition),
        Key::Named(NamedKey::Space) if ctrl => Some(TriggerCompletion),
        Key::Named(NamedKey::Space) if !ctrl => Some(Insert(" ".to_string())),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => Some(SelectAll),
            "c" => Some(Copy),
            "x" => Some(Cut),
            "v" => Some(Paste),
            "z" => Some(if shift { Redo } else { Undo }),
            "y" => Some(Redo),
            "d" => Some(SelectNextOccurrence),
            "/" => Some(ToggleComment),
            "f" => Some(Find),
            "h" => Some(Replace),
            "p" => Some(CommandPalette),
            _ => None,
        },
        Key::Character(s) if s.chars().all(|ch| !ch.is_control()) => Some(Insert(s.to_string())),
        _ => None,
    }
}

/// The editor "chord" intents the shell resolves from a shortcut token + modifiers,
/// which do NOT flow through [`to_command`]: **Shift+Alt+F** = [`KeyInput::Format`],
/// **Tab** = [`KeyInput::Indent`], **Shift+Tab** = [`KeyInput::Outdent`]. `wants_tab`
/// is whether the focused widget is a code editor that consumes Tab (elsewhere Tab
/// traverses focus, so Tab yields `None`). `None` for anything that isn't a chord.
pub(super) fn editor_chord_intent(
    mods: pebbles_core::Mods,
    key: pebbles_core::ShortcutKey,
    wants_tab: bool,
) -> Option<KeyInput> {
    use pebbles_core::ShortcutKey as SK;
    match key {
        SK::Char('f') if mods.shift && mods.alt => Some(KeyInput::Format),
        SK::Tab if wants_tab => Some(if mods.shift { KeyInput::Outdent } else { KeyInput::Indent }),
        _ => None,
    }
}

/// Map a winit key to a [`ShortcutKey`] token (B2) — `None` for keys outside
/// the shortcut grammar. A thin wrapper over [`shortcut_key_for`].
pub(super) fn to_shortcut_key(event: &KeyEvent) -> Option<pebbles_core::ShortcutKey> {
    shortcut_key_for(event.logical_key.as_ref())
}

/// The pure logical-key → [`ShortcutKey`] mapping behind [`to_shortcut_key`] (see
/// [`command_for`] for why the winit `KeyEvent` wrapper is peeled off first).
fn shortcut_key_for(key: Key<&str>) -> Option<pebbles_core::ShortcutKey> {
    use pebbles_core::ShortcutKey as SK;
    use winit::keyboard::{Key, NamedKey};
    match key {
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

#[cfg(test)]
mod tests {
    //! The editor shortcut wiring, tested at the pure-mapping layer. A real
    //! `winit::KeyEvent` can't be built outside winit (its `platform_specific` field
    //! is `pub(crate)`), so these drive [`command_for`] / [`shortcut_key_for`] /
    //! [`editor_chord_intent`] directly on constructed logical keys — the same values
    //! the shell feeds them at runtime. Covers the code-editor command intents that
    //! previously had zero tests: Find / Replace / CommandPalette / TriggerCompletion /
    //! GoToDefinition / SelectNextOccurrence / ToggleComment (via `command_for`) and
    //! Format / Indent / Outdent (the chord path, via `editor_chord_intent`).

    use pebbles_core::{KeyInput, Mods, Motion, ShortcutKey};
    use winit::keyboard::{Key, NamedKey};

    use super::{command_for, editor_chord_intent, shortcut_key_for};

    /// A single printable character as its winit logical key.
    fn ch(c: &'static str) -> Key<&'static str> {
        Key::Character(c)
    }

    // ----- command_for: the new IDE command intents ------------------------

    #[test]
    fn ctrl_chords_map_to_editor_commands() {
        // ctrl = true, shift = false unless noted.
        assert_eq!(command_for(ch("f"), true, false), Some(KeyInput::Find));
        assert_eq!(command_for(ch("h"), true, false), Some(KeyInput::Replace));
        assert_eq!(command_for(ch("p"), true, false), Some(KeyInput::CommandPalette));
        assert_eq!(command_for(ch("d"), true, false), Some(KeyInput::SelectNextOccurrence));
        assert_eq!(command_for(ch("/"), true, false), Some(KeyInput::ToggleComment));
        assert_eq!(command_for(ch("a"), true, false), Some(KeyInput::SelectAll));
        assert_eq!(command_for(ch("c"), true, false), Some(KeyInput::Copy));
        assert_eq!(command_for(ch("x"), true, false), Some(KeyInput::Cut));
        assert_eq!(command_for(ch("v"), true, false), Some(KeyInput::Paste));
    }

    #[test]
    fn ctrl_z_is_undo_and_shift_flips_to_redo() {
        assert_eq!(command_for(ch("z"), true, false), Some(KeyInput::Undo));
        assert_eq!(command_for(ch("z"), true, true), Some(KeyInput::Redo));
        assert_eq!(command_for(ch("y"), true, false), Some(KeyInput::Redo));
    }

    #[test]
    fn uppercase_character_is_lowered_before_matching() {
        // The OS reports the shifted glyph for Ctrl+Shift+P; the palette must still open.
        assert_eq!(command_for(ch("P"), true, true), Some(KeyInput::CommandPalette));
        assert_eq!(command_for(ch("F"), true, false), Some(KeyInput::Find));
    }

    #[test]
    fn completion_and_definition_intents() {
        // Ctrl+Space requests completion; a bare Space just types one.
        assert_eq!(command_for(Key::Named(NamedKey::Space), true, false), Some(KeyInput::TriggerCompletion));
        assert_eq!(
            command_for(Key::Named(NamedKey::Space), false, false),
            Some(KeyInput::Insert(" ".to_string()))
        );
        // F12 goes to definition, with or without modifiers.
        assert_eq!(command_for(Key::Named(NamedKey::F12), false, false), Some(KeyInput::GoToDefinition));
    }

    #[test]
    fn motion_respects_ctrl_word_jump_and_shift_extend() {
        assert_eq!(
            command_for(Key::Named(NamedKey::ArrowLeft), false, false),
            Some(KeyInput::Move { motion: Motion::Left, extend: false })
        );
        assert_eq!(
            command_for(Key::Named(NamedKey::ArrowLeft), true, true),
            Some(KeyInput::Move { motion: Motion::WordLeft, extend: true })
        );
        assert_eq!(
            command_for(Key::Named(NamedKey::Home), true, false),
            Some(KeyInput::Move { motion: Motion::DocStart, extend: false })
        );
    }

    #[test]
    fn deletion_and_plain_insert() {
        assert_eq!(command_for(Key::Named(NamedKey::Backspace), false, false), Some(KeyInput::Backspace));
        assert_eq!(command_for(Key::Named(NamedKey::Backspace), true, false), Some(KeyInput::DeleteWordBack));
        assert_eq!(command_for(ch("q"), false, false), Some(KeyInput::Insert("q".to_string())));
    }

    #[test]
    fn unknown_ctrl_chord_is_not_captured() {
        // Ctrl+B has no editor meaning — falls through to `None` (not a literal insert).
        assert_eq!(command_for(ch("b"), true, false), None);
    }

    // ----- editor_chord_intent: Format / Indent / Outdent ------------------

    #[test]
    fn shift_alt_f_is_format() {
        let shift_alt = Mods { shift: true, alt: true, ctrl: false, meta: false };
        assert_eq!(editor_chord_intent(shift_alt, ShortcutKey::Char('f'), false), Some(KeyInput::Format));
        // Missing Alt → not Format.
        let shift_only = Mods { shift: true, ..Default::default() };
        assert_eq!(editor_chord_intent(shift_only, ShortcutKey::Char('f'), false), None);
    }

    #[test]
    fn tab_indents_and_shift_tab_outdents_in_an_editor() {
        let none = Mods::default();
        let shift = Mods { shift: true, ..Default::default() };
        // wants_tab = a focused code editor consumes Tab.
        assert_eq!(editor_chord_intent(none, ShortcutKey::Tab, true), Some(KeyInput::Indent));
        assert_eq!(editor_chord_intent(shift, ShortcutKey::Tab, true), Some(KeyInput::Outdent));
        // Not in an editor → Tab is left for focus traversal (None here).
        assert_eq!(editor_chord_intent(none, ShortcutKey::Tab, false), None);
    }

    // ----- shortcut_key_for: the tokens the chord path is keyed on ----------

    #[test]
    fn shortcut_tokens_for_the_chord_keys() {
        assert_eq!(shortcut_key_for(ch("f")), Some(ShortcutKey::Char('f')));
        // Uppercase is normalized to lowercase.
        assert_eq!(shortcut_key_for(ch("F")), Some(ShortcutKey::Char('f')));
        assert_eq!(shortcut_key_for(Key::Named(NamedKey::Tab)), Some(ShortcutKey::Tab));
        assert_eq!(shortcut_key_for(Key::Named(NamedKey::F12)), Some(ShortcutKey::F(12)));
        assert_eq!(shortcut_key_for(Key::Named(NamedKey::Enter)), Some(ShortcutKey::Enter));
        // A multi-char logical key is not a shortcut token.
        assert_eq!(shortcut_key_for(ch("ab")), None);
    }
}
