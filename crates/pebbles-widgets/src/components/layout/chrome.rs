//! App-shell chrome: [`Scaffold`] (composes the shell) plus the independent,
//! individually-optional [`TopPanel`], [`SideNav`] and [`BottomNav`].
//!
//! Each is standalone — an app can use a `SideNav` with no `TopPanel`, a `TopPanel`
//! with no `SideNav`, and so on. `Scaffold` just arranges whichever you provide:
//! top bar on top, side nav on the left, body filling the rest, bottom nav below.

use std::rc::Rc;

use pebbles_core::IntoCallback;
use pebbles_foundation::{
    Alignment, Color, CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize, palette,
};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, IconData, IconKind};

use crate::theme::{mix, theme};
use crate::widgets::{
    Container, Expanded, GestureDetector, Padding, SingleChildScrollView, center, column, gap_h, gap_w,
    positioned, row, spacer, stack, text,
};
use pebbles_core::children;
use pebbles_core::context::Callback;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, component_props, consume_context, create_signal, provide_context};

use crate::Side;
use crate::components::{icon, tooltip};

/// Context (C5): a [`SideNav`] provides this so its [`NavItem`]s render icon-only and
/// grow a right-side tooltip when the rail is collapsed. `bool` = collapsed.
#[derive(Clone, Copy)]
struct NavCollapsed(bool);

// ===========================================================================
// Scaffold
// ===========================================================================

/// The app shell. Arranges the optional chrome around a body that fills the rest.
#[derive(Clone)]
pub struct Scaffold {
    body: Option<AnyWidget>,
    top: Option<AnyWidget>,
    side: Option<AnyWidget>,
    bottom: Option<AnyWidget>,
    fab: Option<AnyWidget>,
    persistent_footer: Option<AnyWidget>,
    bottom_sheet: Option<AnyWidget>,
    drawer: Option<AnyWidget>,
    end_drawer: Option<AnyWidget>,
    background: Option<Color>,
}

/// Create a [`Scaffold`] with a `body`. Attach chrome with `.top()/.side()/.bottom()`.
pub fn scaffold(body: impl IntoWidget) -> Scaffold {
    Scaffold {
        body: Some(body.into_widget()),
        top: None,
        side: None,
        bottom: None,
        fab: None,
        persistent_footer: None,
        bottom_sheet: None,
        drawer: None,
        end_drawer: None,
        background: None,
    }
}

