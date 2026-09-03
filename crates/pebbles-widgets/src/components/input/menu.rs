//! [`DropdownMenu`] — shadcn's **action** menu (distinct from [`Select`](super::Select),
//! which picks a form value). A trigger pops a menu of *actions*: items with
//! optional icons, keyboard-shortcut hints, destructive styling, disabled state,
//! section labels, separators, and checkbox items. Opened in the global overlay.

use std::rc::Rc;

use pebbles_foundation::{Alignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, IconData, IconKind, PointerEvent};

use super::list_nav::{ListNav, list_nav};
use super::popover::{anchor_below, popover_surface};
use crate::components::icon;
use crate::overlay::{hide_overlay, show_overlay};
use crate::theme::{mix, theme};
use crate::widgets::{Container, GestureDetector, Opacity, column, gap_h, row, spacer, text};
use pebbles_core::focus::create_focus;
use pebbles_core::keyboard::{KeyInput, Motion};
use pebbles_core::reactive::Signal;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action_event, animated, children, component_props, create_signal};

// ---------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------

/// One row in a [`DropdownMenu`].
pub enum MenuEntry {
    Item(MenuItem),
    /// A small uppercase section header.
    Label(String),
    /// A hairline divider between groups.
    Separator,
    /// A toggleable item that shows a check when on.
    Check { label: String, checked: bool, on_toggle: Rc<dyn Fn(bool)> },
    /// A submenu: hovering the row opens a second panel to the right with
    /// `entries` (one level deep — nested submenus render as plain rows).
    Sub { label: String, entries: Vec<MenuEntry> },
}

/// A single actionable menu item. Build with [`menu_item`].
pub struct MenuItem {
    label: String,
    icon: Option<IconData>,
    shortcut: Option<String>,
    on_select: Option<Rc<dyn Fn()>>,
    disabled: bool,
    destructive: bool,
}

/// Create a [`MenuItem`].
pub fn menu_item(label: impl Into<String>) -> MenuItem {
    MenuItem {
        label: label.into(),
        icon: None,
        shortcut: None,
        on_select: None,
        disabled: false,
        destructive: false,
    }
}

