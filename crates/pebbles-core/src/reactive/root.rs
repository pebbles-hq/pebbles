//! Reactive ownership scope — SolidJS `createRoot`.

use super::{RootId, dispose_signal, with_rt};

/// A handle that disposes a [`create_root`] scope: every signal, memo, effect, and
/// cleanup made inside it. `Copy`, so store it and dispose later.
#[derive(Clone, Copy)]
pub struct RootDisposer {
    id: RootId,
}

impl RootDisposer {
    /// Dispose the scope: run its cleanups, then free its signals (and the memos they
    /// back) and effects. Idempotent — a second call is a no-op.
    pub fn dispose(self) {
        // Take the scope out first so cleanups run OUTSIDE the runtime borrow (they may
        // touch other signals), matching `dispose_component`.
        let scope = with_rt(|rt| rt.roots.remove(self.id));
        let Some(scope) = scope else { return };
        for cleanup in scope.cleanups {
            cleanup();
        }
        with_rt(|rt| {
            for sid in scope.signals {
                dispose_signal(rt, sid);
            }
            for eid in scope.effects {
                // Removing the slot is enough: a stale id lingering in some signal's
                // `effect_subs` self-cleans on the next write (see `dispose_component`).
                rt.effects.remove(eid);
            }
        });
    }
}

/// Create a reactive **ownership scope** (SolidJS `createRoot`): run `f`, giving it a
/// [`RootDisposer`], and return whatever `f` returns. Every signal / memo / effect /
/// cleanup created while `f` runs is owned by the scope and freed together when the
/// disposer is called.
///
/// Use it for reactive graphs that live **outside** a component and need an explicit
/// lifetime — a detached subscription, a computation you spin up and later tear down,
/// a test. Inside the scope the current component owner is detached (so signals are
/// scope-owned, not component-local) and reads are untracked (so a stray read can't
/// subscribe an enclosing component).
///
/// ```ignore
/// let (value, dispose) = create_root(|dispose| {
///     let n = create_signal(0);
///     create_effect(move || log(n.get()));
///     (n, dispose)
/// });
/// // …use `value`…
/// dispose.dispose(); // frees the signal + effect
/// ```
///
/// For a single ownerless signal, [`create_root_signal`](super::create_root_signal) +
/// [`dispose_root_signal`](super::dispose_root_signal) are lighter; reach for
/// `create_root` when a whole graph shares one lifetime.
pub fn create_root<T>(f: impl FnOnce(RootDisposer) -> T) -> T {
    let (id, prev_owner, prev_observer) = with_rt(|rt| {
        let id = rt.roots.insert(super::RootScope::default());
        rt.active_roots.push(id);
        // Detach: nodes created inside are scope-owned (owner cleared) and reads don't
        // subscribe an enclosing component (observer cleared).
        (id, rt.owner.take(), rt.observer.take())
    });
    let out = f(RootDisposer { id });
    with_rt(|rt| {
        rt.owner = prev_owner;
        rt.observer = prev_observer;
        // Stop collecting; the scope stays in `roots` (its nodes live) until disposed.
        rt.active_roots.retain(|&r| r != id);
    });
    out
}

#[cfg(test)]
mod tests {
    use super::create_root;
    use crate::reactive::{create_effect, create_memo, create_signal};

    #[test]
    fn dispose_frees_everything_made_inside() {
        let (sig, memo, dispose) = create_root(|dispose| {
            let s = create_signal(1);
            let m = create_memo(move || s.get() + 1);
            create_effect(move || {
                let _ = m.get();
            });
            (s, m, dispose)
        });
        assert!(sig.alive() && memo.alive(), "nodes are live inside the scope");
        dispose.dispose();
        assert!(!sig.alive(), "root dispose frees the signal");
        assert!(!memo.alive(), "and the memo it backs");
    }

    #[test]
    fn dispose_runs_scope_cleanups_and_is_idempotent() {
        use std::cell::Cell;
        use std::rc::Rc;

        let ran = Rc::new(Cell::new(0));
        let dispose = create_root(|dispose| {
            let r = ran.clone();
            crate::reactive::create_cleanup(move || r.set(r.get() + 1));
            dispose
        });
        assert_eq!(ran.get(), 0, "cleanup waits for dispose");
        dispose.dispose();
        assert_eq!(ran.get(), 1, "cleanup runs on dispose");
        dispose.dispose();
        assert_eq!(ran.get(), 1, "second dispose is a no-op");
    }
}