impl Scaffold {
    pub fn top(mut self, top: impl IntoWidget) -> Self {
        self.top = Some(top.into_widget());
        self
    }
    pub fn side(mut self, side: impl IntoWidget) -> Self {
        self.side = Some(side.into_widget());
        self
    }
    pub fn bottom(mut self, bottom: impl IntoWidget) -> Self {
        self.bottom = Some(bottom.into_widget());
        self
    }
    /// A floating action button ([`fab`](crate::fab)), overlaid at the bottom-right of
    /// the body (Flutter's `Scaffold.floatingActionButton`).
    pub fn fab(mut self, fab: impl IntoWidget) -> Self {
        self.fab = Some(fab.into_widget());
        self
    }
    /// A row of persistent action buttons pinned directly above the bottom bar, with a
    /// top divider (Flutter's `Scaffold.persistentFooterButtons`).
    pub fn persistent_footer(mut self, footer: impl IntoWidget) -> Self {
        self.persistent_footer = Some(footer.into_widget());
        self
    }
    /// A **persistent** (non-modal) bottom sheet pinned above the bottom bar — always
    /// visible, part of the layout, with a top divider (Flutter's `Scaffold.bottomSheet`).
    /// For a dismissible, scrim-backed sheet use the [`sheet`](crate::sheet) service.
    pub fn bottom_sheet(mut self, sheet: impl IntoWidget) -> Self {
        self.bottom_sheet = Some(sheet.into_widget());
        self
    }
    /// A left drawer opened by [`open_drawer`] (or [`drawer_button`]) — Flutter's
    /// `Scaffold.drawer`. Registered per-window on build; it slides in as a
    /// [`sheet`](crate::sheet) from the left.
    pub fn drawer(mut self, drawer: impl IntoWidget) -> Self {
        self.drawer = Some(drawer.into_widget());
        self
    }
    /// A right (end) drawer opened by [`open_end_drawer`] — Flutter's
    /// `Scaffold.endDrawer`. Slides in as a [`sheet`](crate::sheet) from the right.
    pub fn end_drawer(mut self, drawer: impl IntoWidget) -> Self {
        self.end_drawer = Some(drawer.into_widget());
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
}

// ---------------------------------------------------------------------------
// Drawer registry — Scaffold registers its (end-)drawer content per window so the
// free `open_drawer`/`open_end_drawer` functions can present it via the sheet
// service. Presentation lives in the conversation/app layer, not inside a render.
// ---------------------------------------------------------------------------

/// A window's registered `(drawer, end_drawer)` content.
type DrawerSlots = (Option<AnyWidget>, Option<AnyWidget>);

thread_local! {
    static DRAWERS: std::cell::RefCell<std::collections::HashMap<u32, DrawerSlots>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The drawer width used by [`open_drawer`]/[`open_end_drawer`] (logical px).
const DRAWER_WIDTH: f64 = 300.0;

fn register_drawers(drawer: Option<AnyWidget>, end_drawer: Option<AnyWidget>) {
    let window = pebbles_core::reactive::current_window();
    DRAWERS.with(|d| {
        d.borrow_mut().insert(window, (drawer, end_drawer));
    });
}

/// Open the current window's [`Scaffold::drawer`] as a left sheet. No-op if the
/// scaffold declared no drawer.
pub fn open_drawer() {
    let window = pebbles_core::reactive::current_window();
    let content = DRAWERS.with(|d| d.borrow().get(&window).and_then(|(l, _)| l.clone()));
    if let Some(content) = content {
        crate::sheet(content).side(Side::Left).size(DRAWER_WIDTH).open();
    }
}

/// Open the current window's [`Scaffold::end_drawer`] as a right sheet. No-op if the
/// scaffold declared no end drawer.
pub fn open_end_drawer() {
    let window = pebbles_core::reactive::current_window();
    let content = DRAWERS.with(|d| d.borrow().get(&window).and_then(|(_, r)| r.clone()));
    if let Some(content) = content {
        crate::sheet(content).side(Side::Right).size(DRAWER_WIDTH).open();
    }
}

/// A hamburger button that opens the [`Scaffold::drawer`] — drop it in a
/// [`TopPanel::leading`] slot (Flutter auto-inserts this; Pebbles keeps it explicit).
pub fn drawer_button() -> AnyWidget {
    crate::components::icon_button(pebbles_render::lucide::MENU).on_pressed(open_drawer).into_widget()
}

impl IntoWidget for Scaffold {
    fn into_widget(mut self) -> AnyWidget {
        let bg = self.background.unwrap_or(theme().colors.background);
        let body = self.body.take().unwrap_or_else(|| gap_h(0.0).into_widget());

        // Register (end-)drawer content so open_drawer/open_end_drawer can present it.
        register_drawers(self.drawer.take(), self.end_drawer.take());

        // side (fixed) + body (fills)
        let mut middle: AnyWidget = match self.side.take() {
            Some(side) => row(children![side, Expanded::new(Container::new().color(bg).child(body))])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .into_widget(),
            None => Container::new().color(bg).child(body).into_widget(),
        };

        // A floating action button overlays the body, pinned bottom-right.
        if let Some(fab) = self.fab.take() {
            middle = stack(children![middle, positioned(fab).right(16.0).bottom(16.0)]).into_widget();
        }

        let mut col: Vec<AnyWidget> = Vec::new();
        if let Some(top) = self.top.take() {
            col.push(top);
        }
        col.push(Expanded::new(middle).into_widget());
        // Persistent footer buttons sit above the bottom bar, with a top divider.
        if let Some(footer) = self.persistent_footer.take() {
            let c = theme().colors;
            col.push(
                Container::new()
                    .decoration(BoxDecoration::new().color(c.background).border(Border::new(c.border, 1.0)))
                    .padding(EdgeInsets::symmetric(12.0, 8.0))
                    .child(footer)
                    .into_widget(),
            );
        }
        // A persistent (non-modal) bottom sheet sits above the bottom bar: a card
        // surface with a top divider, always part of the layout.
        if let Some(sheet) = self.bottom_sheet.take() {
            let c = theme().colors;
            col.push(
                Container::new()
                    .decoration(BoxDecoration::new().color(c.card).border(Border::new(c.border, 1.0)))
                    .padding(EdgeInsets::all(16.0))
                    .child(sheet)
                    .into_widget(),
            );
        }
        if let Some(bottom) = self.bottom.take() {
            col.push(bottom);
        }

        column(col).cross_axis_alignment(CrossAxisAlignment::Stretch).into_widget()
    }
}

// ===========================================================================
// TopPanel
// ===========================================================================

/// A top app bar: optional leading widget, a title, and trailing actions.
#[derive(Clone)]
pub struct TopPanel {
    title: String,
    leading: Option<AnyWidget>,
    actions: Vec<AnyWidget>,
    height: f64,
}

/// Create a [`TopPanel`] with a title.
pub fn top_panel(title: impl Into<String>) -> TopPanel {
    TopPanel { title: title.into(), leading: None, actions: Vec::new(), height: 56.0 }
}

impl TopPanel {
    pub fn leading(mut self, leading: impl IntoWidget) -> Self {
        self.leading = Some(leading.into_widget());
        self
    }
    pub fn action(mut self, action: impl IntoWidget) -> Self {
        self.actions.push(action.into_widget());
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }
}

impl IntoWidget for TopPanel {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let mut items: Vec<AnyWidget> = Vec::new();
        if let Some(leading) = self.leading.take() {
            items.push(leading);
            items.push(gap_w(12.0).into_widget());
        }
        items.push(
            text(std::mem::take(&mut self.title)).size(16.0).semibold().color(c.foreground).into_widget(),
        );
        items.push(spacer().into_widget());
        for action in std::mem::take(&mut self.actions) {
            items.push(action);
            items.push(gap_w(6.0).into_widget());
        }

        Container::new()
            .decoration(BoxDecoration::new().color(c.background).border(Border::new(c.border, 1.0)))
            .height(self.height)
            .padding(EdgeInsets::symmetric(16.0, 0.0))
            .child(row(items).cross_axis_alignment(CrossAxisAlignment::Center))
            .into_widget()
    }
}

// ===========================================================================
// NavItem (shared by SideNav)
// ===========================================================================

/// A single side-nav row: optional icon + label, a selected flag, and a callback.
#[derive(Clone)]
pub struct NavItem {
    icon: Option<IconData>,
    label: String,
    selected: bool,
    on_select: Option<Callback>,
}

/// Create a [`NavItem`] with a label.
pub fn nav_item(label: impl Into<String>) -> NavItem {
    NavItem { icon: None, label: label.into(), selected: false, on_select: None }
}

impl NavItem {
    pub fn icon(mut self, kind: impl Into<IconData>) -> Self {
        self.icon = Some(kind.into());
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn on_select(mut self, cb: impl IntoCallback) -> Self {
        self.on_select = Some(cb.into_callback());
        self
    }
}

impl IntoWidget for NavItem {
    fn into_widget(self) -> AnyWidget {
        component_props(render_nav_item, self).into_widget()
    }
}

fn render_nav_item(w: &NavItem) -> AnyWidget {
    let c = theme().colors;
    let collapsed = consume_context::<NavCollapsed>().map(|x| x.0).unwrap_or(false);
    let hovered = create_signal(false);
    let bg = if w.selected {
        c.accent
    } else if hovered.get() {
        mix(c.background, c.accent, 0.6)
    } else {
        palette::TRANSPARENT
    };
    let fg = if w.selected { c.accent_foreground } else { c.foreground };
    let weight = if w.selected { 600.0 } else { 500.0 };

    // Collapsed rail (C5): icon centered, label hidden; falls back to the label's
    // first glyph when an item has no icon so the row is never blank.
    let inner: AnyWidget = if collapsed {
        let glyph: AnyWidget = match w.icon {
            Some(kind) => icon(kind).size(18.0).color(fg).into_widget(),
            None => text(w.label.chars().next().map(|ch| ch.to_string()).unwrap_or_default())
                .size(14.0)
                .weight(weight)
                .color(fg)
                .into_widget(),
        };
        center(glyph).into_widget()
    } else {
        let mut cells: Vec<AnyWidget> = Vec::new();
        if let Some(kind) = w.icon {
            cells.push(icon(kind).size(18.0).color(fg).into_widget());
            cells.push(gap_w(10.0).into_widget());
        }
        cells.push(text(w.label.clone()).size(14.0).weight(weight).color(fg).into_widget());
        row(cells).into_widget()
    };

    let container = Container::new()
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(theme().radius)))
        .padding(EdgeInsets::symmetric(if collapsed { 0.0 } else { 10.0 }, 9.0))
        .child(inner);

