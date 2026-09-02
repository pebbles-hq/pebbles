//! [`tooltip`] — a hover-delayed hint in the passive overlay layer (no scrim, never
//! blocks clicks). Hovering the trigger arms a delay ([`create_timeout`]-style keyed
//! timer); when it elapses a small chip is shown on the chosen [`Side`] of the pointer
//! — flipping to the opposite side and clamping when it would exit the window.
//!
//! # C2 note on geometry
//! The chip anchors to the **pointer** (the geometry available in a hover event), not
//! the trigger's laid-out rect — a widget can't read its own render bounds today. The
//! side/flip/clamp math ([`chip_anchor`]) is exact and unit-tested; refining the anchor
//! to the trigger rect (and wiring `show_on_focus` to focus geometry) is a shell-level
//! follow-up. `show_on_focus` is accepted (default on) and reserved for that.

use pebbles_foundation::{Color, Offset};
use pebbles_render::{Border, BoxShadow, PointerEvent};

use crate::Side;
use crate::overlay::{hide_passive, show_passive, window_size};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, text};
use pebbles_core::context::action_event;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animation, component_props, create_signal};

/// A tooltip wrapping a trigger. Build with [`tooltip`].
#[derive(Clone)]
pub struct Tooltip {
    child: Option<AnyWidget>,
    label: String,
    rich: Option<AnyWidget>,
    delay: f64,
    side: Side,
    show_on_focus: bool,
    style: Option<crate::style::Style>,
}

impl Default for Tooltip {
    fn default() -> Self {
        Tooltip {
            child: None,
            label: String::new(),
            rich: None,
            delay: 0.5,
            side: Side::Top,
            show_on_focus: true,
            style: None,
        }
    }
}

/// Show `label` when hovering `child` after a short delay. `child` is last (the
/// in-tree child convention).
pub fn tooltip(label: impl Into<String>, child: impl IntoWidget) -> Tooltip {
    Tooltip { child: Some(child.into_widget()), label: label.into(), ..Default::default() }
}

impl Tooltip {
    /// Seconds to hover before the tooltip appears (default 0.5).
    pub fn delay(mut self, secs: f64) -> Self {
        self.delay = secs;
        self
    }
    /// Which [`Side`] of the trigger the chip prefers (default [`Side::Top`]); it flips
    /// to the opposite side when there's no room.
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
    /// Whether the tooltip also shows when the trigger gains keyboard focus (default
    /// `true`, the a11y win). Reserved: focus-driven show needs shell-level geometry
    /// (see the module note) and is not yet wired.
    pub fn show_on_focus(mut self, yes: bool) -> Self {
        self.show_on_focus = yes;
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
    side: Side,
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
                side: self.side,
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

/// Estimate the chip's rendered size from its label (plain chips only) — the passive
/// layer positions by top-left, so a rough size is enough to place the chip on the
/// requested side. Rich chips fall back to a compact default.
fn estimate_chip_size(p: &Props) -> (f64, f64) {
    match &p.rich {
        Some(_) => (160.0, 40.0),
        None => {
            let w = (p.label.chars().count() as f64 * 7.0 + 20.0).clamp(40.0, 320.0);
            (w, 28.0)
        }
    }
}

/// Compute the chip's top-left for `side`, flipping to the opposite side when the chip
/// would exit the window and clamping into an 8px margin. `(ax, ay)` is the anchor
/// point, `(cw, ch)` the chip size, `gap` the offset from the anchor, `(ww, wh)` the
/// window (a non-positive dimension disables the corresponding flip/clamp — headless).
/// Pure — the C2 geometry under test.
pub(crate) fn chip_anchor(
    side: Side,
    ax: f64,
    ay: f64,
    cw: f64,
    ch: f64,
    gap: f64,
    ww: f64,
    wh: f64,
) -> (f64, f64) {
    let fits = |s: Side| match s {
        Side::Top => ay - gap - ch >= 0.0,
        Side::Bottom => ay + gap + ch <= wh,
        Side::Left => ax - gap - cw >= 0.0,
        Side::Right => ax + gap + cw <= ww,
    };
    // Flip only when the window is known, the requested side doesn't fit, and the
    // opposite one does.
    let side = if ww > 0.0 && wh > 0.0 && !fits(side) && fits(side.flip()) { side.flip() } else { side };
    let (mut left, mut top) = match side {
        Side::Top => (ax - cw / 2.0, ay - gap - ch),
        Side::Bottom => (ax - cw / 2.0, ay + gap),
        Side::Left => (ax - gap - cw, ay - ch / 2.0),
        Side::Right => (ax + gap, ay - ch / 2.0),
    };
    if ww > 0.0 {
        left = left.clamp(8.0, (ww - cw - 8.0).max(8.0));
    }
    if wh > 0.0 {
        top = top.clamp(8.0, (wh - ch - 8.0).max(8.0));
    }
    (left, top)
}

fn render_tooltip(p: &Props) -> AnyWidget {
    // A stable per-instance key for the show-delay timer (survives re-renders).
    let key = create_signal(()).raw_id();
    let delay = p.delay;
    let side = p.side;
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
                let props = Props {
                    child: Container::new().into_widget(),
                    label: label.clone(),
                    rich: rich.clone(),
                    delay: 0.0,
                    side,
                    style: tstyle.clone(),
                };
                let (cw, ch) = estimate_chip_size(&props);
                let (ww, wh) = window_size();
                let (left, top) = chip_anchor(side, gx, gy, cw, ch, 12.0, ww, wh);
                show_passive(chip(&props), left, top);
            });
        }))
        .on_hover_exit(move || {
            animation::clear_timeout(key);
            hide_passive();
        })
        .into_widget()
}

