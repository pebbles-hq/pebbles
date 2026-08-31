//! [`context_menu`] — a right-click menu, opened at the cursor. Reuses the same
//! [`MenuEntry`] blueprint as [`DropdownMenu`](super::DropdownMenu); the only
//! difference is the trigger (secondary-tap) and the anchor (the pointer, clamped
//! on-screen). Left-click elsewhere / Escape dismisses via the overlay scrim.

use pebbles_render::PointerEvent;

use super::menu::{RebuildableMenu, estimate_height};
use crate::components::MenuEntry;
use crate::overlay::{show_overlay, window_size};
use crate::widgets::GestureDetector;
use pebbles_core::context::action_event;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use std::rc::Rc;

/// A right-click (context) menu wrapping a child. Build with [`context_menu`].
pub struct ContextMenu {
    child: Option<AnyWidget>,
    entries: Vec<MenuEntry>,
    width: f64,
}

/// Wrap `child` so a secondary (right) click opens a menu at the cursor.
pub fn context_menu(child: impl IntoWidget) -> ContextMenu {
    ContextMenu { child: Some(child.into_widget()), entries: Vec::new(), width: 220.0 }
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
    pub fn check(mut self, label: impl Into<String>, checked: bool, on_toggle: impl Fn(bool) + 'static) -> Self {
        self.entries.push(MenuEntry::Check { label: label.into(), checked, on_toggle: Rc::new(on_toggle) });
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
}

impl IntoWidget for ContextMenu {
    fn into_widget(mut self) -> AnyWidget {
        let child = self.child.take().unwrap_or_else(|| crate::widgets::Container::new().into_widget());
        let width = self.width;
        let blueprint = Rc::new(RebuildableMenu::from(&self.entries));
        let menu_h = estimate_height(&self.entries);
        GestureDetector::new(child)
            .on_secondary_tap_down(action_event(move |e: PointerEvent| {
                // Open at the cursor, clamped to stay on-screen.
                let (ww, wh) = window_size();
                let (gx, gy) = (e.global.x, e.global.y);
                let left = if ww > 0.0 { gx.min(ww - width - 8.0).max(8.0) } else { gx };
                let top = if wh > 0.0 { gy.min(wh - menu_h - 8.0).max(8.0) } else { gy };
                show_overlay(blueprint.build(width), left, top, width, menu_h);
            }))
            .into_widget()
    }
}