impl MenuItem {
    /// A leading icon.
    pub fn icon(mut self, icon: impl Into<IconData>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    /// A trailing keyboard-shortcut hint (e.g. `"⇧⌘P"`).
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    /// The action to run when chosen (the menu then closes).
    pub fn on_select(mut self, f: impl Fn() + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
    /// Dim and disable the item.
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }
    /// Render in the destructive (danger) color.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    // --- read accessors for the native-menu builder (B3) -------------------
    /// The item's label.
    pub(crate) fn label_str(&self) -> &str {
        &self.label
    }
    /// The shortcut string as authored — for a native menu it is parsed as a B2
    /// binding grammar (`"Mod+S"`); for the in-window menu it is a display hint.
    pub(crate) fn shortcut_str(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }
    /// The select callback, if any.
    pub(crate) fn on_select_rc(&self) -> Option<Rc<dyn Fn()>> {
        self.on_select.clone()
    }
    /// Whether the item is disabled.
    pub(crate) fn is_disabled(&self) -> bool {
        self.disabled
    }
}

impl From<MenuItem> for MenuEntry {
    fn from(i: MenuItem) -> MenuEntry {
        MenuEntry::Item(i)
    }
}

/// A section label entry.
pub fn menu_label(label: impl Into<String>) -> MenuEntry {
    MenuEntry::Label(label.into())
}

/// A divider entry.
pub fn menu_separator() -> MenuEntry {
    MenuEntry::Separator
}

/// A checkbox entry.
pub fn menu_check(label: impl Into<String>, checked: bool, on_toggle: impl Fn(bool) + 'static) -> MenuEntry {
    MenuEntry::Check { label: label.into(), checked, on_toggle: Rc::new(on_toggle) }
}

/// A submenu entry: hovering it opens a second panel to the right.
pub fn menu_sub<I, E>(label: impl Into<String>, entries: I) -> MenuEntry
where
    I: IntoIterator<Item = E>,
    E: Into<MenuEntry>,
{
    MenuEntry::Sub { label: label.into(), entries: entries.into_iter().map(Into::into).collect() }
}

// ---------------------------------------------------------------------------
// DropdownMenu
// ---------------------------------------------------------------------------

/// An action menu. Build with [`dropdown_menu`].
#[derive(Default)]
pub struct DropdownMenu {
    label: String,
    trigger: Option<AnyWidget>,
    entries: Vec<MenuEntry>,
    width: f64,
    style: Option<crate::style::Style>,
}

/// Create a [`DropdownMenu`] whose default trigger is a button showing `label`.
pub fn dropdown_menu(label: impl Into<String>) -> DropdownMenu {
    DropdownMenu { label: label.into(), width: 240.0, ..Default::default() }
}

impl DropdownMenu {
    /// Replace the default button trigger with a custom (non-interactive) widget,
    /// e.g. an icon for an overflow (`⋯`) menu.
    pub fn trigger(mut self, w: impl IntoWidget) -> Self {
        self.trigger = Some(w.into_widget());
        self
    }
    /// The menu width.
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
    /// Append an actionable item.
    pub fn item(mut self, item: impl Into<MenuEntry>) -> Self {
        self.entries.push(item.into());
        self
    }
    /// Append a section label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.entries.push(menu_label(label));
        self
    }
    /// Append a divider.
    pub fn separator(mut self) -> Self {
        self.entries.push(menu_separator());
        self
    }
    /// Append a checkbox item.
    pub fn check(mut self, label: impl Into<String>, checked: bool, on_toggle: impl Fn(bool) + 'static) -> Self {
        self.entries.push(menu_check(label, checked, on_toggle));
        self
    }
    /// Append a submenu: hovering the row opens a second panel to the right.
    pub fn sub<I, E>(mut self, label: impl Into<String>, entries: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<MenuEntry>,
    {
        self.entries.push(menu_sub(label, entries));
        self
    }
    /// Merge a [`Style`](crate::Style) onto the default trigger box (ignored when a
    /// custom `.trigger(..)` is set).
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
}

struct Props {
    label: String,
    trigger: Option<AnyWidget>,
    entries: Vec<MenuEntry>,
    width: f64,
    style: Option<crate::style::Style>,
}

impl IntoWidget for DropdownMenu {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_dropdown,
            Props {
                label: self.label,
                trigger: self.trigger,
                entries: self.entries,
                width: self.width,
                style: self.style,
            },
        )
        .into_widget()
    }
}

/// The default outline-button-style trigger (a Container, not a button, so the
/// wrapping open-gesture receives the tap). Bounded to `width` with the chevron
/// pushed to the right edge, like a Select. `hovered` tints it so it reads as
/// interactive.
fn default_trigger(label: &str, width: f64, hovered: bool, user: Option<crate::style::Style>) -> AnyWidget {
    let c = theme().colors;
    let bg = if hovered { c.accent } else { c.background };
    let deco = crate::style::style()
        .background(bg)
        .border(Border::new(c.input, 1.0))
        .radius_all(theme().radius)
        .merge(user.unwrap_or_default())
        .decoration()
        .unwrap_or_else(BoxDecoration::new);
    Container::new()
        .width(width)
        .height(38.0)
        .decoration(deco)
        .padding(EdgeInsets::symmetric(12.0, 0.0))
        .alignment(Alignment::CENTER_LEFT)
        .child(row(children![
            text(label.to_string()).size(14.0).color(c.foreground),
            spacer(),
            icon(IconKind::ChevronDown).size(16.0).color(c.muted_foreground),
        ]))
        .into_widget()
}

pub(crate) fn estimate_height(entries: &[MenuEntry]) -> f64 {
    let rows: f64 = entries
        .iter()
        .map(|e| match e {
            MenuEntry::Item(_) | MenuEntry::Check { .. } | MenuEntry::Sub { .. } => 32.0,
            MenuEntry::Label(_) => 28.0,
            MenuEntry::Separator => 9.0,
        })
        .sum();
    rows + 8.0
}

