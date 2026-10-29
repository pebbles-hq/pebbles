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

`just gate` runs all of these (plus the wasm check).

> CI runs clippy with `-D warnings` (a new lint fails the build) and rustfmt as a
> separate blocking gate — run both before pushing. CI floats on `stable`, so
> `rustup update stable` first if a lint disagrees with your local toolchain.

> Avoid `cargo build --workspace --all-targets` on small disks: it links every
> test binary with the full renderer and text graph embedded.

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

## Debugging the GPU renderer

### Environment knobs

| Variable | Effect |
|---|---|
| `PEBBLES_GPU_INFO=1` | Print the `GPU:` adapter line and the `surface:` line to stderr (the web always logs them to the console). |
| `PEBBLES_GPU_STRICT=1` / `=0` | Panic on (or only log) any uncaptured wgpu error. Defaults to on in debug builds, off in release. |
| `PEBBLES_GPU_TRACE` (any value) | Log a backtrace with each uncaptured wgpu error, so you can see which call issued it. |
| `WGPU_VALIDATION=1` / `=0` | Force the backend validation layers on or off; read by wgpu's `InstanceFlags::with_env()`. They default to on in debug builds. |
| `WGPU_DEBUG=1` | Keep debug labels and markers in release builds so captures stay readable. |
| `WGPU_BACKEND` | Pin the backend: `vulkan`, `dx12`, `metal` or `gl`. |
| `WGPU_ADAPTER_NAME`, `WGPU_POWER_PREF` | Choose the adapter by name substring, or `low` / `high` power. |
| `PEBBLES_CPU_RENDER=1` / `PEBBLES_REQUIRE_GPU=1` | Force the software adapter, or fail instead of falling back to it. |
| `PEBBLES_PRESENT_MODE`, `PEBBLES_FRAME_LATENCY` | Override the present mode and the frame latency. |
| `PEBBLES_GPU_TIMING=1` | Request timestamp queries and report `FrameStats::gpu_us`. |
| `PEBBLES_SYNTH_GPU_LOSS` | Simulate a lost device to exercise the rebuild path. |
| `PEBBLES_SYNTH_GPU_DEAD=1` | Test tool: every GPU context after the first finds no adapter, so with `PEBBLES_SYNTH_GPU_LOSS` every rebuild fails and the reset backoff (1, 2, 4, 8, then 10 s) runs. |
| `PEBBLES_FRAME_STATS=1` | Per-frame `[pebbles frame]` / `[pebbles gpu]` lines plus a once-a-second `[pebbles run]` summary (presents per window, `resize_sync_timeouts`, present gaps, configure counts). On the web: `?pebbles-frame-stats=1`. |

### Reading the `GPU:` line

```text
GPU: AMD Radeon RX 6600 | backend=vulkan type=DiscreteGpu | driver="radv Mesa 25.2" | max_tex=16384 | power=HighPerformance
```

The fields, in order: the adapter name; the backend wgpu chose and the device type; the
driver name and version; the largest texture dimension; and the power preference that
picked the adapter. If the line ends with `| software`, you are on a CPU rasterizer
(lavapipe, WARP or SwiftShader). Most "it's slow" or "it looks wrong only on my
machine" reports come down to that, or to an unexpected `backend=gl`. Paste this line,
plus the `surface:` line printed after it, into every rendering bug report.

### Reproducing a validation error

```bash
PEBBLES_GPU_STRICT=1 PEBBLES_GPU_TRACE=1 WGPU_VALIDATION=1 cargo run --example <name>
```

Strict mode turns the first invalid call into a panic at the frame that caused it. The
trace adds the backtrace of the wgpu call that issued it. CI runs every GPU lane with
`PEBBLES_GPU_STRICT=1`, so a validation error is a failing test, not a warning. On Linux
without a GPU, add `VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json` to run on
lavapipe.

### Frame capture

Build with `--release` and set `WGPU_DEBUG=1` so labels survive. Capture tools hook the
graphics API from outside the process, so do not add capture APIs to the code.

- **RenderDoc** (Linux and Windows, Vulkan or DX12): launch the release binary from
  RenderDoc's *Launch Application* tab, with `WGPU_BACKEND=vulkan` or `dx12` in its
  environment list. Press F12 or Print Screen in the app to capture a frame.
- **PIX** (Windows, DX12): set `WGPU_BACKEND=dx12`, launch the binary from PIX's
  *Launch Win32* target and take a GPU capture.
- **Xcode** (macOS, Metal): run the binary, then in Xcode choose *Debug → Attach to
  Process*, and then *Debug → Capture GPU Workload*.
- **Web** (WebGPU or WebGL2): open `chrome://gpu` to see the GPU features and the
  driver blocklist. The `GPU:` and `surface:` lines, and any validation errors, appear
  in the page's DevTools console.

In a capture, every resource is named `pebbles.gpu.*`. Textures, buffers, bind groups and
pipelines are named by role: `pebbles.gpu.atlas.mask`, `pebbles.gpu.buffer.glyphs`,
`pebbles.gpu.bind.composite` and so on. Each frame is one `pebbles.gpu.frame` debug
group, with nested `pebbles.gpu.layer`, `pebbles.gpu.effect` and `pebbles.gpu.custom`
groups, so the event browser follows the layer tree. An unnamed resource is a bug;
please report it.

## Rendering regressions

The post-removal regression policy (wgpu migration 11 §7.4):

1. The golden lane (05 §9) is blocking. A golden/reference update is allowed only in a dedicated commit titled
   `goldens: <reason>` whose PR attaches the render-check report and side-by-side images and has reviewer sign-off.
2. The committed self-goldens of our renderer stay the regression baseline (DR-21); a deliberate visual change is a
   reviewed spec change that updates the spec, the golden model and the affected self-goldens in one PR (05 §9).
3. Any rendering regression is **P1**: fixed or reverted within 5 working days.
4. Bisect with `just render-check-refs` (O07): `git bisect start <bad> <good>` then `git bisect run just
   render-check-refs` (only between commits that contain tools/render-check, i.e. after V10).
5. Write the policy into `pebbles/CONTRIBUTING.md` (O07).

`just render-check-refs` checks against the golden model and the committed self-goldens and exits non-zero on any
FAIL; it never updates goldens.

## Publish checklist (future)

- [ ] Per-crate `README.md`s (crates.io renders them)
- [ ] `CHANGELOG.md` entry + version bump across `workspace.package`
- [ ] `cargo publish` bottom-up: foundation → icons → render → core → widgets → shell → macros → pebbles

## Release gate: every required CI lane green on the release commit

```sh
GH_TOKEN=$(gh auth token) scripts/release-check.py <sha> \
  --sibling pebbles-hq/pebbles-markdown=<sha> \
  --sibling pebbles-hq/pebbles-charts=<sha> \
  --sibling pebbles-hq/pebbles-code-editor=<sha>
```

For each workflow (CI, Security, Licenses) the latest run on `<sha>` counts, whatever triggered it
(re-run or `gh workflow run ci.yml --ref <branch>` to refresh it). The script prints every required
lane (`REQUIRED` in the script) as `ok`, `RED` or `MISSING` with its run URL and exits 1 unless
all are green; each `--sibling owner/repo=sha` needs a run on that sha with every non-skipped job
green. `scripts/release-check.py --self-test` checks the matching logic offline.
