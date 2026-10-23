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
- **Text** — cosmic-text shapes and breaks lines; `TextLayout`,
  caret / selection / hit-testing and keyboard motion are Pebbles' own, and glyphs reach
  the renderer as glyph runs.

Rendering is done by `pebbles-gpu` (wgpu), which consumes this crate's display list. This crate sits above `pebbles-icons` and below `pebbles-core` in
the 11-crate framework
(foundation → icons → render → {core → widgets, gpu} → shell → umbrella `pebbles`).

Bundles OFL fonts; see `NOTICE`.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
