//! Event → intent translation: winit keys/cursors to Pebbles [`KeyInput`] commands,
//! shortcut tokens, cursor icons, and the overlay-aware wheel router. Pure functions
//! over the parent's imports.

#[allow(clippy::wildcard_imports)]
use super::*;
use winit::keyboard::PhysicalKey;

/// Whether `event` is user input (keyboard, IME, pointer, wheel, touch): the window's next
/// frame is input-driven and the render thread draws it ahead of animation-only frames
/// (`FramePacket::input`, 08 §17.3 step 4).
pub(super) fn is_user_input(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::Touch(_)
            | WindowEvent::PinchGesture { .. }
            | WindowEvent::HoveredFile(_)
            | WindowEvent::DroppedFile(_)
    )
}

/// Route an OS file drag (`HoveredFile` / `DroppedFile` / `HoveredFileCancelled`) at the
/// last known pointer position to the [`pebbles_widgets::file_drop`] targets. `None`
/// when `event` is not a file-drag event; otherwise whether a repaint is needed.
/// Dropped files are queued and delivered as one batch by `file_drop::flush`, which the
/// runner calls from `about_to_wait` (winit sends one `DroppedFile` per file).
pub(super) fn file_drop_event(event: &WindowEvent, cursor: Offset) -> Option<bool> {
    use pebbles_widgets::file_drop;
    match event {
        WindowEvent::HoveredFile(path) => Some(file_drop::hover(path.clone(), cursor)),
        WindowEvent::DroppedFile(path) => {
            file_drop::drop_file(path.clone(), cursor);
            Some(false)
        }
        WindowEvent::HoveredFileCancelled => Some(file_drop::cancel()),
        _ => None,
    }
}

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

/// The cursor icon to switch to, or `None` to keep `current`. While the pointer is
/// captured by a drag (a resize handle, a text selection, a pan, a drag-scroll), the
/// icon the drag started with stays put even when the pointer leaves the widget —
/// what browsers do under pointer capture, and GTK/Qt during a grab — so a split
/// handle keeps its resize arrow while dragged. `under` is the icon the widget under
/// the pointer asks for (`None` = default arrow).
pub(super) fn cursor_update(current: Cursor, under: Option<Cursor>, captured: bool) -> Option<Cursor> {
    if captured {
        return None;
    }
    let want = under.unwrap_or(Cursor::Default);
    (want != current).then_some(want)
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
    command_for(event.logical_key.as_ref(), event.text.as_deref(), event.physical_key, ctrl, shift)
}

/// The Latin letter printed on a physical key of the US layout (`KeyA` → `'a'`).
fn us_letter(physical: PhysicalKey) -> Option<char> {
    use winit::keyboard::KeyCode as K;
    let PhysicalKey::Code(code) = physical else { return None };
    let letters = [
        K::KeyA,
        K::KeyB,
        K::KeyC,
        K::KeyD,
        K::KeyE,
        K::KeyF,
        K::KeyG,
        K::KeyH,
        K::KeyI,
        K::KeyJ,
        K::KeyK,
        K::KeyL,
        K::KeyM,
        K::KeyN,
        K::KeyO,
        K::KeyP,
        K::KeyQ,
        K::KeyR,
        K::KeyS,
        K::KeyT,
        K::KeyU,
        K::KeyV,
        K::KeyW,
        K::KeyX,
        K::KeyY,
        K::KeyZ,
    ];
    letters.iter().position(|&k| k == code).map(|i| char::from(b'a' + i as u8))
}

/// The character a shortcut matches on, from the layout-aware logical key: the
/// character the user's layout produces (so AZERTY / QWERTZ / Dvorak users press
/// the key labelled Z for Ctrl+Z wherever it sits), lowercased. On a non-Latin
/// layout (Cyrillic, Greek, Hebrew, Arabic, …) the logical key is a non-ASCII
/// letter, and shortcuts fall back to the US letter of the physical key — what
/// Chromium, GTK and VS Code do, so Ctrl+С on a Russian layout still copies.
fn shortcut_char(logical: &str, physical: PhysicalKey) -> Option<char> {
    let mut chars = logical.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    if c.is_control() {
        return None;
    }
    if c.is_ascii() {
        return Some(c.to_ascii_lowercase());
    }
    if c.is_alphabetic()
        && let Some(latin) = us_letter(physical)
    {
        return Some(latin);
    }
    c.to_lowercase().next()
}

