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

use std::cell::{Cell, RefCell};
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
    /// Remembered scroll offset per history index (SolidJS scroll restoration). A
    /// scroll view saves its offset here and restores it when its route re-appears via
    /// Back/Forward.
    scrolls: BTreeMap<usize, f64>,
}

thread_local! {
    /// The reactive current location — components read it (and re-render on change).
    static LOCATION: RefCell<Option<Signal<Location>>> = const { RefCell::new(None) };
    /// The history mechanics (non-reactive): the entry stack, the cursor, and the
    /// optional web bridge.
    static CORE: RefCell<Core> = RefCell::new(Core {
        entries: vec![Location { path: String::new(), query: BTreeMap::new() }],
        index: 0,
        sync: None,
        scrolls: BTreeMap::new(),
    });
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

/// Navigate to `to`, pushing a history entry. `to` is a `path` with an optional
/// `?query`; a leading `/` is absolute, and `./x` / `../x` resolve **relative** to the
/// current path (a bare `x` stays absolute, so existing call sites are unchanged). Any
/// forward history is discarded (standard browser semantics). A registered
/// [guard](add_guard) can block or redirect the navigation. Returns whether it moved.
pub fn navigate(to: &str) -> bool {
    navigate_guarded(to, 0)
}

fn navigate_guarded(to: &str, depth: u8) -> bool {
    let loc = Location::parse(&resolve(to));
    match run_guards(&loc) {
        NavGuard::Block => return false,
        NavGuard::Redirect(target) => {
            return depth < MAX_REDIRECTS && navigate_guarded(&target, depth + 1);
        }
        NavGuard::Allow => {}
    }
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
    true
}

/// Replace the current entry in place — no new history entry (a redirect / a
/// side-nav selection you don't want Back to undo). Resolves + guards like
/// [`navigate`]. Returns whether it moved.
pub fn replace(to: &str) -> bool {
    replace_guarded(to, 0)
}

fn replace_guarded(to: &str, depth: u8) -> bool {
    let loc = Location::parse(&resolve(to));
    match run_guards(&loc) {
        NavGuard::Block => return false,
        NavGuard::Redirect(target) => {
            return depth < MAX_REDIRECTS && replace_guarded(&target, depth + 1);
        }
        NavGuard::Allow => {}
    }
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
    true
}

/// Resolve `to` against the current path: `/x` is absolute; `./x` and `../x` (and
/// bare `.`/`..`) resolve relative to the current path treated as a directory
/// (`../sibling` from `/a/b` → `/a/sibling`); anything else is taken as-is (absolute,
/// so existing `navigate("/home")`-style calls are unchanged).
fn resolve(to: &str) -> String {
    let t = to.trim();
    let is_relative = t == "." || t == ".." || t.starts_with("./") || t.starts_with("../");
    if !is_relative {
        return t.to_string();
    }
    let (path_part, query_part) = t.split_once('?').unwrap_or((t, ""));
    let mut segs: Vec<String> =
        current_from_core().path.split('/').filter(|s| !s.is_empty()).map(String::from).collect();
    for seg in path_part.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segs.pop();
            }
            other => segs.push(other.to_string()),
        }
    }
    let mut out = if segs.is_empty() { "/".to_string() } else { format!("/{}", segs.join("/")) };
    if !query_part.is_empty() {
        out.push('?');
        out.push_str(query_part);
    }
    out
}

// ---------------------------------------------------------------------------
// Query parameters — reactive setters (SolidJS `useSearchParams` set half).
// ---------------------------------------------------------------------------

/// Set a `?query` value on the current location (replacing the entry in place — the
/// path is unchanged, so Back doesn't accumulate one entry per keystroke). Reactive.
pub fn set_query(key: &str, value: &str) {
    let (k, v) = (key.to_string(), value.to_string());
    update_query(move |q| {
        q.insert(k, v);
    });
}

/// Remove a `?query` key from the current location (in place). Reactive.
pub fn remove_query(key: &str) {
    let k = key.to_string();
    update_query(move |q| {
        q.remove(&k);
    });
}

