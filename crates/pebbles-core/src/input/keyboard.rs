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
    Move {
        motion: Motion,
        extend: bool,
    },
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
    /// Tab in a focused code editor: indent the current line / selected lines (the shell
    /// only sends this to an editor; elsewhere Tab traverses focus).
    Indent,
    /// Shift+Tab in a focused code editor: outdent the current line / selected lines.
    Outdent,
    /// Add a cursor at the next occurrence of the current selection — or select the word
    /// under the caret when there's no selection yet (Ctrl+D / Cmd+D). The multi-cursor
    /// "add selection to next match" command.
    SelectNextOccurrence,
    /// Toggle line comments on the selected lines / caret line (Ctrl+/ / Cmd+/).
    ToggleComment,
    /// Request the completion popup (Ctrl+Space) — the IntelliSense trigger.
    TriggerCompletion,
    /// Go to the definition of the symbol under the caret (F12).
    GoToDefinition,
    /// Format the document (or selection) via the format provider (Shift+Alt+F).
    Format,
    /// Open the find widget (Ctrl+F).
    Find,
    /// Open the find-and-replace widget (Ctrl+H).
    Replace,
    /// Open the command palette (Ctrl+P / Ctrl+Shift+P) — run any registered command.
    CommandPalette,
}

thread_local! {
    static SHIFT: Cell<bool> = const { Cell::new(false) };
    static CTRL: Cell<bool> = const { Cell::new(false) };
    static ALT: Cell<bool> = const { Cell::new(false) };
    static META: Cell<bool> = const { Cell::new(false) };
}

/// Update the live modifier state (called by the shell on every change).
pub fn set_modifiers(shift: bool, ctrl: bool, alt: bool, meta: bool) {
    SHIFT.with(|s| s.set(shift));
    CTRL.with(|c| c.set(ctrl));
    ALT.with(|a| a.set(alt));
    META.with(|m| m.set(meta));
}

/// Whether Shift is currently held (for Shift-click selection).
pub fn shift_held() -> bool {
    SHIFT.with(Cell::get)
}

/// Whether Ctrl is currently held.
pub fn ctrl_held() -> bool {
    CTRL.with(Cell::get)
}

/// Whether Alt is currently held (B1 — the modifier shortcuts need it).
pub fn alt_held() -> bool {
    ALT.with(Cell::get)
}

/// Whether Meta (Super/Command) is currently held.
pub fn meta_held() -> bool {
    META.with(Cell::get)
}

// ---------------------------------------------------------------------------
// Raw key events (Flutter's `KeyEvent` / `Focus.onKeyEvent`)
// ---------------------------------------------------------------------------

/// A named (non-printing) logical key — Flutter's `LogicalKeyboardKey` constants for
/// keys that produce no character. `Space` is reported here (not as `Character(" ")`),
/// matching the W3C `key` values winit follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum NamedKey {
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Escape,
    Home,
    End,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// A function key `F1`..=`F35`.
    F(u8),
    Shift,
    Control,
    Alt,
    Meta,
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,
    ContextMenu,
    Compose,
    /// Any other named key (media keys, browser keys, …).
    Other,
}

/// The layout-aware meaning of a key (Flutter's `LogicalKeyboardKey`): the character
/// the active layout produces for it, or a [`NamedKey`].
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum LogicalKey {
    /// A printable key: the character(s) the layout produces with the current modifiers.
    Character(String),
    /// A non-printing key.
    Named(NamedKey),
    /// A dead key (the next key composes with it); carries the dead character if known.
    Dead(Option<char>),
    /// The platform could not identify the key.
    Unidentified,
}

impl LogicalKey {
    /// A printable-key logical key.
    pub fn char(s: impl Into<String>) -> Self {
        LogicalKey::Character(s.into())
    }
}

/// The layout-independent position of a key (Flutter's `PhysicalKeyboardKey`), named by
/// its W3C UI Events `code` value: `"KeyA"`, `"Digit1"`, `"Backquote"`, `"BracketLeft"`,
/// `"F5"`, `"PageUp"`, `"Numpad0"`, … — the US-layout position, whatever the layout prints
/// on it. Use it for bindings that must stay on the same key on every layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PhysicalKey(String);

impl PhysicalKey {
    /// The key at W3C `code` (e.g. `PhysicalKey::new("Backquote")`).
    pub fn new(code: impl Into<String>) -> Self {
        PhysicalKey(code.into())
    }

    /// A key the platform could not identify.
    pub fn unidentified() -> Self {
        PhysicalKey(String::from("Unidentified"))
    }

    /// The W3C `code` name.
    pub fn code(&self) -> &str {
        &self.0
    }

