//! The framework router — a history stack of [`Location`]s (path + query) with
//! `navigate` / `replace` / `back` / `forward`, driven by a global reactive signal
//! so any component re-renders when the route changes.
//!
//! **One model, every platform.** On desktop and mobile it is pure in-memory
//! history (no URL). On web the shell installs a [`UrlSync`] bridge and the same
//! navigation mirrors to the browser: deep-linkable URLs, working Back/Forward, and
//! `popstate` fed back in — with `index` carried in `history.state` so it stays
//! authoritative. This core never touches `web-sys`; the shell owns the browser.
//!
//! Route PARAMETERS (`/user/:id`) are matched at the widget layer (`router_view`),
//! which extracts them from the current [`Location::path`]. Query values
//! (`?tab=activity`) are parsed here and read with [`Location::query`].

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::reactive::{Signal, create_root_signal};

/// A resolved location: the URL **path** plus its parsed **`?query`** pairs.
///
/// Route params (`:id`) are not stored here — they are matched against a pattern by
/// [`router_view`](../../pebbles_widgets/index.html) when it selects the page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    path: String,
    query: BTreeMap<String, String>,
}

impl Location {
    /// Parse a URL-ish string (`"/user/42?tab=activity"`) into path + query.
    /// The path is normalized to a leading `/` and no trailing slash (root stays `/`).
    pub fn parse(url: &str) -> Self {
        let (path, q) = url.trim().split_once('?').unwrap_or((url.trim(), ""));
        let mut query = BTreeMap::new();
        for pair in q.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            query.insert(decode(k), decode(v));
        }
        Location { path: normalize_path(path), query }
    }

    /// The path — always a leading `/`, no trailing slash (root is `/`).
    pub fn path(&self) -> &str {
        &self.path
    }
    /// A query-string value: `?tab=x` ⇒ `query("tab") == Some("x")`.
    pub fn query(&self, key: &str) -> Option<&str> {
        self.query.get(key).map(String::as_str)
    }
    /// All query pairs.
    pub fn query_all(&self) -> &BTreeMap<String, String> {
        &self.query
    }
    /// Render back to a `path?query` string (query keys sorted, values encoded).
    pub fn to_url(&self) -> String {
        if self.query.is_empty() {
            self.path.clone()
        } else {
            let q: Vec<String> =
                self.query.iter().map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect();
            format!("{}?{}", self.path, q.join("&"))
        }
    }
}

/// Normalize a path: leading `/`, drop a trailing `/` (root stays `/`).
fn normalize_path(p: &str) -> String {
    let p = p.trim();
    if p.is_empty() || p == "/" {
        return "/".to_string();
    }
    let with_lead = if p.starts_with('/') { p } else { return format!("/{}", p.trim_end_matches('/')) };
    let trimmed = with_lead.trim_end_matches('/');
    if trimmed.is_empty() { "/".to_string() } else { trimmed.to_string() }
}

/// Minimal percent-decode (`%XX` + `+`→space) for query keys/values.
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Minimal percent-encode of query values — escape everything outside the
/// unreserved set so a value survives a round-trip through the URL.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The platform bridge the **web shell** installs (via [`install_url_sync`]) to
/// mirror routing to `window.history`. The core carries the history `index` so the
/// shell can stash it in `history.state` and hand it back on `popstate`
/// ([`restore`]). No-op / never installed on desktop & mobile.
pub trait UrlSync {
    /// Push a new browser history entry for `url`, stashing `index` in its state.
    fn push(&self, url: &str, index: usize);
    /// Replace the current browser history entry (no new entry).
    fn replace(&self, url: &str, index: usize);
    /// Move the browser history by `delta` (Back/Forward); a `popstate` follows,
    /// which the shell routes back through [`restore`].
    fn go(&self, delta: i32);
}

struct Core {
    entries: Vec<Location>,
    index: usize,
    sync: Option<Rc<dyn UrlSync>>,
}

thread_local! {
    /// The reactive current location — components read it (and re-render on change).
    static LOCATION: RefCell<Option<Signal<Location>>> = const { RefCell::new(None) };
    /// The history mechanics (non-reactive): the entry stack, the cursor, and the
    /// optional web bridge.
    static CORE: RefCell<Core> =
        RefCell::new(Core { entries: vec![Location { path: String::new(), query: BTreeMap::new() }], index: 0, sync: None });
}

fn location_signal() -> Signal<Location> {
    LOCATION.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            let init = CORE.with(|c| {
                let mut c = c.borrow_mut();
                if c.entries[0].path.is_empty() {
                    c.entries[0].path = "/".to_string();
                }
                c.entries[c.index].clone()
            });
            *cell = Some(create_root_signal(init));
        }
        cell.unwrap()
    })
}

fn set_current(loc: Location) {
    location_signal().set(loc);
}

/// The current [`Location`] — **reactive**: reading it inside a component subscribes
/// that component, so it re-renders whenever the route changes.
pub fn location() -> Location {
    location_signal().get()
}

/// The current path (reactive shortcut for `location().path()`).
pub fn path() -> String {
    location_signal().get().path
}

/// A query value on the current location (reactive).
pub fn query(key: &str) -> Option<String> {
    location_signal().get().query.get(key).cloned()
}

