//! [`AnimatedList`] — a list that animates items **in** as they're added and **out**
//! as they're removed (Flutter's `AnimatedList`).
//!
//! Built on keyed reconciliation: each item is a keyed component that animates
//! *itself* (size + fade) via [`transition`], so it enters on mount and — because
//! the list keeps a removed item alive for one exit tween before dropping it — exits
//! smoothly too. No per-item signal juggling: the list holds `(key, child, exiting)`
//! in a plain cell and bumps a version signal only when the structure changes.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use pebbles_foundation::Alignment;
use pebbles_render::BorderRadius;

use crate::widgets::{Align, Opacity, clip_rrect, column, keyed};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Element, component_props, create_signal, create_timeout, transition};

/// One live row: its key, current child, and whether it is animating out.
#[derive(Clone)]
struct Entry {
    key: u64,
    child: AnyWidget,
    exiting: bool,
}

/// A vertical list whose items animate in on add and out on remove.
#[derive(Clone)]
pub struct AnimatedList {
    items: Vec<(u64, AnyWidget)>,
    duration: f64,
}

/// An [`AnimatedList`] from `(key, child)` pairs — give each item a stable `u64`
/// key so adds and removes are tracked by identity (not position).
pub fn animated_list(items: Vec<(u64, AnyWidget)>) -> AnimatedList {
    AnimatedList { items, duration: 0.25 }
}

impl AnimatedList {
    /// Enter/exit duration in seconds (default `0.25`).
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
}

impl IntoWidget for AnimatedList {
    fn into_widget(self) -> AnyWidget {
        component_props(render_animated_list, self).into_widget()
    }
}

fn render_animated_list(b: &AnimatedList) -> Element {
    // Position-stable per-instance state (these two hooks always run first).
    let store = create_signal(Rc::new(RefCell::new(Vec::<Entry>::new())));
    let version = create_signal(0u64);
    let dur = b.duration;
    let cell = store.peek();

    // Reconcile desired items into the live store — a plain RefCell mutation, so it
    // never itself triggers a re-render (no feedback loop).
    {
        let mut live = cell.borrow_mut();
        let desired_keys: HashSet<u64> = b.items.iter().map(|(k, _)| *k).collect();

        // Mark removals; schedule the actual drop after the exit tween.
        for e in live.iter_mut() {
            if !desired_keys.contains(&e.key) && !e.exiting {
                e.exiting = true;
                let (st, ver, key) = (store, version, e.key);
                create_timeout(dur, move || {
                    st.peek().borrow_mut().retain(|x: &Entry| x.key != key);
                    ver.update(|n| *n += 1);
                });
            }
        }

        // Rebuild in desired order (existing updated, new added), then keep any
        // still-exiting items (not in desired) at the end until their timeout fires.
        let mut next: Vec<Entry> = Vec::with_capacity(live.len());
        for (k, child) in &b.items {
            next.push(Entry { key: *k, child: child.clone(), exiting: false });
        }
        for e in live.iter() {
            if e.exiting && !desired_keys.contains(&e.key) {
                next.push(e.clone());
            }
        }
        *live = next;
    }

    let _ = version.get(); // re-render when a removal timeout drops an item
    let entries: Vec<Entry> = cell.borrow().clone();
    let kids: Vec<AnyWidget> = entries
        .into_iter()
        .map(|e| {
            let item = component_props(render_item, ItemProps { child: e.child, exiting: e.exiting, dur });
            keyed(e.key, item).into_widget()
        })
        .collect();
    column(kids).into_widget()
}

/// One animating item: enters (size+fade 0→1) on mount, exits (1→0) when `exiting`.
#[derive(Clone)]
struct ItemProps {
    child: AnyWidget,
    exiting: bool,
    dur: f64,
}

fn render_item(p: &ItemProps) -> Element {
    // `transition` eases 0→1 while visible and 1→0 when hidden — exactly enter/exit.
    let tr = transition(!p.exiting, p.dur);
    let t = tr.t.clamp(0.0, 1.0);
    let faded = Opacity::new(t as f32, p.child.clone());
    // Collapse from the top (reveal downward), clipped to the animating height.
    let sized = Align::new(Alignment { x: 0.0, y: -1.0 }, faded).height_factor(t);
    clip_rrect(BorderRadius::ZERO, sized).into_widget()
}
