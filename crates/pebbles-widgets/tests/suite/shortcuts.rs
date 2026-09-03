//! B2: the shortcut registry — parse grammar (incl. rejects), dispatch,
//! most-recently-registered-wins, and auto-unregister on unmount. The
//! editor-precedence ordering itself lives in the shell's thin dispatch arm
//! (verified by hand in the gallery: Mod+K opens the palette even while a text
//! field is focused, and typing still inserts).

use std::cell::Cell;

use pebbles_core::shortcuts;
use pebbles_core::{IntoWidget, Mods, ShortcutKey, Ui, component, create_shortcut};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, text};

thread_local! {
    static FIRED: Cell<u32> = const { Cell::new(0) };
    static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

#[test]
fn parse_accepts_the_grammar() {
    use ShortcutKey::*;
    let m = |shift, ctrl, alt, meta| Mods { shift, ctrl, alt, meta };
    assert_eq!(shortcuts::parse("k").unwrap(), (m(false, false, false, false), Char('k')));
    assert_eq!(shortcuts::parse("Ctrl+K").unwrap(), (m(false, true, false, false), Char('k')));
    assert_eq!(shortcuts::parse("CTRL+SHIFT+P").unwrap(), (m(true, true, false, false), Char('p')));
    assert_eq!(shortcuts::parse("Alt+F4").unwrap(), (m(false, false, true, false), F(4)));
    assert_eq!(shortcuts::parse("Meta+ArrowUp").unwrap(), (m(false, false, false, true), ArrowUp));
    assert_eq!(shortcuts::parse("Ctrl+Shift+Enter").unwrap(), (m(true, true, false, false), Enter));
    assert_eq!(shortcuts::parse("Escape").unwrap(), (m(false, false, false, false), Escape));
    assert_eq!(shortcuts::parse("F12").unwrap(), (m(false, false, false, false), F(12)));
    // Mod resolves to Ctrl on this platform (non-macOS).
    assert_eq!(shortcuts::parse("Mod+K").unwrap(), (m(false, true, false, false), Char('k')));
}

#[test]
fn accelerator_strings_match_the_tauri_grammar() {
    // The exact strings muda (B3) and global-hotkey (B4) parse via keyboard-types.
    let to = shortcuts::to_accelerator;
    assert_eq!(to("Ctrl+K").unwrap(), "Control+KeyK");
    assert_eq!(to("Ctrl+Shift+P").unwrap(), "Control+Shift+KeyP");
    assert_eq!(to("Alt+F4").unwrap(), "Alt+F4");
    assert_eq!(to("Meta+ArrowUp").unwrap(), "Super+ArrowUp");
    assert_eq!(to("Space").unwrap(), "Space");
    assert_eq!(to("Ctrl+1").unwrap(), "Control+Digit1");
    assert_eq!(to("Ctrl+Shift+Alt+Meta+Enter").unwrap(), "Control+Shift+Alt+Super+Enter");
    // `Mod` resolves at parse time — Meta on macOS, Control elsewhere.
    #[cfg(target_os = "macos")]
    assert_eq!(to("Mod+S").unwrap(), "Super+KeyS");
    #[cfg(not(target_os = "macos"))]
    assert_eq!(to("Mod+S").unwrap(), "Control+KeyS");
    // Parse errors propagate unchanged.
    assert!(to("Ctrl+").is_err());
    assert!(to("Ctrl+F13").is_err());
}

#[test]
fn parse_rejects_nonsense() {
    assert!(shortcuts::parse("Ctrl+").is_err(), "trailing modifier");
    assert!(shortcuts::parse("+K").is_err(), "leading separator");
    assert!(shortcuts::parse("Ctrl+F13").is_err(), "out-of-range function key");
    assert!(shortcuts::parse("Ctrl+Tab+Q").is_err(), "two keys");
    assert!(shortcuts::parse("Ctrl+§§").is_err(), "multi-char key token");
    assert!(shortcuts::parse("Huper+K").is_err(), "unknown modifier");
}

fn shortcut_root() -> impl IntoWidget {
    create_shortcut("Ctrl+K", || FIRED.with(|f| f.set(f.get() + 1)));
    create_shortcut("Ctrl+Shift+S", || LAST.with(|l| *l.borrow_mut() = "save".to_string()));
    text("shortcuts mounted")
}

#[test]
fn registered_shortcuts_fire_until_the_component_unmounts() {
    FIRED.with(|f| f.set(0));
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(shortcut_root)).into_widget());
    ui.layout(&mut env, Size::new(200.0, 100.0));

    let mods = Mods { ctrl: true, ..Default::default() };
    assert!(shortcuts::dispatch(0, mods, ShortcutKey::Char('k')), "Ctrl+K fires");
    assert_eq!(FIRED.with(Cell::get), 1);
    // Exact-match modifiers only: Shift+Ctrl+K is a different binding.
    let mods2 = Mods { shift: true, ctrl: true, ..Default::default() };
    assert!(!shortcuts::dispatch(0, mods2, ShortcutKey::Char('k')), "modifier mismatch doesn't fire");
    assert_eq!(FIRED.with(Cell::get), 1);

    // Unmount → the bindings deregister.
    ui.dispose();
    assert!(!shortcuts::dispatch(0, mods, ShortcutKey::Char('k')), "unmounted binding is gone");
    assert_eq!(FIRED.with(Cell::get), 1);
}

