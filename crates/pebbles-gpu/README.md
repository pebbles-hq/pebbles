# pebbles-gpu

The GPU renderer of the Pebbles GUI framework, written directly on [wgpu].

- **Prepare (CPU, unit-testable without a GPU):** turns the display list that
  `pebbles-render` records into an ordered draw plan — path tessellation, stroke
  expansion, clip/layer planning, glyph/image/shadow placement.
- **Execute (GPU):** device and window surfaces, pipelines, WGSL shaders, glyph and
  image atlases, and an offscreen path that renders a scene to RGBA without a window.

Apps never depend on this crate directly: `pebbles-shell` drives it, and offscreen
rendering is exposed as `pebbles::shell::capture`. No semver guarantees before 1.0.

It sits above `pebbles-render` and below `pebbles-shell` in the 11-crate framework
(foundation → icons → render → {core → widgets, gpu} → shell → umbrella `pebbles`).

Part of [Pebbles](https://github.com/pebbles-hq/pebbles). Licensed under Apache-2.0.

## Layout

| Module | Responsibility |
|---|---|
| `prepare/` | Display list → `FramePlan`-shaped CPU geometry (tessellation, strokes, clips, glyph/image/shadow requests). No GPU device. |
| `execute/` | wgpu pipelines, resources (atlas, image cache), the ordered encode loop. |
| `raster/` | Glyph rasterization with swash from the op's font bytes. |
| `context.rs` / `surface.rs` | Instance, adapter, device pool and surface configuration. |
| `offscreen.rs` / `readback.rs` | The public headless `OffscreenRenderer`. |
| `renderer.rs` | One `Renderer` per device: pipelines, caches, atlases; `render`. |
| `shaders/` | WGSL sources. |

See `documentations/wgpu-migration/06-rust-architecture.md` for the full target design.

[wgpu]: https://wgpu.rs