fn render_dropdown(p: &Props) -> AnyWidget {
    let width = p.width;
    let hovered = create_signal(false);
    let node = create_focus();
    let nav = list_nav();
    let child_nav = list_nav();
    let child_ctx = create_signal::<Option<Rc<ChildCtx>>>(None);

    // A custom trigger is used verbatim; the default one gets button-like hover.
    let trigger = p
        .trigger
        .clone()
        .unwrap_or_else(|| default_trigger(&p.label, p.width, hovered.get(), p.style.clone()));

    // The menu entries are consumed into a fresh menu widget on each open. We can't
    // clone the closures generically, so rebuild the menu from a shared blueprint.
    let blueprint = Rc::new(RebuildableMenu::from(&p.entries));
    let menu_h = estimate_height(&p.entries);

    // Keyboard: navigable rows (enabled items + checks + sub rows) drive the SI-4
    // model — Up/Down move, Enter runs (sub rows are entered with Right), Escape
    // dismisses. While a child menu is open it owns the keyboard.
    let actions = blueprint.actions();
    let navigable = blueprint.navigable();
    let sub_rows = blueprint.sub_rows();
    let handles = SubMenuHandles {
        nav: child_nav,
        ctx: child_ctx,
        subs: Rc::new(sub_rows.clone()),
    };
    node.register(Rc::new(|| {}), None, false);
    {
        let actions = actions.clone();
        let navigable = navigable.clone();
        let sub_rows = sub_rows.clone();
        let pick = nav.handler(
            navigable.len(),
            {
                let actions = actions.clone();
                let navigable = navigable.clone();
                move |row| {
                    if let Some(RowTarget::Action(a)) = navigable.get(row) {
                        actions[*a]();
                    }
                }
            },
            hide_overlay,
        );
        node.register_editor(Rc::new(move |k: KeyInput| {
            // The child menu owns the keyboard while its panel is open: Up/Down/
            // Enter drive it, Left closes it, Escape closes everything. (The panel
            // check drops a stale context after a pick closed the whole overlay.)
            if child_ctx.peek().is_some() && crate::overlay::child_is_open() {
                match k {
                    KeyInput::Move { motion: Motion::Left, .. } => {
                        close_sub_child(child_nav, child_ctx)
                    }
                    KeyInput::Escape => hide_overlay(),
                    _ => {
                        if let Some(ctx) = child_ctx.peek() {
                            let h = child_nav.handler(
                                ctx.actions.len(),
                                {
                                    let actions = ctx.actions.clone();
                                    move |row| actions[row]()
                                },
                                hide_overlay,
                            );
                            let _ = h(k);
                        }
                    }
                }
                return;
            }
            // Right opens the submenu under the active row (if it is one).
            if let KeyInput::Move { motion: Motion::Right, .. } = k
                && let Some(row) = nav.active()
                && let Some(RowTarget::Sub(si)) = navigable.get(row)
                && let Some((bp, top)) = sub_rows.get(*si)
            {
                open_sub_child(bp, *top, child_nav, child_ctx, None);
                return;
            }
            let _ = pick(k);
        }));
    }

    let open_menu = {
        let blueprint = blueprint.clone();
        move |e: PointerEvent| {
            let trigger_left = e.global.x - e.position.x;
            let trigger_top = e.global.y - e.position.y;
            let (left, top) = anchor_below(trigger_left, trigger_top, 38.0, width, menu_h);
            let menu = component_props(
                render_dd_menu,
                DdMenuProps {
                    blueprint: blueprint.clone(),
                    width,
                    nav,
                    actions: actions.clone(),
                    handles: handles.clone(),
                },
            );
            show_overlay(menu.into_widget(), left, top, width, menu_h);
            node.request_focus();
        }
    };

    GestureDetector::new(trigger)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_tap(action_event(open_menu))
        .into_widget()
}

/// Approximate panel height for a `BpEntry` list.
fn bp_height(entries: &[BpEntry]) -> f64 {
    entries
        .iter()
        .map(|e| match e {
            BpEntry::Item { .. } | BpEntry::Check { .. } | BpEntry::Sub { .. } => 32.0,
            BpEntry::Label(_) => 28.0,
            BpEntry::Separator => 9.0,
        })
        .sum::<f64>()
        + 8.0
}

/// The width of a submenu panel.
const SUB_WIDTH: f64 = 200.0;

/// The grace before a submenu closes once neither the row nor the panel is
/// hovered (moving row→panel never flickers it shut).
const SUB_CLOSE_DELAY: f64 = 0.3;

