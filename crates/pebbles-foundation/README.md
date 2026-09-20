# pebbles-foundation

The bottom layer of the Pebbles GUI framework — the vocabulary every other layer
speaks.

It has no knowledge of widgets, render objects, or the GPU. It provides:

- **geometry** — `Offset`, `Size`, `Rect`, `EdgeInsets`, `Alignment`, plus the
  kurbo re-exports (`Point`, `Affine`, `Vec2`).
- **layout enums** — `Axis`, `MainAxisAlignment`, `CrossAxisAlignment`, and the
  rest of the Flutter layout vocabulary.
- **color** — the `peniko::Color` re-export.
- **palette** — the full Tailwind/shadcn color scale (every family, shades
  50–950).

Everything here is `Copy`, allocation-free, and cheap to pass by value. It also
re-exports `kurbo` (2D geometry) and `peniko` (brushes, gradients, colors) so
downstream code can reach the full APIs.

Foundation sits at the base of the 10-crate framework
(foundation → icons → render → core → widgets → shell → umbrella `pebbles`).
Most app authors depend on the `pebbles` umbrella instead of this crate directly.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
