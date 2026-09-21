//! [`Menubar`] — a desktop menu strip: horizontal text triggers, each opening a
//! [`DropdownMenu`](super::super::input::DropdownMenu)-style menu below it. Click to
//! open; **while one is open, hovering a sibling switches to it**; Esc / outside-click
//! closes (via the overlay scrim). Reuses the `MenuEntry` machinery. shadcn's Menubar.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_foundation::{Alignment, MainAxisSize};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, PointerEvent};

use crate::components::input::list_nav::{ListNav, list_nav};
use crate::components::input::menu::{
    ChildCtx, MenuEntry, RebuildableMenu, RowTarget, SubMenuHandles, estimate_height, menu_sub,
};
use crate::components::input::popover::anchor_below;
use crate::overlay::{hide_overlay, is_open, show_overlay_guarded};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, row, text};
use pebbles_core::context::action_event;
use pebbles_core::focus::create_focus;
use pebbles_core::keyboard::{KeyInput, Motion};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{component_props, create_signal, create_signal_with};

/// One top-level menu in a [`Menubar`]: a label + its entries.
pub struct MenubarMenu {
    label: String,
    entries: Vec<MenuEntry>,
    width: f64,
}

/// Create a [`MenubarMenu`] labelled `label` over `entries`.
pub fn menubar_menu<I, E>(label: impl Into<String>, entries: I) -> MenubarMenu
where
    I: IntoIterator<Item = E>,
    E: Into<MenuEntry>,
{
    MenubarMenu { label: label.into(), entries: entries.into_iter().map(Into::into).collect(), width: 220.0 }
}

impl MenubarMenu {
    /// The dropdown width for this menu (default 220).
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
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
}

/// A horizontal menu strip. Build with [`menubar`], add menus with [`menu`](Menubar::menu).
#[derive(Default)]
pub struct Menubar {
    menus: Vec<MenubarMenu>,
    style: Option<crate::style::Style>,
}

/// Create an empty [`Menubar`]; add menus with [`menu`](Menubar::menu).
pub fn menubar() -> Menubar {
    Menubar::default()
}

impl Menubar {
    /// Append a top-level menu.
    pub fn menu<I, E>(mut self, label: impl Into<String>, entries: I) -> Self
    where
        I: IntoIterator<Item = E>,
        E: Into<MenuEntry>,
    {
        self.menus.push(menubar_menu(label, entries));
        self
    }
    /// Merge a [`Style`](crate::Style) over the strip: box props (background,
    /// border, radius, padding) style the bar; text props (color, size) style
    /// the trigger labels.
    pub fn style(mut self, style: crate::style::Style) -> Self {
        self.style = Some(style);
        self
    }
}

struct Props {
    menus: Vec<MenubarMenu>,
    style: Option<crate::style::Style>,
}

impl IntoWidget for Menubar {
    fn into_widget(self) -> AnyWidget {
        component_props(render_menubar, Props { menus: self.menus, style: self.style }).into_widget()
    }
}

/// Per-menu prebuilt blueprint, geometry and keyboard rows — rebuilt each render and
/// captured at open time by the overlay and the strip's key handler.
struct MenuData {
    bp: Rc<RebuildableMenu>,
    width: f64,
    menu_h: f64,
    actions: Vec<Rc<dyn Fn()>>,
    navigable: Vec<RowTarget>,
    sub_rows: Vec<(Rc<RebuildableMenu>, f64)>,
}

/// Props for the open menu — a component so the keyboard highlight re-renders
/// reactively as the shared [`ListNav`] active row changes (mirrors the dropdown).
struct MbMenuProps {
    bp: Rc<RebuildableMenu>,
    width: f64,
    nav: ListNav,
    actions: Vec<Rc<dyn Fn()>>,
    handles: SubMenuHandles,
}

fn render_mb_menu(p: &MbMenuProps) -> AnyWidget {
    p.bp.build_rows(p.width, p.nav.active(), &p.actions, &p.handles)
}

