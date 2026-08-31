//! A **global right-click menu** — the standard desktop fallback: right-clicking
//! anywhere nothing else claims it (no widget context menu, no blocker) opens a
//! small menu with Cut / Copy / Paste / Select All, at the cursor.
//!
//! Controllable by app developers:
//! * [`set_global_menu_enabled`] — off/on app-wide (on by default).
//! * [`block_context_menu`] — wrap any widget/area to suppress the menu there
//!   (widgets with their OWN context menu — [`ContextMenu`](crate::ContextMenu),
//!   the file explorer — suppress it automatically).
//! * [`set_global_menu`] — replace the options entirely; [`reset_global_menu`]
//!   restores the standard set.
//! * [`set_global_menu_style`] / [`set_global_menu_width`] — the look.
//!
//! The standard items route to the focused editor (clipboard + selection), and
//! disable themselves when no editor holds focus — the IDE convention.

use std::cell::RefCell;
use std::rc::Rc;

use crate::components::input::menu::{RebuildableMenu, SubMenuHandles, estimate_height};
use crate::components::input::list_nav::list_nav;
use crate::components::menu_item;
use crate::components::MenuEntry;
use crate::overlay::{show_overlay, window_size};
use crate::style::Style;
use crate::widgets::GestureDetector;
use pebbles_core::keyboard::KeyInput;
use pebbles_core::widget::IntoWidget;
use pebbles_core::{create_signal, editor_is_focused, focus};

struct Config {
    enabled: bool,
    /// None = the standard set (with focus-aware disabling); Some = the
    /// developer's own options, used verbatim.
    entries: Option<Vec<MenuEntry>>,
    style: Option<Style>,
    width: f64,
}

thread_local! {
    static CONFIG: RefCell<Config> = RefCell::new(Config {
        enabled: true,
        entries: None,
        style: None,
        width: 220.0,
    });
}

/// Enable or disable the global right-click menu (default: enabled).
pub fn set_global_menu_enabled(enabled: bool) {
    CONFIG.with(|c| c.borrow_mut().enabled = enabled);
}

/// Whether the global right-click menu is enabled.
pub fn is_global_menu_enabled() -> bool {
    CONFIG.with(|c| c.borrow().enabled)
}

/// Replace the global menu's options entirely (the standard Cut/Copy/Paste/
/// Select All set is replaced — restore it with [`reset_global_menu`]).
pub fn set_global_menu(entries: Vec<MenuEntry>) {
    CONFIG.with(|c| c.borrow_mut().entries = Some(entries));
}

/// Restore the standard Cut/Copy/Paste/Select All options.
pub fn reset_global_menu() {
    CONFIG.with(|c| c.borrow_mut().entries = None);
}

/// Style the menu surface (background, border, radius, shadow, …).
pub fn set_global_menu_style(style: Style) {
    CONFIG.with(|c| c.borrow_mut().style = Some(style));
}

/// The menu width (default 220).
pub fn set_global_menu_width(width: f64) {
    CONFIG.with(|c| c.borrow_mut().width = width);
}

/// Wrap `child` so a right-click on it is CONSUMED — the global menu never
/// opens over this area (use it for canvases, custom surfaces, …).
pub fn block_context_menu(child: impl IntoWidget) -> GestureDetector {
    GestureDetector::new(child).on_secondary_tap(|| {})
}

/// The standard options, focus-aware: clipboard/selection intents disable when
/// no editor holds focus (the IDE convention).
fn standard_entries() -> Vec<MenuEntry> {
    let editor = editor_is_focused();
    vec![
        menu_item("Cut").shortcut("⌘X").disabled(!editor).on_select(move || {
            focus::dispatch_key(KeyInput::Cut);
        }).into(),
        menu_item("Copy").shortcut("⌘C").disabled(!editor).on_select(move || {
            focus::dispatch_key(KeyInput::Copy);
        }).into(),
        menu_item("Paste").shortcut("⌘V").disabled(!editor).on_select(move || {
            focus::dispatch_key(KeyInput::Paste);
        }).into(),
        crate::components::menu_separator(),
        menu_item("Select All").shortcut("⌘A").disabled(!editor).on_select(move || {
            focus::dispatch_key(KeyInput::SelectAll);
        }).into(),
    ]
}

/// Open the global menu at the cursor (the shell calls this when a right-click
/// found no other claimant). A no-op while disabled.
pub fn show(x: f64, y: f64) {
    CONFIG.with(|c| {
        let c = c.borrow();
        if !c.enabled {
            return;
        }
        let width = c.width;
        let style = c.style.clone();
        match c.entries.as_deref() {
            Some(entries) => show_inner(x, y, entries, style, width),
            // The standard set is rebuilt per open on purpose — its disabled
            // state tracks whether an editor holds focus right now.
            None => {
                let std = standard_entries();
                show_inner(x, y, &std, style, width);
            }
        }
    });
}

thread_local! {
    /// App-scope submenu plumbing (created once, shared across opens).
    static HANDLES: RefCell<Option<SubMenuHandles>> = const { RefCell::new(None) };
}

fn show_inner(x: f64, y: f64, entries: &[MenuEntry], style: Option<Style>, width: f64) {
    let menu_h = estimate_height(entries).min(300.0);

    // App-scope submenu plumbing (created once, shared across opens).
    let handles = HANDLES.with(|h| {
        let mut h = h.borrow_mut();
        if h.is_none() {
            *h = Some(SubMenuHandles {
                nav: list_nav(),
                ctx: create_signal(None),
                subs: Rc::new(Vec::new()),
            });
        }
        h.clone().expect("handles")
    });

    let blueprint = RebuildableMenu::from(entries);
    let (ww, wh) = window_size();
    let left = if ww > 0.0 { x.min(ww - width - 8.0).max(8.0) } else { x };
    let top = if wh > 0.0 { y.min(wh - menu_h - 8.0).max(8.0) } else { y };

    let menu = crate::style::styled(blueprint.build(width, &handles), style.unwrap_or_default());
    show_overlay(menu.into_widget(), left, top, width, menu_h);
}
