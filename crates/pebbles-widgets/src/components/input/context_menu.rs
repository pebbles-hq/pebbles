//! [`context_menu`] — a right-click menu, opened at the cursor. Reuses the same
//! [`MenuEntry`] blueprint as [`DropdownMenu`](super::DropdownMenu); the only
//! difference is the trigger (secondary-tap) and the anchor (the pointer, clamped
//! on-screen). Left-click elsewhere / Escape dismisses via the overlay scrim.

use std::rc::Rc;

use pebbles_render::PointerEvent;

use super::list_nav::list_nav;
use super::menu::{ChildCtx, RebuildableMenu, SubMenuHandles, estimate_height};
use crate::components::{MenuEntry, menu_sub};
use crate::overlay::{show_overlay_guarded, window_size};
use crate::widgets::GestureDetector;
use pebbles_core::context::action_event;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{component_props, create_signal};

/// A right-click (context) menu wrapping a child. Build with [`context_menu`].
#[derive(Default)]
pub struct ContextMenu {
    child: Option<AnyWidget>,
    entries: Vec<MenuEntry>,
    width: f64,
    on_open: Option<Rc<dyn Fn(PointerEvent)>>,
}

/// Wrap `child` so a secondary (right) click opens a menu at the cursor.
pub fn context_menu(child: impl IntoWidget) -> ContextMenu {
    ContextMenu { child: Some(child.into_widget()), width: 220.0, ..Default::default() }
}

impl ContextMenu {
    /// Add a menu entry (`menu_item(..)`, or a `MenuEntry` from `menu_check`, etc.).
    pub fn item(mut self, item: impl Into<MenuEntry>) -> Self {
        self.entries.push(item.into());
        self
    }
    /// A non-interactive section label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.entries.push(MenuEntry::Label(label.into()));
        self
    }
    /// A divider between groups.
    pub fn separator(mut self) -> Self {
        self.entries.push(MenuEntry::Separator);
        self
    }
    /// A checkable row.
    pub fn check(
        mut self,
        label: impl Into<String>,
        checked: bool,
        on_toggle: impl Fn(bool) + 'static,
    ) -> Self {
        self.entries.push(MenuEntry::Check { label: label.into(), checked, on_toggle: Rc::new(on_toggle) });
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
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
    /// Run `f` on the right-click, just before the menu opens (e.g. sync the
    /// selection to the clicked row). Receives the pointer event.
    pub fn on_open(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_open = Some(Rc::new(f));
        self
    }
}

struct CtxProps {
    child: AnyWidget,
    width: f64,
    entries: Vec<MenuEntry>,
    on_open: Option<Rc<dyn Fn(PointerEvent)>>,
}

impl IntoWidget for ContextMenu {
    fn into_widget(mut self) -> AnyWidget {
        let child = self.child.take().unwrap_or_else(|| crate::widgets::Container::new().into_widget());
        component_props(
            render_context,
            CtxProps {
                child,
                width: self.width,
                entries: std::mem::take(&mut self.entries),
                on_open: self.on_open.take(),
            },
        )
        .into_widget()
    }
}

fn render_context(p: &CtxProps) -> AnyWidget {
    let child_nav = list_nav();
    let child_ctx = create_signal::<Option<Rc<ChildCtx>>>(None);
    let blueprint = Rc::new(RebuildableMenu::from(&p.entries));
    let handles = SubMenuHandles { nav: child_nav, ctx: child_ctx, subs: Rc::new(blueprint.sub_rows()) };
    let width = p.width;
    let menu_h = estimate_height(&p.entries);
    let on_open = p.on_open.clone();
    GestureDetector::new(p.child.clone())
        .on_secondary_tap_down(action_event(move |e: PointerEvent| {
            if let Some(f) = &on_open {
                f(e);
            }
            // Open at the cursor, clamped to stay on-screen.
            let (ww, wh) = window_size();
            let (gx, gy) = (e.global.x, e.global.y);
            let left = if ww > 0.0 { gx.min(ww - width - 8.0).max(8.0) } else { gx };
            let top = if wh > 0.0 { gy.min(wh - menu_h - 8.0).max(8.0) } else { gy };
            show_overlay_guarded(blueprint.build(width, &handles), left, top, width, menu_h, move || {
                child_ctx.alive()
            });
        }))
        .into_widget()
}
