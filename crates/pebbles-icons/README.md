# pebbles-icons

The Pebbles icon data model and the bundled Tabler icon set.

An icon is plain data — `IconData`, a `Copy`, `'static`-friendly value
describing drawable geometry in an SVG-style viewbox out of `IconPrim`
primitives (path, line, polyline, polygon, circle, ellipse, rect). Nothing here
renders; `pebbles-render`'s `RenderIcon` consumes an `IconData` and paints it.
That split is what makes the set **pluggable**: the default is Tabler, but any
`IconData` — including a compile-time `const` of your own — drops straight in.

Tabler ships in two styles: **outline** (the default stroke look) at the `tabler`
module root, and **filled** (solid) under `tabler::filled`, so the same glyph is
available either way (`tabler::HEART` / `tabler::filled::HEART`). The `IconKind`
enum names a small set of ergonomic built-ins used across the widget catalog,
each resolving to a Tabler glyph.

```ignore
use pebbles::prelude::*;

icon(IconKind::Check);        // a named built-in (resolves to a Tabler glyph)
icon(tabler::CAMERA);         // any of the ~5100 bundled outline icons
icon(tabler::filled::STAR);   // …or its solid variant
```

Icons sits just above `pebbles-foundation` in the 11-crate framework
(foundation → icons → render → {core → widgets, gpu} → shell → umbrella `pebbles`).

The bundled glyphs are derived from Tabler Icons (MIT); see `NOTICE`.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
