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
            // Dev hook: GALLERY_ROUTE=<route-id> opens straight on a screen.
            let initial = std::env::var("GALLERY_ROUTE").unwrap_or_else(|_| String::from("overview"));
            *cell = Some(create_signal(initial));
        }
        cell.unwrap()
    })
}

/// Navigate to a route.
pub fn navigate(to: &str) {
    pebbles::core::log::info(pebbles::core::log::Cat::Nav, format!("navigate → {to}"));
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
            ("chips", lucide::BADGE_CHECK, "Chip"),
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
            ("file-explorer", lucide::FOLDER_TREE, "File Explorer"),
            ("typography", lucide::TYPE, "Typography"),
            ("fonts", lucide::PILCROW, "Fonts"),
            ("icons", lucide::SHAPES, "Icons"),
            ("images", lucide::IMAGE, "Images"),
        ],
    },
    NavGroup {
        label: "LAYOUT",
        routes: &[
            ("rows", lucide::ROWS_2, "Row"),
            ("columns", lucide::COLUMNS_2, "Column"),
            ("stacks", lucide::LAYERS, "Stack"),
            ("wraps", lucide::TEXT_WRAP, "Wrap"),
            ("boxes", lucide::BOX, "Boxes & Sizing"),
            ("scrolling", lucide::MOVE_VERTICAL, "Scroll & Motion"),
            ("canvas", lucide::SHAPES, "Canvas"),
            ("list-view", lucide::LIST_ORDERED, "List View"),
            ("grid-view", lucide::LAYOUT_GRID, "Grid View"),
            ("split-view", lucide::COLUMNS_2, "Split View"),
            ("resizable", lucide::PANEL_LEFT, "Resizable"),
            ("accordion", lucide::LIST_COLLAPSE, "Accordion"),
            ("collapsible", lucide::CHEVRONS_DOWN_UP, "Collapsible"),
        ],
    },
    NavGroup {
        label: "MOTION & TRANSITIONS",
        routes: &[
            ("animated-container", lucide::WAND, "Animated Container"),
            ("animated-opacity", lucide::WAND, "Animated Opacity"),
            ("animated-scale", lucide::WAND, "Animated Scale"),
            ("animated-rotation", lucide::WAND, "Animated Rotation"),
            ("animated-slide", lucide::MOVE_VERTICAL, "Animated Slide"),
            ("animated-align", lucide::COLUMNS_2, "Animated Align"),
            ("animated-padding", lucide::BOX, "Animated Padding"),
            ("animated-positioned", lucide::LAYERS, "Animated Positioned"),
            ("fade-transition", lucide::WAND, "Fade Transition"),
            ("scale-transition", lucide::WAND, "Scale Transition"),
            ("rotation-transition", lucide::WAND, "Rotation Transition"),
            ("slide-transition", lucide::MOVE_VERTICAL, "Slide Transition"),
            ("size-transition", lucide::BOX, "Size Transition"),
            ("positioned-transition", lucide::LAYERS, "Positioned Transition"),
            ("decorated-box-transition", lucide::PAINTBRUSH, "Decorated Box Transition"),
            ("animated-switcher", lucide::LAYERS, "Animated Switcher"),
            ("animated-cross-fade", lucide::LAYERS, "Animated Cross Fade"),
            ("dismissible", lucide::INBOX, "Dismissible"),
            ("animated-list", lucide::LIST_ORDERED, "Animated List"),
            ("animated-grid", lucide::LAYOUT_GRID, "Animated Grid"),
            ("hero", lucide::WAND, "Hero"),
        ],
    },
    NavGroup {
        label: "DRAG & DROP / POINTER",
        routes: &[
            ("draggable", lucide::MOVE_VERTICAL, "Draggable"),
            ("drag-target", lucide::INBOX, "Drag Target"),
            ("long-press-draggable", lucide::MOVE_VERTICAL, "Long Press Draggable"),
            ("ignore-pointer", lucide::BOX, "Ignore Pointer"),
            ("absorb-pointer", lucide::BOX, "Absorb Pointer"),
            ("interactive-viewer", lucide::LAYOUT_GRID, "Interactive Viewer"),
            ("reorderable-list-view", lucide::LIST_ORDERED, "Reorderable List View"),
        ],
    },
    NavGroup {
        label: "LAYOUT (LONG-TAIL)",
        routes: &[
            ("indexed-stack", lucide::LAYERS, "Indexed Stack"),
            ("offstage", lucide::BOX, "Offstage"),
            ("visibility", lucide::BOX, "Visibility"),
            ("baseline", lucide::COLUMNS_2, "Baseline"),
            ("rotated-box", lucide::WAND, "Rotated Box"),
            ("unconstrained-box", lucide::BOX, "Unconstrained Box"),
            ("sized-overflow-box", lucide::BOX, "Sized Overflow Box"),
            ("fractional-translation", lucide::MOVE_VERTICAL, "Fractional Translation"),
            ("table-layout", lucide::LAYOUT_GRID, "Table (layout)"),
            ("custom-single-child", lucide::BOX, "Custom Single Child"),
            ("custom-multi-child", lucide::LAYOUT_GRID, "Custom Multi Child"),
            ("flow", lucide::WAND, "Flow"),
            ("layout-builder", lucide::COLUMNS_2, "Layout Builder"),
        ],
    },
    NavGroup {
        label: "PAINTING & EFFECTS",
        routes: &[
            ("clip-rect", lucide::BOX, "Clip Rect"),
            ("clip-oval", lucide::BOX, "Clip Oval"),
            ("clip-path", lucide::PAINTBRUSH, "Clip Path"),
            ("color-filtered", lucide::PAINTBRUSH, "Color Filtered"),
            ("shader-mask", lucide::PAINTBRUSH, "Shader Mask"),
        ],
    },
    NavGroup {
        label: "INPUT & FORMS (MORE)",
        routes: &[
            ("choice-chip", lucide::INBOX, "Choice Chip"),
            ("filter-chip", lucide::INBOX, "Filter Chip"),
            ("action-chip", lucide::INBOX, "Action Chip"),
            ("stepper", lucide::LIST_ORDERED, "Stepper"),
            ("selectable-text", lucide::SEARCH, "Selectable Text"),
        ],
    },
    NavGroup {
        label: "MATERIAL STAPLES",
        routes: &[
            ("fab", lucide::PLUS, "Floating Action Button"),
            ("grid-tile", lucide::LAYOUT_GRID, "Grid Tile"),
        ],
    },
    NavGroup {
        label: "ASYNC & A11Y",
        routes: &[
            ("stream-builder", lucide::WAND, "Stream Builder"),
            ("semantics-combinators", lucide::INFO, "Semantics Combinators"),
        ],
    },
    NavGroup {
        label: "SCAFFOLD & MOBILE",
        routes: &[
            ("media-query", lucide::INFO, "Media Query"),
            ("safe-area", lucide::BOX, "Safe Area"),
            ("orientation-builder", lucide::COLUMNS_2, "Orientation Builder"),
            ("scaffold-slots", lucide::LAYOUT_GRID, "Scaffold Slots"),
        ],
    },
    NavGroup {
        label: "LONG-TAIL",
        routes: &[
            ("placeholder", lucide::BOX, "Placeholder"),
            ("banner", lucide::INFO, "Banner"),
            ("scroll-notification", lucide::MOVE_VERTICAL, "Scroll Notification"),
            ("list-body", lucide::LIST, "List Body"),
            ("list-tiles", lucide::LIST_CHECKS, "List Tiles"),
            ("draggable-sheet", lucide::PANEL_BOTTOM, "Draggable Sheet"),
            ("scaffold-drawer", lucide::PANEL_LEFT, "Scaffold Drawer"),
            ("default-text-style", lucide::TYPE, "Default Text Style"),
            ("mobile-runtime", lucide::SMARTPHONE, "Mobile Runtime"),
        ],
    },
    NavGroup {
        label: "NAVIGATION",
        routes: &[
            ("tabs", lucide::SQUARE_STACK, "Tabs"),
            ("breadcrumb", lucide::CHEVRON_RIGHT, "Breadcrumb"),
            ("menubar", lucide::MENU, "Menubar"),
            ("pagination", lucide::CHEVRONS_LEFT, "Pagination"),
            ("toolbar", lucide::LAYOUT_DASHBOARD, "Toolbar & Status"),
            ("chrome", lucide::APP_WINDOW, "App Shell"),
            ("routing", lucide::LAYERS, "Routing"),
        ],
    },
    NavGroup {
        label: "OVERLAYS",
        routes: &[
            ("tooltips", lucide::INFO, "Tooltip"),
            ("popovers", lucide::SQUARE_ARROW_OUT_UP_LEFT, "Popover"),
            ("sheets", lucide::PANEL_RIGHT_OPEN, "Sheet & Drawer"),
            ("toasts", lucide::BELL, "Toast"),
            ("context-menu", lucide::MOUSE_POINTER_2, "Context Menu"),
        ],
    },
    NavGroup {
        label: "FOUNDATIONS",
        routes: &[
            ("colors", lucide::PALETTE, "Colors"),
            ("styling", lucide::PAINTBRUSH, "Styling"),
            ("context", lucide::TREE_DECIDUOUS, "Context & Scopes"),
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
