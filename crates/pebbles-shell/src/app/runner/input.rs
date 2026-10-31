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
pub(super) fn wheel_with_overlay(ui: &mut Ui, cursor: Offset, delta: Offset) -> bool {
    use pebbles_widgets::overlay;
    let dy = delta.y;
    if !overlay::is_open() {
        return ui.dispatch_wheel(cursor, delta);
    }
    if overlay::over_panel(cursor.x, cursor.y) {
        // Wheel over the popover itself → scroll its own content only.
        ui.dispatch_wheel(cursor, delta)
    } else if ui.dispatch_wheel(cursor, delta) {
        // Wheel over the page behind the popover → slide the popover with it.
        overlay::shift(0.0, -dy);
        true
    } else {
        // Nowhere to scroll → dismiss so it never floats detached.
        overlay::hide_overlay();
        true
    }
}

/// The winit mouse button as a Pebbles [`PointerButton`](pebbles_render::PointerButton)
/// (`None` for Back/Forward/other buttons, which drive router history instead).
pub(super) fn pointer_button(button: MouseButton) -> Option<pebbles_render::PointerButton> {
    use pebbles_render::PointerButton as P;
    match button {
        MouseButton::Left => Some(P::Primary),
        MouseButton::Right => Some(P::Secondary),
        MouseButton::Middle => Some(P::Middle),
        _ => None,
    }
}

/// A winit wheel delta as a [`PointerScrollEvent`](pebbles_render::PointerScrollEvent) in
/// Flutter's `scrollDelta` sign (+y scrolls down): line deltas are converted at
/// [`LINE_SCROLL`] px per line, pixel deltas from physical to logical pixels.
pub(super) fn scroll_signal(
    delta: MouseScrollDelta,
    scale: f64,
    at: Offset,
) -> pebbles_render::PointerScrollEvent {
    use pebbles_render::{PointerScrollEvent, ScrollUnit};
    let (delta, unit) = match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            let lines = Offset::new(-f64::from(x), -f64::from(y));
            (Offset::new(lines.x * LINE_SCROLL, lines.y * LINE_SCROLL), ScrollUnit::Lines(lines))
        }
        MouseScrollDelta::PixelDelta(p) => (Offset::new(-p.x / scale, -p.y / scale), ScrollUnit::Pixels),
    };
    PointerScrollEvent { position: at, global: at, delta, unit }
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
        Cursor::NsResize => CursorIcon::NsResize,
        Cursor::EwResize => CursorIcon::EwResize,
        Cursor::NwseResize => CursorIcon::NwseResize,
        Cursor::NeswResize => CursorIcon::NeswResize,
    }
}

/// Translate a winit key press (+ Ctrl/Shift) into an editing command, or `None`
/// if it isn't one the focused editor cares about. Shift extends selections; Ctrl
/// switches arrows to word motion and enables the clipboard/undo/select-all set.
///
/// A thin wrapper over [`command_for`], which converts to the platform-neutral key
/// types and applies [`pebbles_core::keyboard::intent_for`] (the single mapping the
/// headless test harness uses too).
pub(super) fn to_command(event: &KeyEvent, ctrl: bool, shift: bool) -> Option<KeyInput> {
    command_for(event.logical_key.as_ref(), event.text.as_deref(), event.physical_key, ctrl, shift)
}