    /// The Latin letter printed on this key on the US layout (`KeyA` → `'a'`).
    pub fn us_letter(&self) -> Option<char> {
        let rest = self.0.strip_prefix("Key")?;
        let mut chars = rest.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_uppercase() => Some(c.to_ascii_lowercase()),
            _ => None,
        }
    }
}

/// Whether a [`KeyEvent`] is a press, an OS auto-repeat of a held key, or a release
/// (Flutter's `KeyDownEvent` / `KeyRepeatEvent` / `KeyUpEvent`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyEventKind {
    Down,
    Repeat,
    Up,
}

/// A raw keyboard event, delivered to the focused widget's
/// [`FocusNode::on_key_event`](crate::focus::FocusNode::on_key_event) handler BEFORE the
/// shell turns it into an editing intent ([`KeyInput`]) or an app shortcut — Flutter's
/// `KeyEvent`. Every key reaches it: PageUp/PageDown, Insert, F1–F35, bare modifiers, …
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub kind: KeyEventKind,
    /// The layout-aware key.
    pub logical: LogicalKey,
    /// The layout-independent key position.
    pub physical: PhysicalKey,
    /// The text this press produces (after dead-key / compose processing), if any.
    /// `None` on release and for non-printing keys.
    pub text: Option<String>,
    /// The modifiers held when the event fired.
    pub mods: crate::shortcuts::Mods,
}

impl KeyEvent {
    /// A key-down event with no text and no modifiers (build tests / synthetic input).
    pub fn down(logical: LogicalKey, physical: PhysicalKey) -> Self {
        KeyEvent {
            kind: KeyEventKind::Down,
            logical,
            physical,
            text: None,
            mods: crate::shortcuts::Mods::default(),
        }
    }

    /// Whether this is a press or an auto-repeat (not a release).
    pub fn is_press(&self) -> bool {
        self.kind != KeyEventKind::Up
    }

    /// Whether this is an OS auto-repeat of a held key.
    pub fn is_repeat(&self) -> bool {
        self.kind == KeyEventKind::Repeat
    }

    /// The editing intent this press maps to (what a focused text editor receives when
    /// the raw handler ignores the event). `None` for releases and unmapped keys. The
    /// command modifier is Ctrl, plus ⌘ on macOS.
    pub fn intent(&self) -> Option<KeyInput> {
        if !self.is_press() {
            return None;
        }
        let cmd = self.mods.ctrl || (cfg!(target_os = "macos") && self.mods.meta);
        intent_for(&self.logical, self.text.as_deref(), &self.physical, cmd, self.mods.shift)
    }

    /// The logical shortcut token for this key (see [`crate::shortcuts`]).
    pub fn shortcut_key(&self) -> Option<crate::ShortcutKey> {
        shortcut_key_for(&self.logical, &self.physical)
    }
}

/// What a raw key handler did with an event (Flutter's `KeyEventResult`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyEventResult {
    /// Consumed: no editing intent, shortcut, focus traversal or scroll sees it.
    Handled,
    /// Not consumed: the key continues to the editing-intent mapping, app shortcuts,
    /// Tab traversal, activation and key-scrolling, as if no handler were installed.
    Ignored,
}

/// The character a shortcut matches on, from the layout-aware logical key: the
/// character the user's layout produces (so AZERTY / QWERTZ / Dvorak users press
/// the key labelled Z for Ctrl+Z wherever it sits), lowercased. On a non-Latin
/// layout (Cyrillic, Greek, Hebrew, Arabic, …) the logical key is a non-ASCII
/// letter, and shortcuts fall back to the US letter of the physical key — what
/// Chromium, GTK and VS Code do, so Ctrl+С on a Russian layout still copies.
pub fn shortcut_char(logical: &str, physical: &PhysicalKey) -> Option<char> {
    let mut chars = logical.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return None };
    if c.is_control() {
        return None;
    }
    if c.is_ascii() {
        return Some(c.to_ascii_lowercase());
    }
    if c.is_alphabetic()
        && let Some(latin) = physical.us_letter()
    {
        return Some(latin);
    }
    c.to_lowercase().next()
}

