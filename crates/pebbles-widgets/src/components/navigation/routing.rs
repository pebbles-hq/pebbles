//! Built-in routing: [`NavStack`] (a navigation history you keep in a signal) and
//! [`RouteView`] (renders the page for the current route).
//!
//! The model is declarative and Flutter-like: a route maps to a **page builder**,
//! and only the active page is built. Each page is its own component with its own
//! signals, so a page's interactions target that page — not the shell.
//!
//! ```ignore
//! let nav = create_signal(NavStack::new("home"));
//! // nav item:
//! nav_item("Settings").on_select(move || nav.update(|n| n.replace("settings")))
//! // content:
//! route_view(nav.get().current())
//!     .route("home", || component(home_page))
//!     .route("settings", || component(settings_page))
//! ```
//!
//! ## Path parameters — passing values through the route
//!
//! A route can carry values in its **path** (`/user/:id`). Register it with
//! [`RouteView::param_route`]; the builder receives the captured [`RouteParams`].
//! Drive the view from the framework router's path so browser URLs, Back/Forward,
//! and deep links all flow through (query values read via `router::query(..)`):
//!
//! ```ignore
//! route_view(pebbles::core::router::path())
//!     .route("/", || component(home_page))
//!     .param_route("/user/:id", |p| {
//!         let id = p.get("id").unwrap_or_default().to_string();
//!         component(move || user_page(&id))
//!     })
//!     .fallback(|| component(not_found))
//! ```
//!
//! Matching is **registration order, first match wins** — register a literal
//! (`/user/new`) before its pattern (`/user/:id`) to give the literal precedence.
//! Segment splitting ignores leading/trailing slashes, so `"/user/:id"`,
//! `"user/:id"`, and a current path of `"/user/42/"` all line up.

use crate::widgets::gap_h;
use std::collections::BTreeMap;
use std::rc::Rc;

use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A navigation history — a stack of route names. Keep one in a signal; it is
/// `Clone` + `Default` and mutated through `signal.update(..)`.
#[derive(Clone, Default, Debug)]
pub struct NavStack {
    stack: Vec<String>,
}

impl NavStack {
    /// A stack starting at `initial`.
    pub fn new(initial: impl Into<String>) -> Self {
        NavStack { stack: vec![initial.into()] }
    }

    /// The current (top) route.
    pub fn current(&self) -> &str {
        self.stack.last().map(String::as_str).unwrap_or("")
    }

    /// Push a new route (keeps history for `pop`).
    pub fn push(&mut self, route: impl Into<String>) {
        self.stack.push(route.into());
    }

    /// Replace the current route in place (no history entry). Typical for a
    /// side-nav / tab selection.
    pub fn replace(&mut self, route: impl Into<String>) {
        match self.stack.last_mut() {
            Some(top) => *top = route.into(),
            None => self.stack.push(route.into()),
        }
    }

    /// Pop back to the previous route. Returns `false` if already at the root.
    pub fn pop(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }

    /// Whether there is history to pop.
    pub fn can_pop(&self) -> bool {
        self.stack.len() > 1
    }

    /// The number of entries in the history.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

/// The path values captured by a [`param_route`](RouteView::param_route) pattern —
/// e.g. matching `/user/:id` against `/user/42` yields `{ "id": "42" }`. Values are
/// URL-path segments exactly as they appear in the current route; decode further if
/// your app percent-encodes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RouteParams {
    params: BTreeMap<String, String>,
}

impl RouteParams {
    /// The captured value for `name` (the `:name` in the pattern), if present.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(String::as_str)
    }

    /// The captured value for `name`, or `default` when the segment is absent.
    pub fn get_or<'a>(&'a self, name: &str, default: &'a str) -> &'a str {
        self.get(name).unwrap_or(default)
    }

    /// Whether any path parameter was captured.
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }

    /// Iterate the captured `(name, value)` pairs (sorted by name).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.params.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// One compiled pattern segment.
#[derive(Clone)]
enum Seg {
    /// A literal segment that must match exactly.
    Lit(String),
    /// A `:name` capture segment.
    Param(String),
}

/// Split a path/pattern into non-empty segments (leading/trailing slashes ignored).
fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Compile a pattern string (`/user/:id`) into segments.
fn compile(pattern: &str) -> Vec<Seg> {
    segments(pattern)
        .into_iter()
        .map(|s| match s.strip_prefix(':') {
            Some(name) => Seg::Param(name.to_string()),
            None => Seg::Lit(s.to_string()),
        })
        .collect()
}

/// Match a compiled pattern against a current path, capturing `:name` segments.
/// Returns `None` unless every segment lines up (literals equal, counts match).
fn match_path(pattern: &[Seg], current: &str) -> Option<RouteParams> {
    let segs = segments(current);
    if segs.len() != pattern.len() {
        return None;
    }
    let mut params = BTreeMap::new();
    for (pat, seg) in pattern.iter().zip(segs.iter()) {
        match pat {
            Seg::Lit(lit) if lit != seg => return None,
            Seg::Lit(_) => {}
            Seg::Param(name) => {
                params.insert(name.clone(), (*seg).to_string());
            }
        }
    }
    Some(RouteParams { params })
}