fn render_menubar(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let th = theme();
    // The trigger row height is the uniform control height (density-scaled); the
    // dropdown anchors directly below it, so both use the same value.
    let trigger_h = th.control_height;
    // Which top-level menu is open (index), so the trigger can highlight and hover can
    // switch. `is_open()` (the overlay) is the source of truth for "a menu is showing".
    let open = create_signal(Option::<usize>::None);
    let child_nav = list_nav();
    let child_ctx = create_signal::<Option<Rc<ChildCtx>>>(None);
    let merged = crate::style::style().merge(p.style.clone().unwrap_or_default());
    let label_color = merged.color.unwrap_or(c.foreground);
    let label_size = merged.font_size.unwrap_or(14.0);

    let n = p.menus.len();
    // One focus node for the whole strip + a roving keyboard index selecting which
    // trigger Enter/Space/Down opens (shadcn menubar roving-tabindex; the tabs.rs idiom
    // — per-trigger `create_focus()` would collide on the shared component owner).
    let node = create_focus();
    let focused = create_signal(0usize);
    // The open menu's row cursor (keyboard highlight); reset on every open.
    let nav = list_nav();
    // One window-rect cell per trigger, filled by a position_probe each paint, so
    // keyboard-open anchors under the focused trigger with no pointer event. Stable
    // across renders (a signal-held Rc), unlike the per-render `data` below.
    let rects = create_signal_with(|| {
        Rc::new(
            std::iter::repeat_with(|| Rc::new(Cell::new(None::<(f64, f64, f64, f64)>)))
                .take(n)
                .collect::<Vec<_>>(),
        )
    })
    .peek();
    let data: Rc<Vec<MenuData>> = Rc::new(
        p.menus
            .iter()
            .map(|m| {
                let bp = Rc::new(RebuildableMenu::from(&m.entries));
                MenuData {
                    actions: bp.actions(),
                    navigable: bp.navigable(),
                    sub_rows: bp.sub_rows(),
                    menu_h: estimate_height(&m.entries),
                    width: m.width,
                    bp,
                }
            })
            .collect(),
    );

    // Open menu `i` anchored at a trigger's window top-left. Shared by pointer
    // tap/hover and keyboard-open. The strip keeps focus and drives the open menu's
    // nav (the Select model — the open menu has no field of its own).
    let open_at: Rc<dyn Fn(usize, f64, f64)> = {
        let data = data.clone();
        Rc::new(move |i: usize, left: f64, top: f64| {
            let Some(d) = data.get(i) else {
                return;
            };
            let (l, t) = anchor_below(left, top, trigger_h, d.width, d.menu_h);
            let handles =
                SubMenuHandles { nav: child_nav, ctx: child_ctx, subs: Rc::new(d.sub_rows.clone()) };
            nav.set_active(None);
            let menu = component_props(
                render_mb_menu,
                MbMenuProps { bp: d.bp.clone(), width: d.width, nav, actions: d.actions.clone(), handles },
            );
            show_overlay_guarded(menu.into_widget(), l, t, d.width, d.menu_h, move || open.alive());
            open.set(Some(i));
        })
    };
    // Keyboard-open: anchor under the focused trigger via its probed rect (top-left
    // fallback only before the first paint).
    let open_kbd: Rc<dyn Fn(usize)> = {
        let open_at = open_at.clone();
        let rects = rects.clone();
        Rc::new(move |i: usize| {
            let (l, t) =
                rects.get(i).and_then(|cell| cell.get()).map(|(l, t, _, _)| (l, t)).unwrap_or((8.0, 8.0));
            open_at(i, l, t);
        })
    };

    // Enter/Space activation opens the focused trigger's menu when none is open.
    node.register(
        {
            let open_kbd = open_kbd.clone();
            Rc::new(move || {
                if !is_open() {
                    open_kbd(focused.peek());
                }
            })
        },
        None,
        false,
    );
    // register_keys (NOT register_editor): Left/Right roam the top-level menus (and
    // switch the open one), Down/Up/Enter open the focused menu when closed and drive
    // the open menu's row cursor when open. Everything else returns false so shell
    // shortcuts / Tab still work while the strip holds focus.
    {
        let open_kbd = open_kbd.clone();
        let data = data.clone();
        node.register_keys(Rc::new(move |k: KeyInput| {
            if n == 0 {
                return false;
            }
            // Route Down/Up/Enter to the currently-open menu's list nav.
            let route = |k: KeyInput| -> bool {
                if let Some(oi) = open.peek()
                    && let Some(d) = data.get(oi)
                {
                    let navigable = d.navigable.clone();
                    let actions = d.actions.clone();
                    let handler = nav.handler(
                        navigable.len(),
                        move |row| {
                            if let Some(RowTarget::Action(a)) = navigable.get(row) {
                                actions[*a]();
                            }
                        },
                        hide_overlay,
                    );
                    handler(k)
                } else {
                    false
                }
            };
            match k {
                KeyInput::Move { motion: Motion::Left, .. } => {
                    let next = (focused.peek() + n - 1) % n;
                    focused.set(next);
                    if is_open() {
                        open_kbd(next);
                    }
                    true
                }
                KeyInput::Move { motion: Motion::Right, .. } => {
                    let next = (focused.peek() + 1) % n;
                    focused.set(next);
                    if is_open() {
                        open_kbd(next);
                    }
                    true
                }
                KeyInput::Move { motion: Motion::Down | Motion::Up, .. } => {
                    if is_open() {
                        route(k)
                    } else {
                        open_kbd(focused.peek());
                        true
                    }
                }
                KeyInput::Enter => {
                    if is_open() {
                        route(k)
                    } else {
                        open_kbd(focused.peek());
                        true
                    }
                }
                KeyInput::Escape if is_open() => {
                    hide_overlay();
                    open.set(None);
                    true
                }
                _ => false,
            }
        }));
    }

    let strip_focused = node.is_focused();
    let mut triggers: Vec<AnyWidget> = Vec::with_capacity(n);
    for (i, m) in p.menus.iter().enumerate() {
        let active = open.get() == Some(i);
        let bg = if active { c.accent } else { palette_transparent() };
        let fg = if active { c.accent_foreground } else { label_color };

        let trigger = Container::new()
            .height(trigger_h)
            .alignment(Alignment::CENTER)
            .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(th.radius)))
            .padding(th.pad(12.0, 0.0))
            .child(text(m.label.clone()).size(label_size).weight(500.0).color(fg));

        let tap_open = open_at.clone();
        let hover_open = open_at.clone();
        let mut g: AnyWidget = GestureDetector::new(trigger)
            .cursor(Cursor::Pointer)
            // Click toggles this menu (and moves keyboard focus to it).
            .on_tap(action_event(move |e: PointerEvent| {
                if is_open() && open.peek() == Some(i) {
                    hide_overlay();
                    open.set(None);
                } else {
                    focused.set(i);
                    node.request_focus();
                    tap_open(i, e.global.x - e.position.x, e.global.y - e.position.y);
                }
            }))
            // While any menu is open, hovering a different trigger switches to it.
            .on_hover_enter(action_event(move |e: PointerEvent| {
                if is_open() && open.peek() != Some(i) {
                    focused.set(i);
                    hover_open(i, e.global.x - e.position.x, e.global.y - e.position.y);
                }
            }))
            .into_widget();

        // Focus ring on the roving-focused trigger while the strip holds focus.
        if strip_focused && focused.get() == i {
            g = Container::new()
                .decoration(
                    BoxDecoration::new()
                        .border(Border::new(c.ring, 2.0))
                        .radius(BorderRadius::all(th.radius)),
                )
                .child(g)
                .into_widget();
        }
        // Report this trigger's window rect each paint for keyboard-open anchoring.
        if let Some(cell) = rects.get(i).cloned() {
            g = crate::widgets::position_probe(move |o, s| cell.set(Some((o.x, o.y, s.width, s.height))), g)
                .into_widget();
        }
        // Each trigger is a button that opens a menu.
        let g = crate::widgets::semantics(pebbles_render::SemanticsRole::Button, m.label.clone(), g)
            .into_widget();
        triggers.push(g);
    }

    crate::style::styled(row(triggers).spacing(2.0).main_axis_size(MainAxisSize::Min), merged).into_widget()
}

fn palette_transparent() -> pebbles_foundation::Color {
    pebbles_foundation::palette::TRANSPARENT
}
