# Changelog

All notable changes to Pebbles are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org) once published.

## [Unreleased]

### Changed
- Repository restructured to open-source conventions: consolidated the
  pebbles-widgets integration tests into a single harness
  (`tests/suite/`), split `RenderList` out of `objects/scroll.rs`, split
  `file_explorer.rs` into a module directory, added workspace lints, CI,
  `deny.toml`, `typos.toml`, `rustfmt.toml`, and this changelog.
- Split the two largest modules by concern: `pebbles-shell/src/app.rs` →
  `app.rs` (the `App` builder) + `app/runner/{mod,input,render,windows}.rs`,
  and `pebbles-core/src/element.rs` → `element/{mod,build,dispatch}.rs`.
- The image stack (`ImageView`, `image_from_bytes`/`image_from_path`) and the
  OS folder picker (`pick_folder`) are now **opt-in cargo features**
  (`image-view`, `file-dialogs`) on `pebbles-widgets`, forwarded by the
  umbrella crate — a default Pebbles app no longer links image codecs, an
  HTTP client, or rfd's async runtime. `Avatar` with a `src` URL degrades to
  its initials face when `image-view` is off.

## [0.0.1] — unpublished

Initial development version: the full P2 roadmap — widget catalog +
shadcn-style components, SolidJS-style reactivity, Flutter three-tree layout,
Vello/wgpu rendering, Parley text (IME/CJK), theming with live light/dark,
multi-window + IPC, async resources, AccessKit accessibility, native menus and
global hotkeys (feature-gated), `#[component]` macro, tooltips/overlays,
virtualized lists, canvas, and the gallery example.
