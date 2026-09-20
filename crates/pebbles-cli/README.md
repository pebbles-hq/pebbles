# pebbles-cli

`pebbles` — the developer CLI for the Pebbles GUI framework.

Flutter-style tooling for a Rust desktop UI:

```text
pebbles create <name>   scaffold a new Pebbles project (app or widget package)
pebbles run             build + run in dev mode (rich logs, hot-restart on save)
pebbles doctor          check your environment
```

It is deliberately std-only — no clap, no notify. Arg parsing is hand-rolled,
file watching is a light mtime poll, and process control is `std::process`.

This crate ships the `pebbles` binary. It is the CLI companion to the 10-crate
framework whose libraries layer foundation → icons → render → core → widgets →
shell → umbrella `pebbles`.

Part of [Pebbles](https://github.com/pebbles-hq/pebbles).
Licensed under Apache-2.0.
