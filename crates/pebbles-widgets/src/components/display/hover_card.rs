//! [`HoverCard`] — a rich hover popover: hovering the trigger shows a floating card
//! after a short delay; unlike [`tooltip`](super::tooltip) the card holds arbitrary
//! content and **stays open while the pointer is over it** (a hover ref-count across
//! trigger + card, with a short close delay so moving between them doesn't flicker).
//! Rendered in the passive overlay layer (click-through). Mirrors shadcn's Hover Card.


use pebbles_foundation::{Color, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, PointerEvent};

use crate::overlay::{hide_passive, show_passive};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animation, component_props, create_signal};

/// A hover card wrapping a trigger. Build with [`hover_card`].
#[derive(Clone, Default)]
pub struct HoverCard {
    content: Option<AnyWidget>,
    trigger: Option<AnyWidget>,
    width: f64,
    delay: f64,
}

/// Show `content` in a floating card when `trigger` is hovered. `trigger` is last (the
/// in-tree child convention).
pub fn hover_card(content: impl IntoWidget, trigger: impl IntoWidget) -> HoverCard {
    HoverCard {
        content: Some(content.into_widget()),
        trigger: Some(trigger.into_widget()),
        width: 320.0,
        ..Default::default()
    }
}

impl HoverCard {
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    /// Seconds to hover before the card appears (default 0.6).
    pub fn delay(mut self, secs: f64) -> Self {
        self.delay = secs;
        self
    }
}

struct Props {
    content: AnyWidget,
    trigger: AnyWidget,
    width: f64,
    delay: f64,
}

impl IntoWidget for HoverCard {
    fn into_widget(mut self) -> AnyWidget {
        component_props(
            render_hover_card,
            Props {
                content: self.content.take().unwrap_or_else(|| Container::new().into_widget()),
                trigger: self.trigger.take().unwrap_or_else(|| Container::new().into_widget()),
                width: self.width,
                delay: self.delay,
            },
        )
        .into_widget()
    }
}

fn surface(content: AnyWidget, width: f64) -> AnyWidget {
    let c = theme().colors;
    Container::new()
        .width(width)
        .decoration(
            BoxDecoration::new()
                .color(c.popover)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius + 2.0))
                .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 45), Offset::new(0.0, 8.0), 24.0, -6.0)),
        )
        .padding(EdgeInsets::all(16.0))
        .child(content)
        .into_widget()
}

const CLOSE_DELAY: f64 = 0.28;

fn render_hover_card(p: &Props) -> AnyWidget {
    // Hover ref-count over {trigger, card}; the card closes only when it hits 0 and
    // stays 0 past CLOSE_DELAY (so moving trigger→card never flickers it shut).
    let over = create_signal(0i32);
    let show_key = create_signal(()).raw_id();
    let close_key = create_signal(()).raw_id().wrapping_add(1);
    let width = p.width;
    let delay = p.delay;
    let content = p.content.clone();

    let schedule_close = move || {
        animation::set_timeout(close_key, CLOSE_DELAY, move || {
            if over.peek() <= 0 {
                hide_passive();
            }
        });
    };

    GestureDetector::new(p.trigger.clone())
        .on_hover_enter(pebbles_core::context::action_event(move |e: PointerEvent| {
            over.update(|n| *n += 1);
            animation::clear_timeout(close_key);
            let (gx, gy) = (e.global.x, e.global.y);
            let content = content.clone();
            animation::set_timeout(show_key, delay, move || {
                if over.peek() <= 0 {
                    return;
                }
                // The card itself tracks hover so it stays open while pointed at.
                let card = GestureDetector::new(surface(content.clone(), width))
                    .on_hover_enter(move || {
                        over.update(|n| *n += 1);
                        animation::clear_timeout(close_key);
                    })
                    .on_hover_exit(move || {
                        over.update(|n| *n -= 1);
                        schedule_close();
                    });
                show_passive(card.into_widget(), gx + 4.0, gy + 20.0);
            });
        }))
        .on_hover_exit(move || {
            over.update(|n| *n -= 1);
            animation::clear_timeout(show_key);
            schedule_close();
        })
        .into_widget()
}
