//! App-level keyboard shortcuts (B2): the reactive hook [`create_shortcut`] plus
//! the parse grammar and the dispatch registry the shell drives.
//!
//! # Grammar
//! `[Ctrl+|Shift+|Alt+|Meta+|Mod+]* key` — `Mod` = Ctrl on Windows/Linux, Meta
//! (Command) on macOS, resolved at parse time. Key tokens: single characters,
//! `F1..F12`, `Enter`, `Escape`, `Space`, `Tab`, `Arrow{Up,Down,Left,Right}`,
//! `Home`, `End`, `Delete`, `Backspace`, `PageUp`, `PageDown`, `Insert`.
//!
//! # Dispatch precedence (the shell honors this order per key press)
//! 1. window chrome (Escape → dialog/sheet dismiss)
//! 2. a **focused editor** for its editing intents (typing, arrows,
//!    Ctrl+A/C/X/V/Z/Y, Home/End, Enter, Backspace/Delete) — bindings can't
//!    break typing
//! 3. this registry ([`dispatch`]) — most-recently-registered wins, so a
//!    dialog's bindings naturally shadow the page
//! 4. Tab focus-move / Enter-Space activation

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::reactive::{create_cleanup, current_window};

/// A platform-neutral key token a shortcut can bind to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutKey {
    /// A single printable character (lowercased).
    Char(char),
    /// A function key `F1`..=`F12`.
    F(u8),
    Enter,
    Escape,
    Space,
    Tab,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    Delete,
    Backspace,
    PageUp,
    PageDown,
    Insert,
}

/// The modifier set a shortcut requires (all listed must be held; unlisted must
/// be up).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

/// A parsed binding: (modifiers, key).
type Binding = (Mods, ShortcutKey);

/// What a registration matches on: the layout-aware logical key (the default) or the
/// layout-independent physical key position.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutTrigger {
    /// Match the character / named key the active layout produces ([`ShortcutKey`]).
    Logical(ShortcutKey),
    /// Match the key at this W3C `code` position ([`PhysicalKey`](crate::PhysicalKey)),
    /// whatever the layout prints on it.
    Physical(String),
}

/// A shortcut binding for [`create_shortcut`] / [`create_shortcut_if`] (Flutter's
/// `SingleActivator`): modifiers + a key, matched **logically** by default. A plain
/// string converts into one, so `create_shortcut("Mod+K", ..)` is the common form.
///
/// [`Shortcut::physical`] matches the key **position** instead (Flutter's
/// `PhysicalKeyboardKey`), for bindings that must stay on the same key on every layout —
/// `` Ctrl+Shift+` `` (the terminal toggle) sits under Esc on US, German, French, …
/// keyboards even though those layouts print `` ` ``, `^`, `²` there:
///
/// ```no_run
/// use pebbles_core::{create_shortcut, shortcuts::Shortcut};
/// create_shortcut(Shortcut::physical("Ctrl+Shift+`"), move || { /* toggle the terminal */ });
/// create_shortcut(Shortcut::physical("Ctrl+Shift+Backquote"), move || {}); // same binding
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut(Result<(Mods, ShortcutTrigger), String>);

impl Shortcut {
    /// A logical binding (the default; same as passing the string).
    pub fn logical(binding: &str) -> Self {
        Shortcut(parse(binding).map(|(m, k)| (m, ShortcutTrigger::Logical(k))))
    }

    /// A physical binding: the modifiers as in [`parse`], then a key POSITION — a W3C
    /// `code` name (`Backquote`, `KeyK`, `Digit1`, `BracketLeft`, `F5`, `NumpadEnter`, …;
    /// case-sensitive) or the character / key name printed there on the US layout
    /// (`` ` ``, `k`, `1`, `[`, `F5`, `Enter`, `PageUp`, …).
    pub fn physical(binding: &str) -> Self {
        Shortcut(parse_physical(binding))
    }

    /// The parsed `(mods, trigger)`, or the grammar error.
    pub fn parsed(&self) -> Result<&(Mods, ShortcutTrigger), &str> {
        self.0.as_ref().map_err(String::as_str)
    }
}