/// Edit the current location's query map in place and replace the entry. Reactive —
/// readers of [`query`]/[`location`] re-render.
pub fn update_query(f: impl FnOnce(&mut BTreeMap<String, String>)) {
    let mut loc = current_from_core();
    f(&mut loc.query);
    replace(&loc.to_url());
}

// ---------------------------------------------------------------------------
// Guards & redirects (SolidJS `useBeforeLeave` + `<Navigate>` / route guards).
// ---------------------------------------------------------------------------

const MAX_REDIRECTS: u8 = 8;

/// A guard's verdict for a pending navigation.
pub enum NavGuard {
    /// Let the navigation proceed.
    Allow,
    /// Cancel it and go to this target instead (auth gate, `/old`→`/new`).
    Redirect(String),
    /// Cancel it and stay put (unsaved-changes blocker — SolidJS `useBeforeLeave`).
    Block,
}

type GuardFn = Rc<dyn Fn(&Location) -> NavGuard>;

thread_local! {
    static GUARDS: RefCell<Vec<(u64, GuardFn)>> = const { RefCell::new(Vec::new()) };
    static NEXT_GUARD: Cell<u64> = const { Cell::new(0) };
}

/// A handle to a registered [guard](add_guard); drop it via [`remove_guard`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct GuardHandle(u64);

/// Register a navigation guard consulted on every [`navigate`] / [`replace`] (and the
/// in-memory [`back`] / [`forward`]): it receives the target [`Location`] and returns
/// [`NavGuard`] to allow, redirect, or block. Guards run in registration order; the
/// first non-`Allow` verdict wins. Use it for auth gating, `/old`→`/new` redirects, and
/// unsaved-changes blocking. Pair with [`remove_guard`] on teardown (a component can do
/// so in `create_cleanup`).
pub fn add_guard(f: impl Fn(&Location) -> NavGuard + 'static) -> GuardHandle {
    let id = NEXT_GUARD.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    });
    GUARDS.with(|g| g.borrow_mut().push((id, Rc::new(f))));
    GuardHandle(id)
}

/// Remove a guard registered with [`add_guard`].
pub fn remove_guard(handle: GuardHandle) {
    GUARDS.with(|g| g.borrow_mut().retain(|(id, _)| *id != handle.0));
}

fn run_guards(target: &Location) -> NavGuard {
    let guards: Vec<GuardFn> = GUARDS.with(|g| g.borrow().iter().map(|(_, f)| f.clone()).collect());
    for guard in guards {
        match guard(target) {
            NavGuard::Allow => {}
            other => return other,
        }
    }
    NavGuard::Allow
}

// ---------------------------------------------------------------------------
// Document / window title per route (applied by the shell).
// ---------------------------------------------------------------------------

thread_local! {
    static TITLE: RefCell<Option<Signal<Option<String>>>> = const { RefCell::new(None) };
}

fn title_signal() -> Signal<Option<String>> {
    TITLE.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_root_signal(None));
        }
        cell.unwrap()
    })
}

/// Set the window / document title for the current route. The shell applies it
/// (winit window title on desktop, `document.title` on web). Call it from a route's
/// page (e.g. in `on_mount`) so the title tracks navigation.
pub fn set_title(title: impl Into<String>) {
    title_signal().set(Some(title.into()));
}

/// The current route title set via [`set_title`] (reactive — the shell reads it).
pub fn title() -> Option<String> {
    title_signal().get()
}

// ---------------------------------------------------------------------------
// Navigation hook.
// ---------------------------------------------------------------------------

/// Run `f` whenever the route changes (after the change), receiving the new
/// [`Location`]. Sugar for `on(|| location(), f)`; use it to move focus to the new
/// view for accessibility, log page views, or sync external state. Skips the initial
/// mount (fires on real changes only).
pub fn on_route_change(f: impl Fn(Location) + 'static) {
    crate::reactive::on_defer(location, f);
}