/// The platform-neutral key → editing-intent mapping the shell applies to a press the
/// focused widget's raw handler ignored. `cmd` is the command modifier (Ctrl, or ⌘ on
/// macOS); Shift extends selections; `cmd` switches arrows to word motion and enables
/// the clipboard / undo / select-all set.
///
/// Typed text comes from `text` (the characters the key produced after dead-key /
/// compose processing), falling back to the logical key; a bare dead key types nothing.
pub fn intent_for(
    key: &LogicalKey,
    text: Option<&str>,
    physical: &PhysicalKey,
    cmd: bool,
    shift: bool,
) -> Option<KeyInput> {
    use KeyInput::*;
    use Motion::*;
    let mv = |m: Motion| Some(Move { motion: m, extend: shift });
    match key {
        LogicalKey::Named(NamedKey::Backspace) => Some(if cmd { DeleteWordBack } else { Backspace }),
        LogicalKey::Named(NamedKey::Delete) => Some(if cmd { DeleteWordForward } else { Delete }),
        LogicalKey::Named(NamedKey::ArrowLeft) => mv(if cmd { WordLeft } else { Left }),
        LogicalKey::Named(NamedKey::ArrowRight) => mv(if cmd { WordRight } else { Right }),
        LogicalKey::Named(NamedKey::ArrowUp) => mv(Up),
        LogicalKey::Named(NamedKey::ArrowDown) => mv(Down),
        LogicalKey::Named(NamedKey::Home) => mv(if cmd { DocStart } else { LineStart }),
        LogicalKey::Named(NamedKey::End) => mv(if cmd { DocEnd } else { LineEnd }),
        LogicalKey::Named(NamedKey::Enter) => Some(Enter),
        LogicalKey::Named(NamedKey::Escape) => Some(Escape),
        LogicalKey::Named(NamedKey::F(12)) => Some(GoToDefinition),
        LogicalKey::Named(NamedKey::Space) if cmd => Some(TriggerCompletion),
        LogicalKey::Named(NamedKey::Space) => Some(Insert(" ".to_string())),
        LogicalKey::Character(s) if cmd => match shortcut_char(s, physical)? {
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
        LogicalKey::Character(s) => {
            let typed = text.unwrap_or(s);
            (!typed.is_empty() && typed.chars().all(|ch| !ch.is_control())).then(|| Insert(typed.to_string()))
        }
        _ => None,
    }
}

/// The logical key → [`ShortcutKey`](crate::ShortcutKey) token mapping (`None` for keys
/// outside the shortcut grammar).
pub fn shortcut_key_for(key: &LogicalKey, physical: &PhysicalKey) -> Option<crate::ShortcutKey> {
    use crate::ShortcutKey as SK;
    match key {
        LogicalKey::Named(n) => match n {
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
            NamedKey::PageUp => Some(SK::PageUp),
            NamedKey::PageDown => Some(SK::PageDown),
            NamedKey::Insert => Some(SK::Insert),
            NamedKey::F(n) if (1..=12).contains(n) => Some(SK::F(*n)),
            _ => None,
        },
        LogicalKey::Character(s) => shortcut_char(s, physical).map(SK::Char),
        _ => None,
    }
}

#[cfg(test)]
mod raw_key_tests {
    use super::*;
    use crate::shortcuts::Mods;

    fn press(logical: LogicalKey, code: &str, mods: Mods) -> KeyEvent {
        KeyEvent { mods, ..KeyEvent::down(logical, PhysicalKey::new(code)) }
    }

    #[test]
    fn physical_key_us_letter() {
        assert_eq!(PhysicalKey::new("KeyQ").us_letter(), Some('q'));
        assert_eq!(PhysicalKey::new("Backquote").us_letter(), None);
        assert_eq!(PhysicalKey::new("Keyboard").us_letter(), None);
    }

    #[test]
    fn release_maps_to_no_intent() {
        let mut e = press(LogicalKey::char("a"), "KeyA", Mods::default());
        assert_eq!(e.intent(), Some(KeyInput::Insert("a".into())));
        e.kind = KeyEventKind::Up;
        assert_eq!(e.intent(), None);
    }

    #[test]
    fn page_and_insert_keys_reach_the_shortcut_grammar() {
        let e = press(LogicalKey::Named(NamedKey::PageUp), "PageUp", Mods::default());
        assert_eq!(e.intent(), None, "PageUp is no editing intent");
        assert_eq!(e.shortcut_key(), Some(crate::ShortcutKey::PageUp));
        let e = press(LogicalKey::Named(NamedKey::Insert), "Insert", Mods::default());
        assert_eq!(e.shortcut_key(), Some(crate::ShortcutKey::Insert));
    }

    #[test]
    fn ctrl_shift_p_is_the_palette_and_cyrillic_falls_back_to_physical() {
        let cs = Mods { ctrl: true, shift: true, ..Default::default() };
        assert_eq!(press(LogicalKey::char("P"), "KeyP", cs).intent(), Some(KeyInput::CommandPalette));
        let c = Mods { ctrl: true, ..Default::default() };
        assert_eq!(press(LogicalKey::char("с"), "KeyC", c).intent(), Some(KeyInput::Copy));
    }
}
