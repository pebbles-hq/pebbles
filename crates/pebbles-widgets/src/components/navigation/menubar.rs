//! [`Menubar`] — a desktop menu strip: horizontal text triggers, each opening a
//! [`DropdownMenu`](super::super::input::DropdownMenu)-style menu below it. Click to
//! open; **while one is open, hovering a sibling switches to it**; Esc / outside-click
//! closes (via the overlay scrim). Reuses the `MenuEntry` machinery. shadcn's Menubar.

use std::rc::Rc;

use pebbles_foundation::{Alignment, EdgeInsets, MainAxisSize};
use pebbles_render::{BorderRadius, BoxDecoration, Cursor, PointerEvent};

use crate::components::input::list_nav::list_nav;
use crate::components::input::menu::{
    ChildCtx, MenuEntry, RebuildableMenu, SubMenuHandles, estimate_height, menu_sub,
};
use crate::components::input::popover::anchor_below;
use crate::overlay::{hide_overlay, is_open, show_overlay_guarded};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, row, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::context::action_event;
use pebbles_core::{component_props, create_signal};

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
    MenubarMenu {
        label: label.into(),
        entries: entries.into_iter().map(Into::into).collect(),
        width: 220.0,
    }
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

const TRIGGER_H: f64 = 34.0;

fn render_menubar(p: &Props) -> AnyWidget {
    let c = theme().colors;
    // Which top-level menu is open (index), so the trigger can highlight and hover can
    // switch. `is_open()` (the overlay) is the source of truth for "a menu is showing".
    let open = create_signal(Option::<usize>::None);
    let child_nav = list_nav();
    let child_ctx = create_signal::<Option<Rc<ChildCtx>>>(None);
    let merged = crate::style::style().merge(p.style.clone().unwrap_or_default());
    let label_color = merged.color.unwrap_or(c.foreground);
    let label_size = merged.font_size.unwrap_or(14.0);

    let mut triggers: Vec<AnyWidget> = Vec::with_capacity(p.menus.len());
    for (i, m) in p.menus.iter().enumerate() {
        let bp = Rc::new(RebuildableMenu::from(&m.entries));
        let menu_h = estimate_height(&m.entries);
        let width = m.width;
        let active = open.get() == Some(i);
        let bg = if active { c.accent } else { palette_transparent() };
        let fg = if active { c.accent_foreground } else { label_color };

        let show_at = {
            let bp = bp.clone();
            move |e: &PointerEvent| {
                let left = e.global.x - e.position.x;
                let top = e.global.y - e.position.y;
                let (l, t) = anchor_below(left, top, TRIGGER_H, width, menu_h);
                let handles = SubMenuHandles {
                    nav: child_nav,
                    ctx: child_ctx,
                    subs: Rc::new(bp.sub_rows()),
                };
                show_overlay_guarded(bp.build(width, &handles), l, t, width, menu_h, move || {
                    open.alive()
                });
                open.set(Some(i));
            }
        };

        let trigger = Container::new()
            .height(TRIGGER_H)
            .alignment(Alignment::CENTER)
            .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(theme().radius)))
            .padding(EdgeInsets::symmetric(12.0, 0.0))
            .child(text(m.label.clone()).size(label_size).weight(500.0).color(fg));

        let hover_show = show_at.clone();
        let g = GestureDetector::new(trigger)
            .cursor(Cursor::Pointer)
            // Click toggles this menu.
            .on_tap(action_event(move |e: PointerEvent| {
                if is_open() && open.peek() == Some(i) {
                    hide_overlay();
                    open.set(None);
                } else {
                    show_at(&e);
                }
            }))
            // While any menu is open, hovering a different trigger switches to it.
            .on_hover_enter(action_event(move |e: PointerEvent| {
                if is_open() && open.peek() != Some(i) {
                    hover_show(&e);
                }
            }));
        triggers.push(g.into_widget());
    }

    crate::style::styled(
        row(triggers).spacing(2.0).main_axis_size(MainAxisSize::Min),
        merged,
    )
    .into_widget()
}

fn palette_transparent() -> pebbles_foundation::Color {
    pebbles_foundation::palette::TRANSPARENT
}
