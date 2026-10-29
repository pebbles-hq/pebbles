# pebbles-testing

The headless test harness for Pebbles widgets and apps.

It owns the three things every test needs — a `Ui`, a `TextEnv`, and a window
size — and exposes the frame lifecycle as methods, so a test says what it *means*
instead of re-deriving the pipeline:

```ignore
use pebbles_testing::Harness;

#[test]
fn it_counts() {
    let mut h = Harness::new();
    h.mount(counter);
    h.draw();                          // rebuild → layout → paint (settled)
    h.tap(Offset::new(20.0, 20.0));
    h.draw();
    assert!(h.element_count() > 0);
}
```

Input helpers mirror the shell: `click`, `hover`, `drag`, `scroll` / `wheel` (pointer
signal first, then the scroll view), `button_down` / `pointer_move` / `button_up` (any
button, captured moves), `press("Ctrl+Shift+P")` (a US-layout chord through the shell's
full key routing: raw `on_key_event` → editing intent → shortcuts → Tab → activation) and
`key_down` / `key_up` for raw `KeyEvent`s. `frame` / `draw` / `settle` deliver finished
background tasks (`spawn`, `create_resource`) and `wait_for_tasks` waits for in-flight ones,
so a test never pumps `task::pump` by hand.

The lifecycle lives here once instead of being hand-rolled in every test file, so
a change to the frame pipeline is a single edit. `Harness::draw` also runs the
corrective-relayout settle loop (a lazy paint-time measurement can invalidate the
geometry just computed), so tests can't silently assert against unsettled
geometry. The harness is deliberately transparent: `ui` and `env` are public, so
anything not wrapped here is still reachable, and free-function lifecycle helpers
exist for tests that own their own `Ui` / `TextEnv`.

A development/test-only crate in the 11-crate framework — not part of the runtime
layering (foundation → icons → render → {core → widgets, gpu} → shell → umbrella
`pebbles`).

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
