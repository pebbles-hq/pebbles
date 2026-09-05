//! [`hero`] — shared-element transitions between routes (Flutter's `Hero`).
//!
//! A `hero(tag, child)` publishes its on-screen rect (via [`use_bounds`]) into a
//! tag-keyed registry each frame. When you navigate through [`fly_heroes`], it
//! snapshots the outgoing screen's hero rects, runs your route change, then — once
//! the new screen has laid out — animates an overlay "flight" for every tag present
//! on both screens, easing each hero's `child` from its old rect to its new one.
//!
//! ```ignore
//! // grid screen and detail screen both have `hero("photo-7", image)`
//! button("open").on_pressed(move || fly_heroes(0.3, move || nav.update(|n| n.push("detail"))));
//! ```
//!
//! Because Pebbles' `RouteView` swaps instantly (no built-in page transition), the
//! flight is driven from the navigation callback rather than a route-transition hook.

use std::cell::RefCell;
use std::collections::HashMap;

use pebbles_foundation::Rect;

use crate::widgets::{Container, SizedBox, positioned, stack};
use crate::{hide_overlay, show_overlay};
use pebbles_core::reactive::dispose_root_signal;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    Element, Signal, animate_to, component_props, create_cleanup, create_root_signal, create_signal,
    create_timeout, use_bounds,
};

/// A hero's last-known geometry + the widget to fly.
#[derive(Clone)]
struct HeroSnapshot {
    rect: Rect,
    child: AnyWidget,
}

thread_local! {
    /// tag → latest on-screen rect + child, refreshed each frame by mounted heroes.
    static REGISTRY: RefCell<HashMap<String, HeroSnapshot>> = RefCell::new(HashMap::new());
}

/// A shared-element hero. Give the matching widgets on both screens the same `tag`.
#[derive(Clone)]
pub struct Hero {
    tag: String,
    child: Option<AnyWidget>,
}

/// Mark `child` as the shared element identified by `tag` — the same `tag` on the
/// next screen is where it flies to (see [`fly_heroes`]).
pub fn hero(tag: impl Into<String>, child: impl IntoWidget) -> Hero {
    Hero { tag: tag.into(), child: Some(child.into_widget()) }
}

impl IntoWidget for Hero {
    fn into_widget(self) -> AnyWidget {
        component_props(render_hero, self).into_widget()
    }
}

fn render_hero(h: &Hero) -> Element {
    let rect = use_bounds(); // window-space rect, one frame behind
    let child = h.child.clone().unwrap_or_else(|| Container::new().into_widget());
    let tag = h.tag.clone();

    // Publish this hero's current geometry so a flight can read it.
    if rect.width() > 0.5 && rect.height() > 0.5 {
        REGISTRY.with(|r| r.borrow_mut().insert(tag.clone(), HeroSnapshot { rect, child: child.clone() }));
    }

    // Drop this tag from the registry on unmount (registered once).
    let registered = create_signal(false);
    if !registered.peek() {
        registered.set(true);
        let t = tag.clone();
        create_cleanup(move || {
            REGISTRY.with(|r| {
                r.borrow_mut().remove(&t);
            });
        });
    }

    child
}

/// The current window-space rect of the hero tagged `tag`, if one is mounted.
pub fn hero_rect(tag: &str) -> Option<Rect> {
    REGISTRY.with(|r| r.borrow().get(tag).map(|s| s.rect))
}

/// Run shared-element flights across a navigation. Call your route change inside
/// `navigate`; every tag present on both the old and new screen flies from its old
/// rect to its new one over `duration` seconds.
pub fn fly_heroes(duration: f64, navigate: impl FnOnce()) {
    // Snapshot the outgoing screen's heroes (registry is one frame behind, which is
    // exactly the last painted layout — correct for the source rects).
    let old: HashMap<String, HeroSnapshot> = REGISTRY.with(|r| r.borrow().clone());
    navigate();
    // Let the new screen mount, lay out, and publish its bounds (~2 frames), then
    // fly every tag that exists on both screens.
    create_timeout(0.032, move || {
        let flights: Vec<(Rect, Rect, AnyWidget)> = REGISTRY.with(|r| {
            let new = r.borrow();
            old.iter()
                .filter_map(|(tag, from)| new.get(tag).map(|to| (from.rect, to.rect, from.child.clone())))
                .filter(|(from, to, _)| from != to)
                .collect()
        });
        start_flights(flights, duration.max(0.0));
    });
}

fn start_flights(flights: Vec<(Rect, Rect, AnyWidget)>, dur: f64) {
    if flights.is_empty() || dur <= 0.0 {
        return;
    }
    let progress = create_root_signal(0.0_f64);
    animate_to(progress, 1.0, dur);
    let content = component_props(render_flights, FlightsProps { flights, progress }).into_widget();
    // One overlay covering the window; flight children use absolute (window) coords.
    show_overlay(content, 0.0, 0.0, 1.0e6, 1.0e6);
    create_timeout(dur, move || {
        hide_overlay();
        dispose_root_signal(progress);
    });
}

#[derive(Clone)]
struct FlightsProps {
    flights: Vec<(Rect, Rect, AnyWidget)>,
    progress: Signal<f64>,
}

fn render_flights(p: &FlightsProps) -> Element {
    let t = p.progress.get().clamp(0.0, 1.0);
    let kids: Vec<AnyWidget> = p
        .flights
        .iter()
        .map(|(from, to, child)| {
            let r = lerp_rect(*from, *to, t);
            positioned(SizedBox::exact(r.width().max(0.0), r.height().max(0.0), child.clone()))
                .left(r.x0)
                .top(r.y0)
                .into_widget()
        })
        .collect();
    stack(kids).into_widget()
}

/// Linearly interpolate two rects corner-to-corner.
fn lerp_rect(a: Rect, b: Rect, t: f64) -> Rect {
    let l = |x: f64, y: f64| x + (y - x) * t;
    Rect::new(l(a.x0, b.x0), l(a.y0, b.y0), l(a.x1, b.x1), l(a.y1, b.y1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_rect_hits_endpoints_and_midpoint() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(100.0, 200.0, 140.0, 260.0);
        assert_eq!(lerp_rect(a, b, 0.0), a);
        assert_eq!(lerp_rect(a, b, 1.0), b);
        let m = lerp_rect(a, b, 0.5);
        assert_eq!((m.x0, m.y0), (50.0, 100.0));
        assert_eq!((m.width(), m.height()), (25.0, 35.0)); // (10+40)/2, (10+60)/2
    }
}