impl From<&str> for Shortcut {
    fn from(binding: &str) -> Self {
        Shortcut::logical(binding)
    }
}
impl From<&String> for Shortcut {
    fn from(binding: &String) -> Self {
        Shortcut::logical(binding)
    }
}
impl From<String> for Shortcut {
    fn from(binding: String) -> Self {
        Shortcut::logical(&binding)
    }
}

/// Split a binding into its modifiers and the raw key token.
fn split_mods(binding: &str) -> Result<(Mods, &str), String> {
    let mut mods = Mods::default();
    let mut rest = binding;
    loop {
        let (head, tail) = match rest.find('+') {
            Some(i) => (&rest[..i], &rest[i + 1..]),
            None => (rest, ""),
        };
        match head.to_ascii_uppercase().as_str() {
            "CTRL" | "CONTROL" => mods.ctrl = true,
            "SHIFT" => mods.shift = true,
            "ALT" => mods.alt = true,
            "META" | "SUPER" | "CMD" | "COMMAND" | "WIN" => mods.meta = true,
            "MOD" => {
                if cfg!(target_os = "macos") {
                    mods.meta = true;
                } else {
                    mods.ctrl = true;
                }
            }
            "" => return Err(format!("empty token in binding {binding:?}")),
            _ => {
                if !tail.is_empty() {
                    return Err(format!("binding {binding:?} has trailing tokens after the key"));
                }
                return Ok((mods, head));
            }
        }
        if tail.is_empty() {
            return Err(format!("binding {binding:?} ends with a modifier"));
        }
        rest = tail;
    }
}

/// Parse a [`Shortcut::physical`] binding to `(mods, Physical(code))`.
fn parse_physical(binding: &str) -> Result<(Mods, ShortcutTrigger), String> {
    let (mods, token) = split_mods(binding)?;
    Ok((mods, ShortcutTrigger::Physical(physical_code(token)?)))
}

/// The W3C `code` for a physical key token: a US-layout character / key name, or a code
/// name verbatim.
fn physical_code(token: &str) -> Result<String, String> {
    let mut chars = token.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        let code = match c.to_ascii_lowercase() {
            l @ 'a'..='z' => format!("Key{}", l.to_ascii_uppercase()),
            d @ '0'..='9' => format!("Digit{d}"),
            '`' | '~' => "Backquote".into(),
            '-' | '_' => "Minus".into(),
            '=' => "Equal".into(),
            '[' | '{' => "BracketLeft".into(),
            ']' | '}' => "BracketRight".into(),
            '\\' | '|' => "Backslash".into(),
            ';' | ':' => "Semicolon".into(),
            '\'' | '"' => "Quote".into(),
            ',' | '<' => "Comma".into(),
            '.' | '>' => "Period".into(),
            '/' | '?' => "Slash".into(),
            _ => return Err(format!("no US-layout key prints {token:?}; use its W3C code name")),
        };
        return Ok(code);
    }
    // Named keys of the logical grammar map to their W3C codes.
    if let Ok(key) = key_token(token) {
        return Ok(key_code(key));
    }
    if !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Ok(token.to_string());
    }
    Err(format!("unknown physical key {token:?}"))
}

struct Entry {
    /// Serial for exact removal on unmount (Rc pointers don't compare).
    id: u64,
    /// The registering component instance (`owner_id()`), when hook-registered.
    /// Re-registering the same binding from the same instance OVERWRITES in
    /// place instead of stacking — a component that re-renders every frame
    /// neither leaks entries nor shifts dispatch order.
    owner: Option<u64>,
    /// Returns whether the press was consumed; `false` falls through to the
    /// next (older) registration, then to the shell's scroll fallback.
    cb: Rc<dyn Fn() -> bool>,
}

thread_local! {
    static REGISTRY: RefCell<HashMap<(u32, Mods, ShortcutTrigger), Vec<Entry>>> =
        RefCell::new(HashMap::new());
    static NEXT: Cell<u64> = const { Cell::new(0) };
}

