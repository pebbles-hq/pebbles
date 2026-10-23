# pebbles-shell

The desktop shell of the Pebbles GUI framework — the only crate that touches the
GPU and the OS window.

It wires [winit] windowing to a [wgpu] surface and the `pebbles-gpu` renderer, and drives the `Ui` engine each frame.
The public surface is deliberately tiny — build a widget tree and hand it to
`App`:

```ignore
use pebbles_shell::App;
App::new(my_root()).title("Hello").size(480, 320).run()?;
```

It also handles device-loss recovery, global hotkeys, offscreen
widget→PNG/RGBA capture, and the wasm entry-point plumbing (the OS-side
glue behind `#[pebbles::main]`).

Shell sits at the top of the layered stack in the 11-crate framework
(foundation → icons → render → {core → widgets, gpu} → shell → umbrella `pebbles`).
Most app authors reach `App` through the `pebbles` umbrella prelude.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
