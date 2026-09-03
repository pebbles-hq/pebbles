# Changelog

All notable changes to Pebbles are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[SemVer](https://semver.org) once published.

## [Unreleased]

### Added — the `pebbles` developer CLI (`crates/pebbles-cli`)
Flutter-style tooling for a Rust desktop UI, dependency-free (std only):
- `pebbles new <name>` — scaffold a runnable app (Cargo.toml wired to the local
  pebbles checkout, a starter counter `main.rs`, `pebbles.toml`, `.gitignore`,
  README). `--git` uses a git dependency instead of a path.
- `pebbles run` — build, launch with dev diagnostics on (`PEBBLES_DEV=1`,
  `PEBBLES_LOG=debug`), stream the framework's logs **prettified/colorized by
  level and category**, and **hot-restart on every file save** (mtime-poll
  watch → rebuild → relaunch, ~sub-3s for a one-file change; verified). Flags:
  `-p/--package <name>` (also `--example`/`--bin`), `--watch <dir>` (repeatable),
  `--release`, `--no-reload`, `-q/--quiet`, `--log <level>`, `-- <app args>`.
- **Runs workspace samples**, not just scaffolded apps: `pebbles run -p gallery`
  from the repo root, or `cd examples/counter && pebbles run` (auto-detect). It
  finds the workspace root (`[workspace]`) so a member's binary is located in the
  shared `target/`, builds with `cargo build -p <name>`, and lists the available
  packages if you run it at the root with no `-p`. `--watch crates` hot-restarts
  a sample when you edit the framework itself.
  (Note: this is fast hot-*restart*, not state-preserving hot-reload — Rust has
  no VM to swap code in a live process; the runner is structured so a future
  hot-patch engine can slot in.)
- `pebbles doctor` — checks cargo/rustc, the pebbles source, and Vulkan.
- `cargo install --path crates/pebbles-cli` puts a `pebbles` binary on PATH.

### Added — deep dev logging & Flutter-style overflow detection
- The diagnostic log moved from `pebbles-core` to **`pebbles-foundation`** (the
  lowest crate) so every layer — including the render engine below core — logs
  to one stream. `pebbles_core::log` stays valid (re-export). New `Layout` and
  `Perf` categories.
- **Overflow detection**: in dev mode, `Row`/`Column` that can't fit their
  children on a bounded main axis log a Flutter-style warning — *"Row overflowed
  by 280.0px on the horizontal axis (children need 400.0px, only 120.0px
  available; 2 children). Wrap it in a scroll view, use Expanded/Flexible, or
  shrink a child."* Throttled to once per ~3 s per unique overflow; off outside
  dev mode; no false positives across the gallery's 59 screens.
- `PEBBLES_DEV=1` (set by `pebbles run`) turns on the dev diagnostics and
  defaults the log to Debug.

### Fixed — the markdown-screen "black screen" (the real one)
The markdown screen froze to a black window. Root-caused with the new UI log
(below), which showed the frame loop stalling in exactly one place:
- **A blocking GPU barrier deadlocked the render thread.** An earlier "driver
  race" fix called `device.poll(Wait)` with no timeout between the vello pass
  and present. On this Intel/Vulkan setup that submission never signalled, so
  the main thread blocked forever → black window. Replaced with a non-blocking
  `poll(Poll)` (the swapchain + AutoVsync already pace frames).
- **Aggressive GPU-stack resets turned a harmless warning into a freeze.** vello
  0.10 emits a spurious per-frame `create_view` *validation* error for the
  markdown scene on this GPU — but the frame still renders correctly (measured
  125 fps straight through it). The shell was rebuilding the entire GPU stack
  (~3 s) on every such error, so it managed ~1 frame per 3 s: a frozen black
  screen. The uncaptured-error handler now distinguishes **non-fatal validation
  errors** (log, throttled; keep rendering) from **fatal device-lost/OOM
  errors** (rebuild the stack). Result: the markdown screen renders steadily
  (2 239 frames under an input storm, 0 resets, 0 fatal errors).
- Defensive: `RenderDecoratedBox` skips shadow/background/image draws for a
  zero/collapsed size (a 0-sized blur/image would make wgpu allocate an invalid
  texture), and bounded the secondary-window present the same way.

### Added — GUI diagnostic log (`pebbles_core::log`)
A timestamped, leveled, categorized event log for the whole stack with an
in-memory ring buffer. **This is what found every bug above.**
- Levels Trace…Error; categories Frame/Gpu/Input/Nav/Reactive/Overlay/Widget.
- `PEBBLES_LOG=1|trace|debug|info|warn|error` echoes to stderr;
  `PEBBLES_LOG_FILE=<path>` appends every record (flushed per line, so a hard
  freeze still leaves the last event on disk).
- The shell logs a **frame heartbeat** (every 120 frames + every slow frame),
  GPU errors/resets, and navigation; installs a **panic hook** that dumps the
  ring buffer so a crash always shows the run-up. Ring buffer is always on.

### Fixed — the "navigate while anything is in flight" crash family
Found with the new synthetic-input monkey (`PEBBLES_INPUT_STORM=1`, below):
navigating away from a screen while something it owned was still in flight
killed the whole app. Three instances of one class — a callback/handle
outliving its owner — plus a policy fix:
- **Scroll spring vs. navigation** (the reported markdown-screen crash): a
  wheel fling leaves a spring animating; unmounting the scroll view freed its
  `RenderId`; the next frame's `tick_scrolls` indexed the freed node and
  panicked ("invalid SlotMap key used"). Dead springs are now dropped;
  `mark_needs_layout`/`mark_needs_paint` tolerate stale ids; live content
  drags crossing an unmount are dropped the same way (scrollbar drags already
  were). Regression test: `springs_and_drags_survive_unmount_mid_flight`.
- **Pending timers vs. navigation**: hover-card show/close, submenu open/close
  and the explorer's expand-on-hold timers read component signals after their
  owner unmounted. New `Signal::try_peek()` (None after dispose — the read-side
  mirror of the already-safe `set`/`update`) is now used by every timer closure;
  `get`/`peek` on a disposed signal now panic with a message naming this exact
  bug class instead of "invalid SlotMap key used".
- **Open overlays vs. navigation**: a select/menu/picker panel lives in the
  global overlay and re-renders against its opener's signals — after the opener
  unmounted, that read panicked. `show_overlay_guarded(...)` records an
  aliveness probe; the shell GCs a dead overlay each frame AND the overlay host
  skips a mid-rebuild-pass corpse (the frame-start GC can't catch that
  ordering). All component-scoped openers converted: select, dropdown menu,
  context menu, menubar, combobox, multi-select, popover, date field, time
  field.
- **The frame loop can no longer panic on GPU trouble**: a vello render failure
  logs, bumps the error counter (scheduling the existing full GPU reset) and
  skips the frame instead of `.expect`-crashing the app; a missing renderer is
  recreated in place.

- **Overlay guard, third ordering** (found storming `grid-view`): the host can
  pass its aliveness check and the opener still unmount later in the SAME
  rebuild pass, before the panel child inflates. Panel content is now wrapped
  in a guard component that re-checks the probe at its own inflate/render time
  — the moment that matters.
- **The gallery tour never actually toured**: `install_tour()` ran on every
  `app()` re-render (every navigation), replacing the pending hop with a fresh
  index-0 chain — so `GALLERY_TOUR` visited screen #0 forever and earlier
  "all screens" burn-ins were false coverage. Now installed once per process,
  and every hop is logged (`gallery tour → <route>`) so burn-in output PROVES
  coverage.

### Changed
- `markdown_editor`'s source pane now **auto-grows with its content** (`lines`
  is the minimum, default 16) — like the rendered view, the widget always shows
  the full document and never scrolls internally; wrap it in a scroll area to
  box it. The widget remains chrome-free: modes come from your `mode_signal`,
  controls are yours to build.