// ---------------------------------------------------------------------------
// Scroll restoration (SolidJS router scroll restoration).
// ---------------------------------------------------------------------------

/// Remember `offset` as the scroll position for the **current** history entry. A
/// scroll view calls this as it scrolls (or before navigating away) so the position
/// can be restored when the user returns via Back/Forward.
pub fn save_scroll(offset: f64) {
    CORE.with(|c| {
        let mut c = c.borrow_mut();
        let i = c.index;
        c.scrolls.insert(i, offset);
    });
}

/// The remembered scroll offset for the current history entry (`0.0` if none). A
/// scroll view reads this on [`on_route_change`] and scrolls there to restore the
/// position for a Back/Forward navigation.
pub fn saved_scroll() -> f64 {
    CORE.with(|c| {
        let c = c.borrow();
        c.scrolls.get(&c.index).copied().unwrap_or(0.0)
    })
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
        let target = CORE.with(|c| {
            let c = c.borrow();
            c.entries[c.index - 1].clone()
        });
        match run_guards(&target) {
            NavGuard::Block => return false,
            NavGuard::Redirect(t) => return navigate(&t),
            NavGuard::Allow => {}
        }
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
        let target = CORE.with(|c| {
            let c = c.borrow();
            c.entries[c.index + 1].clone()
        });
        match run_guards(&target) {
            NavGuard::Block => return false,
            NavGuard::Redirect(t) => return navigate(&t),
            NavGuard::Allow => {}
        }
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

    #[test]
    fn relative_navigation_resolves_against_current() {
        navigate("/a/b/c");
        navigate("../x"); // /a/b + x
        assert_eq!(path(), "/a/b/x");
        navigate("./y"); // /a/b/x + y
        assert_eq!(path(), "/a/b/x/y");
        navigate("../../z"); // pop y, pop x → /a/b + z
        assert_eq!(path(), "/a/b/z");
        // A bare string stays absolute (back-compat).
        navigate("home");
        assert_eq!(path(), "/home");
    }

    #[test]
    fn set_and_remove_query_stay_on_the_path() {
        navigate("/items");
        set_query("sort", "asc");
        assert_eq!(path(), "/items");
        assert_eq!(query("sort").as_deref(), Some("asc"));
        set_query("page", "2");
        assert_eq!(query("page").as_deref(), Some("2"));
        remove_query("sort");
        assert_eq!(query("sort"), None);
        assert_eq!(query("page").as_deref(), Some("2"));
    }

    #[test]
    fn a_guard_can_block_and_redirect() {
        navigate("/start");
        // Block anything under /locked.
        let block =
            add_guard(
                |loc| {
                    if loc.path().starts_with("/locked") { NavGuard::Block } else { NavGuard::Allow }
                },
            );
        assert!(!navigate("/locked/x"), "blocked navigation reports false");
        assert_eq!(path(), "/start", "and stays put");
        remove_guard(block);
        // Redirect /old → /new.
        let redir =
            add_guard(
                |loc| {
                    if loc.path() == "/old" { NavGuard::Redirect("/new".into()) } else { NavGuard::Allow }
                },
            );
        assert!(navigate("/old"));
        assert_eq!(path(), "/new", "redirected to the target");
        remove_guard(redir);
        // After removal, the path is reachable again.
        assert!(navigate("/old"));
        assert_eq!(path(), "/old");
    }

    #[test]
    fn scroll_is_remembered_per_entry() {
        navigate("/list");
        save_scroll(420.0);
        navigate("/detail");
        assert_eq!(saved_scroll(), 0.0, "a fresh entry starts at the top");
        save_scroll(80.0);
        back();
        assert_eq!(saved_scroll(), 420.0, "returning restores the list's offset");
        forward();
        assert_eq!(saved_scroll(), 80.0, "and the detail's");
    }

    #[test]
    fn title_round_trips() {
        set_title("Dashboard");
        assert_eq!(title().as_deref(), Some("Dashboard"));
    }
}
