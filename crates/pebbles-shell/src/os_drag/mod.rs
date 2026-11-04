//! Outgoing OS drag-and-drop of files ([`WindowCommand::StartFileDrag`]): winit 0.30 has no
//! drag-source API, so the shell speaks the protocols itself next to winit.
//!
//! * **Wayland** — a second `wayland-client` connection object over winit's own
//!   `wl_display` (`Backend::from_foreign_display`) binds the seat, a `wl_pointer` (to
//!   learn the implicit-grab serial of the press, which `start_drag` requires) and a
//!   `wl_data_device`; a `wl_data_source` offering `text/uri-list` +
//!   `text/plain;charset=utf-8` is started from winit's `wl_surface`. Its own event queue
//!   is dispatched on a helper thread (libwayland's `prepare_read` makes the shared
//!   display safe), which answers the target's `send` requests.
//! * **X11** — XDND version 5 as the source on a separate connection: a hidden source
//!   window owns `XdndSelection`; the pointer is polled (winit's window keeps the
//!   implicit grab, so the source can't grab it) and the window under it that is
//!   `XdndAware` gets `XdndEnter` / `XdndPosition` / `XdndDrop`; `SelectionRequest`s for
//!   the data are answered; `XdndFinished` (or a time-out) ends it.
//!
//! [`WindowCommand::StartFileDrag`]: pebbles_widgets::window::WindowCommand::StartFileDrag

#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
mod wayland;
#[cfg(any(
    target_os = "linux",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
pub mod x11;

use std::path::{Path, PathBuf};

/// The `text/uri-list` body for `paths` (RFC 2483: `file://` URIs, CRLF-terminated;
/// bytes outside the unreserved set percent-encoded).
pub fn uri_list(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| format!("{}\r\n", file_uri(p))).collect()
}

/// The paths as plain text, one per line (what terminals and text fields take).
pub fn plain_text(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n")
}

/// A `file://` URI for an absolute path.
pub fn file_uri(p: &Path) -> String {
    use std::os::unix::ffi::OsStrExt as _;
    let mut s = String::from("file://");
    for &b in p.as_os_str().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
            s.push(char::from(b));
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

/// A window was created: get ready to start drags from it (Wayland needs to see the
/// press that a later drag continues).
pub(crate) fn window_created(window: &winit::window::Window) {
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        use raw_window_handle::{HasDisplayHandle as _, RawDisplayHandle};
        if let Ok(d) = window.display_handle()
            && let RawDisplayHandle::Wayland(h) = d.as_raw()
        {
            wayland::init(h.display.as_ptr());
        }
    }
    let _ = window;
}

/// Start dragging `paths` out of `window` (the primary button is held). Returns whether
/// the platform took the drag.
pub(crate) fn start(window: &winit::window::Window, paths: Vec<PathBuf>) -> bool {
    if paths.is_empty() {
        return false;
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};
        match window.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Wayland(h)) => return wayland::start(h.surface.as_ptr(), paths),
            Ok(RawWindowHandle::Xlib(h)) => return x11::start(h.window as u32, paths),
            Ok(RawWindowHandle::Xcb(h)) => return x11::start(h.window.get(), paths),
            _ => {}
        }
    }
    let _ = window;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_lists_percent_encode() {
        let p = vec![PathBuf::from("/tmp/a b/é#.txt"), PathBuf::from("/x")];
        assert_eq!(uri_list(&p), "file:///tmp/a%20b/%C3%A9%23.txt\r\nfile:///x\r\n");
        assert_eq!(plain_text(&p), "/tmp/a b/é#.txt\n/x");
    }
}
