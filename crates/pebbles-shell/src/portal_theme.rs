//! Linux/BSD desktop light/dark preference via the XDG desktop portal.
//!
//! winit 0.30 never reads the desktop color scheme on Linux (X11 reports no theme; Wayland only
//! an app-set override), so the shell asks the portal directly:
//! `org.freedesktop.portal.Settings.ReadOne("org.freedesktop.appearance", "color-scheme")`
//! (`1` = prefer dark, `2` = prefer light, `0` = no preference) and then follows its
//! `SettingChanged` signal. Everything runs on one background thread with a blocking D-Bus
//! connection; each value is handed to `report` (which wakes the event loop). No session bus,
//! no portal, or a portal without the setting ⇒ silently nothing (the theme stays light).

use pebbles_widgets::theme::Brightness;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";

/// Map the portal's `color-scheme` value to a brightness (`0`/unknown ⇒ no preference).
pub(crate) fn brightness_from_scheme(v: u32) -> Option<Brightness> {
    match v {
        1 => Some(Brightness::Dark),
        2 => Some(Brightness::Light),
        _ => None,
    }
}

/// Unwrap a (possibly nested — the deprecated `Read` wraps it twice) variant to a `u32`.
fn scheme_of(v: &Value<'_>) -> Option<u32> {
    match v {
        Value::U32(n) => Some(*n),
        Value::Value(inner) => scheme_of(inner),
        _ => None,
    }
}

/// Spawn the watcher thread. `report` is called with the initial value (if the portal has
/// one) and then on every change.
pub(crate) fn spawn(report: impl Fn(Option<Brightness>) + Send + 'static) {
    let _ = std::thread::Builder::new().name("pebbles-portal-theme".into()).spawn(move || {
        let _ = watch(&report);
    });
}

fn watch(report: &dyn Fn(Option<Brightness>)) -> zbus::Result<()> {
    let conn = Connection::session()?;
    let proxy = Proxy::new(&conn, DEST, PATH, IFACE)?;
    // Subscribe before reading so a change between the two is not lost.
    let changes = proxy.receive_signal("SettingChanged")?;
    let initial: zbus::Result<OwnedValue> = proxy.call("ReadOne", &(NAMESPACE, KEY));
    let initial = match initial {
        Ok(v) => Some(v),
        // Portal versions before 2 only have the deprecated `Read`.
        Err(_) => proxy.call::<_, _, OwnedValue>("Read", &(NAMESPACE, KEY)).ok(),
    };
    if let Some(v) = initial.and_then(|v| scheme_of(&v)) {
        report(brightness_from_scheme(v));
    }
    for msg in changes {
        let Ok((ns, key, value)) = msg.body().deserialize::<(String, String, OwnedValue)>() else {
            continue;
        };
        if ns == NAMESPACE
            && key == KEY
            && let Some(v) = scheme_of(&value)
        {
            report(brightness_from_scheme(v));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheme_values_map_like_the_portal_spec() {
        assert_eq!(brightness_from_scheme(0), None);
        assert_eq!(brightness_from_scheme(1), Some(Brightness::Dark));
        assert_eq!(brightness_from_scheme(2), Some(Brightness::Light));
        assert_eq!(brightness_from_scheme(7), None);
    }

    #[test]
    fn nested_variants_unwrap() {
        let v = Value::Value(Box::new(Value::Value(Box::new(Value::U32(1)))));
        assert_eq!(scheme_of(&v), Some(1));
        assert_eq!(scheme_of(&Value::Str("x".into())), None);
    }

    /// Live check against the session's portal (needs a desktop session): prints what it reports.
    #[test]
    #[ignore = "reads the desktop session bus; prints what the portal reports"]
    fn live_portal_reports_initial_value() {
        let (tx, rx) = std::sync::mpsc::channel();
        spawn(move |b| {
            let _ = tx.send(b);
        });
        // No portal (a CI runner) is a valid outcome: the shell falls back silently.
        let got = rx.recv_timeout(std::time::Duration::from_secs(5));
        println!("portal color-scheme -> {got:?}");
    }
}
