# pebbles

A Flutter-style, desktop-first GUI framework with its own wgpu renderer.

This is the front-door crate app authors depend on. Pebbles pairs Flutter's
widget model for building UI (the three-tree Widget → Element → RenderObject
architecture and box layout protocol) with Solid's signals for state: a
`create_signal` you read and write directly, where reads subscribe and writes
re-render only the readers — no `StatefulWidget`, no `setState`, no
`Rc<RefCell>` dance. A component is a plain function, local state is a signal, a
handler is a closure. Global state uses the same `create_signal` primitive at
app scope.

The umbrella re-exports the layered crates (`foundation`, `render`, `core`,
`widgets`, `shell`) and the `#[pebbles::main]` attribute, and offers a `prelude`
with everything you need in one glob import.

```ignore
use pebbles::prelude::*;

fn counter() -> Element {
    let count = create_signal(0);
    center(column(children![
        text(format!("{}", count.get())).size(48.0),
        button("+").on_pressed(move || count.update(|c| *c += 1)),
    ]))
    .into_widget()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(counter)).title("Counter").run()
}
```

See the [repository](https://github.com/pebbles-hq/pebbles) for the full guide.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
