//! The background-task bridge: `spawn` runs work off-thread and delivers the result
//! on the UI thread via `pump`; `create_resource` exposes that as a `Loading → Ready`
//! signal. Both are driven here by polling `pump` in a loop (what the shell does each
//! frame), without a window or GPU.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use pebbles_core::task::{Resource, create_resource, pump, spawn};

/// Poll `pump` (as the shell would, once per frame) until it reports no work left, or
/// a deadline elapses. Returns whether it drained before the deadline.
fn drain(max: Duration) -> bool {
    let start = Instant::now();
    loop {
        if !pump() {
            return true;
        }
        if start.elapsed() > max {
            return false;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn spawn_delivers_result_on_the_ui_thread() {
    let got = Rc::new(Cell::new(0));
    let sink = got.clone();
    // Rc is !Send: it can only be touched on this (UI) thread, proving `on_done` runs
    // here — not on the worker thread.
    spawn(|| 21 * 2, move |v| sink.set(v));

    assert_eq!(got.get(), 0, "nothing delivered before pump");
    assert!(drain(Duration::from_secs(5)), "the task drained");
    assert_eq!(got.get(), 42, "on_done ran on the UI thread with the worker's result");
}

#[test]
fn spawn_with_no_pending_pump_is_false() {
    assert!(!pump(), "pump with an empty queue reports no pending work");
}

#[test]
fn create_resource_transitions_loading_to_ready() {
    let res = create_resource(|| {
        std::thread::sleep(Duration::from_millis(5));
        "hello".to_string()
    });
    // create_resource kicks the fetch off via an effect that runs immediately.
    assert_eq!(res.peek(), Resource::Loading, "starts Loading");
    assert!(res.peek().is_loading());

    assert!(drain(Duration::from_secs(5)), "the fetch drained");
    assert_eq!(res.peek(), Resource::Ready("hello".to_string()), "flips to Ready");
    assert_eq!(res.peek().value().map(String::as_str), Some("hello"));
}

#[test]
fn create_resource_carries_a_result_for_fallible_work() {
    // Fallible work is just `Resource<Result<T, E>>` — no special error variant needed.
    let res = create_resource(|| -> Result<i32, String> { Err("boom".to_string()) });
    assert!(drain(Duration::from_secs(5)));
    match res.peek() {
        Resource::Ready(Err(e)) => assert_eq!(e, "boom"),
        other => panic!("expected Ready(Err), got {other:?}"),
    }
}