    let mut gesture = GestureDetector::new(container)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false));
    if let Some(cb) = w.on_select.clone() {
        gesture = gesture.on_tap(cb);
    }
    let row_widget = gesture.into_widget();

    // Collapsed rows surface their label as a right-side tooltip (C5 + C2).
    if collapsed { tooltip(w.label.clone(), row_widget).side(Side::Right).into_widget() } else { row_widget }
}

// ===========================================================================
// SideNav
// ===========================================================================

/// A vertical side navigation panel with an optional header/footer and a list of
/// items (usually [`NavItem`]s, but any widget works).
#[derive(Clone)]
pub struct SideNav {
    width: f64,
    header: Option<AnyWidget>,
    footer: Option<AnyWidget>,
    items: Vec<AnyWidget>,
    collapsible: bool,
    collapsed: bool,
    on_collapse_changed: Option<Rc<dyn Fn(bool)>>,
}

/// The collapsed icon-rail width (C5).
const RAIL_WIDTH: f64 = 56.0;

/// Create an empty [`SideNav`]; add rows with `.item(..)`.
pub fn side_nav() -> SideNav {
    SideNav {
        width: 240.0,
        header: None,
        footer: None,
        items: Vec::new(),
        collapsible: false,
        collapsed: false,
        on_collapse_changed: None,
    }
}

