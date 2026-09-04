//! A pluggable system clipboard.
//!
//! The core has no OS bindings, so it holds a **backend** the shell installs at
//! startup ([`install`]) — normally an `arboard`-backed reader/writer for the real
//! system clipboard. Until (or unless) one is installed, an in-process fallback
//! string keeps copy/paste working within the app. Editors call [`read`]/[`write`](fn@write).

use std::cell::RefCell;

enum Backend {
    /// In-process fallback (works app-internally with no OS binding).
    Internal(String),
    /// Real system clipboard, provided by the shell.
    External { read: Box<dyn Fn() -> String>, write: Box<dyn Fn(&str)> },
}

thread_local! {
    static CLIPBOARD: RefCell<Backend> = const { RefCell::new(Backend::Internal(String::new())) };
}

/// Install a system-clipboard backend (called once by the shell).
pub fn install(read: impl Fn() -> String + 'static, write: impl Fn(&str) + 'static) {
    CLIPBOARD.with(|c| {
        *c.borrow_mut() = Backend::External { read: Box::new(read), write: Box::new(write) };
    });
}

/// Read the current clipboard text.
pub fn read() -> String {
    CLIPBOARD.with(|c| match &*c.borrow() {
        Backend::Internal(s) => s.clone(),
        Backend::External { read, .. } => read(),
    })
}

/// Write `text` to the clipboard.
pub fn write(text: &str) {
    CLIPBOARD.with(|c| match &mut *c.borrow_mut() {
        Backend::Internal(s) => *s = text.to_string(),
        Backend::External { write, .. } => write(text),
    });
}
