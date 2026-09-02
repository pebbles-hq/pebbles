//! B3 — the data model for a **native OS menu bar**. `menu_bar([...])` + `menu(..)`
//! describe a File/Edit/… bar, reusing the [`MenuEntry`](crate::components::MenuEntry)
//! vocabulary; the shell (`App::menu`) turns it into a
//! real `muda` menu behind its default-off `native-menus` feature.
//!
//! Building the bar is **always available and pure** — only the OS attachment is
//! feature-gated — so app code (and this module's tests) compile and run whether or
//! not `native-menus` is on. The in-window [`menubar`](crate::components::menubar)
//! remains THE cross-platform form; a native bar is the macOS-style global menu /
//! Windows window menu layered on top.
//!
//! A menu-item `.shortcut("Mod+S")` string is parsed here through B2's binding
//! grammar ([`shortcuts::to_accelerator`](pebbles_core::shortcuts::to_accelerator))
//! into the accelerator string `muda` accepts; an unparseable string yields no
//! accelerator rather than an error (it is a hint, not a hard contract).

use std::rc::Rc;

use crate::components::{MenuEntry, MenuItem};

/// One resolved row of a native menu: callbacks kept as `Rc`, the shortcut hint
/// already translated to an accelerator string via B2's grammar. This is the plain
/// spec the shell consumes to build a `muda` menu.
pub enum NativeEntry {
    /// A clickable command.
    Item {
        label: String,
        /// Accelerator string (e.g. `"Control+KeyS"`), if the authored shortcut
        /// parsed. `None` means "no accelerator", never "invalid".
        accelerator: Option<String>,
        enabled: bool,
        on_select: Option<Rc<dyn Fn()>>,
    },
    /// A divider.
    Separator,
    /// A toggleable command; `on_toggle` receives the new checked state.
    Check {
        label: String,
        checked: bool,
        accelerator: Option<String>,
        enabled: bool,
        on_toggle: Rc<dyn Fn(bool)>,
    },
    /// A nested submenu (native menus allow arbitrary depth).
    Submenu { label: String, entries: Vec<NativeEntry> },
}

/// One top-level menu — `menu("File", [...])`.
pub struct NativeMenu {
    pub label: String,
    pub entries: Vec<NativeEntry>,
}

/// A whole native menu bar — the argument to `App::menu`.
pub struct MenuBar {
    pub menus: Vec<NativeMenu>,
}

/// Translate one authored [`MenuEntry`] into its native [`NativeEntry`] spec.
///
/// A [`MenuEntry::Label`] (a section header) has no native equivalent, so it maps
/// to a disabled item showing the same text — native menus have no section rows.
fn to_native(entry: MenuEntry) -> NativeEntry {
    match entry {
        MenuEntry::Item(item) => native_item(item),
        MenuEntry::Separator => NativeEntry::Separator,
        MenuEntry::Label(label) => NativeEntry::Item {
            label,
            accelerator: None,
            enabled: false,
            on_select: None,
        },
        MenuEntry::Check { label, checked, on_toggle } => NativeEntry::Check {
            label,
            checked,
            accelerator: None,
            enabled: true,
            on_toggle,
        },
        MenuEntry::Sub { label, entries } => NativeEntry::Submenu {
            label,
            entries: entries.into_iter().map(to_native).collect(),
        },
    }
}

fn native_item(item: MenuItem) -> NativeEntry {
    let accelerator = item
        .shortcut_str()
        .and_then(|s| pebbles_core::shortcuts::to_accelerator(s).ok());
    NativeEntry::Item {
        label: item.label_str().to_string(),
        accelerator,
        enabled: !item.is_disabled(),
        on_select: item.on_select_rc(),
    }
}

/// Build one top-level menu from a label and its entries (any `Into<MenuEntry>`:
/// `menu_item(..)`, `menu_check(..)`, `menu_separator()`, `menu_sub(..)`, …).
///
/// ```ignore
/// menu("File", [
///     menu_item("New").shortcut("Mod+N").on_select(|| { /* … */ }).into(),
///     menu_separator(),
///     menu_item("Quit").shortcut("Mod+Q").into(),
/// ])
/// ```
pub fn menu<I, E>(label: impl Into<String>, entries: I) -> NativeMenu
where
    I: IntoIterator<Item = E>,
    E: Into<MenuEntry>,
{
    NativeMenu {
        label: label.into(),
        entries: entries.into_iter().map(|e| to_native(e.into())).collect(),
    }
}

/// Assemble top-level menus into a bar for `App::menu`.
///
/// ```ignore
/// App::new(root).menu(menu_bar([
///     menu("File", [ /* … */ ]),
///     menu("Edit", [ /* … */ ]),
/// ]))
/// ```
pub fn menu_bar<I: IntoIterator<Item = NativeMenu>>(menus: I) -> MenuBar {
    MenuBar { menus: menus.into_iter().collect() }
}
