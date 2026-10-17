# pebbles-render

The render layer of the Pebbles GUI framework — the tree of `RenderObject`s that
actually performs layout and painting.

It sits between the widget layer (which builds and reconciles it) and the GPU
shell (which rasterizes the backend-neutral display list it paints into).

- **Box layout protocol** — layout follows Flutter's model exactly: a parent
  passes each child `BoxConstraints`, the child returns a `Size` that satisfies
  them, and the parent positions the child.
- **Arena trees** — all render objects live in a single `RenderTree` arena keyed
  by generational `RenderId`s, traversed through `LayoutCx` / `PaintCx`.
- **Decoration & primitives** — `BoxDecoration`, borders, shadows, gradients,
  images, text, scrolling, and the full `Render*` object set (flex, stack,
  padding, clip, transform, paragraph, text field, and more).

The render backend is chosen by a feature (Vello Hybrid by default; the first-party wgpu renderer is opt-in during the migration). This crate sits above `pebbles-icons` and below `pebbles-core` in
the 10-crate framework
(foundation → icons → render → core → widgets → shell → umbrella `pebbles`).

Bundles OFL fonts; see `NOTICE`.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
