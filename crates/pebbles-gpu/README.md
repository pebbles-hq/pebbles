# pebbles-gpu

The first-party wgpu 2D renderer for Pebbles: display-list **preparation** (CPU, no device)
and **execution** (wgpu) plus a public offscreen API.

**Implementation crate** — no semver guarantees before 1.0 beyond what `pebbles::shell`
re-exports. Apps use `pebbles::shell`, not this crate directly.

## Layout

| Module | Responsibility |
|---|---|
| `prepare/` | Display list → `FramePlan`-shaped CPU geometry (tessellation, strokes, clips, glyph/image/shadow requests). No GPU device. |
| `execute/` | wgpu pipelines, resources (atlas, image cache), the ordered encode loop. |
| `raster/` | Glyph rasterization (swash / cosmic-text during the transition). |
| `context.rs` / `surface.rs` | Instance, adapter, device pool and surface configuration. |
| `renderer.rs` | One `Renderer` per device: pipelines, caches, atlases; `render`. |
| `shaders/` | WGSL sources. |

See `documentations/wgpu-migration/06-rust-architecture.md` for the full target design.