/// One navigable row of the open menu.
#[derive(Clone)]
pub(crate) enum RowTarget {
    /// Runs `actions[usize]` on Enter.
    Action(usize),
    /// A submenu row — entered with Right, not runnable.
    Sub(usize),
}

/// The open child menu's runnable rows (shared with the keyboard handler).
pub(crate) struct ChildCtx {
    actions: Vec<Rc<dyn Fn()>>,
}

/// The per-menu plumbing the submenu rows need: the child's keyboard cursor,
/// its context, and the sub blueprints (with their panel-top offsets) in
/// entry order.
#[derive(Clone)]
pub(crate) struct SubMenuHandles {
    pub(crate) nav: ListNav,
    pub(crate) ctx: Signal<Option<Rc<ChildCtx>>>,
    pub(crate) subs: Rc<Vec<(Rc<RebuildableMenu>, f64)>>,
}

/// Open a submenu's child panel: position it beside the parent (flipping left
/// when there is no room on the right, clamped vertically), then attach it to
/// the overlay with the child's own keyboard context.
fn open_sub_child(
    bp: &Rc<RebuildableMenu>,
    top_offset: f64,
    child_nav: ListNav,
    child_ctx: Signal<Option<Rc<ChildCtx>>>,
    hover: Option<(Rc<dyn Fn()>, Rc<dyn Fn()>)>,
) {
    let (parent_left, parent_top, parent_w) = match crate::overlay::overlay_signal().peek() {
        Some(e) => (e.left, e.top, e.width),
        None => (0.0, 0.0, SUB_WIDTH),
    };
    let (ww, wh) = crate::overlay::window_size();
    let panel_h = bp.height;
    let right_left = parent_left + parent_w - 4.0;
    let left = if ww > 0.0 && right_left + SUB_WIDTH > ww - 8.0 {
        (parent_left - SUB_WIDTH + 4.0).max(8.0) // flip to the left edge
    } else {
        right_left.max(8.0)
    };
    let top = if wh > 0.0 {
        (parent_top + top_offset - 4.0).clamp(8.0, (wh - panel_h - 8.0).max(8.0))
    } else {
        parent_top + top_offset - 4.0
    };
    let ctx = Rc::new(ChildCtx { actions: bp.actions() });
    child_ctx.set(Some(ctx.clone()));
    let panel = component_props(
        render_sub_menu,
        SubMenuProps {
            bp: bp.clone(),
            nav: child_nav,
            actions: ctx.actions.clone(),
            hover,
        },
    );
    crate::overlay::set_child(panel.into_widget(), left, top, SUB_WIDTH, panel_h);
}

/// Dismiss the child menu (overlay panel + keyboard context).
fn close_sub_child(child_nav: ListNav, child_ctx: Signal<Option<Rc<ChildCtx>>>) {
    child_ctx.set(None);
    child_nav.set_active(None);
    crate::overlay::clear_child();
}

/// Props for one submenu row.
struct SubRowProps {
    label: String,
    width: f64,
    reserve_gutter: bool,
    /// Keyboard highlight (the SI-4 active row).
    active: bool,
    bp: Rc<RebuildableMenu>,
    top_offset: f64,
    child_nav: ListNav,
    child_ctx: Signal<Option<Rc<ChildCtx>>>,
}

