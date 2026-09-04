//! B3 — the native-menu data model: `menu_bar`/`menu` translate the `MenuEntry`
//! vocabulary into the plain `NativeEntry` spec the shell hands to `muda`,
//! including accelerator parsing (via B2's grammar), enabled/disabled state,
//! checks, submenus, and callback retention. Pure — no feature, no OS, no GPU.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_widgets::NativeEntry;
use pebbles_widgets::components::{menu_check, menu_item, menu_separator, menu_sub};
use pebbles_widgets::{menu, menu_bar};

#[test]
fn menu_bar_translates_entries_and_accelerators() {
    let bar = menu_bar([
        menu(
            "File",
            [
                menu_item("New").shortcut("Mod+N").into(),
                menu_item("Open").shortcut("Mod+O").into(),
                menu_separator(),
                menu_item("Quit").shortcut("Mod+Q").into(),
            ],
        ),
        menu(
            "Edit",
            [menu_item("Undo").shortcut("Mod+Z").into(), menu_item("Unavailable").disabled(true).into()],
        ),
    ]);

    assert_eq!(bar.menus.len(), 2);
    assert_eq!(bar.menus[0].label, "File");
    assert_eq!(bar.menus[0].entries.len(), 4);

    let mod_n = if cfg!(target_os = "macos") { "Super+KeyN" } else { "Control+KeyN" };
    match &bar.menus[0].entries[0] {
        NativeEntry::Item { label, accelerator, enabled, on_select } => {
            assert_eq!(label, "New");
            assert_eq!(accelerator.as_deref(), Some(mod_n));
            assert!(*enabled);
            assert!(on_select.is_none(), "no on_select was set");
        }
        _ => panic!("entry 0 should be an Item"),
    }
    assert!(matches!(bar.menus[0].entries[2], NativeEntry::Separator));

    // Disabled item carries through as `enabled: false`.
    match &bar.menus[1].entries[1] {
        NativeEntry::Item { label, enabled, .. } => {
            assert_eq!(label, "Unavailable");
            assert!(!*enabled);
        }
        _ => panic!("expected a disabled Item"),
    }
}

#[test]
fn submenus_checks_and_callbacks_survive_translation() {
    let fired = Rc::new(Cell::new(0));
    let toggled = Rc::new(Cell::new(false));

    let bar = {
        let f = fired.clone();
        let t = toggled.clone();
        menu_bar([menu(
            "View",
            [
                menu_check("Word Wrap", true, move |on| t.set(on)).into(),
                menu_sub("Share", [menu_item("Email"), menu_item("Slack")]),
                menu_item("Run").on_select(move || f.set(f.get() + 1)).into(),
            ],
        )])
    };

    let entries = &bar.menus[0].entries;
    assert_eq!(entries.len(), 3);

    // Check row keeps its state and its toggle callback (invoked with the NEW state).
    match &entries[0] {
        NativeEntry::Check { label, checked, on_toggle, .. } => {
            assert_eq!(label, "Word Wrap");
            assert!(*checked);
            on_toggle(false);
            assert!(!toggled.get(), "on_toggle received the new state");
        }
        _ => panic!("entry 0 should be a Check"),
    }

    // Submenu nests its two items.
    match &entries[1] {
        NativeEntry::Submenu { label, entries } => {
            assert_eq!(label, "Share");
            assert_eq!(entries.len(), 2);
            assert!(matches!(&entries[0], NativeEntry::Item { .. }));
        }
        _ => panic!("entry 1 should be a Submenu"),
    }

    // Item callback is retained and callable.
    match &entries[2] {
        NativeEntry::Item { on_select: Some(cb), .. } => {
            cb();
            cb();
            assert_eq!(fired.get(), 2);
        }
        _ => panic!("entry 2 should be an Item with a callback"),
    }
}
