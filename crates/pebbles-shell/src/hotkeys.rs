//! B4 — **global (system-wide) hotkeys**: [`register_global_hotkey`] fires a
//! callback even while the app is unfocused. Backed by the [`global-hotkey`] crate
//! behind the default-OFF `global-hotkeys` feature.
//!
//! The public API is **always present** so app code compiles either way: with the
//! feature off, [`register_global_hotkey`] / [`unregister_global_hotkey`] return a
//! graceful `Err` instead of panicking. Bindings use B2's grammar
//! (`"Ctrl+Shift+Space"`, `"Mod+Alt+K"`), translated via
//! [`shortcuts::to_accelerator`](pebbles_core::shortcuts::to_accelerator).
//!
//! # Caveats (documented, never a panic)
//! * **Wayland** has no global-hotkey protocol — [`GlobalHotKeyManager::new`] fails
//!   there and the error is returned.
//! * **macOS** needs Accessibility permission; the OS error surfaces the same way.
//! * Events are delivered on the UI thread, drained each turn in the shell's
//!   `about_to_wait`. Live firing can only be verified manually (a headless test
//!   can't press a system hotkey).

#![allow(dead_code)] // `HotkeyId.0` is unused when the feature is off.

/// A handle to a registered global hotkey — pass it to [`unregister_global_hotkey`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotkeyId(pub(crate) u32);

#[cfg(feature = "global-hotkeys")]
mod imp {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::str::FromStr;

    use global_hotkey::hotkey::HotKey;
    use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

    use super::HotkeyId;

    thread_local! {
        static MANAGER: RefCell<Option<GlobalHotKeyManager>> = const { RefCell::new(None) };
        static CALLBACKS: RefCell<HashMap<u32, Rc<dyn Fn()>>> = RefCell::new(HashMap::new());
        static HOTKEYS: RefCell<HashMap<u32, HotKey>> = RefCell::new(HashMap::new());
    }

    pub(super) fn register(binding: &str, on_fire: impl Fn() + 'static) -> Result<HotkeyId, String> {
        let accel = pebbles_core::shortcuts::to_accelerator(binding)?;
        let hotkey = HotKey::from_str(&accel).map_err(|e| format!("unsupported hotkey {binding:?}: {e}"))?;
        MANAGER.with(|m| -> Result<(), String> {
            let mut m = m.borrow_mut();
            if m.is_none() {
                *m = Some(
                    GlobalHotKeyManager::new()
                        .map_err(|e| format!("global hotkeys unavailable (Wayland/permissions): {e}"))?,
                );
            }
            m.as_ref().unwrap().register(hotkey).map_err(|e| format!("could not register {binding:?}: {e}"))
        })?;
        let id = hotkey.id();
        CALLBACKS.with(|c| c.borrow_mut().insert(id, Rc::new(on_fire)));
        HOTKEYS.with(|h| h.borrow_mut().insert(id, hotkey));
        Ok(HotkeyId(id))
    }

    pub(super) fn unregister(id: HotkeyId) -> Result<(), String> {
        let hotkey = HOTKEYS.with(|h| h.borrow_mut().remove(&id.0));
        CALLBACKS.with(|c| {
            c.borrow_mut().remove(&id.0);
        });
        if let Some(hotkey) = hotkey {
            MANAGER.with(|m| -> Result<(), String> {
                if let Some(mgr) = m.borrow().as_ref() {
                    mgr.unregister(hotkey).map_err(|e| format!("could not unregister hotkey: {e}"))?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Drain queued global-hotkey events, invoking each Pressed hotkey's callback.
    /// Returns whether anything fired (the shell then requests a repaint).
    pub(crate) fn drain() -> bool {
        let mut fired = false;
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                let cb = CALLBACKS.with(|c| c.borrow().get(&event.id).cloned());
                if let Some(cb) = cb {
                    cb();
                    fired = true;
                }
            }
        }
        fired
    }
}

/// Register `binding` (B2 grammar, e.g. `"Ctrl+Shift+Space"`) as a system-wide
/// hotkey. `on_fire` runs on the UI thread whenever the combo is pressed, even
/// while another app is focused. Returns a [`HotkeyId`] for later removal.
///
/// Returns `Err` (never panics) when the `global-hotkeys` feature is off, when the
/// binding is unparseable, or when the OS refuses registration (Wayland, missing
/// macOS Accessibility permission, a combo already taken by another app).
///
/// # Platform support
/// **Desktop only.** macOS/Windows and Linux/X11 are supported; Linux/Wayland
/// returns `Err`. Not available on web or mobile (also `Err`). See `PLATFORMS.md`.
pub fn register_global_hotkey(binding: &str, on_fire: impl Fn() + 'static) -> Result<HotkeyId, String> {
    #[cfg(feature = "global-hotkeys")]
    {
        imp::register(binding, on_fire)
    }
    #[cfg(not(feature = "global-hotkeys"))]
    {
        let _ = (binding, on_fire);
        Err("global hotkeys require the `global-hotkeys` feature on pebbles-shell".to_string())
    }
}

/// Remove a hotkey registered with [`register_global_hotkey`]. Idempotent — an
/// already-removed id is `Ok`. Returns `Err` when the feature is off.
pub fn unregister_global_hotkey(id: HotkeyId) -> Result<(), String> {
    #[cfg(feature = "global-hotkeys")]
    {
        imp::unregister(id)
    }
    #[cfg(not(feature = "global-hotkeys"))]
    {
        let _ = id;
        Err("global hotkeys require the `global-hotkeys` feature on pebbles-shell".to_string())
    }
}

/// Drain queued hotkey events (called by the shell each turn). No-op with the
/// feature off.
#[cfg(feature = "global-hotkeys")]
pub(crate) fn drain() -> bool {
    imp::drain()
}

#[cfg(all(test, not(feature = "global-hotkeys")))]
mod tests {
    use super::*;

    #[test]
    fn api_is_graceful_when_the_feature_is_off() {
        // Present and callable, never a panic — just a clear error.
        assert!(register_global_hotkey("Ctrl+Shift+Space", || {}).is_err());
        assert!(unregister_global_hotkey(HotkeyId(0)).is_err());
    }
}
