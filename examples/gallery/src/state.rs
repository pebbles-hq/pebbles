//! Global app state — a single [`Signal`] holding the current route.
//!
//! This shows SolidJS's headline feature: the **same** `create_signal` primitive is
//! used for global state. Created once at app scope (via [`init`]), it is read and
//! written from any component without prop-drilling. `route().get()` subscribes the
//! calling component; `route().set(..)` re-renders everyone who read it.

use std::cell::RefCell;

use pebbles::prelude::*;

thread_local! {
    static ROUTE: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
}

/// Create the global route signal (call once, before any component renders, so it
/// is owned globally rather than by a component).
pub fn init() {
    let _ = route();
}

/// The global current-route signal.
pub fn route() -> Signal<String> {
    ROUTE.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_signal(String::from("overview")));
        }
        cell.unwrap()
    })
}

/// Navigate to a route.
pub fn navigate(to: &str) {
    route().set(to.to_string());
}

/// A single sidebar entry: (route id, icon, label).
pub type Route = (&'static str, IconKind, &'static str);

/// A labelled group of routes — the sidebar renders one section per group so
/// components of different categories are never jumbled together.
pub struct NavGroup {
    pub label: &'static str,
    pub routes: &'static [Route],
}

/// The categorized sidebar. Groups mirror the component taxonomy
/// (input / display / layout / navigation) plus foundations.
pub const NAV: &[NavGroup] = &[
    NavGroup { label: "GET STARTED", routes: &[("overview", IconKind::Circle, "Overview")] },
    NavGroup {
        label: "INPUT",
        routes: &[
            ("buttons", IconKind::Star, "Buttons"),
            ("text-fields", IconKind::Menu, "Text Fields"),
            ("select", IconKind::ChevronDown, "Select"),
            ("toggles", IconKind::Check, "Toggles"),
            ("slider", IconKind::Minus, "Slider"),
        ],
    },
    NavGroup {
        label: "DISPLAY",
        routes: &[
            ("surfaces", IconKind::Info, "Surfaces"),
            ("data", IconKind::ArrowRight, "Data & Desktop"),
            ("typography", IconKind::Minus, "Typography"),
            ("icons", IconKind::Search, "Icons"),
        ],
    },
    NavGroup { label: "LAYOUT", routes: &[("layout", IconKind::Menu, "Layout")] },
    NavGroup {
        label: "NAVIGATION",
        routes: &[("navigation", IconKind::ChevronRight, "Navigation")],
    },
    NavGroup {
        label: "FOUNDATIONS",
        routes: &[
            ("colors", IconKind::Circle, "Colors"),
            ("styling", IconKind::Dot, "Styling"),
        ],
    },
];

pub fn label_for(route: &str) -> &'static str {
    NAV.iter()
        .flat_map(|g| g.routes.iter())
        .find(|(r, _, _)| *r == route)
        .map(|(_, _, l)| *l)
        .unwrap_or("Pebbles")
}
