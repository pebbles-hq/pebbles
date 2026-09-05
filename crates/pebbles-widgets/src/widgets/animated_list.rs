//! [`AnimatedList`] / [`AnimatedGrid`] — a list/grid that animates items **in** as
//! they're added and **out** as they're removed (Flutter's `AnimatedList`/`AnimatedGrid`).
//!
//! Built on keyed reconciliation: each item is a keyed component that animates
//! *itself* (list = size+fade, grid = scale+fade) via [`transition`], so it enters
//! on mount and — because the list keeps a removed item alive for one exit tween
//! before dropping it — exits smoothly too. No per-item signal juggling: the list
//! holds `(key, child, exiting)` in a plain cell and bumps a version signal only
//! when the structure changes.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use pebbles_foundation::Alignment;
use pebbles_render::BorderRadius;

use crate::widgets::{Align, Opacity, Transform, clip_rrect, column, keyed, wrap};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Element, Signal, component_props, create_signal, create_timeout, transition};

/// One live cell: its key, current child, and whether it is animating out.
#[derive(Clone)]
struct Entry {
    key: u64,
    child: AnyWidget,
    exiting: bool,
}

type Store = Signal<Rc<RefCell<Vec<Entry>>>>;

/// Reconcile desired `items` into the live store (a RefCell mutation, so it never
/// itself triggers a re-render), scheduling exits, and return the ordered entries.
fn reconcile_entries(store: Store, version: Signal<u64>, items: &[(u64, AnyWidget)], dur: f64) -> Vec<Entry> {
    let cell = store.peek();
    {
        let mut live = cell.borrow_mut();
        let desired_keys: HashSet<u64> = items.iter().map(|(k, _)| *k).collect();

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

        // Desired order (existing updated, new added), then keep still-exiting items.
        let mut next: Vec<Entry> = Vec::with_capacity(live.len());
        for (k, child) in items {
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
    cell.borrow().clone()
}

// ===========================================================================
// AnimatedList — vertical, items reveal/collapse (size + fade)
// ===========================================================================

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
    let store = create_signal(Rc::new(RefCell::new(Vec::<Entry>::new())));
    let version = create_signal(0u64);
    let entries = reconcile_entries(store, version, &b.items, b.duration);
    let dur = b.duration;
    let kids: Vec<AnyWidget> = entries
        .into_iter()
        .map(|e| {
            let item =
                component_props(render_list_item, ItemProps { child: e.child, exiting: e.exiting, dur });
            keyed(e.key, item).into_widget()
        })
        .collect();
    column(kids).into_widget()
}

// ===========================================================================
// AnimatedGrid — a flowing grid (Wrap), items scale + fade
// ===========================================================================

/// A grid (a flowing [`Wrap`](crate::widgets::Wrap)) whose items scale + fade in on
/// add and out on remove. Items keep their size; give them fixed dimensions so they
/// tile evenly.
#[derive(Clone)]
pub struct AnimatedGrid {
    items: Vec<(u64, AnyWidget)>,
    duration: f64,
    spacing: f64,
}

/// An [`AnimatedGrid`] from `(key, child)` pairs (see [`animated_list`]).
pub fn animated_grid(items: Vec<(u64, AnyWidget)>) -> AnimatedGrid {
    AnimatedGrid { items, duration: 0.25, spacing: 8.0 }
}

impl AnimatedGrid {
    /// Enter/exit duration in seconds (default `0.25`).
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs.max(0.0);
        self
    }
    /// Gap between cells, both axes (default `8`).
    pub fn spacing(mut self, px: f64) -> Self {
        self.spacing = px.max(0.0);
        self
    }
}

impl IntoWidget for AnimatedGrid {
    fn into_widget(self) -> AnyWidget {
        component_props(render_animated_grid, self).into_widget()
    }
}

fn render_animated_grid(b: &AnimatedGrid) -> Element {
    let store = create_signal(Rc::new(RefCell::new(Vec::<Entry>::new())));
    let version = create_signal(0u64);
    let entries = reconcile_entries(store, version, &b.items, b.duration);
    let dur = b.duration;
    let kids: Vec<AnyWidget> = entries
        .into_iter()
        .map(|e| {
            let item =
                component_props(render_grid_item, ItemProps { child: e.child, exiting: e.exiting, dur });
            keyed(e.key, item).into_widget()
        })
        .collect();
    wrap(kids).spacing(b.spacing).run_spacing(b.spacing).into_widget()
}

// ===========================================================================
// item components
// ===========================================================================

/// One animating item: `transition` eases 0→1 on mount, 1→0 when `exiting`.
#[derive(Clone)]
struct ItemProps {
    child: AnyWidget,
    exiting: bool,
    dur: f64,
}

/// List item — reveal/collapse from the top (size) + fade.
fn render_list_item(p: &ItemProps) -> Element {
    let tr = transition(!p.exiting, p.dur);
    let t = tr.t.clamp(0.0, 1.0);
    let faded = Opacity::new(t as f32, p.child.clone());
    let sized = Align::new(Alignment { x: 0.0, y: -1.0 }, faded).height_factor(t);
    clip_rrect(BorderRadius::ZERO, sized).into_widget()
}

/// Grid item — scale + fade (keeps the cell's footprint stable-ish for tiling).
fn render_grid_item(p: &ItemProps) -> Element {
    let tr = transition(!p.exiting, p.dur);
    let t = tr.t.clamp(0.0, 1.0);
    Opacity::new(t as f32, Transform::scale(t.max(0.001), p.child.clone())).into_widget()
}