/// The pure key → editing-command mapping behind [`to_command`]. Kept free of the
/// winit `KeyEvent` wrapper (whose `platform_specific` field is `pub(crate)` and so
/// can't be built in a test) — it takes the event's borrowed logical [`Key`], its
/// `text`, its physical key and the Ctrl/Shift flags, which ARE constructible
/// headlessly.
///
/// Typed text comes from `text` (the characters the key produced after dead-key /
/// compose processing — winit's documented field for text input), falling back to
/// the logical key; a bare dead key (`Key::Dead`, no text) types nothing and waits
/// for the next key. Shortcuts match on [`shortcut_char`].
fn command_for(
    key: Key<&str>,
    text: Option<&str>,
    physical: PhysicalKey,
    ctrl: bool,
    shift: bool,
) -> Option<KeyInput> {
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
        Key::Character(s) if ctrl => match shortcut_char(s, physical)? {
            'a' => Some(SelectAll),
            'c' => Some(Copy),
            'x' => Some(Cut),
            'v' => Some(Paste),
            'z' => Some(if shift { Redo } else { Undo }),
            'y' => Some(Redo),
            'd' => Some(SelectNextOccurrence),
            '/' => Some(ToggleComment),
            // VS Code conventions: Shift is significant. Ctrl+F / Ctrl+H are the
            // editor's find / replace, but Ctrl+Shift+F / Ctrl+Shift+H are workbench
            // "find/replace in files" and Ctrl+P is workbench Quick Open — none of
            // those belong to the focused editor, so they map to no intent and the key
            // falls through to app-level shortcuts. Ctrl+Shift+P is the command palette.
            'f' if !shift => Some(Find),
            'h' if !shift => Some(Replace),
            'p' if shift => Some(CommandPalette),
            _ => None,
        },
        Key::Character(s) => {
            let typed = text.unwrap_or(s);
            (!typed.is_empty() && typed.chars().all(|ch| !ch.is_control())).then(|| Insert(typed.to_string()))
        }
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
    shortcut_key_for(event.logical_key.as_ref(), event.physical_key)
}

