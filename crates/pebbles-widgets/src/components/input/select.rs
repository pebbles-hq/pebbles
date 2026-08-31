//! [`Select`] — a dropdown picker. Clicking the trigger pops a menu into the
//! global overlay layer (so it escapes clipping and scrolling); picking an option
//! sets the value and dismisses. `on_changed` reports the chosen index + label.
//!
//! Options are [`SelectItem`]s — a label with an optional leading icon — so both
//! the trigger and the menu rows can carry icons (Flutter's `DropdownMenuEntry`
//! style). A bare `&str`/`String` converts to an icon-less item, so
//! `select(["A", "B"])` still works. Items can be grouped
//! ([`.group(..)`](SelectItem::group) / [`select_group`]) and individually
//! [disabled](SelectItem::disabled). The open menu is keyboard-navigable
//! (Up/Down move, Enter picks, Escape closes — the SI-4 list model).

use std::rc::Rc;

use pebbles_foundation::{Alignment, Color, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{
    Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconData, IconKind, PointerEvent,
};

use super::list_nav::list_nav;
use crate::components::icon;
use crate::overlay::{hide_overlay, show_overlay, window_size};
use crate::theme::{mix, theme};
use crate::widgets::{
    Container, GestureDetector, Opacity, SingleChildScrollView, column, gap_h, gap_w, row, spacer,
    text,
};
use pebbles_core::focus::create_focus;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, action_event, animated, component_props, create_signal};

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// One option in a [`Select`] — a label with an optional leading icon, an optional
/// group header, and an optional disabled state.
#[derive(Clone)]
pub struct SelectItem {
    label: String,
    icon: Option<IconData>,
    group: Option<String>,
    disabled: bool,
}

/// Create a [`SelectItem`]. Chain [`icon`](SelectItem::icon) to add a glyph.
pub fn select_item(label: impl Into<String>) -> SelectItem {
    SelectItem { label: label.into(), icon: None, group: None, disabled: false }
}