#[cfg(test)]
mod tests {
    use super::chip_anchor;
    use crate::Side;

    #[test]
    fn bottom_side_sits_below_and_centers() {
        // Plenty of room: Bottom keeps the side, top is anchor+gap, centered on x.
        let (left, top) = chip_anchor(Side::Bottom, 100.0, 100.0, 80.0, 28.0, 12.0, 400.0, 400.0);
        assert_eq!(top, 112.0, "chip top = anchor + gap");
        assert_eq!(left, 60.0, "centered: anchor - cw/2");
    }

    #[test]
    fn top_near_the_edge_flips_to_bottom() {
        // Anchor near the top: Side::Top doesn't fit (would go negative) → flips down.
        let (_left, top) = chip_anchor(Side::Top, 100.0, 10.0, 80.0, 28.0, 12.0, 400.0, 400.0);
        assert_eq!(top, 22.0, "flipped below: anchor + gap");
    }

    #[test]
    fn right_near_the_edge_flips_to_left() {
        // Anchor near the right edge: Side::Right overflows → flips left.
        let (left, _top) = chip_anchor(Side::Right, 395.0, 200.0, 80.0, 28.0, 12.0, 400.0, 400.0);
        assert_eq!(left, 303.0, "flipped left: anchor - gap - cw");
    }

    #[test]
    fn lateral_clamp_keeps_the_chip_in_the_window() {
        // Bottom side, anchor hard against the left: centered x would be negative → clamped.
        let (left, _top) = chip_anchor(Side::Bottom, 2.0, 100.0, 80.0, 28.0, 12.0, 400.0, 400.0);
        assert_eq!(left, 8.0, "clamped to the 8px left margin");
    }

    #[test]
    fn headless_window_disables_flip_and_clamp() {
        // ww/wh <= 0 (no window): keep the requested side, no clamp.
        let (left, top) = chip_anchor(Side::Top, 100.0, 100.0, 80.0, 28.0, 12.0, 0.0, 0.0);
        assert_eq!(top, 60.0, "Top: anchor - gap - ch");
        assert_eq!(left, 60.0);
    }
}