/// A winit logical key as the platform-neutral [`LogicalKey`](pebbles_core::LogicalKey).
pub(super) fn logical_key(key: Key<&str>) -> pebbles_core::LogicalKey {
    use pebbles_core::LogicalKey as L;
    use pebbles_core::keyboard::NamedKey as N;
    match key {
        Key::Character(s) => L::Character(s.to_string()),
        Key::Dead(c) => L::Dead(c),
        Key::Unidentified(_) => L::Unidentified,
        Key::Named(n) => L::Named(match n {
            NamedKey::Enter => N::Enter,
            NamedKey::Tab => N::Tab,
            NamedKey::Space => N::Space,
            NamedKey::Backspace => N::Backspace,
            NamedKey::Delete => N::Delete,
            NamedKey::Insert => N::Insert,
            NamedKey::Escape => N::Escape,
            NamedKey::Home => N::Home,
            NamedKey::End => N::End,
            NamedKey::PageUp => N::PageUp,
            NamedKey::PageDown => N::PageDown,
            NamedKey::ArrowUp => N::ArrowUp,
            NamedKey::ArrowDown => N::ArrowDown,
            NamedKey::ArrowLeft => N::ArrowLeft,
            NamedKey::ArrowRight => N::ArrowRight,
            NamedKey::Shift => N::Shift,
            NamedKey::Control => N::Control,
            NamedKey::Alt | NamedKey::AltGraph => N::Alt,
            NamedKey::Super | NamedKey::Meta => N::Meta,
            NamedKey::CapsLock => N::CapsLock,
            NamedKey::NumLock => N::NumLock,
            NamedKey::ScrollLock => N::ScrollLock,
            NamedKey::PrintScreen => N::PrintScreen,
            NamedKey::Pause => N::Pause,
            NamedKey::ContextMenu => N::ContextMenu,
            NamedKey::Compose => N::Compose,
            other => function_key(other).map_or(N::Other, N::F),
        }),
    }
}

/// `F1`..=`F35` → its number.
fn function_key(key: NamedKey) -> Option<u8> {
    const KEYS: [NamedKey; 35] = [
        NamedKey::F1,
        NamedKey::F2,
        NamedKey::F3,
        NamedKey::F4,
        NamedKey::F5,
        NamedKey::F6,
        NamedKey::F7,
        NamedKey::F8,
        NamedKey::F9,
        NamedKey::F10,
        NamedKey::F11,
        NamedKey::F12,
        NamedKey::F13,
        NamedKey::F14,
        NamedKey::F15,
        NamedKey::F16,
        NamedKey::F17,
        NamedKey::F18,
        NamedKey::F19,
        NamedKey::F20,
        NamedKey::F21,
        NamedKey::F22,
        NamedKey::F23,
        NamedKey::F24,
        NamedKey::F25,
        NamedKey::F26,
        NamedKey::F27,
        NamedKey::F28,
        NamedKey::F29,
        NamedKey::F30,
        NamedKey::F31,
        NamedKey::F32,
        NamedKey::F33,
        NamedKey::F34,
        NamedKey::F35,
    ];
    KEYS.iter().position(|&k| k == key).map(|i| i as u8 + 1)
}

/// A winit physical key as its W3C `code` name ([`pebbles_core::PhysicalKey`]). winit's
/// `KeyCode` variants are named after the W3C codes, so `Debug` is the code.
pub(super) fn physical_key(physical: PhysicalKey) -> pebbles_core::PhysicalKey {
    match physical {
        PhysicalKey::Code(code) => pebbles_core::PhysicalKey::new(format!("{code:?}")),
        PhysicalKey::Unidentified(_) => pebbles_core::PhysicalKey::unidentified(),
    }
}

/// The platform-neutral raw [`KeyEvent`](pebbles_core::KeyEvent) for a winit key event,
/// offered to the focused widget's `on_key_event` handler before intents and shortcuts.
pub(super) fn to_key_event(event: &KeyEvent, mods: pebbles_core::Mods) -> pebbles_core::KeyEvent {
    use pebbles_core::KeyEventKind;
    let kind = match (event.state, event.repeat) {
        (ElementState::Released, _) => KeyEventKind::Up,
        (ElementState::Pressed, true) => KeyEventKind::Repeat,
        (ElementState::Pressed, false) => KeyEventKind::Down,
    };
    pebbles_core::KeyEvent {
        kind,
        logical: logical_key(event.logical_key.as_ref()),
        physical: physical_key(event.physical_key),
        text: if kind == KeyEventKind::Up { None } else { event.text.as_ref().map(|t| t.to_string()) },
        mods,
    }
}

