//! The router, MOUNTED end-to-end (§2 blocker): the matcher + history store have
//! unit tests, but nothing mounted a `RouteView` before. Here a real `route_view`
//! is mounted and driven through `router::navigate`, and we assert the rendered
//! subtree for exact routes, param routes, a nested layout + outlet, a declarative
//! `redirect`, and a navigation guard that blocks / redirects.
//!
//! The router is a single app-global (thread-local) history, so every test seeds a
//! known path first and any registered guard is removed before returning.

use std::cell::RefCell;

use pebbles_core::router::{self, NavGuard};
use pebbles_core::{IntoWidget, component};
use pebbles_testing::Harness;
use pebbles_widgets::{column, outlet, redirect, route_view, text};

thread_local! {
    /// Every page builder that ran this render pass records its name here, so a test
    /// can assert which route(s) actually built.
    static MARKS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn mark(name: &str) {
    MARKS.with(|m| m.borrow_mut().push(name.to_string()));
}

fn clear_marks() {
    MARKS.with(|m| m.borrow_mut().clear());
}

fn built(name: &str) -> bool {
    MARKS.with(|m| m.borrow().iter().any(|s| s == name))
}

fn settings_layout() -> impl IntoWidget {
    mark("settings-layout");
    // Shared chrome + the matched child route placed by `outlet()`.
    column(vec![text("settings").into_widget(), outlet().into_widget()])
}

/// The app under test — a `route_view` driven by the framework router's live path,
/// so `navigate`/`replace` re-render it exactly as in a real app.
fn app() -> impl IntoWidget {
    route_view(router::path())
        .route("/", || {
            mark("home");
            text("home")
        })
        .route("/home", || {
            mark("home");
            text("home")
        })
        .route("/new", || {
            mark("new");
            text("new")
        })
        .route("/redirect-me", || redirect("/home"))
        .param_route("/user/:id", |p| {
            mark(&format!("user:{}", p.get("id").unwrap_or_default()));
            text(format!("user {}", p.get("id").unwrap_or_default()))
        })
        .nest("/settings", settings_layout, |r| {
            r.route("/settings/profile", || {
                mark("profile");
                text("profile")
            })
            .route("/settings/billing", || {
                mark("billing");
                text("billing")
            })
        })
        .fallback(|| {
            mark("fallback");
            text("not found")
        })
}

fn mount_app_at(path: &str) -> Harness {
    router::navigate(path);
    clear_marks();
    let mut h = Harness::new();
    h.mount(|| component(app));
    h.draw();
    h
}

#[test]
fn exact_param_and_nested_routes_render_the_matching_page() {
    let mut h = mount_app_at("/");
    assert!(built("home"), "the root route renders home, got {:?}", MARKS.with(|m| m.borrow().clone()));

    // A param route captures and renders.
    clear_marks();
    router::navigate("/user/42");
    h.draw();
    assert!(built("user:42"), "the param route renders with the captured id");
    assert!(!built("home"), "and the previous page is gone");

    // A nested layout renders its chrome AND the matched child via the outlet.
    clear_marks();
    router::navigate("/settings/profile");
    h.frames(2);
    assert!(built("settings-layout"), "the nested layout renders");
    assert!(built("profile"), "and the outlet renders the matched child route");

    // An unmatched path falls through to the fallback.
    clear_marks();
    router::navigate("/nope");
    h.draw();
    assert!(built("fallback"), "an unmatched route renders the fallback");
}

#[test]
fn a_declarative_redirect_route_sends_you_onward() {
    let mut h = mount_app_at("/");
    clear_marks();
    router::navigate("/redirect-me");
    // Frame 1 mounts the redirect (its on_mount `replace`s); a following frame renders
    // the destination.
    h.frames(3);
    assert_eq!(router::path(), "/home", "the redirect replaced the route");
    assert!(built("home"), "and the destination page rendered");
}

#[test]
fn a_guard_blocks_and_redirects_navigation() {
    let mut h = mount_app_at("/");

    let guard = router::add_guard(|loc| {
        if loc.path().starts_with("/locked") {
            NavGuard::Block
        } else if loc.path() == "/old" {
            NavGuard::Redirect("/new".into())
        } else {
            NavGuard::Allow
        }
    });

    // Blocked: the navigation reports false and the tree stays on the current page.
    clear_marks();
    assert!(!router::navigate("/locked/x"), "a blocked navigation returns false");
    assert_eq!(router::path(), "/", "and the path is unchanged");
    h.draw();
    assert!(!built("fallback"), "the blocked target never rendered");

    // Redirected: /old is rewritten to /new before it lands.
    clear_marks();
    assert!(router::navigate("/old"), "the redirected navigation still moves");
    assert_eq!(router::path(), "/new", "to the guard's target");
    h.draw();
    assert!(built("new"), "and the target page rendered");

    router::remove_guard(guard);
}