/// Parse a shortcut binding string (see the module grammar). Returns the
/// normalized `(Mods, ShortcutKey)` or an error describing the offending token.
pub fn parse(binding: &str) -> Result<Binding, String> {
    let (mods, token) = split_mods(binding)?;
    Ok((mods, key_token(token)?))
}

/// Map the last (key) token of a binding to a [`ShortcutKey`].
fn key_token(token: &str) -> Result<ShortcutKey, String> {
    let upper = token.to_ascii_uppercase();
    // F1..F12
    if upper.len() >= 2 && upper.starts_with('F') {
        if let Ok(n) = upper[1..].parse::<u8>()
            && (1..=12).contains(&n)
        {
            return Ok(ShortcutKey::F(n));
        }
        return Err(format!("unsupported function key {token:?}"));
    }
    match upper.as_str() {
        "ENTER" | "RETURN" => Ok(ShortcutKey::Enter),
        "ESCAPE" | "ESC" => Ok(ShortcutKey::Escape),
        "SPACE" => Ok(ShortcutKey::Space),
        "TAB" => Ok(ShortcutKey::Tab),
        "ARROWUP" | "UP" => Ok(ShortcutKey::ArrowUp),
        "ARROWDOWN" | "DOWN" => Ok(ShortcutKey::ArrowDown),
        "ARROWLEFT" | "LEFT" => Ok(ShortcutKey::ArrowLeft),
        "ARROWRIGHT" | "RIGHT" => Ok(ShortcutKey::ArrowRight),
        "HOME" => Ok(ShortcutKey::Home),
        "END" => Ok(ShortcutKey::End),
        "DELETE" | "DEL" => Ok(ShortcutKey::Delete),
        "BACKSPACE" => Ok(ShortcutKey::Backspace),
        "PAGEUP" | "PGUP" => Ok(ShortcutKey::PageUp),
        "PAGEDOWN" | "PGDN" => Ok(ShortcutKey::PageDown),
        "INSERT" | "INS" => Ok(ShortcutKey::Insert),
        _ => {
            let mut chars = token.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => Ok(ShortcutKey::Char(c.to_ascii_lowercase())),
                _ => Err(format!("unknown shortcut key {token:?}")),
            }
        }
    }
}

/// A component hook: register `binding` in the current window. Fires `cb` on
/// every press while the component is mounted; unregisters automatically on
/// unmount.
///
/// # Panics
/// Panics at first render on an unparsable `binding` (e.g. `"Mod+"` or an unknown key) — the
/// binding is a compile-time constant in practice, so this surfaces the typo loudly rather
/// than silently never firing. Keep bindings as string literals.
///
/// ```no_run
/// use pebbles_core::create_shortcut;
/// create_shortcut("Mod+K", move || { /* open the command palette */ });
/// ```
pub fn create_shortcut(binding: impl Into<Shortcut>, cb: impl Fn() + 'static) {
    register(binding, move || {
        cb();
        true
    });
}

/// [`create_shortcut`] with a *conditional* handler: return `true` to consume
/// the press, `false` to decline it — dispatch then falls through to the next
/// (older) registration for the same binding, and finally to the shell's
/// key-scroll fallback. Use it for bindings that should only act while a widget
/// is "engaged" (e.g. the file explorer's arrows act only while it has a
/// selection, so they never hijack page scrolling).
///
/// # Panics
/// Same as [`create_shortcut`] — panics at first render on an unparsable `binding`.
pub fn create_shortcut_if(binding: impl Into<Shortcut>, cb: impl Fn() -> bool + 'static) {
    register(binding, cb);
}

