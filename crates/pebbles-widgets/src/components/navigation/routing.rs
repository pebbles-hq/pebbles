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

use std::rc::Rc;

use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::SizedBox;

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

type PageBuilder = Rc<dyn Fn() -> AnyWidget>;

/// Renders the page for the current route. Only the matching route's builder runs,
/// so inactive pages are never constructed.
#[derive(Clone)]
pub struct RouteView {
    current: String,
    routes: Vec<(String, PageBuilder)>,
    fallback: Option<PageBuilder>,
}

/// Create a [`RouteView`] for `current` route.
pub fn route_view(current: impl Into<String>) -> RouteView {
    RouteView { current: current.into(), routes: Vec::new(), fallback: None }
}

impl RouteView {
    /// Register a route → page builder. The builder returns any widget
    /// (e.g. `|| component(home_screen)`), no `.into_widget()` needed.
    pub fn route<F, W>(mut self, name: impl Into<String>, builder: F) -> Self
    where
        F: Fn() -> W + 'static,
        W: IntoWidget,
    {
        self.routes.push((name.into(), Rc::new(move || builder().into_widget())));
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
        for (name, builder) in &self.routes {
            if *name == self.current {
                return builder();
            }
        }
        match &self.fallback {
            Some(builder) => builder(),
            None => SizedBox::spacer(0.0, 0.0).into_widget(),
        }
    }
}
