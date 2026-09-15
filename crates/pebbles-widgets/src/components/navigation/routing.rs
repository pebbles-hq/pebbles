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

use crate::widgets::{gap_h, gesture_detector};
use std::collections::BTreeMap;
use std::rc::Rc;

use pebbles_core::router;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    Component, FocusNode, component, component_props, consume_context, on_mount, provide_context,
};

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

    /// The captured value for `name`, **parsed** into `T` (e.g. `get_as::<u64>("id")`).
    /// `None` if the segment is absent or doesn't parse — so a `/user/:id` route can
    /// take a real number instead of a `&str`.
    pub fn get_as<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
        self.get(name)?.parse().ok()
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
    /// A `:name` capture segment (one segment).
    Param(String),
    /// A `*name` catch-all — captures the whole rest of the path (must be last).
    Wildcard(String),
}

/// Split a path/pattern into non-empty segments (leading/trailing slashes ignored).
fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Compile a pattern string (`/user/:id`, `/files/*rest`) into segments.
fn compile(pattern: &str) -> Vec<Seg> {
    segments(pattern)
        .into_iter()
        .map(|s| {
            if let Some(name) = s.strip_prefix(':') {
                Seg::Param(name.to_string())
            } else if let Some(name) = s.strip_prefix('*') {
                Seg::Wildcard(name.to_string())
            } else {
                Seg::Lit(s.to_string())
            }
        })
        .collect()
}

/// Match a compiled pattern against a current path, capturing `:name` and a trailing
/// `*name`. Returns `None` unless every segment lines up.
fn match_path(pattern: &[Seg], current: &str) -> Option<RouteParams> {
    let segs = segments(current);

    // A trailing `*name` matches the fixed prefix, then captures the rest (possibly
    // empty) as a single `/`-joined value.
    if let Some(Seg::Wildcard(name)) = pattern.last() {
        let fixed = &pattern[..pattern.len() - 1];
        if segs.len() < fixed.len() {
            return None;
        }
        let mut params = BTreeMap::new();
        for (pat, seg) in fixed.iter().zip(segs.iter()) {
            match pat {
                Seg::Lit(lit) if lit != seg => return None,
                Seg::Lit(_) => {}
                Seg::Param(n) => {
                    params.insert(n.clone(), (*seg).to_string());
                }
                Seg::Wildcard(_) => return None, // a wildcard is only valid as the last segment
            }
        }
        params.insert(name.clone(), segs[fixed.len()..].join("/"));
        return Some(RouteParams { params });
    }

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
            Seg::Wildcard(_) => unreachable!("handled above"),
        }
    }
    Some(RouteParams { params })
}

/// Whether `current`'s leading segments match `prefix` (used for nested layouts —
/// `/settings` prefixes `/settings/profile`). Params/wildcards in the prefix match any
/// segment.
fn prefix_matches(prefix: &[Seg], current: &str) -> bool {
    let segs = segments(current);
    if segs.len() < prefix.len() {
        return false;
    }
    prefix.iter().zip(segs.iter()).all(|(pat, seg)| !matches!(pat, Seg::Lit(l) if l != seg))
}

type PageBuilder = Rc<dyn Fn() -> AnyWidget>;
type ParamBuilder = Rc<dyn Fn(&RouteParams) -> AnyWidget>;

/// A registered route: an exact-string match, a `:param` pattern, or a nested layout.
#[derive(Clone)]
enum RouteEntry {
    /// Matches when the current route equals `name` exactly.
    Exact(String, PageBuilder),
    /// Matches when the current path fits `segs`, passing the captures to `builder`.
    Pattern { segs: Vec<Seg>, builder: ParamBuilder },
    /// Matches when the current path is under `prefix`: renders `layout`, which places
    /// the matched child route via [`outlet`].
    Nested { prefix: Vec<Seg>, layout: PageBuilder, child: Rc<RouteView> },
}

/// The nested child route the enclosing layout should render — provided into context
/// by a [`RouteView::nest`] match and consumed by [`outlet`].
#[derive(Clone)]
struct OutletContent(Rc<dyn Fn() -> AnyWidget>);

