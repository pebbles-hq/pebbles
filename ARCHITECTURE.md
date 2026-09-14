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
                     reconciler, components, input (focus/keyboard/shortcuts/scroll),
                     the router, animation, bounds
pebbles-widgets      the widget catalog: Flutter-style primitive widgets (grouped by
                     concern) + the shadcn-style component set, theming, overlays,
                     windows API
pebbles-shell        the app runner: winit event loop, wgpu surface, vello Renderer,
                     AccessKit a11y, native menus / global hotkeys (feature-gated)
pebbles-macros       the #[component] proc-macro
pebbles              the umbrella crate: re-exports + `pebbles::prelude`
```

`examples/*` (counter, todo, temperature, stopwatch) are workspace members that consume only the umbrella
crate — they are the consumer-facing API check.

## Module organization

Within a crate, code is grouped **by concern into folders, each with a `mod.rs`
front door** that re-exports the concern's public surface (siblings stay private
files). Three rules keep this from swinging into the opposite kind of mess:

- **Earn the folder.** A concern gets a folder once it's ~2–3 files or one file big
  enough to split. A folder wrapping a single small file is noise — genuine
  singletons stay as loose files at the crate's module root.
- **`mod.rs` is the public face.** You read one file to see a concern's surface; the
  implementation files behind it are private. Re-exports flatten, so grouping a file
  into a subfolder never changes its public path (`pebbles_widgets::<Item>` /
  `pebbles_core::<module>` stay put).
- **A concern may span crates — don't force it into one place.** The crate DAG wins.
  Routing lives as a core model (`pebbles-core/src/router.rs`), a web bridge
  (`pebbles-shell/src/web_router.rs`), and a widget view
  (`pebbles-widgets/.../navigation/routing.rs`). That spread is correct layering.

Concretely, the two large crates:

- **`pebbles-widgets/src/widgets/`** (Flutter-style primitives) is grouped:
  `layout/` (boxes, flex, stack, sizing), `animation/` (implicit `Animated*` +
  explicit `*Transition`), `interaction/` (gesture, pointer, drag-and-drop, focus),
  `scrolling/`, `painting/` (canvas, clip, effects), `text/`. Cross-cutting
  singletons (`view`, `media`, `semantics`, `keyed`, `probe`, `spinner`,
  `stream_builder`, `mobile_runtime`) stay loose. The higher-level catalog is
  separately under `components/{input,display,layout,navigation}/`.
- **`pebbles-core/src/`** keeps its public vocabulary at the root (`widget`,
  `element`, `component`, `context`, `reactive`, `router`, `animation`), with
  `input/` grouping focus/keyboard/key/shortcuts/scroll (re-exported to the root so
  `pebbles_core::focus` etc. are unchanged) and `element/` holding the reconciler
  internals (`build.rs` / `dispatch.rs`).

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

SolidJS semantics in `pebbles-core/src/reactive/`:

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
| add a layout/paint behavior | `pebbles-render/src/objects/` + a widget wrapper in the matching `pebbles-widgets/src/widgets/{layout,painting,…}/` group |
| add a primitive widget | `pebbles-widgets/src/widgets/<concern>/` (layout, animation, interaction, scrolling, painting, text) — or a loose file if it's a genuine singleton |
| add a catalog component | `pebbles-widgets/src/components/{input,display,layout,navigation}/` |
| change input handling (focus/keyboard/shortcuts/scroll) | `pebbles-core/src/input/` |
| change routing | model in `pebbles-core/src/router.rs`, browser bridge in `pebbles-shell/src/web_router.rs`, `RouteView` in `pebbles-widgets/src/components/navigation/routing.rs` |
| change theming / the style system | `pebbles-widgets/src/design/` (theme, style, modifiers, fonts, text direction) |
| change an overlay layer | `pebbles-widgets/src/services/` (overlay, dialog, sheet, toast, global menu) |
| change window / native-menu behavior | `pebbles-widgets/src/platform/` |
| change reactivity/scheduling | `pebbles-core/src/reactive/` (contains `unsafe`; tread carefully) |
| change reconciliation / hit-testing | `pebbles-core/src/element/` (`build.rs` / `dispatch.rs`) |
| change event routing / windowing | `pebbles-shell/src/app/runner/` |
| add a hook | define it next to what it drives, then index it in `crates/pebbles/src/hooks.rs` **and** the prelude |
| add a public API | re-export it from `pebbles::prelude` (`crates/pebbles/src/lib.rs`) |
| change the test frame pipeline | `crates/pebbles-testing/` only — tests must not re-derive it |
| add/edit a project template | `crates/pebbles-cli/templates/<kind>/` (real files, `include_str!`-embedded) + a `Template` entry in `crates/pebbles-cli/src/template.rs` |

## Testing model

Tests are **headless**: they mount a `Ui`, drive layout with a `TextEnv`, and
dispatch synthetic pointer/key events — no window, no GPU. All pebbles-widgets
integration tests live in **one** harness (`crates/pebbles-widgets/tests/suite/`,
one module per file) so the workspace links a single test binary; add new files
there and register them in `suite/main.rs`.

**Use `pebbles-testing`.** It owns the frame lifecycle so tests don't re-derive
it (and so a pipeline change edits one crate, not every test file):

```rust
let mut h = Harness::new().window(500.0, 200.0);
h.mount(my_component);
h.draw();                       // rebuild -> layout -> paint, settled
h.click(Offset::new(20.0, 9.0));
assert!(h.render_node_count() < 900);
```

`Harness::new()` initializes every global service (theme, overlay, dialog,
sheet, focus, animation); `mount` adds the `View` + `OverlayHost` wrapper the
shell would. `draw()` loops the **corrective relayout** until geometry settles —
paint can invalidate the layout it just ran on (a lazily materialized text line
measuring taller than its estimate), so asserting on the first pass can read
unsettled geometry. `ui`/`env` stay public for anything not wrapped. Tests that
own their own `Ui` (multi-window) use the free `frame(..)` / `draw_frame(..)`
functions instead.