impl SideNav {
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    pub fn header(mut self, header: impl IntoWidget) -> Self {
        self.header = Some(header.into_widget());
        self
    }
    pub fn footer(mut self, footer: impl IntoWidget) -> Self {
        self.footer = Some(footer.into_widget());
        self
    }
    pub fn item(mut self, item: impl IntoWidget) -> Self {
        self.items.push(item.into_widget());
        self
    }
    /// Show a chevron toggle (pinned bottom) that collapses the nav to a 56px icon
    /// rail. Pair with `.collapsed(..)` + `.on_collapse_changed(..)` — the state is
    /// controlled, like every value in the catalog (C5).
    pub fn collapsible(mut self, yes: bool) -> Self {
        self.collapsible = yes;
        self
    }
    /// The current collapsed state (controlled).
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
    /// Called with the requested new collapsed state when the chevron is clicked.
    pub fn on_collapse_changed(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_collapse_changed = Some(Rc::new(f));
        self
    }
}

impl IntoWidget for SideNav {
    fn into_widget(self) -> AnyWidget {
        component_props(render_side_nav, self).into_widget()
    }
}

fn render_side_nav(p: &SideNav) -> AnyWidget {
    let c = theme().colors;
    let collapsed = p.collapsible && p.collapsed;
    // Width animates between the full width and the icon rail (C5).
    let target = if collapsed { RAIL_WIDTH } else { p.width };
    let w = animated(target, 0.15);

    // Tell descendant NavItems whether to render as an icon rail.
    provide_context(NavCollapsed(collapsed));

    // Items live in a scroll view that fills the space between a fixed header and
    // footer, so they scroll (never clip) when the window is short.
    let mut items: Vec<AnyWidget> = Vec::new();
    for (i, item) in p.items.iter().enumerate() {
        if i > 0 {
            items.push(gap_h(2.0).into_widget());
        }
        items.push(item.clone());
    }
    let scroller = SingleChildScrollView::vertical(Padding::new(
        EdgeInsets::symmetric(if collapsed { 8.0 } else { 10.0 }, 0.0),
        column(items).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    ))
    .scrollbar_thickness(6.0);

    let mut col: Vec<AnyWidget> = Vec::new();
    if let Some(header) = &p.header {
        if !collapsed {
            col.push(Padding::new(EdgeInsets::only(10.0, 10.0, 10.0, 6.0), header.clone()).into_widget());
        }
    }
    col.push(Expanded::new(scroller).into_widget());
    if let Some(footer) = &p.footer {
        if !collapsed {
            col.push(Padding::new(EdgeInsets::only(10.0, 6.0, 10.0, 10.0), footer.clone()).into_widget());
        }
    }
    // The collapse chevron, pinned to the bottom.
    if p.collapsible {
        let on_change = p.on_collapse_changed.clone();
        let next = !collapsed;
        let glyph = if collapsed { IconKind::ChevronRight } else { IconKind::ChevronLeft };
        let mut toggle = GestureDetector::new(
            Container::new()
                .alignment(Alignment::CENTER)
                .padding(EdgeInsets::symmetric(0.0, 10.0))
                .child(icon(glyph).size(18.0).color(c.muted_foreground)),
        )
        .cursor(Cursor::Pointer);
        if let Some(cb) = on_change {
            toggle = toggle.on_tap(move || cb(next));
        }
        col.push(toggle.into_widget());
    }

    let content = Container::new()
        .color(c.card)
        .width((w - 1.0).max(0.0))
        .child(column(col).cross_axis_alignment(CrossAxisAlignment::Stretch));

    // Panel + a 1px right divider (we have no per-side borders yet).
    row(children![content, Container::new().color(c.border).width(1.0)])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .into_widget()
}

