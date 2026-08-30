//! [`DropdownMenu`] — shadcn's **action** menu (distinct from [`Select`](super::Select),
//! which picks a form value). A trigger pops a menu of *actions*: items with
//! optional icons, keyboard-shortcut hints, destructive styling, disabled state,
//! section labels, separators, and checkbox items. Opened in the global overlay.

use std::rc::Rc;

use pebbles_foundation::{Alignment, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, IconData, IconKind, PointerEvent};

use super::popover::{anchor_below, popover_surface};
use crate::components::icon;
use crate::overlay::{hide_overlay, show_overlay};
use crate::theme::{mix, theme};
use crate::widgets::{Container, GestureDetector, Opacity, SizedBox, column, row, spacer, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{action, action_event, children, component_props, create_signal};

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

// ---------------------------------------------------------------------------
// DropdownMenu
// ---------------------------------------------------------------------------

/// An action menu. Build with [`dropdown_menu`].
pub struct DropdownMenu {
    label: String,
    trigger: Option<AnyWidget>,
    entries: Vec<MenuEntry>,
    width: f64,
}

/// Create a [`DropdownMenu`] whose default trigger is a button showing `label`.
pub fn dropdown_menu(label: impl Into<String>) -> DropdownMenu {
    DropdownMenu { label: label.into(), trigger: None, entries: Vec::new(), width: 240.0 }
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
}

struct Props {
    label: String,
    trigger: Option<AnyWidget>,
    entries: Vec<MenuEntry>,
    width: f64,
}

impl IntoWidget for DropdownMenu {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_dropdown,
            Props { label: self.label, trigger: self.trigger, entries: self.entries, width: self.width },
        )
        .into_widget()
    }
}

/// The default outline-button-style trigger (a Container, not a button, so the
/// wrapping open-gesture receives the tap). Bounded to `width` with the chevron
/// pushed to the right edge, like a Select. `hovered` tints it so it reads as
/// interactive.
fn default_trigger(label: &str, width: f64, hovered: bool) -> AnyWidget {
    let c = theme().colors;
    let bg = if hovered { c.accent } else { c.background };
    Container::new()
        .width(width)
        .height(38.0)
        .decoration(
            BoxDecoration::new()
                .color(bg)
                .border(Border::new(c.input, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .padding(EdgeInsets::symmetric(12.0, 0.0))
        .alignment(Alignment::CENTER_LEFT)
        .child(row(children![
            text(label.to_string()).size(14.0).color(c.foreground),
            spacer(),
            icon(IconKind::ChevronDown).size(16.0).color(c.muted_foreground),
        ]))
        .into_widget()
}

fn estimate_height(entries: &[MenuEntry]) -> f64 {
    let rows: f64 = entries
        .iter()
        .map(|e| match e {
            MenuEntry::Item(_) | MenuEntry::Check { .. } => 32.0,
            MenuEntry::Label(_) => 28.0,
            MenuEntry::Separator => 9.0,
        })
        .sum();
    rows + 8.0
}

fn render_dropdown(p: &Props) -> AnyWidget {
    let width = p.width;
    let hovered = create_signal(false);
    // A custom trigger is used verbatim; the default one gets button-like hover.
    let trigger =
        p.trigger.clone().unwrap_or_else(|| default_trigger(&p.label, p.width, hovered.get()));

    // The menu entries are consumed into a fresh menu widget on each open. We can't
    // clone the closures generically, so rebuild the menu from a shared blueprint.
    let blueprint = Rc::new(RebuildableMenu::from(&p.entries));
    let menu_h = estimate_height(&p.entries);

    GestureDetector::new(trigger)
        .cursor(Cursor::Pointer)
        .on_hover_enter(action(move || hovered.set(true)))
        .on_hover_exit(action(move || hovered.set(false)))
        .on_tap(action_event(move |e: PointerEvent| {
            let trigger_left = e.global.x - e.position.x;
            let trigger_top = e.global.y - e.position.y;
            let (left, top) = anchor_below(trigger_left, trigger_top, 38.0, width, menu_h);
            show_overlay(blueprint.build(width), left, top, width, menu_h);
        }))
        .into_widget()
}

// A cloneable blueprint of the entries so the menu can be rebuilt each open (the
// overlay takes a fresh widget; entry closures are shared via `Rc`).
struct RebuildableMenu {
    entries: Vec<BpEntry>,
}

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
}

impl RebuildableMenu {
    fn from(entries: &[MenuEntry]) -> Self {
        // Move-free clone: entries hold `Rc` callbacks, so we shallow-copy fields.
        let entries = entries
            .iter()
            .map(|e| match e {
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
            })
            .collect();
        RebuildableMenu { entries }
    }

    fn build(&self, width: f64) -> AnyWidget {
        let inner = width - 8.0;
        // If any row carries an icon or is a checkbox, reserve the leading gutter on
        // every row so labels line up.
        let reserve = self.entries.iter().any(|e| {
            matches!(e, BpEntry::Item { icon: Some(_), .. } | BpEntry::Check { .. })
        });
        let mut kids: Vec<AnyWidget> = Vec::new();
        for e in &self.entries {
            kids.push(match e {
                BpEntry::Item { label, icon, shortcut, on_select, disabled, destructive } => {
                    let cb = on_select.clone();
                    action_row(ActionRowProps {
                        label: label.clone(),
                        icon: *icon,
                        shortcut: shortcut.clone(),
                        leading_check: None,
                        reserve_gutter: reserve,
                        destructive: *destructive,
                        disabled: *disabled,
                        width: inner,
                        on_select: Rc::new(move || {
                            if let Some(f) = &cb {
                                f();
                            }
                            hide_overlay();
                        }),
                    })
                }
                BpEntry::Label(l) => Container::new()
                    .padding(EdgeInsets::symmetric(8.0, 6.0))
                    .alignment(Alignment::CENTER_LEFT)
                    .child(text(l.clone()).size(11.5).semibold().color(theme().colors.muted_foreground))
                    .into_widget(),
                BpEntry::Separator => Container::new()
                    .width(inner)
                    .padding(EdgeInsets::symmetric(0.0, 4.0))
                    .child(
                        Container::new()
                            .width(inner)
                            .height(1.0)
                            .decoration(BoxDecoration::new().color(theme().colors.border)),
                    )
                    .into_widget(),
                BpEntry::Check { label, checked, on_toggle } => {
                    let on_toggle = on_toggle.clone();
                    let now = *checked;
                    action_row(ActionRowProps {
                        label: label.clone(),
                        icon: None,
                        shortcut: None,
                        leading_check: Some(now),
                        reserve_gutter: reserve,
                        destructive: false,
                        disabled: false,
                        width: inner,
                        on_select: Rc::new(move || {
                            on_toggle(!now);
                            hide_overlay();
                        }),
                    })
                }
            });
        }
        popover_surface(width, 4.0, column(kids).main_axis_min().into_widget())
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
    let t = if active && hovered.get() { 1.0 } else { 0.0 };

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
        _ => SizedBox::spacer(0.0, 0.0).into_widget(),
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
        .on_hover_enter(action(move || hovered.set(true)))
        .on_hover_exit(action(move || hovered.set(false)))
        .on_tap(action(move || pick()))
        .into_widget()
}