impl SelectItem {
    /// A leading icon, shown before the label in the menu — and in the trigger
    /// when this item is the selected one.
    pub fn icon(mut self, icon: impl Into<IconData>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    /// Render a section header above this item (and the items after it, until the
    /// next group). [`select_group`] is the bulk form.
    pub fn group(mut self, label: impl Into<String>) -> Self {
        self.group = Some(label.into());
        self
    }
    /// Dim the option and make it unpickable (muted row, not-allowed cursor, and
    /// keyboard navigation skips it).
    pub fn disabled(mut self, yes: bool) -> Self {
        self.disabled = yes;
        self
    }
}

/// A labelled group of options: builds `items` with the group header on the first
/// one, so `select(select_group("Fruits", [...]).into_iter().chain(…))` renders a
/// labelled section.
pub fn select_group<I>(label: impl Into<String>, items: I) -> Vec<SelectItem>
where
    I: IntoIterator<Item = SelectItem>,
{
    let label = label.into();
    let mut out: Vec<SelectItem> = items.into_iter().collect();
    if let Some(first) = out.first_mut() {
        first.group = Some(label);
    }
    out
}

impl From<String> for SelectItem {
    fn from(label: String) -> Self {
        SelectItem { label, icon: None, group: None, disabled: false }
    }
}
impl From<&str> for SelectItem {
    fn from(label: &str) -> Self {
        SelectItem { label: label.to_string(), icon: None, group: None, disabled: false }
    }
}

// ---------------------------------------------------------------------------
// Select
// ---------------------------------------------------------------------------

/// A dropdown select. Build with [`select`].
pub struct Select {
    options: Vec<SelectItem>,
    initial: Option<usize>,
    placeholder: String,
    width: f64,
    leading: Option<IconData>,
    trailing: Option<IconData>,
    clearable: bool,
    on_changed: Option<Rc<dyn Fn(usize, &str)>>,
    on_cleared: Option<Rc<dyn Fn()>>,
    style: Option<crate::style::Style>,
}

/// Create a [`Select`] over `options` — any mix of `&str`/`String` (icon-less) and
/// [`select_item(..).icon(..)`](select_item).
pub fn select<I, S>(options: I) -> Select
where
    I: IntoIterator<Item = S>,
    S: Into<SelectItem>,
{
    Select {
        options: options.into_iter().map(Into::into).collect(),
        initial: None,
        placeholder: "Select…".to_string(),
        width: 220.0,
        leading: None,
        trailing: None,
        clearable: false,
        on_changed: None,
        on_cleared: None,
        style: None,
    }
}

impl Select {
    pub fn value(mut self, index: usize) -> Self {
        self.initial = Some(index);
        self
    }
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
    /// A fixed leading icon on the trigger. When unset, the trigger shows the
    /// selected option's own icon (if it has one).
    pub fn leading(mut self, icon: impl Into<IconData>) -> Self {
        self.leading = Some(icon.into());
        self
    }
    /// Override the trigger's trailing indicator (defaults to a chevron).
    pub fn trailing(mut self, icon: impl Into<IconData>) -> Self {
        self.trailing = Some(icon.into());
        self
    }
    /// Show a ✕ in place of the chevron while a value is selected; clicking it
    /// resets to the placeholder and fires [`on_cleared`](Select::on_cleared).
    pub fn clearable(mut self, yes: bool) -> Self {
        self.clearable = yes;
        self
    }
    pub fn on_changed(mut self, f: impl Fn(usize, &str) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
    /// Reports a clear-button click (only fires when [`clearable`](Select::clearable)).
    pub fn on_cleared(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cleared = Some(Rc::new(f));
        self
    }
    /// Merge a [`Style`](crate::Style) onto the trigger box (bg / border / radius …).
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
}

struct Props {
    options: Vec<SelectItem>,
    initial: Option<usize>,
    placeholder: String,
    width: f64,
    leading: Option<IconData>,
    trailing: Option<IconData>,
    clearable: bool,
    on_changed: Option<Rc<dyn Fn(usize, &str)>>,
    on_cleared: Option<Rc<dyn Fn()>>,
    style: Option<crate::style::Style>,
}

impl IntoWidget for Select {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_select,
            Props {
                options: self.options,
                initial: self.initial,
                placeholder: self.placeholder,
                width: self.width,
                leading: self.leading,
                trailing: self.trailing,
                clearable: self.clearable,
                on_changed: self.on_changed,
                on_cleared: self.on_cleared,
                style: self.style,
            },
        )
        .into_widget()
    }
}

/// Props for the open menu — a component so the keyboard highlight re-renders
/// reactively as the [`ListNav`] active row changes.
struct MenuProps {
    options: Rc<Vec<SelectItem>>,
    width: f64,
    selected: Signal<Option<usize>>,
    on_changed: Option<Rc<dyn Fn(usize, &str)>>,
    max_height: Option<f64>,
    nav: super::list_nav::ListNav,
}

/// The open menu: optional group headers, disabled rows (dimmed, unpickable),
/// and the active keyboard row highlighted.
fn render_select_menu(p: &MenuProps) -> AnyWidget {
    let c = theme().colors;
    let active = p.nav.active();
    // Row index for each option (None when disabled — not navigable).
    let mut row_of: Vec<Option<usize>> = vec![None; p.options.len()];
    {
        let mut row = 0usize;
        for (i, opt) in p.options.iter().enumerate() {
            if !opt.disabled {
                row_of[i] = Some(row);
                row += 1;
            }
        }
    }

    let mut items: Vec<AnyWidget> = Vec::new();
    for (i, opt) in p.options.iter().enumerate() {
        if let Some(group) = &opt.group {
            items.push(
                Container::new()
                    .padding(EdgeInsets::symmetric(8.0, 6.0))
                    .alignment(Alignment::CENTER_LEFT)
                    .child(text(group.clone()).size(11.5).semibold().color(c.muted_foreground))
                    .into_widget(),
            );
        }
        let label = opt.label.clone();
        let oc = p.on_changed.clone();
        let selected = p.selected;
        let nav = p.nav;
        let is_sel = selected.peek() == Some(i);
        let pick: Rc<dyn Fn()> = Rc::new(move || {
            selected.set(Some(i));
            if let Some(cb) = &oc {
                cb(i, &label);
            }
            nav.set_active(None);
            hide_overlay();
        });
        items.push(menu_item(
            opt.label.clone(),
            opt.icon,
            is_sel,
            p.width - 8.0,
            opt.disabled,
            matches!((active, row_of[i]), (Some(a), Some(r)) if r == a),
            pick,
        ));
    }

    // `main_axis_min` so the popover shrink-wraps its rows instead of filling the
    // overlay to the window's bottom edge.
    let list = column(items).main_axis_size(MainAxisSize::Min);
    let body: AnyWidget = match p.max_height {
        Some(h) => Container::new()
            .height(h)
            .child(SingleChildScrollView::vertical(list).scrollbar_thickness(6.0))
            .into_widget(),
        None => list.into_widget(),
    };

    Container::new()
        .width(p.width)
        .decoration(
            BoxDecoration::new()
                .color(c.popover)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius))
                .shadow(BoxShadow::new(
                    Color::from_rgba8(0, 0, 0, 45),
                    Offset::new(0.0, 8.0),
                    22.0,
                    -4.0,
                )),
        )
        .padding(EdgeInsets::all(4.0))
        .child(body)
        .into_widget()
}

