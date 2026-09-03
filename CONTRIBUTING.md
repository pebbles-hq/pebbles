# Contributing

Pebbles is early and moving fast; issues and PRs are welcome. Start with
[ARCHITECTURE.md](ARCHITECTURE.md) to see where a change belongs.

## Toolchain

Pinned by `rust-toolchain.toml` (stable, edition 2024) — `rustup` picks it up
automatically.

## Checks to run

```bash
cargo check --workspace          # fast type-check, no linking
cargo test -p pebbles-widgets    # the consolidated suite (ONE test binary)
cargo clippy --workspace         # warn-only lints from [workspace.lints]
cargo fmt --check                # style (max_width = 110, see rustfmt.toml)
```

> Avoid `cargo build --workspace --all-targets` on small disks: it links every
> test binary with the full vello/parley graph embedded.

## Conventions

- **Tests are headless** (no window/GPU). New pebbles-widgets tests go in
  `crates/pebbles-widgets/tests/suite/<name>.rs` + a `mod <name>;` line in
  `suite/main.rs` — do not add new top-level files under `tests/`.
- **Icons are generated**: `crates/pebbles-icons/src/lucide.rs` comes from
  `scripts/gen-lucide.mjs` (Node). Never hand-edit it.
- New public API must be re-exported from `pebbles::prelude`.
- Optional OS integrations are default-off cargo features forwarded through the
  umbrella crate (see `native-menus` / `global-hotkeys` for the pattern).

## Publish checklist (future)

- [ ] Per-crate `README.md`s (crates.io renders them)
- [ ] `CHANGELOG.md` entry + version bump across `workspace.package`
- [ ] `cargo publish` bottom-up: foundation → icons → render → core → widgets → shell → macros → pebbles