/// A submenu row: label + right chevron, hover highlight (or keyboard active),
/// and a delayed hover-open of the child panel; the child closes (after the
/// grace) when neither the row nor the panel is hovered — the hover-refcount
/// pattern from [`HoverCard`](crate::components::HoverCard).
fn render_sub_row(p: &SubRowProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let over = create_signal(0i32);
    let close_key = create_signal(()).raw_id();
    let show_key = create_signal(()).raw_id();
    let shown = p.active || hovered.get();
    let t = animated(if shown { 1.0 } else { 0.0 }, 0.1);
    let bg = mix(c.popover, c.accent, t as f32);
    let fg = mix(c.popover_foreground, c.accent_foreground, t as f32);
    let child_nav = p.child_nav;
    let child_ctx = p.child_ctx;

    let schedule_close: Rc<dyn Fn()> = Rc::new({
        let over = over;
        move || {
            pebbles_core::animation::set_timeout(close_key, SUB_CLOSE_DELAY, move || {
                if over.peek() <= 0 {
                    close_sub_child(child_nav, child_ctx);
                }
            });
        }
    });

    let mut kids: Vec<AnyWidget> = Vec::new();
    if p.reserve_gutter {
        kids.push(Container::new().width(24.0).into_widget());
    }
    kids.push(text(p.label.clone()).size(14.0).color(fg).into_widget());
    kids.push(spacer().into_widget());
    kids.push(icon(IconKind::ChevronRight).size(14.0).color(c.muted_foreground).into_widget());

    let body = Container::new()
        .width(p.width)
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(4.0)))
        .padding(EdgeInsets::symmetric(8.0, 7.0))
        .child(row(kids));

    let bp = p.bp.clone();
    let top_offset = p.top_offset;
    let enter_close = schedule_close.clone();
    let label = p.label.clone();
    let gesture = GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_hover_enter(action_event(move |_e: PointerEvent| {
            hovered.set(true);
            over.update(|n| *n += 1);
            pebbles_core::animation::clear_timeout(close_key);
            // Open after a short hover delay (cancelled on exit), hover-tracked
            // so moving onto the panel keeps it open.
            let panel_enter: Rc<dyn Fn()> = Rc::new({
                let over = over;
                move || {
                    over.update(|n| *n += 1);
                    pebbles_core::animation::clear_timeout(close_key);
                }
            });
            let panel_exit: Rc<dyn Fn()> = Rc::new({
                let over = over;
                let schedule_close = enter_close.clone();
                move || {
                    over.update(|n| *n -= 1);
                    schedule_close();
                }
            });
            let bp2 = bp.clone();
            pebbles_core::animation::set_timeout(show_key, 0.25, move || {
                if over.peek() <= 0 {
                    return;
                }
                open_sub_child(
                    &bp2,
                    top_offset,
                    child_nav,
                    child_ctx,
                    Some((panel_enter.clone(), panel_exit.clone())),
                );
            });
        }))
        .on_hover_exit(move || {
            hovered.set(false);
            over.update(|n| *n -= 1);
            pebbles_core::animation::clear_timeout(show_key);
            schedule_close();
        });
    // C7: a submenu trigger is a MenuItem too (label = row text).
    crate::widgets::semantics(pebbles_render::SemanticsRole::MenuItem, label, gesture).into_widget()
}

/// Props for the open child menu — a component so the keyboard highlight
/// re-renders reactively as the child's [`ListNav`] active row changes.
struct SubMenuProps {
    bp: Rc<RebuildableMenu>,
    nav: ListNav,
    actions: Vec<Rc<dyn Fn()>>,
    hover: Option<(Rc<dyn Fn()>, Rc<dyn Fn()>)>,
}

fn render_sub_menu(p: &SubMenuProps) -> AnyWidget {
    let active = p.nav.active();
    let empty_handles = SubMenuHandles {
        nav: list_nav(),
        ctx: create_signal(None),
        subs: Rc::new(Vec::new()),
    };
    let mut g =
        GestureDetector::new(p.bp.build_rows(SUB_WIDTH, active, &p.actions, &empty_handles));
    if let Some((enter, exit)) = &p.hover {
        let enter = enter.clone();
        let exit = exit.clone();
        g = g.on_hover_enter(move || enter()).on_hover_exit(move || exit());
    }
    g.into_widget()
}

/// Props for the open dropdown menu — a component so the keyboard highlight
/// re-renders reactively as the [`ListNav`] active row changes.
#[derive(Clone)]
struct DdMenuProps {
    blueprint: Rc<RebuildableMenu>,
    width: f64,
    nav: ListNav,
    actions: Vec<Rc<dyn Fn()>>,
    handles: SubMenuHandles,
}

fn render_dd_menu(p: &DdMenuProps) -> AnyWidget {
    p.blueprint
        .build_rows(p.width, p.nav.active(), &p.actions, &p.handles)
}

// A cloneable blueprint of the entries so the menu can be rebuilt each open (the
// overlay takes a fresh widget; entry closures are shared via `Rc`).
#[derive(Clone)]
pub(crate) struct RebuildableMenu {
    entries: Vec<BpEntry>,
    /// Approximate open-panel height (rows summed).
    pub(crate) height: f64,
}

