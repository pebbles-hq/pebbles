//! Shared building blocks for overlay menus (Select, DropdownMenu, Combobox,
//! MultiSelect): the popover **surface** (bg + border + shadow) and the anchoring
//! math that flips a menu above its trigger when there isn't room below — plus the
//! public [`Popover`] component (arbitrary content under any trigger).

use pebbles_foundation::{Color, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, PointerEvent};

use crate::overlay::{show_overlay_guarded, window_size};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector};
use pebbles_core::context::action_event;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// The standard popover container — popover background, hairline border, themed
/// radius and a soft drop shadow. `width` is the menu width; `pad` the inner inset.
pub(crate) fn popover_surface(width: f64, pad: f64, child: AnyWidget) -> AnyWidget {
    let c = theme().colors;
    Container::new()
        .width(width)
        .decoration(
            BoxDecoration::new()
                .color(c.popover)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius))
                .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 45), Offset::new(0.0, 8.0), 22.0, -4.0)),
        )
        .padding(EdgeInsets::all(pad))
        .child(child)
        .into_widget()
}

/// Window-space top-left for a menu of `width × content_h` anchored to a trigger
/// whose window rect is `(trigger_left, trigger_top, _, trigger_h)`. Opens below;
/// flips above when there's no room below but room above; clamps horizontally.
pub(crate) fn anchor_below(
    trigger_left: f64,
    trigger_top: f64,
    trigger_h: f64,
    width: f64,
    content_h: f64,
) -> (f64, f64) {
    let (ww, wh) = window_size();
    let below = trigger_top + trigger_h + 6.0;
    let above = trigger_top - content_h - 6.0;
    let top = if wh > 0.0 && below + content_h > wh - 8.0 && above >= 8.0 { above } else { below };
    let left = if ww > 0.0 { trigger_left.min(ww - width - 8.0).max(8.0) } else { trigger_left };
    (left, top)
}

// ---------------------------------------------------------------------------
// Popover — public component (arbitrary content under a trigger)
// ---------------------------------------------------------------------------

/// A click-triggered floating panel with arbitrary content, rendered in the overlay
/// layer (so it flips near edges, follows page scroll, and hosts real inputs). Build
/// with [`popover`]; dismisses on outside-click / Escape via the overlay scrim.
#[derive(Clone, Default)]
pub struct Popover {
    trigger: Option<AnyWidget>,
    content: Option<AnyWidget>,
    width: f64,
    /// Content-height hint used to flip above/below and drive scroll-follow.
    height: f64,
    /// Trigger height used for anchoring (the panel opens just below it).
    trigger_height: f64,
    pad: f64,
    style: Option<crate::style::Style>,
}

/// Create a [`Popover`]: clicking `trigger` opens `content` in a floating surface.
/// `trigger` is last (the in-tree child convention).
pub fn popover(content: impl IntoWidget, trigger: impl IntoWidget) -> Popover {
    Popover {
        trigger: Some(trigger.into_widget()),
        content: Some(content.into_widget()),
        width: 280.0,
        height: 220.0,
        trigger_height: 40.0,
        ..Default::default()
    }
}

/// The default popover-surface presentation as a [`Style`], so callers can merge
/// overrides onto it.
fn popover_base() -> crate::style::Style {
    let c = theme().colors;
    crate::style::style()
        .background(c.popover)
        .border(Border::new(c.border, 1.0))
        .radius_all(theme().radius)
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 45), Offset::new(0.0, 8.0), 22.0, -4.0))
}

impl Popover {
    /// The panel width (default 280).
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    /// Content-height hint for edge-flipping + scroll-follow (default 220).
    pub fn height(mut self, height: f64) -> Self {
        self.height = height;
        self
    }
    /// The trigger's height, used to anchor the panel just below it (default 40).
    pub fn trigger_height(mut self, h: f64) -> Self {
        self.trigger_height = h;
        self
    }
    /// Inner padding of the surface (default 8).
    pub fn pad(mut self, pad: f64) -> Self {
        self.pad = pad;
        self
    }
    /// Merge a [`Style`](crate::Style) onto the popover surface (bg / border / radius /
    /// shadow overrides).
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
}

impl IntoWidget for Popover {
    fn into_widget(mut self) -> AnyWidget {
        let content = self.content.take().unwrap_or_else(|| Container::new().into_widget());
        let trigger = self.trigger.take().unwrap_or_else(|| Container::new().into_widget());
        let (width, height, trigger_height, pad) = (self.width, self.height, self.trigger_height, self.pad);
        let user_style = self.style.take();
        // Owner token: created during the parent component's render, so it dies
        // with the parent — the probe that GCs an orphaned panel.
        let owner = pebbles_core::create_signal(());
        GestureDetector::new(trigger)
            .cursor(Cursor::Pointer)
            .on_tap(action_event(move |e: PointerEvent| {
                // The trigger's window-space top-left = click global − click local.
                let trigger_left = e.global.x - e.position.x;
                let trigger_top = e.global.y - e.position.y;
                let (left, top) = anchor_below(trigger_left, trigger_top, trigger_height, width, height);
                let surface = match &user_style {
                    None => popover_surface(width, pad, content.clone()),
                    Some(st) => {
                        // Merge onto the default surface look; keep the fixed width.
                        let merged = popover_base().width(width).merge(st.clone());
                        crate::style::styled(
                            crate::widgets::Padding::new(EdgeInsets::all(pad), content.clone()),
                            merged,
                        )
                    }
                };
                show_overlay_guarded(surface, left, top, width, height, move || owner.alive());
            }))
            .into_widget()
    }
}
