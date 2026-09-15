//! `for_each` — SolidJS `<For>`: map a list to keyed children.

use pebbles_core::Key;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// Map `items` to widgets, each tagged with a reconciliation [`Key`] (SolidJS
/// `<For>`). Drop the result straight into a `column`/`row`/list:
///
/// ```ignore
/// column(for_each(todos.get(), |t| t.id, |t| todo_row(t)))
/// ```
///
/// Because every child is [`keyed`](crate::widgets::keyed), inserting, removing, or
/// reordering the list preserves the surviving children's element state (focus, scroll
/// offset, animations) instead of rebuilding by position. Reactivity comes from the
/// enclosing component reading the list signal — when it changes, the component
/// re-renders and the keyed diff moves elements to match.
pub fn for_each<T, K, W>(
    items: impl IntoIterator<Item = T>,
    key: impl Fn(&T) -> K,
    view: impl Fn(&T) -> W,
) -> Vec<AnyWidget>
where
    K: Into<Key>,
    W: IntoWidget,
{
    items.into_iter().map(|item| crate::widgets::keyed(key(&item), view(&item)).into_widget()).collect()
}

#[cfg(test)]
mod tests {
    use super::for_each;
    use crate::widgets::text;

    #[test]
    fn maps_each_item_to_a_keyed_child() {
        let items = vec![1, 2, 3];
        let children = for_each(items, |n| *n as u64, |n| text(format!("{n}")));
        assert_eq!(children.len(), 3);
    }
}