/// The pure logical-key → [`ShortcutKey`] mapping behind [`to_shortcut_key`] (see
/// [`command_for`] for why the winit `KeyEvent` wrapper is peeled off first).
fn shortcut_key_for(key: Key<&str>, physical: PhysicalKey) -> Option<pebbles_core::ShortcutKey> {
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
        Key::Character(s) => shortcut_char(s, physical).map(SK::Char),
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
    use winit::keyboard::{Key, NamedKey, NativeKey};

    use winit::keyboard::{KeyCode, NativeKeyCode, PhysicalKey};

    use super::{command_for, editor_chord_intent, shortcut_key_for};

    const NO_PHYSICAL: PhysicalKey = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);

    /// [`command_for`] for a key whose text equals its logical key, physical unknown.
    fn cmd(key: Key<&str>, ctrl: bool, shift: bool) -> Option<KeyInput> {
        command_for(key, None, NO_PHYSICAL, ctrl, shift)
    }

    fn sk(key: Key<&str>) -> Option<ShortcutKey> {
        shortcut_key_for(key, NO_PHYSICAL)
    }

    /// A single printable character as its winit logical key.
    fn ch(c: &'static str) -> Key<&'static str> {
        Key::Character(c)
    }

    // ----- command_for: the new IDE command intents ------------------------

    #[test]
    fn ctrl_chords_map_to_editor_commands() {
        // ctrl = true, shift = false unless noted.
        assert_eq!(cmd(ch("f"), true, false), Some(KeyInput::Find));
        assert_eq!(cmd(ch("h"), true, false), Some(KeyInput::Replace));
        assert_eq!(cmd(ch("p"), true, true), Some(KeyInput::CommandPalette));
        assert_eq!(cmd(ch("d"), true, false), Some(KeyInput::SelectNextOccurrence));
        assert_eq!(cmd(ch("/"), true, false), Some(KeyInput::ToggleComment));
        assert_eq!(cmd(ch("a"), true, false), Some(KeyInput::SelectAll));
        assert_eq!(cmd(ch("c"), true, false), Some(KeyInput::Copy));
        assert_eq!(cmd(ch("x"), true, false), Some(KeyInput::Cut));
        assert_eq!(cmd(ch("v"), true, false), Some(KeyInput::Paste));
    }

    #[test]
    fn shift_is_significant_for_p_f_h() {
        // Ctrl+P (Quick Open) and Ctrl+Shift+F/H (find/replace in files) are app-level:
        // the editor gets no intent, so the key reaches app shortcuts.
        assert_eq!(cmd(ch("p"), true, false), None);
        assert_eq!(cmd(ch("p"), true, true), Some(KeyInput::CommandPalette));
        assert_eq!(cmd(ch("f"), true, false), Some(KeyInput::Find));
        assert_eq!(cmd(ch("f"), true, true), None);
        assert_eq!(cmd(ch("F"), true, true), None);
        assert_eq!(cmd(ch("h"), true, true), None);
    }

    #[test]
    fn ctrl_z_is_undo_and_shift_flips_to_redo() {
        assert_eq!(cmd(ch("z"), true, false), Some(KeyInput::Undo));
        assert_eq!(cmd(ch("z"), true, true), Some(KeyInput::Redo));
        assert_eq!(cmd(ch("y"), true, false), Some(KeyInput::Redo));
    }

    #[test]
    fn uppercase_character_is_lowered_before_matching() {
        // The OS reports the shifted glyph for Ctrl+Shift+P; the palette must still open.
        assert_eq!(cmd(ch("P"), true, true), Some(KeyInput::CommandPalette));
        assert_eq!(cmd(ch("F"), true, false), Some(KeyInput::Find));
    }

    #[test]
    fn completion_and_definition_intents() {
        // Ctrl+Space requests completion; a bare Space just types one.
        assert_eq!(cmd(Key::Named(NamedKey::Space), true, false), Some(KeyInput::TriggerCompletion));
        assert_eq!(cmd(Key::Named(NamedKey::Space), false, false), Some(KeyInput::Insert(" ".to_string())));
        // F12 goes to definition, with or without modifiers.
        assert_eq!(cmd(Key::Named(NamedKey::F12), false, false), Some(KeyInput::GoToDefinition));
    }

    #[test]
    fn motion_respects_ctrl_word_jump_and_shift_extend() {
        assert_eq!(
            cmd(Key::Named(NamedKey::ArrowLeft), false, false),
            Some(KeyInput::Move { motion: Motion::Left, extend: false })
        );
        assert_eq!(
            cmd(Key::Named(NamedKey::ArrowLeft), true, true),
            Some(KeyInput::Move { motion: Motion::WordLeft, extend: true })
        );
        assert_eq!(
            cmd(Key::Named(NamedKey::Home), true, false),
            Some(KeyInput::Move { motion: Motion::DocStart, extend: false })
        );
    }

    #[test]
    fn deletion_and_plain_insert() {
        assert_eq!(cmd(Key::Named(NamedKey::Backspace), false, false), Some(KeyInput::Backspace));
        assert_eq!(cmd(Key::Named(NamedKey::Backspace), true, false), Some(KeyInput::DeleteWordBack));
        assert_eq!(cmd(ch("q"), false, false), Some(KeyInput::Insert("q".to_string())));
    }

    #[test]
    fn unknown_ctrl_chord_is_not_captured() {
        // Ctrl+B has no editor meaning — falls through to `None` (not a literal insert).
        assert_eq!(cmd(ch("b"), true, false), None);
    }

    // ----- non-US layouts, dead keys, compose ---------------------------------
    //
    // The events below are what winit's xkb backend produces (X11 and Wayland):
    // `logical_key` follows the active layout, `physical_key` is the US-position
    // scancode, and `text` carries the dead-key / compose result.

    #[test]
    fn azerty_shortcuts_follow_the_printed_letter() {
        // AZERTY: the key labelled A sits at US Q, W at US Z, Z at US W.
        let ctrl = |logical, code| {
            command_for(Key::Character(logical), Some(logical), PhysicalKey::Code(code), true, false)
        };
        assert_eq!(ctrl("a", KeyCode::KeyQ), Some(KeyInput::SelectAll));
        assert_eq!(ctrl("z", KeyCode::KeyW), Some(KeyInput::Undo));
        assert_eq!(ctrl("w", KeyCode::KeyZ), None, "US-Z position is W on AZERTY: not undo");
        assert_eq!(
            command_for(Key::Character("Z"), Some("Z"), PhysicalKey::Code(KeyCode::KeyW), true, true),
            Some(KeyInput::Redo),
            "Ctrl+Shift+Z"
        );
    }

    #[test]
    fn qwertz_shortcuts_follow_the_printed_letter() {
        // German QWERTZ: Z and Y are swapped against US.
        let k = |logical, code, shift| {
            command_for(Key::Character(logical), Some(logical), PhysicalKey::Code(code), true, shift)
        };
        assert_eq!(k("z", KeyCode::KeyY, false), Some(KeyInput::Undo));
        assert_eq!(k("Z", KeyCode::KeyY, true), Some(KeyInput::Redo));
        assert_eq!(k("y", KeyCode::KeyZ, false), Some(KeyInput::Redo));
        assert_eq!(k("c", KeyCode::KeyC, false), Some(KeyInput::Copy));
        assert_eq!(k("v", KeyCode::KeyV, false), Some(KeyInput::Paste));
        assert_eq!(k("x", KeyCode::KeyX, false), Some(KeyInput::Cut));
    }

    #[test]
    fn non_latin_layout_shortcuts_use_the_physical_key() {
        // Russian ЙЦУКЕН: С at US C, М at US V, Ч at US X, Я at US Z, Ф at US A.
        let ctrl = |logical, code, shift| {
            command_for(Key::Character(logical), Some(logical), PhysicalKey::Code(code), true, shift)
        };
        assert_eq!(ctrl("с", KeyCode::KeyC, false), Some(KeyInput::Copy));
        assert_eq!(ctrl("м", KeyCode::KeyV, false), Some(KeyInput::Paste));
        assert_eq!(ctrl("ч", KeyCode::KeyX, false), Some(KeyInput::Cut));
        assert_eq!(ctrl("ф", KeyCode::KeyA, false), Some(KeyInput::SelectAll));
        assert_eq!(ctrl("я", KeyCode::KeyZ, false), Some(KeyInput::Undo));
        assert_eq!(ctrl("Я", KeyCode::KeyZ, true), Some(KeyInput::Redo));
        // App shortcuts (Ctrl+S, …) resolve the same way.
        assert_eq!(
            shortcut_key_for(Key::Character("ы"), PhysicalKey::Code(KeyCode::KeyS)),
            Some(ShortcutKey::Char('s'))
        );
        // Greek too.
        assert_eq!(ctrl("ψ", KeyCode::KeyC, false), Some(KeyInput::Copy));
        // Typing (no Ctrl) still inserts the Cyrillic letter.
        assert_eq!(
            command_for(Key::Character("с"), Some("с"), PhysicalKey::Code(KeyCode::KeyC), false, false),
            Some(KeyInput::Insert("с".into()))
        );
    }

    #[test]
    fn latin_accented_keys_are_not_remapped() {
        // German ö sits on US ';' — not a letter key, so it stays ö (no fallback).
        assert_eq!(
            shortcut_key_for(Key::Character("ö"), PhysicalKey::Code(KeyCode::Semicolon)),
            Some(ShortcutKey::Char('ö'))
        );
        assert_eq!(
            shortcut_key_for(Key::Character("É"), PhysicalKey::Code(KeyCode::Digit2)),
            Some(ShortcutKey::Char('é'))
        );
    }

    #[test]
    fn dead_key_then_letter_types_the_composed_character() {
        // US-international / German: dead acute (no text), then e → "é".
        assert_eq!(
            command_for(Key::Dead(Some('´')), None, PhysicalKey::Code(KeyCode::Equal), false, false),
            None,
            "a bare dead key types nothing"
        );
        assert_eq!(
            command_for(Key::Character("é"), Some("é"), PhysicalKey::Code(KeyCode::KeyE), false, false),
            Some(KeyInput::Insert("é".into()))
        );
        // Platforms that leave the logical key un-composed (macOS) still type the text.
        assert_eq!(
            command_for(Key::Character("e"), Some("é"), PhysicalKey::Code(KeyCode::KeyE), false, false),
            Some(KeyInput::Insert("é".into()))
        );
        // Dead key then a key it can't combine with: xkb cancels — no logical character
        // and no text for that press.
        assert_eq!(
            command_for(
                Key::Unidentified(NativeKey::Unidentified),
                None,
                PhysicalKey::Code(KeyCode::KeyQ),
                false,
                false
            ),
            None
        );
    }

    #[test]
    fn compose_key_sequence_types_only_the_result() {
        // Compose, ', e → é: the intermediate presses carry no text.
        assert_eq!(command_for(Key::Named(NamedKey::Compose), None, NO_PHYSICAL, false, false), None);
        assert_eq!(
            command_for(Key::Dead(Some('\'')), None, PhysicalKey::Code(KeyCode::Quote), false, false),
            None
        );
        assert_eq!(
            command_for(Key::Character("é"), Some("é"), PhysicalKey::Code(KeyCode::KeyE), false, false),
            Some(KeyInput::Insert("é".into()))
        );
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
        assert_eq!(sk(ch("f")), Some(ShortcutKey::Char('f')));
        // Uppercase is normalized to lowercase.
        assert_eq!(sk(ch("F")), Some(ShortcutKey::Char('f')));
        assert_eq!(sk(Key::Named(NamedKey::Tab)), Some(ShortcutKey::Tab));
        assert_eq!(sk(Key::Named(NamedKey::F12)), Some(ShortcutKey::F(12)));
        assert_eq!(sk(Key::Named(NamedKey::Enter)), Some(ShortcutKey::Enter));
        // A multi-char logical key is not a shortcut token.
        assert_eq!(sk(ch("ab")), None);
    }

    // ----- file_drop_event: winit OS file drag → file_drop -------------------

    #[test]
    fn winit_file_drag_events_route_to_file_drop() {
        use std::path::PathBuf;

        use pebbles_foundation::Offset;
        use pebbles_widgets::file_drop;
        use winit::event::WindowEvent;

        use super::file_drop_event;

        let at = Offset::new(10.0, 10.0);
        let a = PathBuf::from("/home/u/a.txt");
        // One HoveredFile per file; the second file of the same drag is still news.
        assert_eq!(file_drop_event(&WindowEvent::HoveredFile(a.clone()), at), Some(true));
        assert_eq!(file_drop_event(&WindowEvent::HoveredFile("/home/u/b.txt".into()), at), Some(true));
        assert_eq!(file_drop::hovered_paths().len(), 2);
        // Leaving the window cancels the drag.
        assert_eq!(file_drop_event(&WindowEvent::HoveredFileCancelled, at), Some(true));
        assert!(file_drop::hovered_paths().is_empty());
        // A drop is queued (no repaint yet) and delivered by the about_to_wait flush.
        assert_eq!(file_drop_event(&WindowEvent::HoveredFile(a.clone()), at), Some(true));
        assert_eq!(file_drop_event(&WindowEvent::DroppedFile(a), at), Some(false));
        assert!(file_drop::flush());
        assert!(file_drop::hovered_paths().is_empty());
        // Anything else is not a file-drag event.
        assert_eq!(file_drop_event(&WindowEvent::Focused(true), at), None);
    }

    // ----- cursor_update: OS cursor icon under pointer capture ---------------

    #[test]
    fn cursor_follows_the_widget_under_the_pointer() {
        use pebbles_render::Cursor;

        use super::cursor_update;
        assert_eq!(cursor_update(Cursor::Default, Some(Cursor::Text), false), Some(Cursor::Text));
        assert_eq!(cursor_update(Cursor::Text, Some(Cursor::Pointer), false), Some(Cursor::Pointer));
        assert_eq!(cursor_update(Cursor::Pointer, None, false), Some(Cursor::Default));
        // Unchanged → no OS call.
        assert_eq!(cursor_update(Cursor::ColResize, Some(Cursor::ColResize), false), None);
    }

    #[test]
    fn captured_drag_keeps_its_cursor() {
        use pebbles_render::Cursor;

        use super::cursor_update;
        // Dragging a resize handle: the pointer slides off the 8 px handle onto a
        // button or plain background — the resize arrow stays until release.
        assert_eq!(cursor_update(Cursor::ColResize, None, true), None);
        assert_eq!(cursor_update(Cursor::ColResize, Some(Cursor::Pointer), true), None);
        // After release (not captured) it catches up with what is under the pointer.
        assert_eq!(cursor_update(Cursor::ColResize, None, false), Some(Cursor::Default));
    }

    #[test]
    fn every_pebbles_cursor_has_a_distinct_winit_icon() {
        use pebbles_render::Cursor;
        use winit::window::CursorIcon;

        use super::to_winit_cursor;
        let all = [
            (Cursor::Default, CursorIcon::Default),
            (Cursor::Pointer, CursorIcon::Pointer),
            (Cursor::Text, CursorIcon::Text),
            (Cursor::Grab, CursorIcon::Grab),
            (Cursor::Grabbing, CursorIcon::Grabbing),
            (Cursor::ColResize, CursorIcon::ColResize),
            (Cursor::RowResize, CursorIcon::RowResize),
            (Cursor::NotAllowed, CursorIcon::NotAllowed),
        ];
        for (ours, theirs) in all {
            assert_eq!(to_winit_cursor(ours), theirs);
        }
    }
}
