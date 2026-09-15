//! Write batching — SolidJS `batch`.

/// Group a burst of writes (SolidJS `batch`).
///
/// **Note the architecture:** in Pebbles, effect re-runs and component re-renders are
/// already deferred to the next frame flush and deduped through membership sets — so
/// writes are *coalesced by default*. Setting N signals in one event handler re-runs
/// each dependent effect/component **once**, not N times, with or without this call.
///
/// `batch` therefore runs `f` and returns its result directly. It exists to (a) express
/// intent ("these writes go together") for readers coming from Solid, and (b) keep the
/// grouping guarantee stable if a synchronous-flush path is ever introduced. The
/// batching property itself is covered by a runtime test, not by this wrapper.
pub fn batch<T>(f: impl FnOnce() -> T) -> T {
    f()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::batch;
    use crate::reactive::{create_effect, create_signal, flush_effects};

    #[test]
    fn batched_writes_coalesce_to_one_run() {
        // The invariant `batch` documents: writes are frame-coalesced, so a burst
        // re-runs each dependent effect once, not once per write.
        let runs = Rc::new(Cell::new(0));
        let a = create_signal(0);
        let b = create_signal(0);
        {
            let runs = runs.clone();
            create_effect(move || {
                let _ = a.get();
                let _ = b.get();
                runs.set(runs.get() + 1);
            });
        }
        flush_effects();
        let base = runs.get();
        batch(|| {
            a.set(1);
            b.set(1);
            a.set(2);
        });
        flush_effects();
        assert_eq!(runs.get() - base, 1, "three writes across two signals re-run the effect once");
    }
}