### Added (dev tooling)
- `PEBBLES_INPUT_STORM=1` — a deterministic input monkey inside the shell:
  synthetic hovers, wheels, taps, double-taps, drags and key presses driven
  through the exact dispatch paths real input takes. Combine with the gallery's
  `GALLERY_TOUR=<ms>` for a full-app burn-in (this pairing found every crash
  above; the final run survived 92k+ events over 5 minutes, all screens).
  `pick_folder` resolves as "cancelled" under the storm so burn-ins stay
  unattended.

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
- **Text editing panicked on multi-byte characters** ("byte index … is not a
  char boundary"): the editor's anchor/focus byte offsets go stale whenever
  the bound value changes underneath it (a Markdown task toggle rewriting the
  source, any external `signal.set`), and clicks/motions resolved against a
  one-frame-old layout — the first slice then landed inside a multi-byte char
  (the demo doc's em dashes) and crashed the app. This was the remaining
  "Markdown screen crash" under real mouse use. Every offset is now snapped to
  a char boundary before slicing (edit application, motion resolution, drag
  selection, and the paint path). Proven by a new interaction-storm test:
  1,764 click/double-click/drag/type points across Edit/Split/Read over
  em-dash-laden content — plus a 75-second all-screens burn-in tour
  (GALLERY_TOUR=<ms>) with zero panics and zero GPU errors.
- **The GPU validation-error crash is root-caused and fixed.** On some
  Linux/Vulkan drivers (seen on RADV/Wayland), queueing the swapchain
  blit/present while the vello compute submission of the same frame is still
  in flight races inside the driver — surfacing as spurious, timing-dependent
  validation errors ("Texture/Buffer … is invalid"; any logging overhead made
  them vanish) that then poison the device, which is why heavy screens (the
  Markdown workbench) crashed and lighter ones didn't. The shell now waits the
  vello pass out (`device.poll(Wait)`) before touching the swapchain — one
  frame in flight, zero errors across every screen. Vello itself is pristine
  upstream 0.10; no fork.
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
