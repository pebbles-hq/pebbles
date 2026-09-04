# Architecture

This document describes how Pebbles is put together: the crate layering, the three
trees, the reactive model, and where each kind of change belongs. Read this before
your first non-trivial change. (What the framework *offers* is the [README](README.md);
the widget catalog is [WIDGETS.md](WIDGETS.md).)

## The one-paragraph version

Pebbles pairs **Flutter's widget model** (Widget → Element → RenderObject, constraints
down / sizes up) with **SolidJS-style signals** for state, painted by **Vello** on wgpu
and shaped by **Parley**. A component is a plain function; reading a signal inside it
subscribes it; writing a signal re-renders only the components that read it. The shell
crate owns the winit event loop and feeds input down / draws scenes up.

## Crate layering

Strictly bottom-up; a crate never depends on one above it:

```
pebbles-foundation   geometry, color, layout enums (Rect, Offset, Size, EdgeInsets, …)
pebbles-icons        generated Lucide icon data (see scripts/gen-lucide.mjs)
pebbles-render       RenderObject trait + the built-in render objects, text (Parley),
                     vello Scene painting, hit-test tree
pebbles-core         reactivity (signals/memos/effects/stores), the Element tree +
                     reconciler, components, focus, keyboard, animation, bounds
pebbles-widgets      the widget catalog: Flutter-style layout widgets + the
                     shadcn-style component set, theming, overlays, windows API
pebbles-shell        the app runner: winit event loop, wgpu surface, vello Renderer,
                     AccessKit a11y, native menus / global hotkeys (feature-gated)
pebbles-macros       the #[component] proc-macro
pebbles              the umbrella crate: re-exports + `pebbles::prelude`
```

`examples/*` (counter, gallery) are workspace members that consume only the umbrella
crate — they are the consumer-facing API check.

## The three trees

Exactly Flutter's model:

1. **Widget** — immutable description, rebuilt freely (`IntoWidget` / `RenderWidget`).
2. **Element** — the retained tree (`pebbles-core/src/element.rs`), reconciled against
   new widgets; owns component state anchoring (hooks are position-based).
3. **RenderObject** — layout + paint (`pebbles-render`), one per render widget;
   receives `BoxConstraints` down, returns `Size` up, parent sets the child offset.

Components can't read the render tree during render — geometry that widgets need
(tooltips, inspector) flows through `use_bounds()` (`pebbles-core/src/bounds.rs`),
published by the shell one frame behind.

## Reactivity

SolidJS semantics in `pebbles-core/src/reactive.rs`:

- `create_signal` / `create_memo` / `create_effect` / `create_store` are **hooks** —
  position-based per component instance; never call them conditionally.
- `create_root_signal` is the non-hook escape hatch for registry-keyed state.
- A write schedules only the subscribing components (deduped), not the whole tree.
  It also allocates nothing on the hot path: the value box is reused in place and
  the scheduler drains subscribers through recycled scratch buffers.
- **Signals are eager, memos are lazy.** A write flips flags — it schedules the
  signal's component/effect readers and marks its memo readers stale
  (`Clean`/`Check`/`Dirty`), computing nothing. A memo recomputes only when
  something rendered pulls it (`get`/`peek`), and cuts the re-render cascade when
  its value is unchanged (`create_memo_with` takes a custom equality policy). A
  memo nothing reads this frame never recomputes. Demanded memos are settled
  before components render, so reads are glitch-free (a leaf never sees a
  half-updated derived graph).
- `on(deps, f)` / `on_defer(deps, f)` are explicit-dependency effects (track
  `deps`, run the body untracked); `Store::select_memo` is a field-scoped lazy
  selector — a write to an untouched field never wakes it.
- Reactive-runtime work is measurable via `pebbles-core::reactive_stats`
  (`PEBBLES_REACTIVE_STATS=1`): writes, notifies, memo recomputes, effect runs,
  and hot-path allocations.

## The shell / engine boundary

`pebbles-shell/src/app.rs` is the public `App` builder; the winit
`ApplicationHandler` engine lives in its child module `app/runner/` — `mod.rs`
(state + the handler), `input.rs` (event → intent translation), `render.rs` (the
frame pipeline, bounds publishing, F2 inspector outline), `windows.rs` (secondary
OS windows). AccessKit lives in `a11y.rs`.

Optional integrations are **default-off cargo features**, forwarded through the
umbrella crate: `native-menus`, `global-hotkeys` (shell), `image-view`
(`ImageView` + `image_from_*`; Avatar `src` degrades to initials without it), and
`file-dialogs` (`pick_folder`).

## Where a change belongs

| You want to… | Touch |
|---|---|
| add a layout/paint behavior | `pebbles-render/src/objects/` + a widget wrapper in `pebbles-widgets/src/widgets/` |
| add a catalog component | `pebbles-widgets/src/components/{input,display,layout,navigation}/` |
| change reactivity/scheduling | `pebbles-core/src/reactive.rs` (contains `unsafe`; tread carefully) |
| change reconciliation / hit-testing | `pebbles-core/src/element/` (`build.rs` / `dispatch.rs`) |
| change event routing / windowing | `pebbles-shell/src/app/runner/` |
| add a public API | re-export it from `pebbles::prelude` (`crates/pebbles/src/lib.rs`) |

## Testing model

Tests are **headless**: they mount a `Ui`, drive layout with a `TextEnv`, and
dispatch synthetic pointer/key events — no window, no GPU. All pebbles-widgets
integration tests live in **one** harness (`crates/pebbles-widgets/tests/suite/`,
one module per file) so the workspace links a single test binary; add new files
there and register them in `suite/main.rs`.