fn shadow_root() -> impl IntoWidget {
    // Two registrations on the same binding: the later one wins.
    create_shortcut("Ctrl+S", || LAST.with(|l| *l.borrow_mut() = "page".to_string()));
    create_shortcut("Ctrl+S", || LAST.with(|l| *l.borrow_mut() = "dialog".to_string()));
    text("shadowed")
}

#[test]
fn most_recently_registered_wins() {
    LAST.with(|l| l.borrow_mut().clear());
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(shadow_root)).into_widget());
    ui.layout(&mut env, Size::new(200.0, 100.0));
    let mods = Mods { ctrl: true, ..Default::default() };
    assert!(shortcuts::dispatch(0, mods, ShortcutKey::Char('s')));
    assert_eq!(LAST.with(|l| l.borrow().clone()), "dialog", "last-registered shadows");
}

// ---------------------------------------------------------------------------
// Conditional shortcuts: decline-to-fall-through + re-render no-stacking
// ---------------------------------------------------------------------------

thread_local! {
    static GATE: Cell<bool> = const { Cell::new(false) };
    static PAGE_HITS: Cell<u32> = const { Cell::new(0) };
    static GATED_HITS: Cell<u32> = const { Cell::new(0) };
    static TICK: std::cell::RefCell<Option<pebbles_core::Signal<u32>>> =
        const { std::cell::RefCell::new(None) };
}

fn page_layer() -> impl IntoWidget {
    // The older, unconditional handler — the fallback when the gated one declines.
    create_shortcut("g", || PAGE_HITS.with(|c| c.set(c.get() + 1)));
    text("page")
}

fn gated_layer() -> impl IntoWidget {
    // Re-renders when TICK changes; registered AFTER page_layer → tried first.
    let tick = pebbles_core::create_signal(0u32);
    TICK.with(|t| *t.borrow_mut() = Some(tick));
    let _ = tick.get(); // subscribe: a TICK write re-renders (and re-registers)
    pebbles_core::create_shortcut_if("g", || {
        if GATE.with(Cell::get) {
            GATED_HITS.with(|c| c.set(c.get() + 1));
            true
        } else {
            false // decline → dispatch falls through to page_layer's handler
        }
    });
    text("gated")
}

fn layered_root() -> impl IntoWidget {
    pebbles_widgets::column(pebbles_core::children![
        component(page_layer),
        component(gated_layer),
    ])
}

#[test]
fn conditional_shortcuts_fall_through_and_rerenders_do_not_stack() {
    GATE.with(|g| g.set(false));
    PAGE_HITS.with(|c| c.set(0));
    GATED_HITS.with(|c| c.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(layered_root)).into_widget());
    ui.layout(&mut env, Size::new(200.0, 100.0));
    let w = ui.window_id();
    let none = Mods::default();

    // Gated declines → the press falls through to the page handler.
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Char('g')), "someone consumed it");
    assert_eq!(PAGE_HITS.with(Cell::get), 1, "fell through to the page layer");
    assert_eq!(GATED_HITS.with(Cell::get), 0);

    // Gated engages → it consumes; the page handler is shadowed.
    GATE.with(|g| g.set(true));
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Char('g')));
    assert_eq!(GATED_HITS.with(Cell::get), 1, "the newer handler consumed it");
    assert_eq!(PAGE_HITS.with(Cell::get), 1, "and the older one did not run");

    // Re-render the registering component several times: registrations
    // OVERWRITE in place (owner-keyed) — no stacking, no duplicate fires.
    #[cfg(debug_assertions)]
    let census = shortcuts::census_shortcuts();
    let tick = TICK.with(|t| t.borrow().expect("tick stored"));
    for i in 1..=3 {
        tick.set(i);
        ui.rebuild_if_dirty();
        ui.layout(&mut env, Size::new(200.0, 100.0));
    }
    #[cfg(debug_assertions)]
    assert_eq!(
        shortcuts::census_shortcuts(),
        census,
        "re-renders overwrite their registration instead of stacking"
    );
    assert!(shortcuts::dispatch(w, none, ShortcutKey::Char('g')));
    assert_eq!(GATED_HITS.with(Cell::get), 2, "exactly one fire per press after re-renders");
    ui.dispose();
}
