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
/// # Contract — call it at a stable position, one key per position
///
/// Each `selector(key)` call allocates a memo **at the current hook position**, exactly like
/// any other hook. Two rules follow, and nothing enforces them for you:
///
/// - **Call it inside a row component, at a stable position** (typically under a keyed list),
///   with a **fixed `key` for that component instance**. If the same hook position is ever
///   reused for a *different* key (e.g. a reordering list without keys), the memo's
///   `already_linked` early-out keeps the *old* closure and the selector silently keeps
///   answering for the old key.
/// - **Do not call it in a loop at app scope.** Outside a component there is no position to
///   reuse, so every call leaks a fresh, never-disposed signal + memo. It is correct there
///   (the test below relies on that) but only for a bounded, one-shot set of keys.
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
        // The selector's whole premise is that keys equate to themselves; a non-reflexive
        // `PartialEq` (e.g. a NaN-bearing key) would make a row never report itself selected.
        // Debug-only guard — free in release, catches a broken key impl at its source.
        debug_assert!(key == key.clone(), "create_selector key must be reflexive under PartialEq");
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
