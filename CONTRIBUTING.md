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
cargo clippy --workspace --all-targets --all-features -- -D warnings  # CI gates on this
cargo fmt --check                # style (max_width = 110, see rustfmt.toml)
```

> CI runs clippy with `-D warnings` (a new lint fails the build) and rustfmt as a
> separate blocking gate — run both before pushing. CI floats on `stable`, so
> `rustup update stable` first if a lint disagrees with your local toolchain.

> Avoid `cargo build --workspace --all-targets` on small disks: it links every
> test binary with the full vello/parley graph embedded.

## Conventions

- **Tests are headless** (no window/GPU). New pebbles-widgets tests go in
  `crates/pebbles-widgets/tests/suite/<name>.rs` + a `mod <name>;` line in
  `suite/main.rs` — do not add new top-level files under `tests/`.
- **Drive tests through the harness.** New tests must use `pebbles_testing::Harness`
  (or its free functions) to mount, tick and query — never hand-roll a `Ui::new()`
  plus a private frame loop. The harness owns service init and the corrective-relayout
  settle; a bespoke loop silently drifts from the real frame pipeline.
- **Icons are generated**: `crates/pebbles-icons/src/tabler.rs` comes from
  `scripts/gen-tabler.mjs` (Node). Never hand-edit it.
- New public API must be re-exported from `pebbles::prelude`.
- Optional OS integrations are default-off cargo features forwarded through the
  umbrella crate (see `native-menus` / `global-hotkeys` for the pattern).

## Test discipline (no flaky tests)

CI tests must be **deterministic**. Do not:

- **Sleep or assert on wall-clock time.** Drive time explicitly through the harness
  (`tick(dt)` / `settle()`); never `std::thread::sleep` or compare against
  `Instant::now()`. Time-dependent behavior is advanced by the harness clock, not the OS.
- **Assert on a wall-clock *budget*** (e.g. "renders in < 5ms"). Bound *work* instead —
  node counts, fragment/encode counts, recompute counts — which are machine-independent.
- **Depend on external state** — the network, the real filesystem outside a temp dir,
  environment, or a real GPU/window (tests are headless).

If a test is flaky, it is a bug in the test, not the CI: fix the nondeterminism. A
genuinely infra-timeout-prone job (not a logic failure) is re-run, not weakened.

## Publish checklist (future)

- [ ] Per-crate `README.md`s (crates.io renders them)
- [ ] `CHANGELOG.md` entry + version bump across `workspace.package`
- [ ] `cargo publish` bottom-up: foundation → icons → render → core → widgets → shell → macros → pebbles
