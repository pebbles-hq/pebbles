# pebbles-core

The runtime layer of the Pebbles GUI framework — the engine every widget is
built on, with no catalog of its own.

It owns three things:

- **Reactivity** — the Solid-style `Signal` / `create_memo` / `create_effect` /
  `create_store` primitives. State is a signal you read and write directly; reads
  inside a component subscribe, and writes re-render only the readers.
- **The widget contract** — `Widget`, the `component` / `component_props`
  function-component adapter (the primary way to author UI), the `RenderWidget` /
  `ParentDataWidget` render layer, and the `IntoWidget` / `AnyWidget` boxing
  layer.
- **The reconciling engine** — `Ui`, the element arena keyed by `ElementId` that
  turns a widget tree into a `pebbles-render` tree each frame, plus the focus
  system and the `Callback` handle.

Also included: animation/motion, IPC channels, the router, async task spawning,
and clipboard access.

The widget catalog lives one layer up in `pebbles-widgets`; the GPU/windowing
shell lives in `pebbles-shell`. This crate depends only on `pebbles-foundation`
and `pebbles-render`, and sits in the middle of the 11-crate framework
(foundation → icons → render → {core → widgets, gpu} → shell → umbrella `pebbles`).

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
