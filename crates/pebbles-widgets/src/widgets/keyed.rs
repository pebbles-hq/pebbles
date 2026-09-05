//! [`Keyed`] — attach a reconciliation [`Key`] to any widget (Flutter's `KeyedSubtree`).
//!
//! Children in a list are matched by **position** by default, so inserting or
//! removing one shifts every element after it (and rebuilds them in place). Wrap a
//! child in [`keyed`]`(k, child)` and the reconciler matches it by `k` **across
//! positions** instead: insert / remove / reorder keeps each child's element — and
//! all its state, focus, scroll offset and animations — intact.
//!
//! This is the prerequisite for `AnimatedList`/`AnimatedGrid` and reorderable lists.
//! `Keyed` is transparent: layout and paint pass straight through to the child.

use std::any::Any;

use pebbles_render::{BoxConstraints, RenderConstrainedBox, RenderObject};

use pebbles_core::Key;
use pebbles_core::widget::{AnyWidget, RenderWidget, Widget};

/// A transparent wrapper that gives its child a reconciliation key.
#[derive(Clone)]
pub struct Keyed {
    key: Key,
    child: Option<AnyWidget>,
}

/// Give `child` the reconciliation key `key` (accepts `&str`, `String`, or `u64`),
/// so the reconciler tracks it by identity across list changes.
pub fn keyed(key: impl Into<Key>, child: impl pebbles_core::IntoWidget) -> Keyed {
    Keyed { key: key.into(), child: Some(child.into_widget()) }
}

impl Widget for Keyed {
    fn debug_name(&self) -> &'static str {
        "Keyed"
    }
    fn key(&self) -> Option<Key> {
        Some(self.key.clone())
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> AnyWidget {
        Box::new(self.clone())
    }
    fn as_render(&self) -> Option<&dyn RenderWidget> {
        Some(self)
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderWidget> {
        Some(self)
    }
}

impl RenderWidget for Keyed {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        // A no-op additional constraint → a pure passthrough: the child gets the
        // incoming constraints and this box takes the child's size.
        Box::new(RenderConstrainedBox::new(BoxConstraints::UNBOUNDED))
    }
    fn update_render_object(&self, _object: &mut dyn RenderObject) {}
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