type PageBuilder = Rc<dyn Fn() -> AnyWidget>;
type ParamBuilder = Rc<dyn Fn(&RouteParams) -> AnyWidget>;

/// A registered route: an exact-string match, or a `:param` pattern.
#[derive(Clone)]
enum RouteEntry {
    /// Matches when the current route equals `name` exactly.
    Exact(String, PageBuilder),
    /// Matches when the current path fits `segs`, passing the captures to `builder`.
    Pattern { segs: Vec<Seg>, builder: ParamBuilder },
}

/// Renders the page for the current route. Only the matching route's builder runs,
/// so inactive pages are never constructed. Routes are tried in registration order,
/// first match wins (see [`param_route`](RouteView::param_route)).
#[derive(Clone)]
pub struct RouteView {
    current: String,
    routes: Vec<RouteEntry>,
    fallback: Option<PageBuilder>,
}

/// Create a [`RouteView`] for `current` route.
pub fn route_view(current: impl Into<String>) -> RouteView {
    RouteView { current: current.into(), routes: Vec::new(), fallback: None }
}

impl RouteView {
    /// Register a route → page builder, matched by **exact** name. The builder
    /// returns any widget (e.g. `|| component(home_screen)`), no `.into_widget()`
    /// needed.
    pub fn route<F, W>(mut self, name: impl Into<String>, builder: F) -> Self
    where
        F: Fn() -> W + 'static,
        W: IntoWidget,
    {
        let builder: PageBuilder = Rc::new(move || builder().into_widget());
        self.routes.push(RouteEntry::Exact(name.into(), builder));
        self
    }

    /// Register a **path-pattern** route (`/user/:id`). On a match the builder
    /// receives the captured [`RouteParams`]; only the active page is built.
    /// Patterns and exact routes share one first-match-wins order, so register a
    /// literal before its pattern to give the literal precedence.
    pub fn param_route<F, W>(mut self, pattern: impl AsRef<str>, builder: F) -> Self
    where
        F: Fn(&RouteParams) -> W + 'static,
        W: IntoWidget,
    {
        let segs = compile(pattern.as_ref());
        let builder: ParamBuilder = Rc::new(move |p| builder(p).into_widget());
        self.routes.push(RouteEntry::Pattern { segs, builder });
        self
    }

    /// A page to show when no route matches.
    pub fn fallback<F, W>(mut self, builder: F) -> Self
    where
        F: Fn() -> W + 'static,
        W: IntoWidget,
    {
        self.fallback = Some(Rc::new(move || builder().into_widget()));
        self
    }
}

impl IntoWidget for RouteView {
    fn into_widget(self) -> AnyWidget {
        for entry in &self.routes {
            match entry {
                RouteEntry::Exact(name, builder) if *name == self.current => {
                    return builder();
                }
                RouteEntry::Exact(..) => {}
                RouteEntry::Pattern { segs, builder } => {
                    if let Some(params) = match_path(segs, &self.current) {
                        return builder(&params);
                    }
                }
            }
        }
        match &self.fallback {
            Some(builder) => builder(),
            None => gap_h(0.0).into_widget(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_and_captures_a_param() {
        let p = compile("/user/:id");
        let got = match_path(&p, "/user/42").expect("should match");
        assert_eq!(got.get("id"), Some("42"));
    }

    #[test]
    fn multiple_params_capture_by_name() {
        let p = compile("/org/:org/repo/:repo");
        let got = match_path(&p, "/org/pebbles/repo/docs").expect("should match");
        assert_eq!(got.get("org"), Some("pebbles"));
        assert_eq!(got.get("repo"), Some("docs"));
    }

    #[test]
    fn literal_segment_must_equal() {
        let p = compile("/user/:id/edit");
        assert!(match_path(&p, "/user/7/view").is_none());
        assert!(match_path(&p, "/user/7/edit").is_some());
    }

    #[test]
    fn segment_count_must_match() {
        let p = compile("/user/:id");
        assert!(match_path(&p, "/user").is_none());
        assert!(match_path(&p, "/user/7/extra").is_none());
    }

    #[test]
    fn slashes_are_normalized() {
        let p = compile("user/:id");
        assert!(match_path(&p, "/user/9/").is_some());
        assert_eq!(match_path(&p, "/user/9/").and_then(|r| r.get("id").map(str::to_owned)), Some("9".into()));
    }

    #[test]
    fn root_matches_empty_path() {
        let p = compile("/");
        assert!(match_path(&p, "/").is_some());
        assert!(match_path(&p, "").is_some());
        assert!(match_path(&p, "/x").is_none());
    }

    #[test]
    fn get_or_falls_back() {
        let params = RouteParams::default();
        assert_eq!(params.get_or("id", "none"), "none");
        assert!(params.is_empty());
    }
}
