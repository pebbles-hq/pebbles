//! Stable unique ids — SolidJS `createUniqueId`.

use std::cell::Cell;

use super::create_signal_with;

thread_local! {
    static COUNTER: Cell<u64> = const { Cell::new(0) };
}

fn next() -> u64 {
    COUNTER.with(|c| {
        let n = c.get() + 1;
        c.set(n);
        n
    })
}

/// A process-unique id string, **stable across re-renders** (SolidJS `createUniqueId`).
/// Use it to tie an input to its label (`labelledby`/`aria-*`) or as a stable key.
///
/// Stability comes from `create_signal_with` being position-stable AND lazy: the id string
/// is generated on the first render only and reused afterwards. The counter advances exactly
/// once per id (no per-render churn). Call it at a stable position like any hook; at app
/// scope each call is simply globally unique.
pub fn create_unique_id() -> String {
    create_signal_with(|| format!("pb-{}", next())).peek()
}

#[cfg(test)]
mod tests {
    use super::create_unique_id;

    #[test]
    fn ids_are_unique_and_prefixed() {
        let a = create_unique_id();
        let b = create_unique_id();
        assert_ne!(a, b);
        assert!(a.starts_with("pb-"));
    }
}
