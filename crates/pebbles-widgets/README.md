# pebbles-widgets

The catalog layer of the Pebbles GUI framework — the Flutter-style widgets and
shadcn-style components developers actually compose.

The runtime that builds and reconciles them lives in `pebbles-core`
(re-exported through the `pebbles` umbrella's prelude).

- **widgets** — the Flutter-style primitives, each backed by a render object:
  `Text`, `Row`, `Column`, `Container`, `GestureDetector`, `Padding`,
  `Align` / `center`, `SizedBox`, `Stack`, and the long tail of layout widgets.
- **components** — the shadcn-style catalog by role (input / display / layout /
  navigation): buttons, toggles, tabs, tables, navigation chrome, surfaces.
- **design** — the look: `Theme` / `Colors` tokens, the general `Style` system,
  modifiers, fonts, and text direction.
- **services** — the ambient layers above the tree: overlays, dialogs, sheets,
  toasts, and the global menu.
- **platform** — OS surfaces: secondary windows and the native menu bar.

`design`, `services`, and `platform` group for navigation only — every module
inside them is re-exported flat (`pebbles_widgets::theme`,
`pebbles_widgets::overlay`, …).

Widgets sits above `pebbles-core` in the 11-crate framework
(foundation → icons → render → {core → widgets, gpu} → shell → umbrella `pebbles`).

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
