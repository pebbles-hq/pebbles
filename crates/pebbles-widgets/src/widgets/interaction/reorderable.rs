//! [`ReorderableListView`] — a vertical list whose rows can be dragged to reorder
//! (Flutter's `ReorderableListView`).
//!
//! Each row is a keyed child, so the reconciler keeps every row's element (and its
//! state) as the order changes. Press and drag a row up or down; the other rows
//! slide to open a gap, and on release `on_reorder(old_index, new_index)` fires —
//! reorder your backing list there and pass the new order back in.
//!
//! Rows share a fixed [`item_extent`](ReorderableListView::item_extent) (default
//! `56`), which the drag uses to compute the drop index.
//!
//! ```ignore
//! reorderable_list_view(rows, move |from, to| items.update(|v| { let r = v.remove(from); v.insert(to, r); }))
//!     .item_extent(48.0)
//! ```

use std::rc::Rc;

use crate::widgets::{GestureDetector, SizedBox, Transform, column, keyed};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Element, action, action_event, component_props, create_signal};

/// A drag-to-reorder vertical list. Built by [`reorderable_list_view`].
#[derive(Clone)]
pub struct ReorderableListView {
    items: Vec<(u64, AnyWidget)>,
    on_reorder: Rc<dyn Fn(usize, usize)>,
    item_extent: f64,
}

/// A reorderable list from `(key, row)` pairs. `on_reorder(from, to)` fires on drop
/// with the row's old and new indices (only when they differ) — apply the move to
/// your backing list and pass the reordered items back on the next build.
pub fn reorderable_list_view(
    items: Vec<(u64, AnyWidget)>,
    on_reorder: impl Fn(usize, usize) + 'static,
) -> ReorderableListView {
    ReorderableListView { items, on_reorder: Rc::new(on_reorder), item_extent: 56.0 }
}

impl ReorderableListView {
    /// The fixed height of every row, in logical pixels (default `56`). The drag uses
    /// it to compute the drop index, so rows should actually be this tall.
    pub fn item_extent(mut self, px: f64) -> Self {
        self.item_extent = px.max(1.0);
        self
    }
}

impl IntoWidget for ReorderableListView {
    fn into_widget(self) -> AnyWidget {
        component_props(render_reorderable, self).into_widget()
    }
}

fn render_reorderable(v: &ReorderableListView) -> Element {
    // Active drag: (source index, accumulated dy in px). `None` when idle.
    let drag = create_signal::<Option<(usize, f64)>>(None);
    let unit = v.item_extent;
    let n = v.items.len();

    let active = drag.get();
    let target = active.map(|(from, dy)| clamp_index(from as i64 + (dy / unit).round() as i64, n));

    let rows: Vec<AnyWidget> = v
        .items
        .iter()
        .enumerate()
        .map(|(i, (key, child))| {
            // Vertical offset: the dragged row follows the pointer; others slide to
            // open the gap between the source and current target slot.
            let dy = match (active, target) {
                (Some((from, drag_dy)), _) if i == from => drag_dy,
                (Some((from, _)), Some(to)) if from < to && i > from && i <= to => -unit,
                (Some((from, _)), Some(to)) if from > to && i >= to && i < from => unit,
                _ => 0.0,
            };

            let on_reorder = v.on_reorder.clone();
            let gd = GestureDetector::new(SizedBox::new(None, Some(unit), Some(child.clone())))
                .on_pan_start(action(move || drag.set(Some((i, 0.0)))))
                .on_pan_update(action_event(move |e: pebbles_render::PointerEvent| {
                    drag.update(|d| {
                        if let Some((_, dy)) = d {
                            *dy += e.delta.y;
                        }
                    });
                }))
                .on_pan_end(action(move || {
                    if let Some((from, dy)) = drag.peek() {
                        let to = clamp_index(from as i64 + (dy / unit).round() as i64, n);
                        if to != from {
                            on_reorder(from, to);
                        }
                    }
                    drag.set(None);
                }));

            keyed(*key, Transform::translate(0.0, dy, gd)).into_widget()
        })
        .collect();

    column(rows).into_widget()
}

/// Clamp a (possibly negative) target index into `0..n`.
fn clamp_index(i: i64, n: usize) -> usize {
    i.clamp(0, n.saturating_sub(1) as i64) as usize
}
