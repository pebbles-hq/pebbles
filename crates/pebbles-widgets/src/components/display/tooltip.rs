//! [`tooltip`] — a hover-delayed hint in the passive overlay layer (no scrim, never
//! blocks clicks). Hovering the trigger arms a delay ([`create_timeout`]-style keyed
//! timer); when it elapses a small chip is shown near the pointer; hover-exit hides it
//! and cancels a pending show.

use pebbles_foundation::{Color, Offset};
use pebbles_render::{Border, BoxShadow, PointerEvent};

use crate::overlay::{hide_passive, show_passive};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, text};
use pebbles_core::context::{action, action_event};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animation, component_props, create_signal};

/// A tooltip wrapping a trigger. Build with [`tooltip`].
pub struct Tooltip {
    child: Option<AnyWidget>,
    label: String,
    rich: Option<AnyWidget>,
    delay: f64,
    style: Option<crate::style::Style>,
}

/// Wrap `child` so hovering it shows `label` after a short delay.
pub fn tooltip(child: impl IntoWidget, label: impl Into<String>) -> Tooltip {
    Tooltip { child: Some(child.into_widget()), label: label.into(), rich: None, delay: 0.5, style: None }
}

impl Tooltip {
    /// Seconds to hover before the tooltip appears (default 0.5).
    pub fn delay(mut self, secs: f64) -> Self {
        self.delay = secs;
        self
    }
    /// Show arbitrary content instead of a plain label.
    pub fn rich(mut self, content: impl IntoWidget) -> Self {
        self.rich = Some(content.into_widget());
        self
    }
    /// Merge a [`Style`](crate::Style) onto the chip surface.
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
}

struct Props {
    child: AnyWidget,
    label: String,
    rich: Option<AnyWidget>,
    delay: f64,
    style: Option<crate::style::Style>,
}

impl IntoWidget for Tooltip {
    fn into_widget(mut self) -> AnyWidget {
        component_props(
            render_tooltip,
            Props {
                child: self.child.take().unwrap_or_else(|| Container::new().into_widget()),
                label: self.label,
                rich: self.rich.take(),
                delay: self.delay,
                style: self.style.take(),
            },
        )
        .into_widget()
    }
}

/// The floating chip: popover surface, hairline border, 12px text.
fn chip(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let body: AnyWidget = match &p.rich {
        Some(w) => w.clone(),
        None => text(p.label.clone()).size(12.0).color(c.popover_foreground).into_widget(),
    };
    let base = crate::style::style()
        .background(c.popover)
        .border(Border::new(c.border, 1.0))
        .radius_all(6.0)
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 45), Offset::new(0.0, 6.0), 16.0, -4.0))
        .padding_xy(10.0, 6.0);
    crate::style::styled(body, base.merge(p.style.clone().unwrap_or_default()))
}

fn render_tooltip(p: &Props) -> AnyWidget {
    // A stable per-instance key for the show-delay timer (survives re-renders).
    let key = create_signal(()).raw_id();
    let delay = p.delay;
    // Capture what the timer needs (chip is rebuilt when it fires).
    let label = p.label.clone();
    let rich = p.rich.clone();
    let tstyle = p.style.clone();

    GestureDetector::new(p.child.clone())
        .on_hover_enter(action_event(move |e: PointerEvent| {
            let (gx, gy) = (e.global.x, e.global.y);
            let label = label.clone();
            let rich = rich.clone();
            let tstyle = tstyle.clone();
            animation::set_timeout(key, delay, move || {
                let props = Props { child: Container::new().into_widget(), label: label.clone(), rich: rich.clone(), delay: 0.0, style: tstyle.clone() };
                // Anchor just below the pointer.
                show_passive(chip(&props), gx + 12.0, gy + 18.0);
            });
        }))
        .on_hover_exit(action(move || {
            animation::clear_timeout(key);
            hide_passive();
        }))
        .into_widget()
}
