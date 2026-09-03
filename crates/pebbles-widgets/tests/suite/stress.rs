//! E1 regression tripwire (`#[ignore]`d — run manually):
//!
//! ```text
//! cargo test -p pebbles-widgets --test stress -- --ignored
//! ```
//!
//! 1 000 components, each subscribed to its own signal. Writing one signal must
//! schedule only that component (isolation); writing all 1 000 then draining a single
//! frame must stay linear (the O(1) membership set replacing the old linear `contains`)
//! and complete well inside a generous wall-clock bound.

use std::cell::RefCell;
use std::time::Instant;

use pebbles_core::{
    AnyWidget, IntoWidget, Signal, Ui, component, component_props, create_signal,
};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{SizedBox, View, column};

const N: usize = 1000;

thread_local! {
    static SIGNALS: RefCell<Vec<Signal<u32>>> = const { RefCell::new(Vec::new()) };
    static RENDERS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct ChildProps {
    index: usize,
}

fn child(p: &ChildProps) -> SizedBox {
    // Subscribe to this component's own signal, and count the render.
    let _ = SIGNALS.with(|s| s.borrow()[p.index].get());
    RENDERS.with(|r| r.borrow_mut()[p.index] += 1);
    SizedBox::new(Some(1.0), Some(1.0), None)
}

fn root() -> impl IntoWidget {
    let kids: Vec<AnyWidget> =
        (0..N).map(|i| component_props(child, ChildProps { index: i }).into_widget()).collect();
    column(kids)
}

#[test]
#[ignore = "stress tripwire — run manually with --ignored"]
fn thousand_components_schedule_and_drain_in_isolation() {
    SIGNALS.with(|s| {
        let mut s = s.borrow_mut();
        s.clear();
        for _ in 0..N {
            s.push(create_signal(0u32));
        }
    });
    RENDERS.with(|r| *r.borrow_mut() = vec![0u32; N]);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(50.0, 50.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, window);

    // Every component rendered exactly once on mount.
    RENDERS.with(|r| assert!(r.borrow().iter().all(|&c| c == 1), "each mounted once"));

    // Writing ONE signal schedules only its component.
    SIGNALS.with(|s| s.borrow()[42].set(1));
    ui.rebuild_if_dirty();
    RENDERS.with(|r| {
        let r = r.borrow();
        assert_eq!(r[42], 2, "the written component re-rendered");
        assert_eq!(r.iter().map(|&c| c as usize).sum::<usize>(), N + 1, "no other component re-rendered");
    });

    // Writing ALL signals then draining one frame stays linear + fast.
    let start = Instant::now();
    SIGNALS.with(|s| {
        for sig in s.borrow().iter() {
            sig.update(|v| *v += 1);
        }
    });
    ui.rebuild_if_dirty();
    let elapsed = start.elapsed();
    RENDERS.with(|r| {
        let r = r.borrow();
        // 42 rendered 3× (mount + single write + all-write); everyone else 2×.
        assert_eq!(r[42], 3);
        assert!(r.iter().enumerate().all(|(i, &c)| c == if i == 42 { 3 } else { 2 }), "each re-rendered once");
    });
    assert!(elapsed.as_secs() < 5, "schedule+drain of {N} components took {elapsed:?} (regression?)");
}
