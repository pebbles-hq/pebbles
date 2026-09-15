//! Selection optimization — SolidJS `createSelector`.

use std::rc::Rc;

use super::create_memo;

/// A selection helper (SolidJS `createSelector`): given a `source` that produces the
/// currently-selected key, returns `selector(key) -> bool` that a row calls to learn
/// whether **it** is the selected one.
///
/// The win over a bare `source() == key` is the memo equality-cut: each key's
/// membership is its own lazy memo, so changing the selection only re-renders the row
/// that **was** selected and the one that now **is** — every other row recomputes a
/// cheap comparison but its `bool` is unchanged, so its readers don't re-render.
///
/// Call `selector(key)` at a stable position inside the **row component** (it creates a
/// position-stable memo, like any hook), typically under a keyed list:
///
/// ```ignore
/// let is_selected = create_selector(move || selected.get());
/// // inside each row component:
/// let active = is_selected(row.id);
/// ```
pub fn create_selector<K>(source: impl Fn() -> K + 'static) -> impl Fn(K) -> bool + Clone
where
    K: PartialEq + Clone + 'static,
{
    let source = Rc::new(source);
    move |key: K| {
        let source = source.clone();
        create_memo(move || source() == key).get()
    }
}

#[cfg(test)]
mod tests {
    use super::create_selector;
    use crate::reactive::{create_signal, flush_effects};

    #[test]
    fn selector_reports_membership() {
        // Correctness at app scope. (The "only wake the two affected rows"
        // optimization is a property of the per-key memo's equality-cut and holds
        // when `selector` is called at a stable position inside a row component.)
        let selected = create_signal(1i32);
        let is_selected = create_selector(move || selected.get());
        assert!(is_selected(1));
        assert!(!is_selected(0));
        selected.set(2);
        flush_effects();
        assert!(is_selected(2));
        assert!(!is_selected(1));
    }
}