#[derive(Clone)]
enum BpEntry {
    Item {
        label: String,
        icon: Option<IconData>,
        shortcut: Option<String>,
        on_select: Option<Rc<dyn Fn()>>,
        disabled: bool,
        destructive: bool,
    },
    Label(String),
    Separator,
    Check { label: String, checked: bool, on_toggle: Rc<dyn Fn(bool)> },
    Sub { label: String, entries: Vec<BpEntry> },
}

/// Move-free clone of one entry: entries hold `Rc` callbacks, so this shallow-copies
/// fields (recursing into submenus).
fn bp_entry(e: &MenuEntry) -> BpEntry {
    match e {
        MenuEntry::Item(i) => BpEntry::Item {
            label: i.label.clone(),
            icon: i.icon,
            shortcut: i.shortcut.clone(),
            on_select: i.on_select.clone(),
            disabled: i.disabled,
            destructive: i.destructive,
        },
        MenuEntry::Label(l) => BpEntry::Label(l.clone()),
        MenuEntry::Separator => BpEntry::Separator,
        MenuEntry::Check { label, checked, on_toggle } => {
            BpEntry::Check { label: label.clone(), checked: *checked, on_toggle: on_toggle.clone() }
        }
        MenuEntry::Sub { label, entries } => {
            debug_assert!(
                entries.iter().all(|e| !matches!(e, MenuEntry::Sub { .. })),
                "submenus are one level deep — a Sub inside a Sub is rejected"
            );
            BpEntry::Sub { label: label.clone(), entries: entries.iter().map(bp_entry).collect() }
        }
    }
}

impl RebuildableMenu {
    pub(crate) fn from(entries: &[MenuEntry]) -> Self {
        let height = estimate_height(entries).min(300.0);
        RebuildableMenu { entries: entries.iter().map(bp_entry).collect(), height }
    }

    /// One run-action per keyboard-navigable row (enabled items + check rows), in
    /// entry order: each runs its callback and closes the menu.
    pub(crate) fn actions(&self) -> Vec<Rc<dyn Fn()>> {
        self.entries
            .iter()
            .filter_map(|e| match e {
                BpEntry::Item { on_select, disabled, .. } if !disabled => {
                    let f = on_select.clone();
                    Some(Rc::new(move || {
                        if let Some(g) = &f {
                            g();
                        }
                        hide_overlay();
                    }) as Rc<dyn Fn()>)
                }
                BpEntry::Check { checked, on_toggle, .. } => {
                    let on_toggle = on_toggle.clone();
                    let now = *checked;
                    Some(Rc::new(move || {
                        on_toggle(!now);
                        hide_overlay();
                    }) as Rc<dyn Fn()>)
                }
                _ => None,
            })
            .collect()
    }

    /// The keyboard-navigable rows in order: runnable entries map to their
    /// action index, sub rows map to their sub index.
    pub(crate) fn navigable(&self) -> Vec<RowTarget> {
        let mut act = 0usize;
        let mut sub = 0usize;
        self.entries
            .iter()
            .filter_map(|e| match e {
                BpEntry::Item { disabled: false, .. } | BpEntry::Check { .. } => {
                    let a = act;
                    act += 1;
                    Some(RowTarget::Action(a))
                }
                BpEntry::Sub { .. } => {
                    let s = sub;
                    sub += 1;
                    Some(RowTarget::Sub(s))
                }
                _ => None,
            })
            .collect()
    }

    /// The sub menus `(blueprint, panel-top offset)` in entry order.
    pub(crate) fn sub_rows(&self) -> Vec<(Rc<RebuildableMenu>, f64)> {
        let mut y = 4.0;
        let mut out = Vec::new();
        for e in &self.entries {
            match e {
                BpEntry::Item { .. } | BpEntry::Check { .. } | BpEntry::Sub { .. } => {
                    if let BpEntry::Sub { entries, .. } = e {
                        out.push((
                            Rc::new(RebuildableMenu {
                                entries: entries.clone(),
                                height: bp_height(entries).min(300.0),
                            }),
                            y,
                        ));
                    }
                    y += 32.0;
                }
                BpEntry::Label(_) => y += 28.0,
                BpEntry::Separator => y += 9.0,
            }
        }
        out
    }