// ---------------------------------------------------------------------------
// Menu item — a hover-highlighting row (check gutter · optional icon · label).
// ---------------------------------------------------------------------------

struct MenuItemProps {
    label: String,
    icon: Option<IconData>,
    selected: bool,
    width: f64,
    disabled: bool,
    active: bool,
    pick: Rc<dyn Fn()>,
}

fn menu_item(
    label: String,
    icon: Option<IconData>,
    selected: bool,
    width: f64,
    disabled: bool,
    active: bool,
    pick: Rc<dyn Fn()>,
) -> AnyWidget {
    component_props(
        render_menu_item,
        MenuItemProps { label, icon, selected, width, disabled, active, pick },
    )
    .into_widget()
}

fn render_menu_item(p: &MenuItemProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let highlighted = !p.disabled && (hovered.get() || p.active);
    let t = animated(if highlighted { 1.0 } else { 0.0 }, 0.1);
    let bg = mix(c.popover, c.accent, t as f32);
    let fg = if p.disabled {
        c.muted_foreground
    } else {
        mix(c.popover_foreground, c.accent_foreground, t as f32)
    };

    // shadcn layout: a fixed left check gutter, an optional item icon, then label.
    let check: AnyWidget = if p.selected {
        icon(IconKind::Check).size(15.0).color(fg).into_widget()
    } else {
        gap_h(0.0).into_widget()
    };
    let mut kids: Vec<AnyWidget> = vec![
        Container::new().width(22.0).alignment(Alignment::CENTER_LEFT).child(check).into_widget(),
    ];
    if let Some(ic) = p.icon {
        kids.push(icon(ic).size(16.0).color(fg).into_widget());
        kids.push(gap_w(8.0).into_widget());
    }
    kids.push(text(p.label.clone()).size(14.0).color(fg).into_widget());

    let body = Container::new()
        .width(p.width)
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(4.0)))
        .padding(EdgeInsets::symmetric(8.0, 7.0))
        .child(row(kids));

    if p.disabled {
        return GestureDetector::new(Opacity::new(0.55, body))
            .cursor(Cursor::NotAllowed)
            .into_widget();
    }

    let pick = p.pick.clone();
    GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_tap(move || pick())
        .into_widget()
}

