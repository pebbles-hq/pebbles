//! Shared building blocks for overlay menus (Select, DropdownMenu, Combobox,
//! MultiSelect): the popover **surface** (bg + border + shadow) and the anchoring
//! math that flips a menu above its trigger when there isn't room below.

use pebbles_foundation::{Color, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow};

use crate::theme::theme;
use crate::widgets::Container;
use crate::overlay::window_size;
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
                .shadow(BoxShadow::new(
                    Color::from_rgba8(0, 0, 0, 45),
                    Offset::new(0.0, 8.0),
                    22.0,
                    -4.0,
                )),
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