/// Navigate to `to` (a `path` with an optional `?query`), pushing a history entry.
/// Any forward history is discarded (standard browser semantics).
pub fn navigate(to: &str) {
    let loc = Location::parse(to);
    let (url, index) = CORE.with(|c| {
        let mut c = c.borrow_mut();
        let keep = c.index + 1;
        c.entries.truncate(keep);
        c.entries.push(loc.clone());
        c.index = c.entries.len() - 1;
        (loc.to_url(), c.index)
    });
    set_current(loc);
    if let Some(s) = CORE.with(|c| c.borrow().sync.clone()) {
        s.push(&url, index);
    }
}

/// Replace the current entry in place — no new history entry (a redirect / a
/// side-nav selection you don't want Back to undo).
pub fn replace(to: &str) {
    let loc = Location::parse(to);
    let (url, index) = CORE.with(|c| {
        let mut c = c.borrow_mut();
        let i = c.index;
        c.entries[i] = loc.clone();
        (loc.to_url(), i)
    });
    set_current(loc);
    if let Some(s) = CORE.with(|c| c.borrow().sync.clone()) {
        s.replace(&url, index);
    }
}

fn current_from_core() -> Location {
    CORE.with(|c| {
        let c = c.borrow();
        c.entries[c.index].clone()
    })
}

/// Go back one history entry. Returns whether it moved. On web the browser drives
/// (a `popstate` restores state); elsewhere it pops the in-memory stack.
pub fn back() -> bool {
    let (has_sync, can) = CORE.with(|c| {
        let c = c.borrow();
        (c.sync.is_some(), c.index > 0)
    });
    if !can {
        return false;
    }
    if has_sync {
        CORE.with(|c| c.borrow().sync.clone()).unwrap().go(-1);
    } else {
        CORE.with(|c| c.borrow_mut().index -= 1);
        set_current(current_from_core());
    }
    true
}

/// Go forward one history entry (if any). Returns whether it moved.
pub fn forward() -> bool {
    let (has_sync, can) = CORE.with(|c| {
        let c = c.borrow();
        (c.sync.is_some(), c.index + 1 < c.entries.len())
    });
    if !can {
        return false;
    }
    if has_sync {
        CORE.with(|c| c.borrow().sync.clone()).unwrap().go(1);
    } else {
        CORE.with(|c| c.borrow_mut().index += 1);
        set_current(current_from_core());
    }
    true
}

/// Whether there is a previous entry to [`back`] to.
pub fn can_back() -> bool {
    CORE.with(|c| c.borrow().index > 0)
}

/// Whether there is a forward entry to [`forward`] to.
pub fn can_forward() -> bool {
    CORE.with(|c| {
        let c = c.borrow();
        c.index + 1 < c.entries.len()
    })
}

// ---------------------------------------------------------------------------
// Shell-facing (web) — install the browser bridge + feed popstate back in.
// ---------------------------------------------------------------------------

/// Install the web URL bridge and seed the initial location from `initial_url`
/// (`window.location.pathname + search`). The shell calls this once at web startup;
/// desktop/mobile never call it and the router stays in-memory.
pub fn install_url_sync(sync: Rc<dyn UrlSync>, initial_url: &str) {
    let loc = Location::parse(initial_url);
    CORE.with(|c| {
        let mut c = c.borrow_mut();
        c.entries = vec![loc.clone()];
        c.index = 0;
        c.sync = Some(sync.clone());
    });
    set_current(loc.clone());
    // Stamp the current entry with index 0 so a later Back lands on real state.
    sync.replace(&loc.to_url(), 0);
}

/// A browser `popstate` (Back/Forward or a manual URL edit): restore to the given
/// history `index` + `url` WITHOUT re-writing history. The shell reads `index` from
/// `event.state` and the `url` from `window.location`, then calls this.
pub fn restore(index: usize, url: &str) {
    let loc = Location::parse(url);
    CORE.with(|c| {
        let mut c = c.borrow_mut();
        if index < c.entries.len() {
            c.entries[index] = loc.clone();
            c.index = index;
        } else {
            c.entries.push(loc.clone());
            c.index = c.entries.len() - 1;
        }
    });
    set_current(loc);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_normalizes_path_and_query() {
        let l = Location::parse("/user/42/?tab=activity&q=hi+there");
        assert_eq!(l.path(), "/user/42");
        assert_eq!(l.query("tab"), Some("activity"));
        assert_eq!(l.query("q"), Some("hi there"));
        assert_eq!(Location::parse("").path(), "/");
        assert_eq!(Location::parse("home").path(), "/home");
    }

    #[test]
    fn url_round_trips() {
        let l = Location::parse("/search?q=a%20b&sort=asc");
        assert_eq!(l.query("q"), Some("a b"));
        let round = Location::parse(&l.to_url());
        assert_eq!(round, l);
    }

    #[test]
    fn history_back_forward_in_memory() {
        // No sync installed (desktop): the stack drives.
        navigate("/a");
        navigate("/b");
        navigate("/c");
        assert_eq!(path(), "/c");
        assert!(back());
        assert_eq!(path(), "/b");
        assert!(back());
        assert_eq!(path(), "/a");
        assert!(can_forward());
        assert!(forward());
        assert_eq!(path(), "/b");
        // navigate discards forward history
        navigate("/d");
        assert_eq!(path(), "/d");
        assert!(!can_forward());
    }
}
