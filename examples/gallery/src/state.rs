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
    static COUNTER: RefCell<Option<Signal<i32>>> = const { RefCell::new(None) };
    static PING: RefCell<Option<Channel<String>>> = const { RefCell::new(None) };
}

/// Create the global app-scope state (call once, before any component renders, so
/// it's owned globally — and thus shared across windows — rather than by a component).
pub fn init() {
    let _ = route();
    let _ = counter();
    let _ = ping();
}

/// A counter shared across every window (the same signal, read by capture).
pub fn counter() -> Signal<i32> {
    COUNTER.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_signal(0));
        }
        cell.unwrap()
    })
}

/// A typed cross-window message channel.
pub fn ping() -> Channel<String> {
    PING.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(channel());
        }
        cell.unwrap()
    })
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
pub type Route = (&'static str, IconData, &'static str);

/// A labelled group of routes — the sidebar renders one section per group so
/// components of different categories are never jumbled together.
pub struct NavGroup {
    pub label: &'static str,
    pub routes: &'static [Route],
}

/// The categorized sidebar. Groups mirror the component taxonomy
/// (input / display / layout / navigation) plus foundations.
pub const NAV: &[NavGroup] = &[
    NavGroup { label: "GET STARTED", routes: &[("overview", lucide::LAYOUT_DASHBOARD, "Overview")] },
    NavGroup {
        label: "INPUT",
        routes: &[
            ("buttons", lucide::MOUSE_POINTER_CLICK, "Buttons"),
            ("button-group", lucide::COLUMNS_2, "Button Group"),
            ("text-fields", lucide::TEXT_CURSOR_INPUT, "Text Fields"),
            ("date-picker", lucide::CALENDAR_DAYS, "Date Picker"),
            ("select", lucide::CHEVRON_DOWN, "Select & Menus"),
            ("combobox", lucide::SEARCH, "Combobox"),
            ("command", lucide::FILE_TERMINAL, "Command"),
            ("toggles", lucide::TOGGLE_RIGHT, "Toggles"),
            ("radio-group", lucide::CIRCLE_DOT, "Radio Group"),
            ("slider", lucide::SLIDERS_HORIZONTAL, "Slider"),
            ("dialog", lucide::MESSAGE_SQUARE, "Dialog"),
            ("windows", lucide::APP_WINDOW, "Windows & IPC"),
        ],
    },
    NavGroup {
        label: "DISPLAY",
        routes: &[
            ("badge", lucide::TAG, "Badge"),
            ("alert", lucide::BELL, "Alert"),
            ("skeleton", lucide::BOXES, "Skeleton"),
            ("kbd", lucide::KEYBOARD, "Kbd"),
            ("empty", lucide::INBOX, "Empty"),
            ("card", lucide::CREDIT_CARD, "Card"),
            ("avatar", lucide::USER, "Avatar"),
            ("separator", lucide::SEPARATOR_HORIZONTAL, "Separator"),
            ("progress", lucide::GAUGE, "Progress"),
            ("list", lucide::LIST, "List"),
            ("data-table", lucide::TABLE, "Data Table"),
            ("tree", lucide::FOLDER_TREE, "Tree"),
            ("split-view", lucide::COLUMNS_2, "Split View"),
            ("virtualization", lucide::LAYERS, "Virtualization"),
            ("typography", lucide::TYPE, "Typography"),
            ("icons", lucide::SHAPES, "Icons"),
            ("images", lucide::IMAGE, "Images"),
        ],
    },
    NavGroup {
        label: "LAYOUT",
        routes: &[
            ("layout", lucide::LAYOUT_TEMPLATE, "Layout"),
            ("resizable", lucide::PANEL_LEFT, "Resizable"),
            ("collapsible", lucide::CHEVRONS_DOWN_UP, "Collapsible"),
        ],
    },
    NavGroup {
        label: "NAVIGATION",
        routes: &[
            ("tabs", lucide::SQUARE_STACK, "Tabs"),
            ("accordion", lucide::LIST_COLLAPSE, "Accordion"),
            ("breadcrumb", lucide::CHEVRON_RIGHT, "Breadcrumb"),
            ("menubar", lucide::MENU, "Menubar"),
            ("pagination", lucide::CHEVRONS_LEFT, "Pagination"),
        ],
    },
    NavGroup {
        label: "OVERLAYS",
        routes: &[("overlays", lucide::MESSAGE_CIRCLE, "Overlays & Feedback")],
    },
    NavGroup {
        label: "FOUNDATIONS",
        routes: &[
            ("colors", lucide::PALETTE, "Colors"),
            ("styling", lucide::PAINTBRUSH, "Styling"),
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