/// The pure key → editing-command mapping behind [`to_command`]. Kept free of the
/// winit `KeyEvent` wrapper (whose `platform_specific` field is `pub(crate)` and so
/// can't be built in a test) — it takes the event's borrowed logical [`Key`], its
/// `text`, its physical key and the Ctrl/Shift flags, which ARE constructible
/// headlessly.
fn command_for(
    key: Key<&str>,
    text: Option<&str>,
    physical: PhysicalKey,
    ctrl: bool,
    shift: bool,
) -> Option<KeyInput> {
    pebbles_core::keyboard::intent_for(&logical_key(key), text, &physical_key(physical), ctrl, shift)
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
    pebbles_core::keyboard::shortcut_key_for(&logical_key(key), &physical_key(physical))
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

    // ----- raw key events: winit → platform-neutral KeyEvent ------------------

    #[test]
    fn raw_keys_beyond_the_intent_set_convert() {
        use pebbles_core::LogicalKey as L;
        use pebbles_core::keyboard::NamedKey as N;

        use super::{logical_key, physical_key};
        // Keys with no editing intent still reach a raw handler with their identity.
        assert_eq!(logical_key(Key::Named(NamedKey::PageUp)), L::Named(N::PageUp));
        assert_eq!(logical_key(Key::Named(NamedKey::PageDown)), L::Named(N::PageDown));
        assert_eq!(logical_key(Key::Named(NamedKey::Insert)), L::Named(N::Insert));
        assert_eq!(logical_key(Key::Named(NamedKey::F5)), L::Named(N::F(5)));
        assert_eq!(logical_key(Key::Named(NamedKey::F24)), L::Named(N::F(24)));
        assert_eq!(logical_key(Key::Named(NamedKey::Control)), L::Named(N::Control));
        assert_eq!(logical_key(ch("é")), L::Character("é".into()));
        // Physical keys are W3C `code` names.
        assert_eq!(physical_key(PhysicalKey::Code(KeyCode::Backquote)).code(), "Backquote");
        assert_eq!(physical_key(PhysicalKey::Code(KeyCode::KeyA)).code(), "KeyA");
        assert_eq!(physical_key(PhysicalKey::Code(KeyCode::PageUp)).code(), "PageUp");
        assert_eq!(physical_key(NO_PHYSICAL).code(), "Unidentified");
        // PageUp is now a shortcut token too.
        assert_eq!(sk(Key::Named(NamedKey::PageUp)), Some(ShortcutKey::PageUp));
    }

    // ----- wheel → pointer signal -------------------------------------------

    #[test]
    fn wheel_deltas_become_scroll_signals_in_flutter_sign() {
        use pebbles_foundation::Offset;
        use pebbles_render::ScrollUnit;
        use winit::dpi::PhysicalPosition;
        use winit::event::MouseScrollDelta;

        use super::{LINE_SCROLL, scroll_signal};
        let at = Offset::new(5.0, 6.0);
        // A notch toward the user (winit +y = up) scrolls up: negative delta.
        let s = scroll_signal(MouseScrollDelta::LineDelta(0.0, 1.0), 2.0, at);
        assert_eq!(s.delta, Offset::new(0.0, -LINE_SCROLL));
        assert_eq!(s.unit, ScrollUnit::Lines(Offset::new(0.0, -1.0)));
        assert_eq!(s.global, at);
        // Pixel deltas are physical: divided by the scale factor.
        let s = scroll_signal(MouseScrollDelta::PixelDelta(PhysicalPosition::new(4.0, -20.0)), 2.0, at);
        assert_eq!(s.delta, Offset::new(-2.0, 10.0));
        assert_eq!(s.unit, ScrollUnit::Pixels);
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
            (Cursor::NsResize, CursorIcon::NsResize),
            (Cursor::EwResize, CursorIcon::EwResize),
            (Cursor::NwseResize, CursorIcon::NwseResize),
            (Cursor::NeswResize, CursorIcon::NeswResize),
        ];
        for (ours, theirs) in all {
            assert_eq!(to_winit_cursor(ours), theirs);
        }
    }
}
