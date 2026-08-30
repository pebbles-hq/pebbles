# Pebbles

A **Flutter-style, desktop-first GUI framework** for Rust, built on
[Vello](https://vello.dev) (GPU 2D rendering) and the Linebender stack
(kurbo · peniko · parley).

Pebbles keeps Flutter's **UI-building syntax** — `Row`/`Column`/`Container`/`Text`,
the box layout protocol, a rich themed widget catalog — but swaps Flutter's
`StatefulWidget`/`setState` boilerplate for **SolidJS-style reactivity**: signals,
memos, effects, and plain **function components**. The result is Flutter's
familiarity with a fraction of the ceremony, in idiomatic Rust.

> Status: early but broad. The catalog, reactivity, layout, text editing, scrolling
> and theming are real and demonstrated in the gallery. It's a personal project and
> the foundation for **Gravel**, an IDE built on top of it — but it stands on its own.

```rust
use pebbles::prelude::*;

fn counter() -> impl IntoWidget {
    let count = create_signal(0); // local state, SolidJS-style

    center(column(children![
        text(format!("{}", count.get())).size(72.0),
        row(children![
            button("−").variant(ButtonVariant::Outline)
                .on_pressed(action(move || count.update(|c| *c -= 1))),
            SizedBox::spacer(16.0, 0.0),
            button("+").on_pressed(action(move || count.update(|c| *c += 1))),
        ])
        .main_axis_min(),
    ]))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(counter)).title("Counter").size(480, 420).run()
}
```

```bash
cargo run -p counter     # the example above
cargo run -p gallery     # the full widget showcase / documentation
```

## Programming model

- **UI syntax is Flutter.** `column(children![...])`, `Container::new().color(..).padding(..)`,
  `Row`/`Expanded`/`Stack`, the constraints-down / sizes-up box layout — a Flutter
  developer is immediately at home.
- **State is SolidJS.** No `StatefulWidget`, no `setState`. Local *and* global state
  use the same `create_signal` primitive; reads auto-subscribe, writes re-render only
  the components that depend on them. Plus `create_memo`, `create_effect`,
  `create_store`, `create_cleanup`.
- **Components are functions.** `fn my_widget() -> impl IntoWidget`, mounted with
  `component(..)` (or `component_props(..)` for parameterized, reusable widgets).
- **Handlers are plain closures.** `action(move || count.update(..))` /
  `action_event(move |e| ..)` — no macros, no interior-mutability dance in user code.

## Architecture

Flutter's **three trees**, made Rust-native:

```
Widget       (immutable config, rebuilt freely)
  │  reconcile
Element      (retained; the reactive owner)   ← arena: SlotMap<ElementId, …>
  │  create / update
RenderObject (layout + paint into a vello::Scene) ← arena: SlotMap<RenderId, …>
  │  vello + wgpu
GPU surface  (winit window)
```

- **Box layout, verbatim:** constraints go down, sizes come up, the parent sets the
  position.
- **Arenas, not `Rc<RefCell>`:** a parent lays out its children (mutating siblings
  while being mutated). Each node lives in a generational [`slotmap`] arena; to recurse
  the framework lifts the child's boxed object out, recurses with a hole-free `&mut`
  tree, and puts it back — no aliasing, no borrow panics.
- **Reactivity engine:** a thread-local runtime tracks which components read each
  signal; a write schedules exactly those components, which re-render and reconcile.
  Dioxus-like internals, Solid-like API.

### Crates

Layered so the GPU stack is quarantined and the core compiles in seconds:

| Crate | Responsibility |
|-------|----------------|
| [`pebbles-foundation`](crates/pebbles-foundation) | geometry (kurbo), layout enums, color + the full Tailwind/shadcn palette |
| [`pebbles-render`](crates/pebbles-render) | `BoxConstraints`, render tree, layout/paint, text (parley), icons |
| [`pebbles-core`](crates/pebbles-core) | the runtime: `Widget`/`Element` traits, reconciler, reactivity, focus, keyboard, animation, clipboard |
| [`pebbles-widgets`](crates/pebbles-widgets) | the catalog: primitives + shadcn-style components, theme, styling |
| [`pebbles-shell`](crates/pebbles-shell) | winit window + wgpu surface + Vello GPU renderer + event loop |
| [`pebbles`](crates/pebbles) | umbrella crate + `prelude` |

`vello`'s GPU deps are optional, so `pebbles-render` uses the CPU-side `vello::Scene`
encoder with **no** wgpu — keeping layout/paint logic unit-testable headlessly. Only
`pebbles-shell` links the GPU renderer.

## What's in the box

Everything below is built and shown in `cargo run -p gallery`, styled to **shadcn**.

- **Layout & primitives** — `Text`, `Container`, `Row`/`Column`, `Expanded`/`Flexible`/
  `spacer`, `Stack`/`Positioned`, `Padding`, `Align`/`center`, `SizedBox`,
  `ConstrainedBox`, `DecoratedBox` (color · border · radius · shadow), `Opacity`,
  `ClipRRect`, `Wrap`, `AspectRatio`, `Icon`, `Spinner`.
- **Icons** — the full **Lucide** set (~1800 glyphs) ships as the default, addressable
  by const (`lucide::CAMERA`), by name (`lucide::by_name("circle-check")`), or via the
  named `IconKind` handles. An icon is plain data (`IconData`), so your own icons drop
  in anywhere an icon is accepted — the set is fully pluggable.
- **Gestures** — `GestureDetector` with the full pointer set: tap, double-tap,
  secondary/tertiary click, the long-press lifecycle, hover enter/exit, and drag/pan.
- **Buttons** — `Button` (Primary/Secondary/Outline/Ghost/Destructive/Link · Sm/Md/Lg ·
  custom colors · leading/trailing icons · `.shadow()` · `.loading()` · full event set ·
  focus ring + keyboard activation) and `IconButton`.
- **Text inputs** — a full editor: selection, clipboard (Ctrl+A/C/X/V via the system
  clipboard), undo/redo, word navigation, and mouse (click-to-caret, drag-select,
  double-click word, shift-click). Plus the input types: `text_field`, `text_area`,
  `password_field` (show/hide), `search_field` (clear), `email`/`number`/`url`/`phone`,
  and `date_field` (auto-formats to MM/DD/YYYY + a **calendar popover** picker).
  Form-field states: label, helper, error, disabled, input filtering, masking.
- **Selection controls** — `Select` (dropdown in an overlay layer, flips near edges),
  `Slider` (draggable), animated `Checkbox`/`Switch`/`Radio`/`Toggle`, `Progress`.
- **Scrolling** — `SingleChildScrollView` with **spring physics** (smooth wheel) and a
  customizable scrollbar; **virtualized** `ListView::builder` and `GridView::builder`
  (only visible items built); a `ScrollController` for programmatic scrolling; keyboard
  scrolling; nested-scroll bubbling; `ScrollExt::scrollable()`.
- **Surfaces & data** — `Card`, `Badge`, `Alert`, `Avatar`, `Separator`, `Skeleton`,
  `ListTile`, `Table`, `TreeView`, typography helpers.
- **App chrome** — `Scaffold`, `SideNav`, `TopPanel`, `BottomNav`, `Tabs`, `SplitView`,
  `Panel`, `Accordion`/`Collapsible`, `Breadcrumb`/`Pagination`/`Toolbar`/`StatusBar`.
- **Overlays** — a global overlay layer (`show_overlay`/`hide_overlay`, `OverlayHost`)
  powering dropdowns, popovers and the date picker.
- **Theming & styling** — a global `Theme` (shadcn light/dark tokens), the complete
  Tailwind/shadcn color palette, and a general CSS-like `Style` system applicable to
  any widget.
- **Animation** — a spring + tween driver (`animated`/`animate_to`) and a looping
  ticker (`create_loop`) behind hover fades, sliding switches, focus rings and spinners.

## Testing

Layout and the whole reconcile loop are proven **without a GPU or window**:

```bash
cargo test
```

The headless engine tests mount a real widget tree, dispatch taps and keystrokes, and
assert the render tree changes — driving `input → signal write → reconcile → relayout`
end to end in-process.

## Roadmap

Natural next steps: a tooltip overlay + accessibility/semantics layer; per-widget error
validation helpers and a time picker; variable-height list virtualization and slivers
(sticky/collapsing headers); scale/spring transforms; and an `InheritedWidget`-style
theme provider (theme is a thread-local today).

## License

Licensed under the **Apache License, Version 2.0** — see [LICENSE](LICENSE).

Copyright © 2026 Reyco Seguma.