/// The shared registrar. Keyed by the calling component instance: a re-render
/// overwrites its own previous registration in place (same dispatch position,
/// no leak — cleanups only run at unmount, so per-render pushes would stack).
fn register(binding: impl Into<Shortcut>, cb: impl Fn() -> bool + 'static) {
    let (mods, key) = binding.into().0.expect("create_shortcut: invalid binding grammar");
    let window = current_window();
    let owner = crate::reactive::owner_id();
    let id = NEXT.with(|n| {
        let v = n.get().wrapping_add(1);
        n.set(v);
        v
    });
    let cb: Rc<dyn Fn() -> bool> = Rc::new(cb);
    REGISTRY.with(|r| {
        let mut r = r.borrow_mut();
        let list = r.entry((window, mods, key.clone())).or_default();
        match owner.and_then(|o| list.iter_mut().find(|e| e.owner == Some(o))) {
            // Same component re-rendering: swap the callback in place.
            Some(existing) => existing.cb = cb,
            None => list.push(Entry { id, owner, cb }),
        }
    });
    create_cleanup(move || {
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            let key = (window, mods, key.clone());
            if let Some(list) = r.get_mut(&key) {
                match owner {
                    Some(o) => list.retain(|e| e.owner != Some(o)),
                    None => list.retain(|e| e.id != id),
                }
                if list.is_empty() {
                    r.remove(&key);
                }
            }
        });
    });
}

/// Dispatch a press to `window`'s registry. Registrations are tried newest →
/// oldest until one CONSUMES the press (plain [`create_shortcut`] handlers
/// always consume; [`create_shortcut_if`] handlers may decline). Returns
/// whether anything consumed it.
pub fn dispatch(window: u32, mods: Mods, key: ShortcutKey) -> bool {
    dispatch_trigger(window, mods, ShortcutTrigger::Logical(key))
}

/// [`dispatch`] for [`Shortcut::physical`] registrations: the press of the key at W3C
/// `code` position `physical` under `mods`.
pub fn dispatch_physical(window: u32, mods: Mods, physical: &crate::PhysicalKey) -> bool {
    dispatch_trigger(window, mods, ShortcutTrigger::Physical(physical.code().to_string()))
}

fn dispatch_trigger(window: u32, mods: Mods, trigger: ShortcutTrigger) -> bool {
    let cbs: Vec<Rc<dyn Fn() -> bool>> = REGISTRY.with(|r| {
        r.borrow()
            .get(&(window, mods, trigger))
            .map(|list| list.iter().rev().map(|e| e.cb.clone()).collect())
            .unwrap_or_default()
    });
    cbs.into_iter().any(|cb| cb())
}

/// Dispatch a raw key press to `window`'s registry under the event's modifiers:
/// logical registrations first (by its [`ShortcutKey`](crate::keyboard::KeyEvent::shortcut_key)),
/// then [`Shortcut::physical`] ones (by its physical key). Releases never match.
/// Returns whether a registration consumed it.
pub fn dispatch_key_event(window: u32, event: &crate::keyboard::KeyEvent) -> bool {
    if !event.is_press() {
        return false;
    }
    event.shortcut_key().is_some_and(|sk| dispatch(window, event.mods, sk))
        || dispatch_physical(window, event.mods, &event.physical)
}

/// Number of live shortcut registrations (debug-only observability).
#[cfg(debug_assertions)]
pub fn census_shortcuts() -> usize {
    REGISTRY.with(|r| r.borrow().values().map(|v| v.len()).sum())
}

// ---------------------------------------------------------------------------
// Accelerator strings (shared by B3 native menus + B4 global hotkeys)
// ---------------------------------------------------------------------------