fn render_select(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let selected = create_signal(p.initial);
    let width = p.width;
    let options = Rc::new(p.options.clone());
    let on_changed = p.on_changed.clone();
    let node = create_focus();
    let nav = list_nav();

    // Keyboard model for the open menu: the enabled options in order; Enter picks
    // the active row, Escape dismisses.
    let enabled: Vec<usize> =
        options.iter().enumerate().filter(|(_, o)| !o.disabled).map(|(i, _)| i).collect();
    node.register(Rc::new(|| {}), None, false);
    {
        let pick_opt = {
            let options = options.clone();
            let on_changed = on_changed.clone();
            Rc::new(move |i: usize| {
                if let Some(opt) = options.get(i) {
                    selected.set(Some(i));
                    if let Some(cb) = &on_changed {
                        cb(i, &opt.label);
                    }
                }
                nav.set_active(None);
                hide_overlay();
            })
        };
        let pick_by_row: Rc<dyn Fn(usize)> = {
            let enabled = enabled.clone();
            let pick_opt = pick_opt.clone();
            Rc::new(move |row| {
                if let Some(&i) = enabled.get(row) {
                    pick_opt(i);
                }
            })
        };
        let handler = nav.handler(enabled.len(), move |row| pick_by_row(row), hide_overlay);
        node.register_editor(Rc::new(move |k| {
            handler(k);
        }));
    }

    // Trigger content: the selected option's label + icon (or the placeholder).
    let (label, sel_icon) = match selected.get() {
        Some(i) => options
            .get(i)
            .map(|o| (o.label.clone(), o.icon))
            .unwrap_or_else(|| (p.placeholder.clone(), None)),
        None => (p.placeholder.clone(), None),
    };
    let has_value = selected.get().is_some();
    let label_color = if has_value { c.foreground } else { c.muted_foreground };
    let lead = p.leading.or(sel_icon);

    let mut trigger_kids: Vec<AnyWidget> = Vec::new();
    if let Some(li) = lead {
        trigger_kids.push(icon(li).size(16.0).color(c.muted_foreground).into_widget());
        trigger_kids.push(gap_w(8.0).into_widget());
    }
    trigger_kids.push(text(label).size(14.0).color(label_color).into_widget());
    trigger_kids.push(spacer().into_widget());
    if p.clearable && has_value {
        let on_cleared = p.on_cleared.clone();
        trigger_kids.push(
            GestureDetector::new(
                icon(IconKind::Close).size(15.0).color(c.muted_foreground),
            )
            .cursor(Cursor::Pointer)
            .on_tap(move || {
                selected.set(None);
                nav.set_active(None);
                if let Some(f) = &on_cleared {
                    f();
                }
            })
            .into_widget(),
        );
    } else {
        let trail = p.trailing.unwrap_or_else(|| IconKind::ChevronDown.into());
        trigger_kids.push(icon(trail).size(16.0).color(c.muted_foreground).into_widget());
    }

    let deco = crate::style::style()
        .background(c.background)
        .border(Border::new(c.input, 1.0))
        .radius_all(theme().radius)
        .merge(p.style.clone().unwrap_or_default())
        .decoration()
        .unwrap_or_else(BoxDecoration::new);
    let trigger = Container::new()
        .width(width)
        .height(38.0)
        .decoration(deco)
        .padding(EdgeInsets::symmetric(12.0, 0.0))
        .alignment(Alignment::CENTER_LEFT)
        .child(row(trigger_kids));

    GestureDetector::new(trigger).cursor(Cursor::Pointer).on_tap(action_event(
        move |e: PointerEvent| {
            // The trigger's window-space rect = click global − click local.
            let trigger_left = e.global.x - e.position.x;
            let trigger_top = e.global.y - e.position.y;
            let trigger_h = 38.0;
            let (ww, wh) = window_size();

            // Menu height: rows + padding + group headers, capped (then it scrolls).
            let group_count = options.iter().filter(|o| o.group.is_some()).count();
            let natural = options.len() as f64 * 34.0 + group_count as f64 * 28.0 + 8.0;
            let menu_h = natural.min(300.0);
            let scrolls = natural > menu_h + 0.5;

            // Flip up when there isn't room below and there is room above.
            let below = trigger_top + trigger_h + 6.0;
            let above = trigger_top - menu_h - 6.0;
            let top = if wh > 0.0 && below + menu_h > wh - 8.0 && above >= 8.0 {
                above
            } else {
                below
            };
            // Shift horizontally to stay on-screen.
            let left = if ww > 0.0 {
                trigger_left.min(ww - width - 8.0).max(8.0)
            } else {
                trigger_left
            };

            let menu = component_props(
                render_select_menu,
                MenuProps {
                    options: options.clone(),
                    width,
                    selected,
                    on_changed: on_changed.clone(),
                    max_height: scrolls.then_some(menu_h),
                    nav,
                },
            );
            show_overlay(menu.into_widget(), left, top, width, menu_h);
            node.request_focus();
        },
    ))
    .into_widget()
}
