//! [`tooltip`] — a hover-delayed hint in the passive overlay layer (no scrim, never
//! blocks clicks). Hovering the trigger arms a delay ([`create_timeout`]-style keyed
//! timer); when it elapses a small chip is shown on the chosen [`Side`] of the pointer
//! — flipping to the opposite side and clamping when it would exit the window.
//!
//! # C2 geometry
//! The chip anchors to the trigger's laid-out rect via
//! [`use_bounds`](pebbles_core::use_bounds) (one frame behind) — `side_anchor` picks the
//! edge, [`chip_anchor`] applies the flip + lateral clamp (exact + unit-tested). It falls
//! back to the pointer only before the trigger has been laid out once. `show_on_focus`
//! (default on) shows the chip without delay while the focused element (via
//! [`focus_bounds`](pebbles_core::bounds::focus_bounds)) sits inside the trigger.

use pebbles_foundation::{Color, Offset, Rect};
use pebbles_render::{Border, BoxShadow, PointerEvent};

use crate::Side;
use crate::overlay::{hide_passive, show_passive, window_size};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, text};
use pebbles_core::bounds::focus_bounds;
use pebbles_core::context::action_event;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animation, component_props, create_signal, use_bounds};

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
    /// `true`, the a11y win) — driven by `focus_bounds` against the trigger's rect.
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
    show_on_focus: bool,
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
                show_on_focus: self.show_on_focus,
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

/// The chip's anchor point: the edge-center of the trigger rect for `side`.
fn side_anchor(side: Side, r: Rect) -> (f64, f64) {
    let cx = (r.x0 + r.x1) / 2.0;
    let cy = (r.y0 + r.y1) / 2.0;
    match side {
        Side::Top => (cx, r.y0),
        Side::Bottom => (cx, r.y1),
        Side::Left => (r.x0, cy),
        Side::Right => (r.x1, cy),
    }
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

    // C2: the trigger's own laid-out rect (one frame behind) → anchor the chip to its
    // edge, and drive show-on-focus by testing whether the focused element is inside it.
    let own = use_bounds();
    let anchored = own.width() > 0.0;
    let build_props = {
        let (label, rich, tstyle) = (label.clone(), rich.clone(), tstyle.clone());
        move || Props {
            child: Container::new().into_widget(),
            label: label.clone(),
            rich: rich.clone(),
            delay: 0.0,
            side,
            show_on_focus: false,
            style: tstyle.clone(),
        }
    };

    // Show-on-focus: show (no delay) while the focused element sits inside the trigger;
    // hide when focus leaves. `focus_showing` gates the hide so we don't fight hover.
    let focus_showing = create_signal(false);
    if p.show_on_focus && anchored {
        let inside = focus_bounds().is_some_and(|fb| own.contains(fb.center()));
        if inside {
            let props = build_props();
            let (cw, ch) = estimate_chip_size(&props);
            let (ww, wh) = window_size();
            let (ax, ay) = side_anchor(side, own);
            let (left, top) = chip_anchor(side, ax, ay, cw, ch, 8.0, ww, wh);
            show_passive(chip(&props), left, top);
            if !focus_showing.peek() {
                focus_showing.set(true);
            }
        } else if focus_showing.peek() {
            hide_passive();
            focus_showing.set(false);
        }
    }

    // Show the chip immediately, anchored — shared by hover-timer fire and long-press.
    let show_chip = {
        let build_props = build_props.clone();
        move |ax: f64, ay: f64, gap: f64| {
            let props = build_props();
            let (cw, ch) = estimate_chip_size(&props);
            let (ww, wh) = window_size();
            let (left, top) = chip_anchor(side, ax, ay, cw, ch, gap, ww, wh);
            show_passive(chip(&props), left, top);
        }
    };

    GestureDetector::new(p.child.clone())
        .on_hover_enter(action_event({
            let show_chip = show_chip.clone();
            move |e: PointerEvent| {
                // Anchor to the trigger edge when the rect is known, else to the pointer.
                let (ax, ay, gap) = if anchored {
                    let (x, y) = side_anchor(side, own);
                    (x, y, 8.0)
                } else {
                    (e.global.x, e.global.y, 12.0)
                };
                let show_chip = show_chip.clone();
                animation::set_timeout(key, delay, move || show_chip(ax, ay, gap));
            }
        }))
        .on_hover_exit(move || {
            animation::clear_timeout(key);
            hide_passive();
        })
        // Touch: a long-press shows the tooltip (no hover on touch devices), and
        // lifting the finger (or the press ending) hides it.
        .on_long_press_start(action_event(move |e: PointerEvent| {
            let (ax, ay, gap) = if anchored {
                let (x, y) = side_anchor(side, own);
                (x, y, 8.0)
            } else {
                (e.global.x, e.global.y, 12.0)
            };
            show_chip(ax, ay, gap);
        }))
        .on_long_press_up(hide_passive)
        .on_long_press_end(hide_passive)
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
