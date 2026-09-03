# Changelog

All notable changes to Pebbles are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org) once published.

## [Unreleased]

### Added
- Markdown reader + editor (feature `markdown`, GFM via pulldown-cmark):
  `markdown(text)` / `markdown().bind(signal)` renders headings, emphasis,
  strikethrough, inline + fenced code (JetBrains Mono), clickable links
  (`on_link`), nested quotes/lists, **task lists whose checkboxes rewrite the
  bound source** (`toggle_task` is public), tables, rules, and images (via
  `image-view`). `markdown_editor(signal)` adds Edit / Split-live-preview /
  Read modes driven by an app-owned mode signal. Fully themable via
  `MarkdownStyle` (theme-following defaults). Gallery screen included.
- File explorer refinement to VSCode parity: built-in right-click menus now
  work regardless of the global-menu switch (widget-specific always wins —
  previously the row's selection handler starved the menu, so it never
  opened); the VSCode keyboard set (arrows + Shift-extend, F2 rename, Delete,
  Mod+A select-all-visible, Escape clear) active only while the explorer has
  a selection; per-node customization (`FsNode::icon`/`FsNode::color`
  builders, `FileTree::insert_node`, public `FileTree::node_mut`,
  `FileExplorer::renaming()`).
- `create_shortcut_if` — conditional shortcut handlers that can decline a
  press (falls through to older registrations, then the shell's scroll
  fallback). Shortcut registrations are now owner-keyed: a re-rendering
  component overwrites its binding in place instead of leaking a registry
  entry per render.
- `ContextMenu::on_open` — a hook that runs just before the menu opens (e.g.
  sync selection to the clicked row).
- File explorer outside-control surface: a bindable `filter()` signal (wire
  any input; live pruning — matched folders keep their subtree, folders
  force-expand while filtering, keyboard navigation walks exactly the filtered
  rows), a focus row independent of the selection (`active_row()`, rendered
  as the ring; Mod+↑/↓ walks it without selecting, Mod+Space toggles it in —
  one-by-one keyboard multi-select), and programmatic control: public
  `reveal(id)` ("Reveal in Explorer"), `expand_all()`, `detach_folder()`.
  Range anchors (Shift-click / Shift-arrows) follow the focus row while it
  remains selected.
- File explorer icon themes: `FileExplorer::set_icon_theme(fn)` /
  `clear_icon_theme` / `resolved_icon` — a VSCode-style resolver mapping every
  node (+ open state) to any of the ~1800 bundled lucide glyphs and an
  optional color; per-node `FsNode::icon`/`color` overrides always win.
  `FsNode::icon` upgraded from `IconKind` to full `IconData`; open folders
  now default to the open-folder glyph; `detach_folder()` leaves filesystem
  mode. The gallery screen is a full configurator (theme dropdown, demo tree,
  per-node override showcase).
- File explorer selection is now clearly visible (the old 12% accent mix was
  imperceptible): selected rows use the full accent background with
  accent-foreground text, the ACTIVE row adds a focus ring, hover/drop-target/
  cut-dim states are distinct — the standard list state set.
- File explorer clipboard + standard rename UX: Mod+C/X/V copy/cut/paste
  (Cut moves on paste, Copy duplicates whole subtrees — on disk too in
  filesystem mode; cut rows render dimmed; Escape cancels), Cut/Copy/Paste in
  the context menus, Home/End row jumps, Mod+A works from idle. Renaming now
  opens PREFILLED with the current name and the stem selected (typing
  replaces, arrows edit in place); New File/Folder still start from an empty
  field. `TextField::select_range` — initial selection applied at mount.
- `untrack` — run a closure with dependency tracking suspended (Solid's
  `untrack`), exported from the prelude.

### Fixed
- GPU error recovery now resets the ENTIRE GPU stack — instance, adapter,
  device, all surfaces, all renderers (throttled to ~1/second): a lost device
  hands out invalid resources forever, so renderer-level rebuilds could never
  recover (observed as `Buffer 'vello.scene' is invalid` on a fresh renderer).
  The event loop also wakes itself when the error count moves, so recovery
  runs even from an idle `Wait` state. Empirically: one recovered error at
  startup, zero repeats, zero panics.
- GPU error recovery now REBUILDS the poisoned state instead of merely
  skipping the frame: a failed resource could stay cached inside vello's
  renderer pool, erroring on every subsequent frame. The render loop watches
  the uncaptured-error count and recreates the renderer + surface target the
  moment it moves — one recovered error at startup, clean frames after.
- The shell no longer dies on a transient GPU validation error: some
  Linux/Vulkan driver startup races surface as ONE spurious wgpu validation
  error on an early frame, which wgpu's default handler turns into a process
  panic (seen as `Texture with '' label is invalid` when opening a heavy
  screen). Pebbles now installs an uncaptured-error handler per device that
  logs and skips the frame — the next frame renders normally; persistent
  errors keep logging so they stay visible.
- `use_bounds()` leaked one immortal root signal per component remount (the
  registry entry was dropped but never its arena slot, and headless runs never
  GC'd at all) — caught by the gallery's lifecycle soak. Bounds signals are
  now freed on unmount (`dispose_root_signal`), and the shell GC frees too.
- `create_memo` computed its initial value under the *calling component's*
  observer, accidentally subscribing the component to the memo's raw inputs —
  every input write re-rendered it, defeating the dedup (`Store::select_memo`
  inherited the bug). The initial compute now runs untracked; the memo's
  effect owns the input subscriptions.

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
