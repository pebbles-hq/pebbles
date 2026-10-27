//! Linux system-clipboard round trip through arboard (the backend the shell installs).
//!
//! Needs a live display server, so it is `#[ignore]`d on CI. Run locally with
//! `cargo test -p pebbles-shell --test linux_clipboard -- --ignored`, once in the
//! Wayland session (exercises `wayland-data-control`) and once with
//! `env -u WAYLAND_DISPLAY` (exercises the X11 fallback via XWayland/Xorg).
#![cfg(target_os = "linux")]

#[test]
#[ignore = "needs a Wayland or X11 display"]
fn set_then_get_text_round_trips() {
    // CI runs `--ignored` GPU tests where DISPLAY may point at nothing reachable: skip,
    // since the shell itself falls back to the in-app clipboard in that case.
    let mut cb = match arboard::Clipboard::new() {
        Ok(cb) => cb,
        Err(e) => return eprintln!("skipped: no reachable clipboard ({e})"),
    };
    let text = format!("pebbles-clipboard-{}", std::process::id());
    cb.set_text(text.clone()).expect("set text");
    assert_eq!(cb.get_text().expect("get text"), text);
}
