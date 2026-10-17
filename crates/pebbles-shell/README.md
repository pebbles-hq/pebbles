# pebbles-shell

The desktop shell of the Pebbles GUI framework — the only crate that touches the
GPU and the OS window.

It wires [winit] windowing to a [wgpu] surface and the render backend (Vello Hybrid by default, the first-party wgpu renderer opt-in), and drives the `Ui` engine each frame.
The public surface is deliberately tiny — build a widget tree and hand it to
`App`:

```ignore
use pebbles_shell::App;
App::new(my_root()).title("Hello").size(480, 320).run()?;
```

The render backend is chosen by a feature that forwards to `pebbles-render`
(`vello-hybrid` default, `wgpu` opt-in); the GPU host for each lives behind a
backend-agnostic frame loop. Also handles global hotkeys, offscreen
widget→PNG/RGBA capture, and the wasm entry-point plumbing (the OS-side
glue behind `#[pebbles::main]`).

Shell sits at the top of the layered stack in the 10-crate framework
(foundation → icons → render → core → widgets → shell → umbrella `pebbles`).
Most app authors reach `App` through the `pebbles` umbrella prelude.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