/// Render one [`ShortcutKey`] as its `keyboard-types::Code` name — the key half of
/// a Tauri-style accelerator (`KeyA`, `Digit1`, `F5`, `ArrowUp`, `Enter`, …).
fn key_code(key: ShortcutKey) -> String {
    use ShortcutKey::*;
    match key {
        Char(c) if c.is_ascii_alphabetic() => format!("Key{}", c.to_ascii_uppercase()),
        Char(c) if c.is_ascii_digit() => format!("Digit{c}"),
        // Non-alphanumeric single chars have no clean Code name; pass the upper form
        // through and let the consumer reject it if it can't map it.
        Char(c) => c.to_ascii_uppercase().to_string(),
        F(n) => format!("F{n}"),
        Enter => "Enter".to_string(),
        Escape => "Escape".to_string(),
        Space => "Space".to_string(),
        Tab => "Tab".to_string(),
        ArrowUp => "ArrowUp".to_string(),
        ArrowDown => "ArrowDown".to_string(),
        ArrowLeft => "ArrowLeft".to_string(),
        ArrowRight => "ArrowRight".to_string(),
        Home => "Home".to_string(),
        End => "End".to_string(),
        Delete => "Delete".to_string(),
        Backspace => "Backspace".to_string(),
        PageUp => "PageUp".to_string(),
        PageDown => "PageDown".to_string(),
        Insert => "Insert".to_string(),
    }
}

/// Format a parsed binding as a Tauri-style accelerator string — the shared
/// grammar both the native menu bar (B3, `muda`) and global hotkeys (B4,
/// `global-hotkey`) parse via `keyboard-types`. Modifiers come first in a stable
/// order (`Control+Shift+Alt+Super`), then the key code.
///
/// Pure and platform-neutral: `mods` is already `Mod`-resolved by [`parse`], so
/// this never inspects `cfg`.
pub fn accelerator_string(mods: Mods, key: ShortcutKey) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if mods.ctrl {
        parts.push("Control");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.meta {
        parts.push("Super");
    }
    let code = key_code(key);
    if parts.is_empty() { code } else { format!("{}+{}", parts.join("+"), code) }
}

/// Parse a binding string and render it as an [`accelerator_string`] in one step
/// — the exact form `muda` / `global-hotkey` accept via their `FromStr`. Returns
/// the same parse error as [`parse`].
///
/// ```no_run
/// use pebbles_core::shortcuts::to_accelerator;
/// let accel = to_accelerator("Mod+S").unwrap(); // "Control+KeyS" on Win/Linux, "Super+KeyS" on macOS
/// ```
pub fn to_accelerator(binding: &str) -> Result<String, String> {
    let (mods, key) = parse(binding)?;
    Ok(accelerator_string(mods, key))
}

#[cfg(test)]
mod physical_tests {
    use super::*;

    fn phys(b: &str) -> (Mods, ShortcutTrigger) {
        Shortcut::physical(b).parsed().cloned().unwrap()
    }

    #[test]
    fn physical_tokens_resolve_to_w3c_codes() {
        let cs = Mods { ctrl: true, shift: true, ..Default::default() };
        assert_eq!(phys("Ctrl+Shift+`"), (cs, ShortcutTrigger::Physical("Backquote".into())));
        assert_eq!(phys("Ctrl+Shift+Backquote"), phys("Ctrl+Shift+`"));
        assert_eq!(phys("Alt+k").1, ShortcutTrigger::Physical("KeyK".into()));
        assert_eq!(phys("1").1, ShortcutTrigger::Physical("Digit1".into()));
        assert_eq!(phys("[").1, ShortcutTrigger::Physical("BracketLeft".into()));
        assert_eq!(phys("F5").1, ShortcutTrigger::Physical("F5".into()));
        assert_eq!(phys("PageUp").1, ShortcutTrigger::Physical("PageUp".into()));
        assert_eq!(phys("Esc").1, ShortcutTrigger::Physical("Escape".into()));
        assert_eq!(phys("NumpadEnter").1, ShortcutTrigger::Physical("NumpadEnter".into()));
        assert!(Shortcut::physical("Ctrl+é").parsed().is_err());
        assert!(Shortcut::physical("Ctrl+").parsed().is_err());
    }

    #[test]
    fn strings_stay_logical() {
        let s: Shortcut = "Ctrl+K".into();
        assert_eq!(
            s.parsed().cloned().unwrap(),
            (Mods { ctrl: true, ..Default::default() }, ShortcutTrigger::Logical(ShortcutKey::Char('k')))
        );
        assert_eq!(parse("Ctrl+PageDown").unwrap().1, ShortcutKey::PageDown);
    }
}