    /// Build the menu without a keyboard highlight (mouse-driven, e.g. the
    /// context menu). `handles` supplies the submenu plumbing.
    pub(crate) fn build(&self, width: f64, handles: &SubMenuHandles) -> AnyWidget {
        self.build_rows(width, None, &self.actions(), handles)
    }

    /// Build the menu, highlighting row `active` (the keyboard cursor, over the
    /// navigable rows) and using `actions` for every runnable row. `handles`
    /// carries the submenu plumbing (child cursor + context + sub blueprints).
    pub(crate) fn build_rows(
        &self,
        width: f64,
        active: Option<usize>,
        actions: &[Rc<dyn Fn()>],
        handles: &SubMenuHandles,
    ) -> AnyWidget {
        let inner = width - 8.0;
        // If any row carries an icon or is a checkbox, reserve the leading gutter on
        // every row so labels line up.
        let reserve = self.entries.iter().any(|e| {
            matches!(e, BpEntry::Item { icon: Some(_), .. } | BpEntry::Check { .. })
        });
        let mut kids: Vec<AnyWidget> = Vec::new();
        let mut row_idx = 0usize;
        let mut sub_idx = 0usize;
        let mut y = 4.0; // surface padding, in panel-local space
        for e in &self.entries {
            let row_top = y;
            kids.push(match e {
                BpEntry::Item { label, icon, shortcut, on_select, disabled, destructive } => {
                    let (highlighted, pick): (bool, Rc<dyn Fn()>);
                    if *disabled {
                        let cb = on_select.clone();
                        highlighted = false;
                        pick = Rc::new(move || {
                            if let Some(f) = &cb {
                                f();
                            }
                            hide_overlay();
                        });
                    } else {
                        let idx = row_idx;
                        row_idx += 1;
                        highlighted = active == Some(idx);
                        let cb = on_select.clone();
                        pick = actions.get(idx).cloned().unwrap_or_else(move || {
                            Rc::new(move || {
                                if let Some(f) = &cb {
                                    f();
                                }
                                hide_overlay();
                            })
                        });
                    }
                    y += 32.0;
                    // C7: each row is a MenuItem (label = row text).
                    crate::widgets::semantics(
                        pebbles_render::SemanticsRole::MenuItem,
                        label.clone(),
                        action_row(ActionRowProps {
                            label: label.clone(),
                            icon: *icon,
                            shortcut: shortcut.clone(),
                            leading_check: None,
                            reserve_gutter: reserve,
                            destructive: *destructive,
                            disabled: *disabled,
                            highlighted,
                            width: inner,
                            on_select: pick,
                        }),
                    )
                    .disabled(*disabled)
                    .into_widget()
                }
                BpEntry::Label(l) => {
                    y += 28.0;
                    Container::new()
                        .padding(EdgeInsets::symmetric(8.0, 6.0))
                        .alignment(Alignment::CENTER_LEFT)
                        .child(text(l.clone()).size(11.5).semibold().color(theme().colors.muted_foreground))
                        .into_widget()
                }
                BpEntry::Separator => {
                    y += 9.0;
                    Container::new()
                        .width(inner)
                        .padding(EdgeInsets::symmetric(0.0, 4.0))
                        .child(
                            Container::new()
                                .width(inner)
                                .height(1.0)
                                .decoration(BoxDecoration::new().color(theme().colors.border)),
                        )
                        .into_widget()
                }
                BpEntry::Check { label, checked, on_toggle } => {
                    let highlighted = active == Some(row_idx);
                    let now = *checked;
                    let on_toggle = on_toggle.clone();
                    let pick: Rc<dyn Fn()> = match actions.get(row_idx) {
                        Some(a) => a.clone(),
                        None => Rc::new(move || {
                            on_toggle(!now);
                            hide_overlay();
                        }),
                    };
                    row_idx += 1;
                    y += 32.0;
                    // C7: a checkable MenuItem (checked state announced).
                    crate::widgets::semantics(
                        pebbles_render::SemanticsRole::MenuItem,
                        label.clone(),
                        action_row(ActionRowProps {
                            label: label.clone(),
                            icon: None,
                            shortcut: None,
                            leading_check: Some(now),
                            reserve_gutter: reserve,
                            destructive: false,
                            disabled: false,
                            highlighted,
                            width: inner,
                            on_select: pick,
                        }),
                    )
                    .checked(now)
                    .into_widget()
                }
                BpEntry::Sub { label, entries } => {
                    // A navigable row (Right enters it); hovering opens the child
                    // panel after a short delay.
                    let highlighted = active == Some(row_idx);
                    row_idx += 1;
                    y += 32.0;
                    let (bp, _) = handles
                        .subs
                        .get(sub_idx)
                        .cloned()
                        .unwrap_or_else(|| (Rc::new(RebuildableMenu { entries: (*entries).clone(), height: bp_height(entries).min(300.0) }), row_top));
                    sub_idx += 1;
                    component_props(
                        render_sub_row,
                        SubRowProps {
                            label: label.clone(),
                            width: inner,
                            reserve_gutter: reserve,
                            active: highlighted,
                            bp,
                            top_offset: row_top,
                            child_nav: handles.nav,
                            child_ctx: handles.ctx,
                        },
                    )
                    .into_widget()
                }
            });
        }
        // C7: the open panel is a Menu (submenu panels build through here too).
        crate::widgets::semantics(
            pebbles_render::SemanticsRole::Menu,
            "",
            popover_surface(width, 4.0, column(kids).main_axis_size(MainAxisSize::Min).into_widget()),
        )
        .into_widget()
    }
}