// ===========================================================================
// BottomNav
// ===========================================================================

/// A bottom navigation bar item: an icon over a label.
#[derive(Clone)]
pub struct BottomNavItem {
    icon: IconData,
    label: String,
    selected: bool,
    on_select: Option<Callback>,
}

/// Create a [`BottomNavItem`].
pub fn bottom_nav_item(icon: impl Into<IconData>, label: impl Into<String>) -> BottomNavItem {
    BottomNavItem { icon: icon.into(), label: label.into(), selected: false, on_select: None }
}

impl BottomNavItem {
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn on_select(mut self, cb: impl IntoCallback) -> Self {
        self.on_select = Some(cb.into_callback());
        self
    }
}

impl IntoWidget for BottomNavItem {
    fn into_widget(self) -> AnyWidget {
        component_props(render_bottom_nav_item, self).into_widget()
    }
}

fn render_bottom_nav_item(w: &BottomNavItem) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let fg = if w.selected {
        c.primary
    } else if hovered.get() {
        c.foreground
    } else {
        c.muted_foreground
    };
    let content = column(children![
        icon(w.icon).size(20.0).color(fg),
        gap_h(4.0),
        text(w.label.clone()).size(11.0).weight(if w.selected { 600.0 } else { 500.0 }).color(fg),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min);

    let container = Container::new().padding(EdgeInsets::symmetric(16.0, 8.0)).child(center(content));
    let mut gesture = GestureDetector::new(container)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false));
    if let Some(cb) = w.on_select.clone() {
        gesture = gesture.on_tap(cb);
    }
    gesture.into_widget()
}

/// A bottom navigation bar.
#[derive(Clone)]
pub struct BottomNav {
    items: Vec<AnyWidget>,
    height: f64,
}

/// Create an empty [`BottomNav`]; add items with `.item(..)`.
pub fn bottom_nav() -> BottomNav {
    BottomNav { items: Vec::new(), height: 62.0 }
}

impl BottomNav {
    pub fn item(mut self, item: impl IntoWidget) -> Self {
        self.items.push(item.into_widget());
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }
}

impl IntoWidget for BottomNav {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let items: Vec<AnyWidget> =
            std::mem::take(&mut self.items).into_iter().map(|it| Expanded::new(it).into_widget()).collect();
        Container::new()
            .decoration(BoxDecoration::new().color(c.background).border(Border::new(c.border, 1.0)))
            .height(self.height)
            .child(
                row(items)
                    .main_axis_alignment(MainAxisAlignment::SpaceEvenly)
                    .cross_axis_alignment(CrossAxisAlignment::Center),
            )
            .into_widget()
    }
}
