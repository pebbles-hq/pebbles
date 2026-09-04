//! B3 — the `muda` glue behind the `native-menus` feature: turn a
//! [`MenuBar`](pebbles_widgets::MenuBar) spec into a real OS menu, attach it to the
//! window, and route menu clicks back to the app's callbacks on the UI thread.
//!
//! Compiled **only** with `native-menus`. The whole module is gated; nothing here
//! touches the default build. Attachment is wired for **macOS and Windows** (the
//! roadmap's first target); on Linux the in-window `menubar(..)` remains the form,
//! so we log once and skip rather than fight winit 0.30 for a GTK window.
//!
//! Callback routing mirrors `task::pump`: `muda` posts a `MenuEvent` (carrying the
//! clicked item's `MenuId`) to a global channel; the shell drains it in
//! `about_to_wait` and looks the id up in this registry.

use std::collections::HashMap;
use std::rc::Rc;

use muda::MenuEvent;
use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, IsMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};

use pebbles_widgets::{MenuBar, NativeEntry, NativeMenu};

/// What a clicked menu id should do.
enum Action {
    /// A plain command.
    Run(Rc<dyn Fn()>),
    /// A checkbox: report the item's NEW state (muda toggles the mark itself).
    Toggle(Rc<dyn Fn(bool)>, CheckMenuItem),
}

/// A live native menu: the root `muda::Menu` (kept alive so the OS menu persists)
/// plus the id → callback registry.
pub(crate) struct NativeMenus {
    menu: Menu,
    callbacks: HashMap<MenuId, Action>,
    next: u64,
}

/// Append one item into a submenu (muda appends through interior mutability, so a
/// shared ref suffices), logging rather than aborting on the rare failure.
fn append(parent: &Submenu, item: &dyn IsMenuItem) {
    if let Err(e) = parent.append(item) {
        eprintln!("pebbles: could not append native menu item: {e}");
    }
}

impl NativeMenus {
    /// Build a live menu from the spec (no OS attachment yet).
    pub(crate) fn build(bar: &MenuBar) -> Self {
        let menu = Menu::new();
        let mut this = NativeMenus { menu, callbacks: HashMap::new(), next: 0 };
        for top in &bar.menus {
            this.add_top(top);
        }
        this
    }

    fn add_top(&mut self, top: &NativeMenu) {
        let sub = Submenu::new(&top.label, true);
        self.populate(&sub, &top.entries);
        if let Err(e) = self.menu.append(&sub) {
            eprintln!("pebbles: could not append native menu {:?}: {e}", top.label);
        }
    }

    fn fresh_id(&mut self) -> MenuId {
        self.next += 1;
        // `MenuId: From<String>` — the same conversion `with_id`'s `Into<MenuId>` uses.
        MenuId::from(format!("pb-menu-{}", self.next))
    }

    fn populate(&mut self, parent: &Submenu, entries: &[NativeEntry]) {
        for entry in entries {
            match entry {
                NativeEntry::Separator => append(parent, &PredefinedMenuItem::separator()),
                NativeEntry::Item { label, accelerator, enabled, on_select } => {
                    let id = self.fresh_id();
                    let accel = accelerator.as_deref().and_then(parse_accelerator);
                    let item = MenuItem::with_id(id.clone(), label, *enabled, accel);
                    append(parent, &item);
                    if let Some(cb) = on_select {
                        self.callbacks.insert(id, Action::Run(cb.clone()));
                    }
                }
                NativeEntry::Check { label, checked, accelerator, enabled, on_toggle } => {
                    let id = self.fresh_id();
                    let accel = accelerator.as_deref().and_then(parse_accelerator);
                    let item = CheckMenuItem::with_id(id.clone(), label, *enabled, *checked, accel);
                    append(parent, &item);
                    self.callbacks.insert(id, Action::Toggle(on_toggle.clone(), item.clone()));
                }
                NativeEntry::Submenu { label, entries } => {
                    let sub = Submenu::new(label, true);
                    self.populate(&sub, entries);
                    append(parent, &sub);
                }
            }
        }
    }

    /// Attach the built menu to the OS window. macOS uses the global app menu;
    /// Windows uses the per-window menu. (This module is only compiled on those two
    /// platforms — see `lib.rs`.)
    pub(crate) fn attach(&self, window: &winit::window::Window) {
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(handle) = window.window_handle() {
                if let RawWindowHandle::Win32(h) = handle.as_raw() {
                    // SAFETY: `hwnd` is a valid, live window handle owned by winit for
                    // the lifetime of this window.
                    unsafe {
                        let _ = self.menu.init_for_hwnd(h.hwnd.get());
                    }
                }
            }
        }
        #[cfg(target_os = "macos")]
        {
            let _ = window;
            self.menu.init_for_nsapp();
        }
    }

    /// Drain queued menu clicks, invoking each item's callback. Returns whether
    /// anything fired (the shell then requests a repaint).
    pub(crate) fn drain(&mut self) -> bool {
        let mut fired = false;
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(action) = self.callbacks.get(&event.id) {
                match action {
                    Action::Run(f) => f(),
                    Action::Toggle(f, item) => f(item.is_checked()),
                }
                fired = true;
            }
        }
        fired
    }
}

/// Parse an accelerator string (already in muda's grammar, from B2) — a bad string
/// simply yields no accelerator rather than aborting the whole menu.
fn parse_accelerator(s: &str) -> Option<Accelerator> {
    s.parse::<Accelerator>().ok()
}