struct NestedProps {
    outlet: Rc<dyn Fn() -> AnyWidget>,
    layout: PageBuilder,
}

fn nested_render(p: &NestedProps) -> AnyWidget {
    // Make the child route available to `outlet()` anywhere in the layout subtree,
    // then render the layout.
    provide_context(OutletContent(p.outlet.clone()));
    (p.layout)()
}

fn outlet_render() -> AnyWidget {
    match consume_context::<OutletContent>() {
        Some(content) => (content.0)(),
        None => gap_h(0.0).into_widget(),
    }
}

/// Render the matched **child** route of the enclosing [`RouteView::nest`] layout
/// (SolidJS `<Outlet>` / the router's `props.children`). Place it wherever the child
/// page should appear inside the layout's chrome. Renders nothing outside a nested
/// layout.
pub fn outlet() -> Component {
    component(outlet_render)
}

/// Renders the page for the current route. Only the matching route's builder runs,
/// so inactive pages are never constructed. Routes are tried in registration order,
/// first match wins (see [`param_route`](RouteView::param_route)).
#[derive(Clone)]
pub struct RouteView {
    current: String,
    routes: Vec<RouteEntry>,
    fallback: Option<PageBuilder>,
    transition: Option<f64>,
}

/// Create a [`RouteView`] for `current` route.
pub fn route_view(current: impl Into<String>) -> RouteView {
    RouteView { current: current.into(), routes: Vec::new(), fallback: None, transition: None }
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

    /// Register a **nested layout**: when the current path is under `prefix`, render
    /// `layout` (shared chrome — sidebar, header) and let it place the matched child
    /// route with [`outlet`]. `children` builds the child routes (matched against the
    /// full current path), so a layout's pages aren't copy-pasted per route:
    ///
    /// ```ignore
    /// route_view(router::path())
    ///     .nest("/settings", settings_layout, |r| r
    ///         .route("/settings/profile", || component(profile))
    ///         .route("/settings/billing", || component(billing)))
    ///     .route("/", || component(home))
    /// // settings_layout renders its sidebar + `outlet()`; the outlet shows profile/billing.
    /// ```
    pub fn nest<F, W>(
        mut self,
        prefix: impl AsRef<str>,
        layout: F,
        children: impl FnOnce(RouteView) -> RouteView,
    ) -> Self
    where
        F: Fn() -> W + 'static,
        W: IntoWidget,
    {
        let child = children(route_view(self.current.clone()));
        let layout: PageBuilder = Rc::new(move || layout().into_widget());
        self.routes.push(RouteEntry::Nested {
            prefix: compile(prefix.as_ref()),
            layout,
            child: Rc::new(child),
        });
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

    /// Animate route changes: cross-fade the matched page over `secs` seconds when
    /// the path changes (wraps the output in an `animated_switcher` keyed by the
    /// current path). Off by default (instant swap).
    pub fn transition(mut self, secs: f64) -> Self {
        self.transition = Some(secs);
        self
    }

    /// The matched page for the current route (before any transition wrapper).
    fn render_match(&self) -> AnyWidget {
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
                RouteEntry::Nested { prefix, layout, child } => {
                    if prefix_matches(prefix, &self.current) {
                        let child = child.clone();
                        let outlet: Rc<dyn Fn() -> AnyWidget> =
                            Rc::new(move || (*child).clone().into_widget());
                        return component_props(
                            nested_render,
                            NestedProps { outlet, layout: layout.clone() },
                        )
                        .into_widget();
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

impl IntoWidget for RouteView {
    fn into_widget(self) -> AnyWidget {
        let matched = self.render_match();
        match self.transition {
            Some(secs) => {
                // Key the switcher by the current path so a route change cross-fades.
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                std::hash::Hash::hash(&self.current, &mut hasher);
                let key = std::hash::Hasher::finish(&hasher);
                crate::widgets::animated_switcher(key, matched).duration(secs).into_widget()
            }
            None => matched,
        }
    }
}

// ---------------------------------------------------------------------------
// Navigation widgets — link (active-aware), redirect, use_match.
// ---------------------------------------------------------------------------

struct LinkProps {
    to: String,
    builder: Rc<dyn Fn(bool) -> AnyWidget>,
}

fn link_render(p: &LinkProps) -> AnyWidget {
    let active = router::path() == p.to; // reactive: re-styles on navigation
    let child = (p.builder)(active);
    let to = p.to.clone();
    gesture_detector(child)
        .on_tap(move || {
            router::navigate(&to);
        })
        .into_widget()
}

/// A navigable link (SolidJS `<A>`): tapping it navigates to `to`, and `builder`
/// receives whether the link is **active** (its target is the current path) so it can
/// style itself. Reactive — the active state updates on navigation.
///
/// ```ignore
/// link("/inbox", |active| text("Inbox").color(if active { ACCENT } else { FG }))
/// ```
///
/// For prefix-active (a section link active on any child route), gate the styling on
/// [`use_match`] instead.
pub fn link<F, W>(to: impl Into<String>, builder: F) -> Component
where
    F: Fn(bool) -> W + 'static,
    W: IntoWidget,
{
    let builder: Rc<dyn Fn(bool) -> AnyWidget> = Rc::new(move |active| builder(active).into_widget());
    component_props(link_render, LinkProps { to: to.into(), builder })
}

fn redirect_render(to: &String) -> AnyWidget {
    let to = to.clone();
    // Replace once on mount (no history entry, no Back-trap); renders nothing.
    on_mount(move || {
        router::replace(&to);
    });
    gap_h(0.0).into_widget()
}

/// A declarative redirect (SolidJS `<Navigate>`): on mount it `replace`s the current
/// route with `to` and renders nothing. Drop it in a route arm to send that route
/// elsewhere (`/` → `/home`, an unauthorized route → `/login`).
pub fn redirect(to: impl Into<String>) -> Component {
    component_props(redirect_render, to.into())
}

/// Whether the current route matches `pattern` (SolidJS `useMatch`) — reactive, for
/// active-link styling or conditional UI. Supports the same `:param` / `*wildcard`
/// patterns as [`RouteView::param_route`].
///
/// ```ignore
/// let on_settings = use_match("/settings/*rest"); // active on any settings sub-page
/// ```
pub fn use_match(pattern: impl AsRef<str>) -> bool {
    match_path(&compile(pattern.as_ref()), &router::path()).is_some()
}

/// Move focus to `node` whenever the route changes — the accessibility pattern for
/// in-app navigation, so a screen reader announces the newly-shown view. Attach `node`
/// (from `create_focus()`) to the routed content container and call this once at the
/// top of the component that owns the router view.
///
/// ```ignore
/// let content_focus = create_focus();
/// use_route_focus(&content_focus);
/// // …attach content_focus to the main content region…
/// ```
pub fn use_route_focus(node: &FocusNode) {
    let node = *node;
    router::on_route_change(move |_| node.request_focus());
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

    #[test]
    fn typed_params_parse() {
        let p = compile("/user/:id");
        let got = match_path(&p, "/user/42").unwrap();
        assert_eq!(got.get_as::<u64>("id"), Some(42));
        assert_eq!(got.get_as::<u64>("missing"), None);
        let bad = match_path(&p, "/user/abc").unwrap();
        assert_eq!(bad.get_as::<u64>("id"), None);
    }

    #[test]
    fn wildcard_captures_the_rest() {
        let p = compile("/files/*rest");
        let got = match_path(&p, "/files/a/b/c.txt").unwrap();
        assert_eq!(got.get("rest"), Some("a/b/c.txt"));
        // Matches with an empty rest, and requires the fixed prefix.
        assert_eq!(match_path(&p, "/files").unwrap().get("rest"), Some(""));
        assert!(match_path(&p, "/other/x").is_none());
    }

    #[test]
    fn nested_prefix_matches_children() {
        let prefix = compile("/settings");
        assert!(prefix_matches(&prefix, "/settings/profile"));
        assert!(prefix_matches(&prefix, "/settings"));
        assert!(!prefix_matches(&prefix, "/dashboard"));
    }
}
