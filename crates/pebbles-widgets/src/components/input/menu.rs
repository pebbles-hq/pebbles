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

    // A custom trigger is used verbatim; the default one gets button-like hover.
    let trigger = p
        .trigger
        .clone()
        .unwrap_or_else(|| default_trigger(&p.label, p.width, hovered.get(), p.style.clone()));

    // The menu entries are consumed into a fresh menu widget on each open. We can't
    // clone the closures generically, so rebuild the menu from a shared blueprint.
    let blueprint = Rc::new(RebuildableMenu::from(&p.entries));
    let menu_h = estimate_height(&p.entries);

    // Keyboard: one action per actionable row (enabled items + check rows); the
    // SI-4 list model drives Up/Down/Enter/Escape while the menu is open.
    let actions = blueprint.actions();
    node.register(Rc::new(|| {}), None, false);
    {
        let actions = actions.clone();
        let handler = nav.handler(actions.len(), move |row| actions[row](), hide_overlay);
        node.register_editor(Rc::new(move |k| {
            handler(k);
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
                DdMenuProps { blueprint: blueprint.clone(), width, nav, actions: actions.clone() },
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

/// Props for one submenu row.
struct SubRowProps {
    label: String,
    width: f64,
    reserve_gutter: bool,
    /// The row's estimated top offset within the parent panel (panel-top + this
    /// = the row's window-space top — used to align the child panel with it).
    top_offset: f64,
    sub: Rc<RebuildableMenu>,
}

/// A submenu row: label + right chevron, hover highlight, and hover-open of a
/// child panel that closes (after a grace delay) when neither the row nor the
/// panel is hovered — the hover-refcount pattern from [`HoverCard`].
fn render_sub_row(p: &SubRowProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let over = create_signal(0i32);
    let close_key = create_signal(()).raw_id();
    let t = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.1);
    let bg = mix(c.popover, c.accent, t as f32);
    let fg = mix(c.popover_foreground, c.accent_foreground, t as f32);

    let schedule_close: Rc<dyn Fn()> = Rc::new({
        let over = over;
        move || {
            pebbles_core::animation::set_timeout(close_key, 0.28, move || {
                if over.peek() <= 0 {
                    crate::overlay::clear_child();
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

    let sub = p.sub.clone();
    let top_offset = p.top_offset;
    let enter_close = schedule_close.clone();
    GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_hover_enter(action_event(move |_e: PointerEvent| {
            hovered.set(true);
            over.update(|n| *n += 1);
            pebbles_core::animation::clear_timeout(close_key);
            // Build the child panel: the same row renderer, hover-tracked so it
            // stays open while the pointer moves onto it.
            let panel_h = sub.height;
            let panel = GestureDetector::new(sub.build(SUB_WIDTH))
                .on_hover_enter({
                    let over = over;
                    move || {
                        over.update(|n| *n += 1);
                        pebbles_core::animation::clear_timeout(close_key);
                    }
                })
                .on_hover_exit({
                    let over = over;
                    let schedule_close = enter_close.clone();
                    move || {
                        over.update(|n| *n -= 1);
                        schedule_close();
                    }
                });
            let (parent_left, parent_top, parent_w) =
                match crate::overlay::overlay_signal().peek() {
                    Some(e) => (e.left, e.top, e.width),
                    None => (0.0, 0.0, SUB_WIDTH),
                };
            let (ww, wh) = crate::overlay::window_size();
            let left = if ww > 0.0 {
                (parent_left + parent_w - 4.0).min(ww - SUB_WIDTH - 8.0).max(8.0)
            } else {
                parent_left + parent_w - 4.0
            };
            let top = if wh > 0.0 {
                (parent_top + top_offset - 4.0).min(wh - panel_h - 8.0).max(8.0)
            } else {
                parent_top + top_offset - 4.0
            };
            crate::overlay::set_child(panel.into_widget(), left, top, SUB_WIDTH, panel_h);
        }))
        .on_hover_exit(move || {
            hovered.set(false);
            over.update(|n| *n -= 1);
            schedule_close();
        })
        .into_widget()
}

/// Props for the open dropdown menu — a component so the keyboard highlight
/// re-renders reactively as the [`ListNav`] active row changes.
struct DdMenuProps {
    blueprint: Rc<RebuildableMenu>,
    width: f64,
    nav: ListNav,
    actions: Vec<Rc<dyn Fn()>>,
}

fn render_dd_menu(p: &DdMenuProps) -> AnyWidget {
    p.blueprint.build_rows(p.width, p.nav.active(), &p.actions)
}

// A cloneable blueprint of the entries so the menu can be rebuilt each open (the
// overlay takes a fresh widget; entry closures are shared via `Rc`).
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

    /// Build the menu without a keyboard highlight (mouse-driven, e.g. the
    /// context menu).
    pub(crate) fn build(&self, width: f64) -> AnyWidget {
        self.build_rows(width, None, &self.actions())
    }

    /// Build the menu, highlighting row `active` (the keyboard cursor, over the
    /// actionable rows only) and using `actions` for every row's run-and-close.
    pub(crate) fn build_rows(&self, width: f64, active: Option<usize>, actions: &[Rc<dyn Fn()>]) -> AnyWidget {
        self.build_rows_inner(width, active, actions, true)
    }

    /// Build the rows; `allow_sub` enables interactive submenu rows (top-level
    /// menus only — submenu panels render nested subs as plain rows).
    pub(crate) fn build_rows_inner(
        &self,
        width: f64,
        active: Option<usize>,
        actions: &[Rc<dyn Fn()>],
        allow_sub: bool,
    ) -> AnyWidget {
        let inner = width - 8.0;
        // If any row carries an icon or is a checkbox, reserve the leading gutter on
        // every row so labels line up.
        let reserve = self.entries.iter().any(|e| {
            matches!(e, BpEntry::Item { icon: Some(_), .. } | BpEntry::Check { .. })
        });
        let mut kids: Vec<AnyWidget> = Vec::new();
        let mut row_idx = 0usize;
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
                    })
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
                    })
                }
                BpEntry::Sub { label, entries } => {
                    y += 32.0;
                    if allow_sub {
                        // Hovering this row opens the submenu panel to the right.
                        component_props(
                            render_sub_row,
                            SubRowProps {
                                label: label.clone(),
                                width: inner,
                                reserve_gutter: reserve,
                                top_offset: row_top,
                                sub: Rc::new(RebuildableMenu {
                                    entries: (*entries).clone(),
                                    height: bp_height(entries).min(300.0),
                                }),
                            },
                        )
                        .into_widget()
                    } else {
                        // Nested submenus are one level deep: render as a plain
                        // muted row (honest limitation, not silently dropped).
                        Opacity::new(
                            0.55,
                            Container::new()
                                .width(inner)
                                .padding(EdgeInsets::symmetric(8.0, 7.0))
                                .child(row(children![
                                    if reserve {
                                        Container::new().width(24.0).into_widget()
                                    } else {
                                        gap_h(0.0).into_widget()
                                    },
                                    text(label.clone()).size(14.0).color(theme().colors.muted_foreground),
                                ])),
                        )
                        .into_widget()
                    }
                }
            });
        }
        popover_surface(width, 4.0, column(kids).main_axis_size(MainAxisSize::Min).into_widget())
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
