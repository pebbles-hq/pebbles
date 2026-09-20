# pebbles-macros

Proc-macros for the Pebbles GUI framework.

Currently just `#[component]` — the ergonomic authoring form. It turns a plain
function into a Pebbles component: hooks and the function's arguments (owned, by
value) are available directly in the body.

```ignore
#[component]
fn stat_card(title: String, value: i64) -> Element {
    column(children![text(title), text(format!("{value}"))]).into_widget()
}
// stat_card("Revenue".into(), 42) — call it like any widget ctor.
```

It expands to a `Clone` props struct, a render `fn(&Props) -> Element`, and a
public ctor `fn name(args…) -> Element`. Every arg is required, becomes an owned
`Clone` props field, and is handed to the body by value; generics, lifetimes, and
`self` are rejected with a clean error. The underlying `component_props` stays the
documented mechanism, not a second style.

Re-exported from `pebbles::prelude`, so app authors get it through the `pebbles`
umbrella rather than depending on this crate directly.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
