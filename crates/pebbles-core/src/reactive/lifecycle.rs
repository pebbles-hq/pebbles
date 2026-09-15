//! Mount lifecycle — SolidJS `onMount`.

use std::cell::Cell;

use super::{create_effect, untrack};

/// Run `f` **once**, when the component first mounts (SolidJS `onMount`). It runs
/// untracked — reads inside never subscribe — so it never re-runs. Call it at the top
/// of a component like any hook; at app scope it simply runs once.
///
/// Sugar for "a `create_effect` that reads nothing and runs a single time"; reach for
/// it when the intent is a one-shot setup (focus something, kick a fetch, log) rather
/// than a reactive effect.
pub fn on_mount(f: impl FnOnce() + 'static) {
    let cell = Cell::new(Some(f));
    create_effect(move || {
        if let Some(f) = cell.take() {
            untrack(f);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::on_mount;
    use crate::reactive::{create_signal, flush_effects};

    #[test]
    fn on_mount_runs_exactly_once() {
        let count = create_signal(0);
        on_mount(move || count.update(|c| *c += 1));
        assert_eq!(count.peek(), 1, "runs immediately on mount");
        flush_effects();
        flush_effects();
        assert_eq!(count.peek(), 1, "and never again");
    }
}