// ---------------------------------------------------------------------------
// Action row (hover-highlighting)
// ---------------------------------------------------------------------------

pub(crate) struct ActionRowProps {
    pub label: String,
    pub icon: Option<IconData>,
    pub shortcut: Option<String>,
    pub leading_check: Option<bool>,
    /// Reserve the leading gutter even when this row has no icon/check, so labels
    /// line up when *other* rows in the menu do have one.
    pub reserve_gutter: bool,
    pub destructive: bool,
    pub disabled: bool,
    /// Force the active/highlight background even without a mouse hover — set by
    /// keyboard list navigation to mark the active row.
    pub highlighted: bool,
    pub width: f64,
    pub on_select: Rc<dyn Fn()>,
}

/// A hover-highlighting menu row — shared by [`DropdownMenu`] and the comboboxes.
pub(crate) fn action_row(p: ActionRowProps) -> AnyWidget {
    component_props(render_action_row, p).into_widget()
}

fn render_action_row(p: &ActionRowProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let active = !p.disabled;
    let t = if active && (hovered.get() || p.highlighted) { 1.0 } else { 0.0 };

    let base_fg = if p.destructive { c.destructive } else { c.popover_foreground };
    let (bg, fg) = if p.destructive {
        (mix(c.popover, c.destructive, 0.12 * t), base_fg)
    } else {
        (mix(c.popover, c.accent, t), mix(base_fg, c.accent_foreground, t))
    };

    // Leading gutter: a check (for check items), an icon, or reserved empty space
    // so all rows in a menu align even when only some have a glyph.
    let leading: AnyWidget = match (p.leading_check, p.icon) {
        (Some(true), _) => icon(IconKind::Check).size(15.0).color(fg).into_widget(),
        (None, Some(ic)) => icon(ic).size(15.0).color(fg).into_widget(),
        _ => gap_h(0.0).into_widget(),
    };
    let show_gutter = p.reserve_gutter || p.leading_check.is_some() || p.icon.is_some();

    let mut rowkids: Vec<AnyWidget> = Vec::new();
    if show_gutter {
        rowkids.push(Container::new().width(24.0).alignment(Alignment::CENTER_LEFT).child(leading).into_widget());
    }
    rowkids.push(text(p.label.clone()).size(14.0).color(fg).into_widget());
    rowkids.push(spacer().into_widget());
    if let Some(sc) = &p.shortcut {
        rowkids.push(text(sc.clone()).size(12.0).color(c.muted_foreground).into_widget());
    }

    let body = Container::new()
        .width(p.width)
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(4.0)))
        .padding(EdgeInsets::symmetric(8.0, 7.0))
        .child(row(rowkids));

    if p.disabled {
        return GestureDetector::new(Opacity::new(0.5, body)).cursor(Cursor::NotAllowed).into_widget();
    }

    let pick = p.on_select.clone();
    GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_tap(move || pick())
        .into_widget()
}
